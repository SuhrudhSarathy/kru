//! TUI state and its update logic. No ratatui types in here, so it can be
//! tested without a terminal.

use kru::bucket::Bucket;
use kru::task::{Priority, Status, Task, TaskEdit};
use kru::{Entry, Store};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Buckets,
    Tasks,
    Done,
}

impl Focus {
    const ORDER: [Focus; 3] = [Focus::Buckets, Focus::Tasks, Focus::Done];
}

/// What a text prompt is asking for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prompt {
    AddBucket,
    RenameBucket,
    AddTask,
    EditDescription,
    EditDue,
    MoveTask,
}

/// A change waiting for the user to confirm it with `y`.
#[derive(Clone, Debug)]
pub struct Pending {
    pub question: String,
    op: Op,
}

#[derive(Clone, Debug)]
enum Op {
    EditTask { id: Uuid, edit: TaskEdit },
    MoveTask { id: Uuid, to: String },
    RemoveTask { id: Uuid },
    RemoveBucket { name: String },
}

#[derive(Debug)]
pub enum Mode {
    Normal,
    Input { prompt: Prompt, buffer: String },
    Confirm(Pending),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Quit,
    Up,
    Down,
    FocusBuckets,
    FocusTasks,
    FocusNext,
    FocusPrev,
    Add,
    Edit,
    Remove,
    UseBucket,
    ToggleDone,
    CycleStatus,
    CyclePriority,
    EditDue,
    MoveTask,
    Input(char),
    Backspace,
    Submit,
    Cancel,
    InvalidKey(String),
}

pub struct Message {
    pub text: String,
    pub is_error: bool,
}

pub struct App {
    pub store: Store,
    pub focus: Focus,
    pub bucket_index: usize,
    pub task_index: usize,
    pub done_index: usize,
    pub mode: Mode,
    pub message: Option<Message>,
    pub should_quit: bool,
}

impl App {
    pub fn new(store: Store) -> App {
        let active = store.active_bucket().map(|b| b.header.id);
        let bucket_index = store
            .buckets()
            .position(|b| Some(b.header.id) == active)
            .unwrap_or(0);
        let focus = match store.buckets().next() {
            Some(_) => Focus::Tasks,
            None => Focus::Buckets,
        };
        App {
            store,
            focus,
            bucket_index,
            task_index: 0,
            done_index: 0,
            mode: Mode::Normal,
            message: None,
            should_quit: false,
        }
    }

    pub fn selected_bucket(&self) -> Option<&Bucket> {
        self.store.buckets().nth(self.bucket_index)
    }

    /// Open tasks of the selected bucket.
    pub fn open_tasks(&self) -> Vec<Entry<'_>> {
        let Some(bucket) = self.selected_bucket() else {
            return Vec::new();
        };
        let bucket = bucket.header.id;
        self.store
            .entries()
            .filter(|e| e.bucket.header.id == bucket && !e.task.is_done())
            .collect()
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

    pub fn update(&mut self, action: Action) {
        if !matches!(action, Action::Input(_) | Action::Backspace) {
            self.message = None;
        }
        let result = match self.mode {
            Mode::Normal => self.on_normal(action),
            Mode::Input { .. } => self.on_input(action),
            Mode::Confirm(_) => self.on_confirm(action),
        };
        if let Err(UiError(err)) = result {
            self.error(err);
        }
        self.clamp_selection();
    }

