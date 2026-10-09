use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::utils::Header;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::Low, Priority::Medium, Priority::High];
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        };
        f.pad(s)
    }
}

impl FromStr for Priority {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "high" | "h" => Ok(Priority::High),
            "medium" | "med" | "m" => Ok(Priority::Medium),
            "low" | "l" => Ok(Priority::Low),
            _ => Err(Error::InvalidPriority(s.to_string())),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Status {
    ToDo,
    InProgress,
    Done,
}

impl Status {
    pub const ALL: [Status; 3] = [Status::ToDo, Status::InProgress, Status::Done];
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Status::ToDo => "todo",
            Status::InProgress => "in-progress",
            Status::Done => "done",
        };
        f.pad(s)
    }
}

impl FromStr for Status {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "todo" | "to-do" | "t" => Ok(Status::ToDo),
            "in-progress" | "inprogress" | "progress" | "doing" | "wip" | "p" => {
                Ok(Status::InProgress)
            }
            "done" | "d" => Ok(Status::Done),
            _ => Err(Error::InvalidStatus(s.to_string())),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub header: Header,
    pub description: String,
    pub priority: Priority,
    pub status: Status,
    pub due: Option<Timestamp>,
    /// When the task was marked done; used to prune old done tasks.
    #[serde(default)]
    pub time_completed: Option<Timestamp>,
}

impl Task {
    pub fn new(description: impl Into<String>) -> Task {
        Task {
            header: Header::new(),
            description: description.into(),
            priority: Priority::Low,
            status: Status::ToDo,
            due: None,
            time_completed: None,
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Task {
        self.header = self.header.update_edited();
        self.priority = priority;
        self
    }

    pub fn with_due(mut self, due: Timestamp) -> Task {
        self.header = self.header.update_edited();

        self.due = Some(due);
        self
    }

    pub fn set_status(&mut self, status: Status) {
        self.header = self.header.update_edited();
        self.time_completed = match status {
            Status::Done if self.is_done() => self.time_completed,
            Status::Done => Some(Timestamp::now()),
            _ => None,
        };
        self.status = status;
    }

    /// Applies every field that is set in `edit`.
    pub fn apply(&mut self, edit: TaskEdit) {
        self.header = self.header.update_edited();
        if let Some(description) = edit.description {
            self.description = description;
        }
        if let Some(priority) = edit.priority {
            self.priority = priority;
        }
        if let Some(due) = edit.due {
            self.due = due;
        }
        if let Some(status) = edit.status {
            self.set_status(status);
        }
    }

    pub fn is_done(&self) -> bool {
        self.status == Status::Done
    }

    pub fn is_overdue(&self, now: Timestamp) -> bool {
        !self.is_done() && self.due.is_some_and(|due| due < now)
    }
}

/// A partial update to a task: `None` fields are left unchanged.
/// `due: Some(None)` clears the due date.
#[derive(Clone, Debug, Default)]
pub struct TaskEdit {
    pub description: Option<String>,
    pub priority: Option<Priority>,
    pub status: Option<Status>,
    pub due: Option<Option<Timestamp>>,
}

impl TaskEdit {
    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.priority.is_none()
            && self.status.is_none()
            && self.due.is_none()
    }
}
