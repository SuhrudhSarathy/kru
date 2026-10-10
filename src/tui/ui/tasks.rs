//! The task tables: open tasks of the selected bucket, and done tasks of
//! every bucket.
//!
//! Columns come from the tasks' own fields ([`kru::task::Task::fields`]),
//! so a new listed field shows up here without changes. Descriptions wrap
//! within the last column, and each row is as tall as its description.

use kru::Entry;
use kru::task::{Field, field};
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

/// Narrowest the description column gets.
const MIN_DESCRIPTION: u16 = 10;

/// Widest the bucket column gets; longer names are cut off.
const MAX_BUCKET: u16 = 20;

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

    let width = block.inner(area).width;
    let table = task_table(&tasks, false, false, width)
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

    let width = block.inner(area).width;
    let table = task_table(&tasks, true, true, width)
        .block(block)
        .row_highlight_style(style::highlight(focused));
    let mut state = TableState::default().with_selected(Some(app.done_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// A table of `entries`, `width` cells wide: the task number, the bucket
/// (if `with_bucket`), every listed field, then the wrapped description.
/// `dim` greys out the rows.
fn task_table(entries: &[Entry], with_bucket: bool, dim: bool, width: u16) -> Table<'static> {
    let fields: Vec<Vec<Field>> = entries.iter().map(|e| e.task.fields()).collect();
    let columns = field::list_columns(&fields);

    // Every column but the description, sized to its widest content.
    let mut titles = vec!["#"];
    let mut widths = vec![column_width("#", entries.iter().map(|e| e.num.to_string()))];
    if with_bucket {
        let names = entries.iter().map(|e| e.bucket.name.clone());
        titles.push("Bucket");
        widths.push(column_width("Bucket", names).min(MAX_BUCKET));
    }
    for &i in &columns {
        let label = fields[0][i].label;
        titles.push(label);
        widths.push(column_width(
            label,
            fields.iter().map(|f| f[i].value.short()),
        ));
    }
    titles.push("Description");

    // The description gets what the other columns and the gaps leave.
    let used = widths.iter().sum::<u16>() + COLUMN_SPACING * widths.len() as u16;
    let description_width = usize::from(width.saturating_sub(used)).max(1);

    let rows: Vec<Row> = entries
        .iter()
        .zip(&fields)
        .map(|(entry, fields)| {
            let mut cells = vec![Cell::from(entry.num.to_string()).style(style::fg(theme::NUMBER))];
            if with_bucket {
                cells.push(Cell::from(entry.bucket.name.clone()));
            }
            for &i in &columns {
                let field = &fields[i];
                cells.push(Cell::from(field.value.short()).style(style::tone(field.tone)));
            }
            let (description, height) =
                description_cell(&entry.task.description, description_width);
            cells.push(description);

            let row = Row::new(cells).height(height);
            if dim { row.dark_gray() } else { row }
        })
        .collect();

    let mut constraints: Vec<Constraint> = widths.into_iter().map(Constraint::Length).collect();
    constraints.push(Constraint::Min(MIN_DESCRIPTION));
    Table::new(rows, constraints)
        .column_spacing(COLUMN_SPACING)
        .header(Row::new(titles).bold())
}

/// Width of a column: its widest value or its title, whichever is wider.
fn column_width(title: &str, values: impl Iterator<Item = String>) -> u16 {
    let widest = values.map(|v| v.chars().count()).max().unwrap_or(0);
    widest.max(title.chars().count()) as u16
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
