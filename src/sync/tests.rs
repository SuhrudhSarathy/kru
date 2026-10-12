//! Syncing two device folders through a local bare repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::Store;
use crate::store::test_util::TempDir;
use crate::task::{Priority, Task};

/// A bare "remote" and two device folders, A and B.
struct Devices {
    _root: TempDir,
    remote: String,
    a: PathBuf,
    b: PathBuf,
}

fn devices() -> Devices {
    let root = TempDir::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let remote = root.0.join("remote.git");
    let status = Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&remote)
        .status()
        .unwrap();
    assert!(status.success());
    Devices {
        remote: remote.display().to_string(),
        a: root.0.join("a"),
        b: root.0.join("b"),
        _root: root,
    }
}

fn sync(dir: &Path) -> Report {
    run(dir, false).unwrap()
}

/// Opens a store, applies `change`, saves.
fn change(dir: &Path, change: impl FnOnce(&mut Store)) {
    let mut store = Store::open(dir).unwrap();
    change(&mut store);
    store.save().unwrap();
}

fn descriptions(dir: &Path) -> Vec<String> {
    let store = Store::open(dir).unwrap();
    let mut all: Vec<String> = store
        .entries()
        .map(|e| e.task.description.clone())
        .collect();
    all.sort();
    all
}

fn task_id(dir: &Path, description: &str) -> Uuid {
    let store = Store::open(dir).unwrap();
    store
        .entries()
        .find(|e| e.task.description == description)
        .map(|e| e.task.header.id)
        .unwrap()
}

#[test]
fn a_second_device_joins_and_both_stores_combine() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
        s.add_task(None, Task::new("from a")).unwrap();
    });
    let first = init(&d.a, &d.remote, false).unwrap();
    assert!(first.pushed > 0);

    // B already had its own tasks, in a bucket with the same name.
    change(&d.b, |s| {
        s.add_bucket("Home").unwrap();
        s.add_task(None, Task::new("from b")).unwrap();
    });
    let joined = init(&d.b, &d.remote, false).unwrap();
    assert!(
        joined
            .conflicts
            .iter()
            .any(|c| matches!(c.what, What::BucketsMerged { .. }))
    );
    sync(&d.a);

    for dir in [&d.a, &d.b] {
        assert_eq!(descriptions(dir), ["from a", "from b"]);
        assert_eq!(
            Store::open(dir).unwrap().buckets().count(),
            1,
            "one home bucket"
        );
    }
}

#[test]
fn edits_on_both_devices_merge() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
        s.add_task(None, Task::new("one")).unwrap();
        s.add_task(None, Task::new("two")).unwrap();
    });
    init(&d.a, &d.remote, false).unwrap();
    init(&d.b, &d.remote, false).unwrap();
    let (one, two) = (task_id(&d.a, "one"), task_id(&d.a, "two"));

    // Different tasks, and then the same field: B edits it last.
    change(&d.a, |s| {
        s.edit_task(one, &[("priority", "high")]).unwrap();
        s.edit_task(two, &[("description", "two, by a")]).unwrap();
    });
    change(&d.b, |s| {
        s.edit_task(two, &[("description", "two, by b")]).unwrap();
        s.add_task(None, Task::new("three")).unwrap();
    });
    sync(&d.a);
    let report = sync(&d.b);
    sync(&d.a);

    assert!(
        report
            .conflicts
            .iter()
            .any(|c| matches!(&c.what, What::Field { key, .. } if key == "description"))
    );
    for dir in [&d.a, &d.b] {
        let store = Store::open(dir).unwrap();
        assert_eq!(store.find(one).unwrap().task.priority, Priority::High);
        assert_eq!(store.find(two).unwrap().task.description, "two, by b");
        assert_eq!(descriptions(dir), ["one", "three", "two, by b"]);
    }
    assert_eq!(
        Store::open(&d.a).unwrap().snapshot(),
        Store::open(&d.b).unwrap().snapshot()
    );
}

