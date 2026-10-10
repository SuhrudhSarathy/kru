//! The expanded view: one task blown up over most of the screen, with its
//! full description and every field. While editing, the same view lists
//! every field as a row with a cursor on the editable ones.

use jiff::Timestamp;
use kru::Entry;
use kru::task::{Field, Tone, Value};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use super::dialog::popup_area;
use super::style;
use super::wrap::{wrap, wrap_display};
use crate::tui::app::{App, Editor, Mode};

/// The expanded task: read-only, or in edit mode while editing.
pub(super) fn render(frame: &mut Frame, app: &App, entry: &Entry) {
    match &app.mode {
        Mode::Edit(editor) => render_editor(frame, app, entry, editor),
        _ => render_view(frame, entry),
    }
}

/// The task as 80% of the screen width; `content` lines tall plus borders.
fn area(frame: &Frame, content: usize) -> ratatui::layout::Rect {
    let screen = frame.area();
    let width = (screen.width * 4 / 5).max(50).min(screen.width);
    let height = (content as u16 + 2).min(screen.height);
    popup_area(screen, width, height)
}

/// Width inside the borders and padding of an expanded view.
fn inner_width(frame: &Frame) -> usize {
    let width = (frame.area().width * 4 / 5).max(50).min(frame.area().width);
    usize::from(width.saturating_sub(4)).max(1)
}

/// Read-only: the description as a title, then the detail fields.
fn render_view(frame: &mut Frame, entry: &Entry) {
    let description = wrap_display(&entry.task.description, inner_width(frame));
    let details = details(entry);

    // Description, a separator line, then the fields.
    let area = area(frame, description.len() + 1 + details.len());
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(format!(" Task #{} · {} ", entry.num, entry.bucket.name))
        .border_style(style::border(true))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [description_area, fields_area] = Layout::vertical([
        Constraint::Length(description.len() as u16),
        Constraint::Min(0),
    ])
    .areas(inner);

    let title_style = if entry.task.is_done() {
        Style::new().bold().crossed_out().dark_gray()
    } else {
        Style::new().bold()
    };
    let lines: Vec<Line> = description
        .into_iter()
        .map(|line| Line::styled(line, title_style))
        .collect();
    frame.render_widget(Paragraph::new(lines), description_area);
    frame.render_widget(
        Paragraph::new(details).block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Style::new().dark_gray()),
        ),
        fields_area,
    );
}

/// One `label  value` line per detail field, after the bucket.
fn details(entry: &Entry) -> Vec<Line<'static>> {
    let now = Timestamp::now();
    let bucket = ("Bucket", entry.bucket.name.clone(), Tone::Neutral);
    let fields = entry
        .task
        .fields()
        .into_iter()
        .filter(|field| field.in_detail)
        .map(|field| (field.label, field.value.long(now), field.tone));
    let rows: Vec<_> = std::iter::once(bucket).chain(fields).collect();

    let label_width = rows
        .iter()
        .map(|(label, ..)| label.len())
        .max()
        .unwrap_or(0)
        + 2;
    rows.into_iter()
        .map(|(label, value, tone)| {
            Line::from(vec![
                Span::raw(format!("{label:<label_width$}")).dark_gray(),
                Span::styled(value, style::tone(tone)),
            ])
        })
        .collect()
}

/// Edit mode: every editable or detail field as a row. The cursor row is
/// marked; choices show arrows, staged changes are highlighted with their
/// old value, and the field being typed into shows the text and cursor.
fn render_editor(frame: &mut Frame, app: &App, entry: &Entry, editor: &Editor) {
    let Some(drafted) = app.draft_task() else {
        return;
    };
    let changed: Vec<&str> = app.draft_changes().iter().map(|c| c.label).collect();
    let now = Timestamp::now();

    let rows: Vec<(Field, Field)> = drafted
        .fields()
        .into_iter()
        .zip(entry.task.fields())
        .filter(|(field, _)| field.is_editable() || field.in_detail)
        .collect();
    let label_width = rows.iter().map(|(f, _)| f.label.len()).max().unwrap_or(0) + 2;
    let value_width = inner_width(frame)
        .saturating_sub(MARKER.len() + label_width)
        .max(10);

    let mut lines = Vec::new();
    let mut cursor = None;
    for (field, before) in &rows {
        let selected = field.key == editor.cursor;
        let typing = editor.typing.as_ref().filter(|_| selected);
        let is_changed = changed.contains(&field.label);

        let values = match typing {
            Some(text) => wrap(text, value_width),
            None => wrap_display(&editor_value(field, selected, now), value_width),
        };
        let value_style = match () {
            _ if typing.is_some() => Style::new().yellow(),
            _ if is_changed => Style::new().yellow().bold(),
            _ if !field.is_editable() => Style::new().dark_gray(),
            _ => style::tone(field.tone),
        };
        let label_style = match () {
            _ if selected => Style::new().cyan().bold(),
            _ if !field.is_editable() => Style::new().dark_gray(),
            _ => Style::new(),
        };

        let last = values.len() - 1;
        for (i, value) in values.into_iter().enumerate() {
            let (marker, label) = match i {
                0 if selected => ("▸ ", field.label),
                0 => (MARKER, field.label),
                _ => (MARKER, ""),
            };
            if typing.is_some() && i == last {
                cursor = Some((
                    lines.len(),
                    MARKER.len() + label_width + value.chars().count(),
                ));
            }
            let mut spans = vec![
                Span::styled(marker, Style::new().cyan()),
                Span::styled(format!("{label:<label_width$}"), label_style),
                Span::styled(value, value_style),
            ];
            if is_changed && typing.is_none() && i == last {
                let was = before.value.short();
                let was = if was.is_empty() {
                    "—".to_string()
                } else {
                    was
                };
                spans.push(Span::raw(format!("  (was {was})")).dark_gray());
            }
            lines.push(Line::from(spans));
        }
    }

    let area = area(frame, lines.len());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(
            " Editing task #{} · {} ",
            entry.num, entry.bucket.name
        ))
        .border_style(Style::new().yellow())
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
    if let Some((row, column)) = cursor {
        frame.set_cursor_position((inner.x + column as u16, inner.y + row as u16));
    }
}

/// Space before each row's label, where the cursor marker goes.
const MARKER: &str = "  ";

/// A field's value in the editor; the choice under the cursor gets arrows
/// to show h/l change it.
fn editor_value(field: &Field, selected: bool, now: Timestamp) -> String {
    match &field.value {
        Value::Choice { value, .. } if selected => format!("‹ {value} ›"),
        _ => field.value.long(now),
    }
}
