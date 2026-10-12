//! Versioning of the store's files, so files written by older versions of
//! kru keep loading.
//!
//! Every file carries `"version"` (its format) and `"kru"` (the kru release
//! that wrote it, for information only). When a file is read, it is
//! upgraded one version at a time by [`STEPS`] before being deserialized,
//! and the next save writes it back in the current format. Files from a
//! newer kru are refused rather than misread.
//!
//! To change the file format: add a step at the end of [`STEPS`] that turns
//! the old shape into the new one, then change the structs. (A new field
//! that can simply start empty could use `#[serde(default)]` instead, but a
//! step keeps the format's history in one place.)

use serde_json::{Map, Value};

/// Which kind of file a step is upgrading.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum File {
    /// `buckets/<id>.json`
    Bucket,
    /// `state.json`
    State,
    /// `tombstones.json`
    Tombstones,
}

/// Turns a file of version `i` into version `i + 1`, where `i` is the
/// step's index in [`STEPS`].
type Step = fn(File, &mut Value);

/// The format history, oldest first.
const STEPS: &[Step] = &[add_task_context, add_task_mdpath];

/// The format this kru reads and writes. Files without a version (written
/// before versioning) are version 0.
pub(super) const VERSION: u64 = STEPS.len() as u64;

/// The version a file's JSON says it is.
pub(super) fn version(file: &Value) -> u64 {
    file.get("version").and_then(Value::as_u64).unwrap_or(0)
}

/// Upgrades a file from `from` to [`VERSION`]. Returns whether any step
/// ran. `from` must not be newer than `VERSION`.
pub(super) fn upgrade(kind: File, file: &mut Value, from: u64) -> bool {
    let steps = &STEPS[from as usize..];
    for step in steps {
        step(kind, file);
    }
    !steps.is_empty()
}

/// Stamps a value with the current format and kru version, for writing.
/// The two keys come first so they are easy to spot in the file.
pub(super) fn stamp(value: Value) -> Value {
    let mut stamped = Map::new();
    stamped.insert("version".into(), VERSION.into());
    stamped.insert("kru".into(), env!("CARGO_PKG_VERSION").into());
    if let Value::Object(fields) = value {
        stamped.extend(fields);
    }
    Value::Object(stamped)
}

// --- Steps --------------------------------------------------------------

/// 0 → 1: tasks gained `ctx` (extra context), empty for existing tasks.
fn add_task_context(kind: File, file: &mut Value) {
    if kind != File::Bucket {
        return;
    }
    let Some(tasks) = file.get_mut("tasks").and_then(Value::as_object_mut) else {
        return;
    };
    for task in tasks.values_mut().filter_map(Value::as_object_mut) {
        task.entry("ctx")
            .or_insert_with(|| Value::String(String::new()));
    }
}

/// 1 → 2: tasks gained `mdpath` (an attached markdown file), unset for
/// existing tasks.
fn add_task_mdpath(kind: File, file: &mut Value) {
    if kind != File::Bucket {
        return;
    }
    let Some(tasks) = file.get_mut("tasks").and_then(Value::as_object_mut) else {
        return;
    };
    for task in tasks.values_mut().filter_map(Value::as_object_mut) {
        task.entry("mdpath").or_insert(Value::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unversioned_buckets_gain_task_context() {
        let mut file = json!({
            "name": "home",
            "tasks": { "a": { "description": "x" }, "b": { "description": "y", "ctx": "kept" } }
        });
        assert_eq!(version(&file), 0);
        assert!(upgrade(File::Bucket, &mut file, 0));
        assert_eq!(file["tasks"]["a"]["ctx"], "");
        assert_eq!(file["tasks"]["b"]["ctx"], "kept");
        assert_eq!(file["tasks"]["a"]["mdpath"], Value::Null);

        // Already current: nothing to do.
        assert!(!upgrade(File::Bucket, &mut file, VERSION));
    }

    #[test]
    fn stamped_files_lead_with_their_version() {
        let stamped = stamp(json!({ "name": "home" }));
        let keys: Vec<_> = stamped.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["version", "kru", "name"]);
        assert_eq!(version(&stamped), VERSION);
    }
}
