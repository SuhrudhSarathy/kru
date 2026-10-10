//! Terminal output for the CLI.

use comfy_table::{Attribute, Cell, ContentArrangement, Table, presets};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use kru::{Entry, Store};

use crate::theme;

/// Prints tasks as a table; done tasks are dimmed, overdue dates red.
pub fn print_tasks(entries: &[Entry]) {
    let now = Timestamp::now();
    let mut table = new_table(["#", "Bucket", "Priority", "Status", "Due", "Description"]);

    for entry in entries {
        let task = entry.task;
        let due = Cell::new(task.due.map(kru::utils::due::format).unwrap_or_default());
        let mut row = vec![
            Cell::new(entry.num).fg(theme::NUMBER.into()),
            Cell::new(&entry.bucket.name),
            Cell::new(task.priority).fg(theme::priority(task.priority).into()),
            Cell::new(task.status).fg(theme::status(task.status).into()),
            if task.is_overdue(now) {
                due.fg(theme::OVERDUE.into())
            } else {
                due
            },
            Cell::new(&task.description),
        ];
        if task.is_done() {
            row = row
                .into_iter()
                .map(|cell| cell.add_attribute(Attribute::Dim))
                .collect();
        }
        table.add_row(row);
    }
    println!("{table}");
}

/// Prints every field of one task.
pub fn print_task_details(entry: &Entry) {
    let task = entry.task;
    println!("#{}  {}", entry.num, task.description);
    let mut rows = vec![
        ("bucket", entry.bucket.name.clone()),
        ("priority", task.priority.to_string()),
        ("status", task.status.to_string()),
        (
            "due",
            task.due
                .map(kru::utils::due::format)
                .unwrap_or_else(|| "-".into()),
        ),
        ("created", local_time(task.header.time_created)),
        ("edited", local_time(task.header.time_last_edited)),
    ];
    if let Some(completed) = task.time_completed {
        rows.push(("completed", local_time(completed)));
    }
    rows.push(("id", task.header.id.to_string()));

    for (label, value) in rows {
        println!("  {label:<10} {value}");
    }
}

/// Prints buckets with open/done counts; `*` marks the active one.
pub fn print_buckets(store: &Store) {
    let active = store.active_bucket().map(|b| b.header.id);
    let mut table = new_table(["", "Bucket", "Open", "Done"]);
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
fn new_table<const N: usize>(header: [&str; N]) -> Table {
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header.map(|h| Cell::new(h).add_attribute(Attribute::Bold)));
    table
}

/// Formats a timestamp in the local time zone.
fn local_time(ts: Timestamp) -> String {
    ts.to_zoned(TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}
