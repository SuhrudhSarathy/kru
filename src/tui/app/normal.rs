//! Actions while no prompt or confirmation is open: navigation, bucket
//! actions and task actions.

use super::confirm::Op;
use super::{Action, App, Focus, Prompt, Result};

impl App {
    /// Handles an action while no prompt or confirmation is open.
    pub(super) fn on_normal(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Sync => self.sync_requested = true,
            Action::Back => match self.expanded {
                Some(_) => self.expanded = None,
                None => self.should_quit = true,
            },
            Action::Expand => {
                self.expanded = match self.expanded {
                    Some(_) => None,
                    None => Some(self.require_selected()?.0),
                };
            }
            Action::Up => self.step(|i| i.saturating_sub(1)),
            Action::Down => self.step(|i| i + 1),
            Action::FocusBuckets => self.set_focus(Focus::Buckets),
            Action::FocusTasks => self.set_focus(Focus::Tasks),
            Action::FocusNext => self.cycle_focus(1),
            Action::FocusPrev => self.cycle_focus(Focus::ORDER.len() - 1),

            Action::Add | Action::Edit | Action::Remove | Action::UseBucket
                if self.focus == Focus::Buckets =>
            {
                self.on_bucket(action)?
            }
            Action::UseBucket => self.error("u works in the buckets pane (press h)"),
            Action::Add
            | Action::Edit
            | Action::Remove
            | Action::ToggleDone
            | Action::CycleStatus
            | Action::CyclePriority
            | Action::EditDue
            | Action::MoveTask
            | Action::OpenMd => self.on_task(action)?,

            Action::InvalidKey(key) => self.error(format!("`{key}` does nothing here")),
            Action::Input(_)
            | Action::Newline
            | Action::Backspace
            | Action::Submit
            | Action::Cancel
            | Action::Left
            | Action::Right => {}
        }
        Ok(())
    }

    /// Moves the selection in the focused pane; the expanded view follows.
    fn step(&mut self, step: impl Fn(usize) -> usize) {
        match self.focus {
            Focus::Buckets => {
                self.bucket_index = step(self.bucket_index);
                self.task_index = 0;
            }
            Focus::Tasks => self.task_index = step(self.task_index),
            Focus::Done => self.done_index = step(self.done_index),
        }
        if self.expanded.is_some() {
            self.clamp_selection();
            self.expanded = self.selected_task().map(|e| e.task.header.id);
        }
    }

    /// Focuses a pane, closing the expanded view.
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.expanded = None;
    }

    /// Moves focus `by` panes forward, wrapping around.
    fn cycle_focus(&mut self, by: usize) {
        let i = Focus::ORDER.iter().position(|&f| f == self.focus).unwrap();
        self.set_focus(Focus::ORDER[(i + by) % Focus::ORDER.len()]);
    }

    /// Add, rename, remove or activate the selected bucket.
    fn on_bucket(&mut self, action: Action) -> Result<()> {
        if action == Action::Add {
            self.prompt(Prompt::AddBucket, String::new());
            return Ok(());
        }
        let Some(bucket) = self.selected_bucket() else {
            return Ok(());
        };
        let name = bucket.name.clone();
        match action {
            Action::Edit => self.prompt(Prompt::RenameBucket, name),
            Action::Remove => {
                let question = format!("Remove bucket {name} and its {} task(s)?", bucket.len());
                self.confirm(question, Op::RemoveBucket { name });
            }
            Action::UseBucket => {
                self.store.set_active_bucket(&name)?;
                self.commit(format!("Active bucket is now {name}"))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Actions on the target task (or adding one to the selected bucket).
    /// Field changes go through the generic editor in `edit`; the keys
    /// below are shortcuts to particular fields.
    fn on_task(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Add if self.selected_bucket().is_none() => {
                self.error("Create a bucket first: press h, then a")
            }
            Action::Add => self.prompt(Prompt::AddTask, String::new()),
            Action::Edit => self.open_editor(None, false)?,
            Action::EditDue => self.open_editor(Some("due"), true)?,
            Action::CycleStatus => self.open_editor(Some("status"), false)?,
            Action::CyclePriority => self.cycle_field("priority")?,
            Action::ToggleDone => {
                let to = if self.require_task()?.is_done() {
                    "todo"
                } else {
                    "done"
                };
                self.request_edit("status", to)?;
            }
            Action::MoveTask => {
                self.require_task()?;
                self.prompt(Prompt::MoveTask, String::new());
            }
            Action::OpenMd => {
                let (id, _) = self.require_selected()?;
                let (path, _) = self.store.attach_md(id)?;
                self.store.save()?;
                self.open_file = Some(path);
            }
            Action::Remove => {
                let (id, label) = self.require_selected()?;
                self.confirm(format!("Remove {label}?"), Op::RemoveTask { id });
            }
            _ => {}
        }
        Ok(())
    }
}
