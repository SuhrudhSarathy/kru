//! Changes waiting for the user to press `y`, and applying them.

use kru::task::TaskEdit;
use uuid::Uuid;

use super::{Action, App, Mode, Result};

/// A change waiting for the user to confirm it with `y`.
#[derive(Clone, Debug)]
pub struct Pending {
    pub question: String,
    op: Op,
}

/// The change itself, carrying the ids it was asked about so it applies to
/// the same task even if the selection moves.
#[derive(Clone, Debug)]
pub(super) enum Op {
    EditTask { id: Uuid, edit: TaskEdit },
    MoveTask { id: Uuid, to: String },
    RemoveTask { id: Uuid },
    RemoveBucket { name: String },
}

impl App {
    /// Opens a confirmation for `op`.
    pub(super) fn confirm(&mut self, question: String, op: Op) {
        self.mode = Mode::Confirm(Pending { question, op });
    }

    /// Opens a confirmation for editing a task.
    pub(super) fn confirm_edit(&mut self, id: Uuid, question: String, edit: TaskEdit) {
        self.confirm(question, Op::EditTask { id, edit });
    }

    /// Applies the pending change on `y`, drops it on `n`/Esc.
    pub(super) fn on_confirm(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Submit => {
                let Mode::Confirm(pending) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    return Ok(());
                };
                self.apply(pending.op)?;
            }
            Action::Cancel => {
                self.mode = Mode::Normal;
                self.info("Cancelled");
            }
            _ => self.error("Press y to confirm or n to cancel"),
        }
        Ok(())
    }

    /// Performs a confirmed change through the store and saves.
    fn apply(&mut self, op: Op) -> Result<()> {
        match op {
            Op::EditTask { id, edit } => {
                self.store.edit_task(id, edit)?;
                // The edit may have moved the task in the sorted list.
                self.select_task(id);
                let num = self.store.find(id)?.num;
                self.commit(format!("Updated task #{num}"))
            }
            Op::MoveTask { id, to } => {
                self.store.move_task(id, &to)?;
                self.commit(format!("Moved task to {to}"))
            }
            Op::RemoveTask { id } => {
                let task = self.store.remove_task(id)?;
                self.commit(format!("Removed \"{}\"", task.description))
            }
            Op::RemoveBucket { name } => {
                self.store.remove_bucket(&name, true)?;
                self.commit(format!("Removed bucket {name}"))
            }
        }
    }
}