#[test]
fn a_later_edit_beats_an_earlier_delete() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
        s.add_task(None, Task::new("keep me")).unwrap();
        s.add_task(None, Task::new("delete me")).unwrap();
    });
    init(&d.a, &d.remote, false).unwrap();
    init(&d.b, &d.remote, false).unwrap();
    let (keep, gone) = (task_id(&d.a, "keep me"), task_id(&d.a, "delete me"));

    change(&d.a, |s| {
        s.remove_task(keep).unwrap();
        s.remove_task(gone).unwrap();
    });
    change(&d.b, |s| {
        s.edit_task(keep, &[("priority", "high")]).unwrap();
    });
    sync(&d.a);
    sync(&d.b);
    sync(&d.a);

    for dir in [&d.a, &d.b] {
        assert_eq!(descriptions(dir), ["keep me"]);
    }
}

#[test]
fn notes_merge_as_text_and_the_newer_wins_a_clash() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
        let id = s.add_task(None, Task::new("notes")).unwrap();
        s.attach_md(id).unwrap();
    });
    let id = task_id(&d.a, "notes");
    let path = |dir: &Path| dir.join(format!("notes/{id}.md"));
    std::fs::write(path(&d.a), "# notes\n\nline 1\nline 2\nline 3\n").unwrap();
    init(&d.a, &d.remote, false).unwrap();
    init(&d.b, &d.remote, false).unwrap();

    // Different lines: both land.
    std::fs::write(path(&d.a), "# notes\n\nline 1, by a\nline 2\nline 3\n").unwrap();
    std::fs::write(path(&d.b), "# notes\n\nline 1\nline 2\nline 3, by b\n").unwrap();
    sync(&d.a);
    sync(&d.b);
    sync(&d.a);
    let both = "# notes\n\nline 1, by a\nline 2\nline 3, by b\n";
    assert_eq!(std::fs::read_to_string(path(&d.a)).unwrap(), both);
    assert_eq!(std::fs::read_to_string(path(&d.b)).unwrap(), both);

    // The same line: the later commit wins (commit times are in seconds).
    std::fs::write(path(&d.a), "# notes\n\nfrom a\n").unwrap();
    sync(&d.a);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(path(&d.b), "# notes\n\nfrom b, later\n").unwrap();
    let report = sync(&d.b);
    sync(&d.a);
    assert!(
        report
            .conflicts
            .iter()
            .any(|c| matches!(c.what, What::NotesOverwritten { .. }))
    );
    for dir in [&d.a, &d.b] {
        assert_eq!(
            std::fs::read_to_string(path(dir)).unwrap(),
            "# notes\n\nfrom b, later\n"
        );
    }
}

#[test]
fn a_store_from_a_newer_kru_is_refused() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
    });
    init(&d.a, &d.remote, false).unwrap();
    init(&d.b, &d.remote, false).unwrap();

    // A newer kru on B wrote a file this kru can't read, and pushed it.
    let future = d.b.join("buckets").join(format!("{}.json", Uuid::now_v7()));
    std::fs::write(
        &future,
        r#"{ "version": 999, "header": {}, "name": "x", "tasks": {} }"#,
    )
    .unwrap();
    for args in [
        &["add", "-A"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "future",
        ],
        &["push", "-q", "origin", "HEAD:main"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&d.b)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }

    // A has its own change, so it must read B's files to merge them.
    change(&d.a, |s| {
        s.add_task(None, Task::new("local change")).unwrap();
    });
    assert!(matches!(
        run(&d.a, false),
        Err(Error::NewerFormat { version: 999, .. })
    ));
}

#[test]
fn status_reports_what_is_pending() {
    let d = devices();
    change(&d.a, |s| {
        s.add_bucket("home").unwrap();
    });
    assert!(matches!(status(&d.a), Err(Error::SyncNotSetUp)));
    init(&d.a, &d.remote, false).unwrap();
    let synced = status(&d.a).unwrap();
    assert_eq!(
        (synced.ahead, synced.behind, synced.local_changes),
        (0, 0, false)
    );
    assert!(synced.last_sync.is_some());

    change(&d.a, |s| {
        s.add_task(None, Task::new("unsynced")).unwrap();
    });
    assert!(status(&d.a).unwrap().local_changes);
}
