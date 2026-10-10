//! Centred popups: text prompts and y/n confirmations.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::wrap::wrap;
use crate::tui::app::{App, Pending, Prompt};

/// Width of prompts and confirmations, if the terminal is wide enough.
const WIDTH: u16 = 70;

/// Most lines of input shown at once; longer input scrolls.
const MAX_INPUT_LINES: usize = 8;

/// A text prompt that grows as the input wraps, with the cursor at the end.
pub(super) fn render_input(frame: &mut Frame, app: &App, prompt: Prompt, buffer: &str) {
    let width = WIDTH.min(frame.area().width);
    let lines = wrap(buffer, usize::from(width.saturating_sub(2)).max(1));

    // Grow with the input, then scroll so the last line stays in view.
    let max_lines = MAX_INPUT_LINES.min(usize::from(frame.area().height.saturating_sub(2)).max(1));
    let visible = &lines[lines.len().saturating_sub(max_lines)..];

    let area = popup_area(frame.area(), width, visible.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let text: Vec<Line> = visible
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    let block = Block::bordered()
        .title(format!(" {} ", prompt_title(app, prompt)))
        .yellow();
    frame.render_widget(Paragraph::new(text).block(block), area);

    let last = visible.last().map_or(0, |line| line.chars().count());
    frame.set_cursor_position((area.x + 1 + last as u16, area.y + visible.len() as u16));
}

/// A y/n popup, tall enough for the wrapped question.
pub(super) fn render_confirm(frame: &mut Frame, pending: &Pending) {
    let width = WIDTH.min(frame.area().width);
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

/// A `width` × `height` rectangle centred in `area`.
pub(super) fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    area
}

/// What the prompt asks for, shown in its border.
fn prompt_title(app: &App, prompt: Prompt) -> String {
    match prompt {
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
    }
}
