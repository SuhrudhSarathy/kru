//! The task tables: open tasks of the selected bucket, and done tasks of
//! every bucket.

use jiff::Timestamp;
use kru::utils::due;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Stylize;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use super::style;
use crate::tui::app::{App, Focus};

/// Open tasks of the selected bucket.
pub(super) fn render_open(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Tasks;
    let title = match app.selected_bucket() {
        Some(bucket) => format!("Tasks in {}", bucket.name),
        None => "Tasks".to_string(),
    };
    let block = style::pane(&title, focused);

    let tasks = app.open_tasks();
    if tasks.is_empty() {
        let text = if app.selected_bucket().is_some() {
            "No open tasks. Press a to add one."
        } else {
            ""
        };
        frame.render_widget(Paragraph::new(text).dark_gray().block(block), area);
        return;
    }

    let now = Timestamp::now();
    let rows = tasks.iter().map(|entry| {
        let task = entry.task;
        let due = Cell::from(task.due.map(due::format).unwrap_or_default());
        Row::new(vec![
            Cell::from(entry.num.to_string()).cyan(),
            Cell::from(task.priority.to_string()).style(style::priority(task.priority)),
            Cell::from(task.status.to_string()).style(style::status(task.status)),
            if task.is_overdue(now) { due.red() } else { due },
            Cell::from(task.description.as_str()),
        ])
    });

    let widths = [
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(20),
        Constraint::Min(10),
    ];
    let header = Row::new(["#", "Priority", "Status", "Due", "Description"]).bold();
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(style::highlight(focused));
    let mut state = TableState::default().with_selected(Some(app.task_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// Done tasks of every bucket, under a full-width line.
pub(super) fn render_done(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Done;
    let tasks = app.done_tasks();
    let block = Block::new()
        .borders(Borders::TOP)
        .title(format!(" Done · all buckets ({}) ", tasks.len()))
        .border_style(style::border(focused));

    if tasks.is_empty() {
        let hint = Paragraph::new("Nothing done yet.").dark_gray().block(block);
        frame.render_widget(hint, area);
        return;
    }

    let rows = tasks.iter().map(|entry| {
        let task = entry.task;
        let completed = task.time_completed.map(due::format).unwrap_or_default();
        Row::new(vec![
            Cell::from(entry.num.to_string()).cyan(),
            Cell::from(entry.bucket.name.as_str()),
            Cell::from(task.priority.to_string()).style(style::priority(task.priority)),
            Cell::from(completed),
            Cell::from(task.description.as_str()),
        ])
        .dark_gray()
    });

    let widths = [
        Constraint::Length(4),
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(20),
        Constraint::Min(10),
    ];
    let header = Row::new(["#", "Bucket", "Priority", "Completed", "Description"]).bold();
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(style::highlight(focused));
    let mut state = TableState::default().with_selected(Some(app.done_index));
    frame.render_stateful_widget(table, area, &mut state);
}