    fn on_normal(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Up | Action::Down => {
                let step = |i: usize| match action {
                    Action::Up => i.saturating_sub(1),
                    _ => i + 1,
                };
                match self.focus {
                    Focus::Buckets => {
                        self.bucket_index = step(self.bucket_index);
                        self.task_index = 0;
                    }
                    Focus::Tasks => self.task_index = step(self.task_index),
                    Focus::Done => self.done_index = step(self.done_index),
                }
            }
            Action::FocusBuckets => self.focus = Focus::Buckets,
            Action::FocusTasks => self.focus = Focus::Tasks,
            Action::FocusNext | Action::FocusPrev => {
                let i = Focus::ORDER.iter().position(|&f| f == self.focus).unwrap();
                let len = Focus::ORDER.len();
                let i = match action {
                    Action::FocusNext => (i + 1) % len,
                    _ => (i + len - 1) % len,
                };
                self.focus = Focus::ORDER[i];
            }
            Action::Add => match self.focus {
                Focus::Buckets => self.prompt(Prompt::AddBucket, String::new()),
                _ if self.selected_bucket().is_none() => {
                    self.error("Create a bucket first: press h, then a")
                }
                _ => self.prompt(Prompt::AddTask, String::new()),
            },
            Action::Edit => match self.focus {
                Focus::Buckets => {
                    if let Some(name) = self.selected_bucket().map(|b| b.name.clone()) {
                        self.prompt(Prompt::RenameBucket, name);
                    }
                }
                _ => {
                    let description = self.require_task()?.description.clone();
                    self.prompt(Prompt::EditDescription, description);
                }
            },
            Action::Remove => match self.focus {
                Focus::Buckets => {
                    if let Some(bucket) = self.selected_bucket() {
                        let question = format!(
                            "Remove bucket {} and its {} task(s)?",
                            bucket.name,
                            bucket.len()
                        );
                        let name = bucket.name.clone();
                        self.confirm(question, Op::RemoveBucket { name });
                    }
                }
                _ => {
                    let (id, label) = self.require_selected()?;
                    self.confirm(format!("Remove {label}?"), Op::RemoveTask { id });
                }
            },
            Action::UseBucket => {
                if self.focus != Focus::Buckets {
                    self.error("u works in the buckets pane (press h)");
                } else if let Some(name) = self.selected_bucket().map(|b| b.name.clone()) {
                    self.store.set_active_bucket(&name)?;
                    self.commit(format!("Active bucket is now {name}"))?;
                }
            }
            Action::ToggleDone => {
                let (id, label) = self.require_selected()?;
                let (status, question) = if self.require_task()?.is_done() {
                    (Status::ToDo, format!("Reopen {label}?"))
                } else {
                    (Status::Done, format!("Mark {label} as done?"))
                };
                self.confirm_edit(
                    id,
                    question,
                    TaskEdit {
                        status: Some(status),
                        ..TaskEdit::default()
                    },
                );
            }
            Action::CycleStatus => {
                let (id, label) = self.require_selected()?;
                let from = self.require_task()?.status;
                let to = next(&Status::ALL, from);
                self.confirm_edit(
                    id,
                    format!("Change status of {label}: {from} → {to}?"),
                    TaskEdit {
                        status: Some(to),
                        ..TaskEdit::default()
                    },
                );
            }
            Action::CyclePriority => {
                let (id, label) = self.require_selected()?;
                let from = self.require_task()?.priority;
                let to = next(&Priority::ALL, from);
                self.confirm_edit(
                    id,
                    format!("Change priority of {label}: {from} → {to}?"),
                    TaskEdit {
                        priority: Some(to),
                        ..TaskEdit::default()
                    },
                );
            }
            Action::EditDue => {
                self.require_task()?;
                self.prompt(Prompt::EditDue, String::new());
            }
            Action::MoveTask => {
                self.require_task()?;
                self.prompt(Prompt::MoveTask, String::new());
            }
            Action::InvalidKey(key) => self.error(format!("`{key}` does nothing here")),
            Action::Input(_) | Action::Backspace | Action::Submit | Action::Cancel => {}
        }
        Ok(())
    }

    fn on_input(&mut self, action: Action) -> Result<()> {
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
            Action::InvalidKey(key) => self.error(format!("`{key}` can't be used while typing")),
            _ => {}
        }
        Ok(())
    }

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
                    self.select_task(id);
                    self.commit(format!("Added task to {bucket}"))?;
                }
            }
            Prompt::EditDescription => {
                let (id, label) = self.require_selected()?;
                let description = text.trim().to_string();
                if description.is_empty() {
                    return Err(kru::Error::EmptyDescription.into());
                }
                self.confirm_edit(
                    id,
                    format!("Rename {label} to \"{description}\"?"),
                    TaskEdit {
                        description: Some(description),
                        ..TaskEdit::default()
                    },
                );
            }
            Prompt::EditDue => {
                let (id, label) = self.require_selected()?;
                let due = kru::due::parse(text)?;
                let question = match due {
                    Some(due) => format!("Set due of {label} to {}?", kru::due::format(due)),
                    None => format!("Clear the due date of {label}?"),
                };
                self.confirm_edit(
                    id,
                    question,
                    TaskEdit {
                        due: Some(due),
                        ..TaskEdit::default()
                    },
                );
            }
            Prompt::MoveTask => {
                let (id, label) = self.require_selected()?;
                let to = self.store.bucket(text)?.name.clone();
                self.confirm(format!("Move {label} to {to}?"), Op::MoveTask { id, to });
            }
        }
        Ok(())
    }

    fn on_confirm(&mut self, action: Action) -> Result<()> {
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

    fn apply(&mut self, op: Op) -> Result<()> {
        match op {
            Op::EditTask { id, edit } => {
                self.store.edit_task(id, edit)?;
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

    // --- Helpers ------------------------------------------------------------

    /// The selected task, or an error explaining that there is none.
    fn require_task(&self) -> Result<&Task> {
        self.selected_task()
            .map(|e| e.task)
            .ok_or_else(|| no_task_selected(self.focus))
    }

    /// Id and a short label like `#3 "Buy milk"` for the selected task.
    fn require_selected(&self) -> Result<(Uuid, String)> {
        let entry = self
            .selected_task()
            .ok_or_else(|| no_task_selected(self.focus))?;
        let label = format!(
            "#{} \"{}\"",
            entry.num,
            shorten(&entry.task.description, 40)
        );
        Ok((entry.task.header.id, label))
    }

    fn confirm(&mut self, question: String, op: Op) {
        self.mode = Mode::Confirm(Pending { question, op });
    }

    fn confirm_edit(&mut self, id: Uuid, question: String, edit: TaskEdit) {
        self.confirm(question, Op::EditTask { id, edit });
    }

    /// Saves the store and shows `message` on success.
    fn commit(&mut self, message: String) -> Result<()> {
        self.store.save()?;
        self.info(message);
        Ok(())
    }

    fn prompt(&mut self, prompt: Prompt, buffer: String) {
        self.mode = Mode::Input { prompt, buffer };
    }

    fn select_bucket(&mut self, id: Uuid) {
        if let Some(index) = self.store.buckets().position(|b| b.header.id == id) {
            self.bucket_index = index;
            self.task_index = 0;
        }
    }

    fn select_task(&mut self, id: Uuid) {
        if let Some(index) = self
            .open_tasks()
            .iter()
            .position(|e| e.task.header.id == id)
        {
            self.task_index = index;
        }
    }

    fn clamp_selection(&mut self) {
        let buckets = self.store.buckets().count();
        self.bucket_index = self.bucket_index.min(buckets.saturating_sub(1));
        let open = self.open_tasks().len();
        self.task_index = self.task_index.min(open.saturating_sub(1));
        let done = self.done_tasks().len();
        self.done_index = self.done_index.min(done.saturating_sub(1));
    }

    fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            is_error: false,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            is_error: true,
        });
    }
}

/// An error shown in the footer. Library errors convert into it, so
/// handlers can use `?` on [`Store`] calls.
struct UiError(String);

impl From<kru::Error> for UiError {
    fn from(err: kru::Error) -> UiError {
        UiError(err.to_string())
    }
}

type Result<T> = std::result::Result<T, UiError>;

fn no_task_selected(focus: Focus) -> UiError {
    let text = match focus {
        Focus::Buckets => "No task selected: press l or tab to go to the tasks",
        Focus::Tasks | Focus::Done => "No task selected: this pane is empty",
    };
    UiError(text.to_string())
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

/// The value after `current` in `all`, wrapping around.
fn next<T: Copy + PartialEq>(all: &[T], current: T) -> T {
    let index = all.iter().position(|&v| v == current).unwrap_or(0);
    all[(index + 1) % all.len()]
}
