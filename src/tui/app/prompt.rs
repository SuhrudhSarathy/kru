//! Text prompts: typing into them and acting on the submitted text.

use kru::task::Task;

use super::confirm::Op;
use super::{Action, App, Focus, Mode, Result};

/// What a text prompt is asking for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prompt {
    AddBucket,
    RenameBucket,
    AddTask,
    MoveTask,
}

impl App {
    /// Opens a text prompt, pre-filled with `buffer`.
    pub(super) fn prompt(&mut self, prompt: Prompt, buffer: String) {
        self.mode = Mode::Input { prompt, buffer };
    }

    /// Handles typing in a prompt; Enter submits, Esc cancels.
    pub(super) fn on_input(&mut self, action: Action) -> Result<()> {
        let Mode::Input { prompt, buffer } = &mut self.mode else {
            return Ok(());
        };
        match action {
            Action::Input(c) => buffer.push(c),
            Action::Backspace => {
                buffer.pop();
            }
            Action::Cancel => self.mode = Mode::Normal,
            Action::Submit => {
                let (prompt, text) = (*prompt, std::mem::take(buffer));
                self.mode = Mode::Normal;
                if let Err(err) = self.submit(prompt, &text) {
                    // Keep the prompt open so the input can be fixed.
                    self.mode = Mode::Input {
                        prompt,
                        buffer: text,
                    };
                    return Err(err);
                }
            }
            Action::Newline => {
                self.error("Line breaks only work in a task's context: press e to edit it")
            }
            Action::InvalidKey(key) => self.error(format!("`{key}` can't be used while typing")),
            _ => {}
        }
        Ok(())
    }

    /// Acts on submitted prompt text. Bucket changes and new tasks apply
    /// immediately; moving a task asks first.
    fn submit(&mut self, prompt: Prompt, text: &str) -> Result<()> {
        match prompt {
            Prompt::AddBucket => {
                let bucket = self.store.add_bucket(text)?;
                let (id, name) = (bucket.header.id, bucket.name.clone());
                self.select_bucket(id);
                self.commit(format!("Added bucket {name}"))?;
            }
            Prompt::RenameBucket => {
                if let Some(bucket) = self.selected_bucket().map(|b| b.name.clone()) {
                    self.store.rename_bucket(&bucket, text)?;
                    self.commit(format!("Renamed {bucket} to {}", text.trim()))?;
                }
            }
            Prompt::AddTask => {
                if let Some(bucket) = self.selected_bucket().map(|b| b.name.clone()) {
                    let id = self.store.add_task(Some(&bucket), Task::new(text.trim()))?;
                    self.focus = Focus::Tasks;
                    self.expanded = None;
                    self.select_task(id);
                    self.commit(format!("Added task to {bucket}"))?;
                }
            }
            Prompt::MoveTask => {
                let (id, label) = self.require_selected()?;
                let to = self.store.bucket(text)?.name.clone();
                self.confirm(format!("Move {label} to {to}?"), Op::MoveTask { id, to });
            }
        }
        Ok(())
    }
}
