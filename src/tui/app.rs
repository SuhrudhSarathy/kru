//! TUI state and its update logic. No ratatui types in here, so it can be
//! tested without a terminal.
//!
//! - `app.rs`: the state types, [`App::update`] and footer messages
//! - `select`: which bucket and task are selected or expanded
//! - `normal`: actions while no prompt or confirmation is open
//! - `prompt`: text prompts and what their input does
//! - `confirm`: changes waiting for `y`, and applying them

mod confirm;
mod normal;
mod prompt;
mod select;

use kru::Store;
use uuid::Uuid;

pub use confirm::Pending;
pub use prompt::Prompt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Buckets,
    Tasks,
    Done,
}

impl Focus {
    const ORDER: [Focus; 3] = [Focus::Buckets, Focus::Tasks, Focus::Done];
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
    /// Esc: closes the expanded view, otherwise quits.
    Back,
    /// Opens or closes the expanded view of the selected task.
    Expand,
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
    /// The task shown in the expanded view, if open.
    pub expanded: Option<Uuid>,
    pub message: Option<Message>,
    pub should_quit: bool,
}

impl App {
    /// Starts with the active bucket selected.
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
            expanded: None,
            message: None,
            should_quit: false,
        }
    }

    /// Handles one action and shows any resulting error in the footer.
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

    /// Saves the store and shows `message` on success.
    fn commit(&mut self, message: String) -> Result<()> {
        self.store.save()?;
        self.info(message);
        Ok(())
    }

    /// Shows a success message in the footer.
    fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            is_error: false,
        });
    }

    /// Shows an error message in the footer.
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
