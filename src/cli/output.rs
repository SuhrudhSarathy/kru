//! Terminal output for the CLI.
//!
//! Task columns and details come from the tasks' own fields
//! ([`kru::task::Task::fields`]), so a new field shows up here without
//! changes.

use comfy_table::{Attribute, Cell, ContentArrangement, Table, presets};
use jiff::Timestamp;
use kru::task::{Field, Tone, field};
use kru::{Entry, Store};

use crate::theme;

/// Prints tasks as a table: number, bucket, every listed field, then the
/// description. Done tasks are dimmed.
pub fn print_tasks(entries: &[Entry]) {
    let fields: Vec<Vec<Field>> = entries.iter().map(|e| e.task.fields()).collect();
    let columns = field::list_columns(&fields);

    let mut header = vec!["#", "Bucket"];
    header.extend(columns.iter().map(|&i| fields[0][i].label));
    header.push("Description");
    let mut table = new_table(&header);

    for (entry, fields) in entries.iter().zip(&fields) {
        let mut row = vec![
            Cell::new(entry.num).fg(theme::NUMBER.into()),
            Cell::new(&entry.bucket.name),
        ];
        row.extend(columns.iter().map(|&i| {
            let field = &fields[i];
            toned(Cell::new(field.value.short()), field.tone)
        }));
        row.push(Cell::new(&entry.task.description));
        if entry.task.is_done() {
            row = row
                .into_iter()
                .map(|cell| cell.add_attribute(Attribute::Dim))
                .collect();
        }
        table.add_row(row);
    }
    println!("{table}");
}

/// Prints the description, the bucket and every detail field of one task.
pub fn print_task_details(entry: &Entry) {
    let now = Timestamp::now();
    println!("#{}  {}", entry.num, entry.task.description);

    let bucket = ("Bucket", entry.bucket.name.clone());
    let fields = entry
        .task
        .fields()
        .into_iter()
        .filter(|field| field.in_detail)
        .map(|field| (field.label, field.value.long(now)));
    let rows: Vec<_> = std::iter::once(bucket).chain(fields).collect();

    // Multi-line values (like context) continue under the value.
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    for (label, value) in rows {
        for (i, line) in value.lines().enumerate() {
            let label = if i == 0 { label } else { "" };
            println!("  {label:<width$}  {line}");
        }
    }
}

/// Prints buckets with open/done counts; `*` marks the active one.
pub fn print_buckets(store: &Store) {
    let active = store.active_bucket().map(|b| b.header.id);
    let mut table = new_table(&["", "Bucket", "Open", "Done"]);
    for bucket in store.buckets() {
        let is_active = active == Some(bucket.header.id);
        let open = bucket.open_count();
        table.add_row(vec![
            Cell::new(if is_active { "*" } else { "" }).fg(theme::ACTIVE.into()),
            Cell::new(&bucket.name),
            Cell::new(open),
            Cell::new(bucket.len() - open),
        ]);
    }
    println!("{table}");
}

/// A borderless table with a bold header row.
fn new_table(header: &[&str]) -> Table {
    let header: Vec<Cell> = header
        .iter()
        .map(|h| Cell::new(h).add_attribute(Attribute::Bold))
        .collect();
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);
    table
}

/// Colours a cell for what its field's value means.
fn toned(cell: Cell, tone: Tone) -> Cell {
    match theme::tone(tone) {
        Some(colour) => cell.fg(colour.into()),
        None => cell,
    }
}
