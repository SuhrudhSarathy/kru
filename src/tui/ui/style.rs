//! Styles shared by the panes and overlays.

use kru::task::{Priority, Status};
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;

/// A bordered block, highlighted when focused.
pub(super) fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::bordered()
        .title(format!(" {title} "))
        .border_style(border(focused))
}

/// Border colour: cyan when focused, grey otherwise.
pub(super) fn border(focused: bool) -> Style {
    if focused {
        Style::new().cyan()
    } else {
        Style::new().dark_gray()
    }
}

/// Row highlight: strong in the focused pane, subtle elsewhere.
pub(super) fn highlight(focused: bool) -> Style {
    if focused {
        Style::new().reversed()
    } else {
        Style::new().bold()
    }
}

/// Colour for a priority.
pub(super) fn priority(priority: Priority) -> Style {
    match priority {
        Priority::High => Style::new().fg(Color::Red),
        Priority::Medium => Style::new().fg(Color::Yellow),
        Priority::Low => Style::new(),
    }
}

/// Colour for a status.
pub(super) fn status(status: Status) -> Style {
    match status {
        Status::ToDo => Style::new(),
        Status::InProgress => Style::new().fg(Color::Blue),
        Status::Done => Style::new().fg(Color::Green),
    }
}
