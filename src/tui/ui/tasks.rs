//! The task tables: open tasks of the selected bucket, and done tasks of
//! every bucket. Descriptions wrap within their column, and each row is as
//! tall as its description.

use jiff::Timestamp;
use kru::utils::due;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use super::style;
use super::wrap::wrap_display;
use crate::theme;
use crate::tui::app::{App, Focus};

/// Gap between columns, in cells.
const COLUMN_SPACING: u16 = 1;

/// Widths of the open-tasks columns before the description: #, priority,
/// status, due.
const OPEN_COLUMNS: [u16; 4] = [4, 8, 11, 20];

/// Widths of the done-tasks columns before the description: #, bucket,
/// priority, completed.
const DONE_COLUMNS: [u16; 4] = [4, 14, 8, 20];

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

    let width = description_width(block.inner(area).width, &OPEN_COLUMNS);
    let now = Timestamp::now();
    let rows = tasks.iter().map(|entry| {
        let task = entry.task;
        let due = Cell::from(task.due.map(due::format).unwrap_or_default());
        let (description, height) = description_cell(&task.description, width);
        Row::new(vec![
            Cell::from(entry.num.to_string()).style(style::fg(theme::NUMBER)),
            Cell::from(task.priority.to_string()).style(style::priority(task.priority)),
            Cell::from(task.status.to_string()).style(style::status(task.status)),
            if task.is_overdue(now) {
                due.style(style::fg(theme::OVERDUE))
            } else {
                due
            },
            description,
        ])
        .height(height)
    });

    let header = Row::new(["#", "Priority", "Status", "Due", "Description"]).bold();
    let table = Table::new(rows, column_widths(&OPEN_COLUMNS))
        .column_spacing(COLUMN_SPACING)
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

    let width = description_width(block.inner(area).width, &DONE_COLUMNS);
    let rows = tasks.iter().map(|entry| {
        let task = entry.task;
        let completed = task.time_completed.map(due::format).unwrap_or_default();
        let (description, height) = description_cell(&task.description, width);
        Row::new(vec![
            Cell::from(entry.num.to_string()).style(style::fg(theme::NUMBER)),
            Cell::from(entry.bucket.name.as_str()),
            Cell::from(task.priority.to_string()).style(style::priority(task.priority)),
            Cell::from(completed),
            description,
        ])
        .height(height)
        .dark_gray()
    });

    let header = Row::new(["#", "Bucket", "Priority", "Completed", "Description"]).bold();
    let table = Table::new(rows, column_widths(&DONE_COLUMNS))
        .column_spacing(COLUMN_SPACING)
        .header(header)
        .block(block)
        .row_highlight_style(style::highlight(focused));
    let mut state = TableState::default().with_selected(Some(app.done_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// The fixed columns, then a description column taking the rest.
fn column_widths(fixed: &[u16; 4]) -> [Constraint; 5] {
    let [a, b, c, d] = fixed.map(Constraint::Length);
    [a, b, c, d, Constraint::Min(10)]
}

/// Width left for the description in a table `table_width` wide: what the
/// fixed columns and the gaps between all columns don't use.
fn description_width(table_width: u16, fixed: &[u16]) -> usize {
    let gaps = COLUMN_SPACING * fixed.len() as u16;
    let used = fixed.iter().sum::<u16>() + gaps;
    usize::from(table_width.saturating_sub(used)).max(1)
}

/// The description wrapped to `width`, and the row height it needs.
fn description_cell(description: &str, width: usize) -> (Cell<'static>, u16) {
    let lines: Vec<Line> = wrap_display(description, width)
        .into_iter()
        .map(Line::from)
        .collect();
    let height = lines.len() as u16;
    (Cell::from(Text::from(lines)), height)
}
