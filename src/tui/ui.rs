//! Rendering. A pure function of `&App`.

use jiff::Timestamp;
use kru::task::{Priority, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
};

use super::app::{App, Focus, Mode, Pending, Prompt};

/// Draws the whole screen: buckets and open tasks on top, done tasks
/// below, the footer, and any open prompt or confirmation.
pub fn render(frame: &mut Frame, app: &App) {
    let [main, done, footer] = Layout::vertical([
        Constraint::Fill(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());
    let [buckets, tasks] =
        Layout::horizontal([Constraint::Length(28), Constraint::Min(0)]).areas(main);

    render_buckets(frame, app, buckets);
    render_tasks(frame, app, tasks);
    render_done(frame, app, done);
    render_footer(frame, app, footer);

    match &app.mode {
        Mode::Normal => {}
        Mode::Input { prompt, buffer } => render_input(frame, app, *prompt, buffer),
        Mode::Confirm(pending) => render_confirm(frame, pending),
    }
}

/// The buckets pane, with open-task counts; `*` marks the active one.
fn render_buckets(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane("Buckets", app.focus == Focus::Buckets);
    if app.store.buckets().next().is_none() {
        let hint = Paragraph::new("No buckets yet.\nPress a to add one.")
            .dark_gray()
            .block(block);
        frame.render_widget(hint, area);
        return;
    }

    let active = app.store.active_bucket().map(|b| b.header.id);
    let items: Vec<ListItem> = app
        .store
        .buckets()
        .map(|bucket| {
            let marker = if Some(bucket.header.id) == active {
                "* "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker).green(),
                Span::raw(bucket.name.as_str()),
                Span::raw(format!(" ({})", bucket.open_count())).dark_gray(),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight(app.focus == Focus::Buckets));
    let mut state = ListState::default().with_selected(Some(app.bucket_index));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Open tasks of the selected bucket.
fn render_tasks(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.selected_bucket() {
        Some(bucket) => format!("Tasks in {}", bucket.name),
        None => "Tasks".to_string(),
    };
    let block = pane(&title, app.focus == Focus::Tasks);

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
        let due = task.due.map(kru::utils::due::format).unwrap_or_default();
        let due = if task.is_overdue(now) {
            Cell::from(due).red()
        } else {
            Cell::from(due)
        };
        Row::new(vec![
            Cell::from(entry.num.to_string()).cyan(),
            Cell::from(task.priority.to_string()).style(priority_style(task.priority)),
            Cell::from(task.status.to_string()).style(status_style(task.status)),
            due,
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
        .row_highlight_style(highlight(app.focus == Focus::Tasks));
    let mut state = TableState::default().with_selected(Some(app.task_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// Done tasks of every bucket, under a full-width line.
fn render_done(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Done;
    let tasks = app.done_tasks();
    let block = Block::new()
        .borders(Borders::TOP)
        .title(format!(" Done · all buckets ({}) ", tasks.len()))
        .border_style(if focused {
            Style::new().cyan()
        } else {
            Style::new().dark_gray()
        });

    if tasks.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing done yet.").dark_gray().block(block),
            area,
        );
        return;
    }

    let rows = tasks.iter().map(|entry| {
        let task = entry.task;
        let completed = task
            .time_completed
            .map(kru::utils::due::format)
            .unwrap_or_default();
        Row::new(vec![
            Cell::from(entry.num.to_string()).cyan(),
            Cell::from(entry.bucket.name.as_str()),
            Cell::from(task.priority.to_string()).style(priority_style(task.priority)),
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
        .row_highlight_style(highlight(focused));
    let mut state = TableState::default().with_selected(Some(app.done_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// The latest message and the keys available right now.
fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let message = match &app.message {
        Some(message) if message.is_error => Line::from(message.text.as_str()).red(),
        Some(message) => Line::from(message.text.as_str()).green(),
        None => Line::default(),
    };
    let keys = match (&app.mode, app.focus) {
        (Mode::Input { .. }, _) => "enter save · esc cancel",
        (Mode::Confirm(_), _) => "y confirm · n/esc cancel",
        (Mode::Normal, Focus::Buckets) => {
            "j/k move · a add · e rename · x remove · u use · tab next pane · q quit"
        }
        (Mode::Normal, Focus::Tasks) => {
            "j/k move · a add · e edit · space done · s status · p priority · d due · m move · x remove · tab next pane · q quit"
        }
        (Mode::Normal, Focus::Done) => {
            "j/k move · space reopen · e edit · s status · p priority · d due · m move · x remove · tab next pane · q quit"
        }
    };
    let footer = Paragraph::new(vec![message, Line::from(keys).dark_gray()]);
    frame.render_widget(footer, area);
}

/// A centred text prompt with the cursor at the end of the input.
fn render_input(frame: &mut Frame, app: &App, prompt: Prompt, buffer: &str) {
    let title = match prompt {
        Prompt::AddBucket => "New bucket".to_string(),
        Prompt::RenameBucket => "Rename bucket".to_string(),
        Prompt::AddTask => match app.selected_bucket() {
            Some(bucket) => format!("New task in {}", bucket.name),
            None => "New task".to_string(),
        },
        Prompt::EditDescription => "Description".to_string(),
        Prompt::EditDue => "Due: 2026-10-12, \"… 17:00\", tomorrow, fri, 3d, none".to_string(),
        Prompt::MoveTask => {
            let names: Vec<_> = app.store.buckets().map(|b| b.name.as_str()).collect();
            format!("Move to bucket ({})", names.join(", "))
        }
    };

    let area = popup_area(frame.area(), 70, 3);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(buffer).block(Block::bordered().title(format!(" {title} ")).yellow()),
        area,
    );
    let cursor_x = area.x + 1 + buffer.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(area.right() - 2), area.y + 1));
}

/// A centred y/n popup, tall enough for the wrapped question.
fn render_confirm(frame: &mut Frame, pending: &Pending) {
    let width = 70.min(frame.area().width);
    let text_width = width.saturating_sub(2).max(1);
    let lines = (pending.question.chars().count() as u16).div_ceil(text_width) + 1;
    let area = popup_area(frame.area(), width, lines + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(pending.question.as_str()),
            Line::from("y confirm · n cancel").dark_gray(),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" Confirm ").yellow()),
        area,
    );
}

/// A bordered block, highlighted when focused.
fn pane(title: &str, focused: bool) -> Block<'static> {
    let block = Block::bordered().title(format!(" {title} "));
    if focused {
        block.border_style(Style::new().cyan())
    } else {
        block.border_style(Style::new().dark_gray())
    }
}

/// Row highlight: strong in the focused pane, subtle elsewhere.
fn highlight(focused: bool) -> Style {
    if focused {
        Style::new().reversed()
    } else {
        Style::new().bold()
    }
}

/// Colour for a priority.
fn priority_style(priority: Priority) -> Style {
    match priority {
        Priority::High => Style::new().fg(Color::Red),
        Priority::Medium => Style::new().fg(Color::Yellow),
        Priority::Low => Style::new(),
    }
}

/// Colour for a status.
fn status_style(status: Status) -> Style {
    match status {
        Status::ToDo => Style::new(),
        Status::InProgress => Style::new().fg(Color::Blue),
        Status::Done => Style::new().fg(Color::Green),
    }
}

/// A `width` × `height` rectangle centred in `area`.
fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
