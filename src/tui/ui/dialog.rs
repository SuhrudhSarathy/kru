//! Centred popups: text prompts and y/n confirmations.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::tui::app::{App, Pending, Prompt, TextInput};
use crate::tui::wrap::wrap;

/// Width of prompts and confirmations, if the terminal is wide enough.
const WIDTH: u16 = 70;

/// Most lines of input shown at once; longer input scrolls.
const MAX_INPUT_LINES: usize = 8;

/// A text prompt that grows as the input wraps, scrolling to keep the cursor in view.
pub(super) fn render_input(frame: &mut Frame, app: &App, prompt: Prompt, buffer: &TextInput) {
    let width = WIDTH.min(frame.area().width);
    let text_width = usize::from(width.saturating_sub(2)).max(1);
    let lines = wrap(buffer.text(), text_width);
    let (row, column) = buffer.place(text_width);

    // Grow with the input, then scroll so the cursor's line stays in view.
    let max_lines = MAX_INPUT_LINES.min(usize::from(frame.area().height.saturating_sub(2)).max(1));
    let first = (row + 1).saturating_sub(max_lines);
    let visible = &lines[first..lines.len().min(first + max_lines)];

    let area = popup_area(frame.area(), width, visible.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let text: Vec<Line> = visible
        .iter()
        .map(|line| match prompt {
            // Same length, so the cursor still lands right.
            Prompt::Unlock => Line::from("•".repeat(line.chars().count())),
            _ => Line::from(line.as_str()),
        })
        .collect();
    let block = Block::bordered()
        .title(format!(" {} ", prompt_title(app, prompt)))
        .yellow();
    frame.render_widget(Paragraph::new(text).block(block), area);

    frame.set_cursor_position((
        area.x + 1 + column as u16,
        area.y + 1 + (row - first) as u16,
    ));
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
        Prompt::MoveTask => {
            let names: Vec<_> = app.store.buckets().map(|b| b.name.as_str()).collect();
            format!("Move to bucket ({})", names.join(", "))
        }
        Prompt::Unlock => "Password for private buckets".to_string(),
    }
}
