//! Which bucket and task are selected or expanded, and keeping the
//! selection valid as lists change.

use kru::Entry;
use kru::bucket::Bucket;
use kru::task::Task;
use uuid::Uuid;

use super::{App, Focus, Result, UiError};

impl App {
    /// The bucket highlighted in the buckets pane.
    pub fn selected_bucket(&self) -> Option<&Bucket> {
        self.store.buckets().nth(self.bucket_index)
    }

    /// Open tasks of the selected bucket, most urgent first.
    pub fn open_tasks(&self) -> Vec<Entry<'_>> {
        let Some(bucket) = self.selected_bucket() else {
            return Vec::new();
        };
        let bucket = bucket.header.id;
        let mut open: Vec<_> = self
            .store
            .entries()
            .filter(|e| e.bucket.header.id == bucket && !e.task.is_done())
            .collect();
        open.sort_by(|a, b| a.task.cmp_urgency(b.task));
        open
    }

    /// Done tasks of every bucket, most recently completed first.
    pub fn done_tasks(&self) -> Vec<Entry<'_>> {
        let mut done: Vec<_> = self.store.entries().filter(|e| e.task.is_done()).collect();
        done.sort_by_key(|e| std::cmp::Reverse(e.task.time_completed));
        done
    }

    /// The highlighted task in the focused pane, if that pane lists tasks.
    pub fn selected_task(&self) -> Option<Entry<'_>> {
        match self.focus {
            Focus::Buckets => None,
            Focus::Tasks => self.open_tasks().into_iter().nth(self.task_index),
            Focus::Done => self.done_tasks().into_iter().nth(self.done_index),
        }
    }

    /// The task in the expanded view, if it is open.
    pub fn expanded_task(&self) -> Option<Entry<'_>> {
        self.expanded.and_then(|id| self.store.find(id).ok())
    }

    /// The task that task actions apply to: the expanded one if the view is
    /// open, otherwise the highlighted one.
    fn target_task(&self) -> Option<Entry<'_>> {
        match self.expanded {
            Some(_) => self.expanded_task(),
            None => self.selected_task(),
        }
    }

    /// The target task, or an error explaining that there is none.
    pub(super) fn require_task(&self) -> Result<&Task> {
        self.target_task()
            .map(|e| e.task)
            .ok_or_else(|| no_task_selected(self.focus))
    }

    /// Id and a short label like `#3 "Buy milk"` for the target task.
    pub(super) fn require_selected(&self) -> Result<(Uuid, String)> {
        let entry = self
            .target_task()
            .ok_or_else(|| no_task_selected(self.focus))?;
        let label = format!(
            "#{} \"{}\"",
            entry.num,
            shorten(&entry.task.description, 40)
        );
        Ok((entry.task.header.id, label))
    }

    /// Highlights the bucket with this id.
    pub(super) fn select_bucket(&mut self, id: Uuid) {
        if let Some(index) = self.store.buckets().position(|b| b.header.id == id) {
            self.bucket_index = index;
            self.task_index = 0;
        }
    }

    /// Highlights the task with this id in the focused pane, if it is
    /// listed there. Used after a change that may reorder the list.
    pub(super) fn select_task(&mut self, id: Uuid) {
        let find = |list: Vec<kru::Entry>| list.iter().position(|e| e.task.header.id == id);
        match self.focus {
            Focus::Buckets => {}
            Focus::Tasks => {
                if let Some(position) = find(self.open_tasks()) {
                    self.task_index = position;
                }
            }
            Focus::Done => {
                if let Some(position) = find(self.done_tasks()) {
                    self.done_index = position;
                }
            }
        }
    }

    /// Keeps every selection index within its list after items change,
    /// and closes the expanded view if its task is gone.
    pub(super) fn clamp_selection(&mut self) {
        let buckets = self.store.buckets().count();
        self.bucket_index = self.bucket_index.min(buckets.saturating_sub(1));
        let open = self.open_tasks().len();
        self.task_index = self.task_index.min(open.saturating_sub(1));
        let done = self.done_tasks().len();
        self.done_index = self.done_index.min(done.saturating_sub(1));
        if self.expanded_task().is_none() {
            self.expanded = None;
        }
    }
}

/// The error for task actions when no task is highlighted.
fn no_task_selected(focus: Focus) -> UiError {
    let text = match focus {
        Focus::Buckets => "No task selected: press l or tab to go to the tasks",
        Focus::Tasks | Focus::Done => "No task selected: this pane is empty",
    };
    UiError(text.to_string())
}

/// Truncates `text` to `max` characters, ending with `…` if cut.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}
