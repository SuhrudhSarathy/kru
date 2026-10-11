//! JSON output for `--json`.
//!
//! Tasks are written from their fields ([`Task::fields`]), keyed by field
//! key, so a new field shows up here without changes. Times are RFC 3339 in
//! UTC and unset values are `null`; text and choices are strings.

use kru::bucket::Bucket;
use kru::task::{Change, Task, Value as FieldValue};
use kru::{Entry, Store};
use serde_json::{Map, Value, json};

/// Prints `value` as indented JSON.
pub fn print(value: Value) {
    let text = serde_json::to_string_pretty(&value).expect("a JSON value always serializes");
    println!("{text}");
}

/// A task's number and bucket, then every field by key.
pub fn task(num: usize, bucket: &str, task: &Task) -> Value {
    let mut object = Map::new();
    object.insert("num".into(), json!(num));
    object.insert("bucket".into(), json!(bucket));
    for field in task.fields() {
        object.insert(field.key.into(), value(&field.value));
    }
    Value::Object(object)
}

pub fn entry(entry: &Entry) -> Value {
    task(entry.num, &entry.bucket.name, entry.task)
}

pub fn entries(entries: &[Entry]) -> Value {
    entries.iter().map(entry).collect()
}

/// A bucket with its open and done task counts, and whether it is active.
pub fn bucket(store: &Store, bucket: &Bucket) -> Value {
    let active = store
        .active_bucket()
        .is_some_and(|active| active.header.id == bucket.header.id);
    let open = bucket.open_count();
    json!({
        "name": bucket.name,
        "id": bucket.header.id,
        "active": active,
        "open": open,
        "done": bucket.len() - open,
    })
}

/// One changed field: `{"field": "priority", "from": "low", "to": "high"}`.
pub fn change(change: &Change) -> Value {
    json!({
        "field": change.key,
        "from": value(&change.from),
        "to": value(&change.to),
    })
}

fn value(value: &FieldValue) -> Value {
    match value {
        FieldValue::Text(text) | FieldValue::Choice { value: text, .. } => json!(text),
        FieldValue::Time(Some(at)) => json!(at.to_string()),
        FieldValue::Time(None) => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use kru::task::Priority;

    #[test]
    fn tasks_are_keyed_by_field() {
        let due: Timestamp = "2026-10-12T11:30:00Z".parse().unwrap();
        let task = Task::new("Pay rent")
            .with_priority(Priority::High)
            .with_due(due);
        let value = super::task(3, "home", &task);

        let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            [
                "num",
                "bucket",
                "description",
                "ctx",
                "priority",
                "status",
                "due",
                "completed",
                "created",
                "edited",
                "id"
            ]
        );
        assert_eq!(value["num"], 3);
        assert_eq!(value["priority"], "high");
        assert_eq!(value["due"], "2026-10-12T11:30:00Z");
        assert_eq!(value["completed"], Value::Null);
    }
}
