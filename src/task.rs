//! Tasks, their priority and status, and editing them field by field.

pub mod field;

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::utils::due;
use crate::utils::error::Error;
use crate::utils::header::Header;
use crate::utils::sanitize;

pub use field::{Change, Choice, Editing, Field, Tone, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    /// How important the priority is: higher is more important.
    pub fn rank(self) -> u8 {
        match self {
            Priority::Low => 0,
            Priority::Medium => 1,
            Priority::High => 2,
        }
    }
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

impl Choice for Priority {
    const ALL: &'static [Priority] = &[Priority::Low, Priority::Medium, Priority::High];

    fn tone(self) -> Tone {
        match self {
            Priority::High => Tone::Danger,
            Priority::Medium => Tone::Warning,
            Priority::Low => Tone::Info,
        }
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

impl Choice for Status {
    const ALL: &'static [Status] = &[Status::ToDo, Status::InProgress, Status::Done];

    fn tone(self) -> Tone {
        match self {
            Status::ToDo => Tone::Info,
            Status::InProgress => Tone::Progress,
            Status::Done => Tone::Success,
        }
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
    /// Creates a low-priority, to-do task with no due date.
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

    /// Builder: sets the priority.
    pub fn with_priority(mut self, priority: Priority) -> Task {
        self.header = self.header.update_edited();
        self.priority = priority;
        self
    }

    /// Builder: sets the due date.
    pub fn with_due(mut self, due: Timestamp) -> Task {
        self.header = self.header.update_edited();

        self.due = Some(due);
        self
    }

    /// Sets the status, recording when the task became done
    /// (and clearing that time when it is reopened).
    pub fn set_status(&mut self, status: Status) {
        self.header = self.header.update_edited();
        self.time_completed = match status {
            Status::Done if self.is_done() => self.time_completed,
            Status::Done => Some(Timestamp::now()),
            _ => None,
        };
        self.status = status;
    }

    /// Sets the field `key` from user input, cleaning and checking it
    /// first: text is sanitised, choices accept their usual spellings, and
    /// times accept everything [`due::parse`] does (`none` clears them).
    ///
    /// Every editable field in [`Task::fields`] needs an arm here.
    pub fn set(&mut self, key: &str, input: &str) -> crate::Result<()> {
        match key {
            "description" => self.description = sanitize::text(input, "description")?,
            "priority" => self.priority = input.parse()?,
            "status" => self.set_status(input.parse()?),
            "due" => self.due = due::parse(input)?,
            _ => {
                return Err(match self.fields().into_iter().find(|f| f.key == key) {
                    Some(field) => Error::ReadOnlyField(field.label),
                    None => Error::UnknownField(key.to_string()),
                });
            }
        }
        Ok(())
    }

    /// The fields named in `keys` whose value differs between `before` and
    /// this task.
    pub fn changes_from(&self, before: &Task, keys: &[&str]) -> Vec<Change> {
        before
            .fields()
            .into_iter()
            .zip(self.fields())
            .filter(|(old, new)| keys.contains(&old.key) && old.value != new.value)
            .map(|(old, new)| Change {
                key: old.key,
                label: old.label,
                from: old.value,
                to: new.value,
            })
            .collect()
    }

    /// Whether the status is [`Status::Done`].
    pub fn is_done(&self) -> bool {
        self.status == Status::Done
    }

    /// Whether the task is still open and its due date is before `now`.
    pub fn is_overdue(&self, now: Timestamp) -> bool {
        !self.is_done() && self.due.is_some_and(|due| due < now)
    }

    /// Every field, in display order. Views render and edit these
    /// generically, so adding a field here (and to [`Task::set`] if it is
    /// editable) is enough to show and edit it everywhere.
    ///
    /// The description is listed for editing but hidden from tables and
    /// detail lists, since views show it as the title.
    ///
    /// Every task returns the same fields in the same order; only values,
    /// tones and visibility differ.
    pub fn fields(&self) -> Vec<Field> {
        let open = !self.is_done();
        let due_tone = if self.is_overdue(Timestamp::now()) {
            Tone::Danger
        } else {
            Tone::Neutral
        };
        vec![
            Field::text("description", "Description", &self.description)
                .detailed(false)
                .editable(),
            Field::choice("priority", "Priority", self.priority)
                .listed(true)
                .instant(),
            Field::choice("status", "Status", self.status)
                .listed(open)
                .editable(),
            Field::time("due", "Due", self.due)
                .listed(open)
                .toned(due_tone)
                .editable(),
            Field::time("completed", "Completed", self.time_completed)
                .listed(!open)
                .detailed(!open),
            Field::time("created", "Created", Some(self.header.time_created)),
            Field::time("edited", "Edited", Some(self.header.time_last_edited)),
            Field::text("id", "Id", self.header.id.to_string()).toned(Tone::Muted),
        ]
    }

    /// Orders tasks most urgent first: earliest due date first (tasks
    /// without one last), then higher priority first. Use with a stable
    /// sort so ties keep their creation order.
    pub fn cmp_urgency(&self, other: &Task) -> Ordering {
        let due = |task: &Task| (task.due.is_none(), task.due);
        due(self)
            .cmp(&due(other))
            .then_with(|| other.priority.rank().cmp(&self.priority.rank()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;

    #[test]
    fn fields_follow_the_task_state() {
        let mut task = Task::new("a").with_priority(Priority::High);
        let listed = |task: &Task| -> Vec<&'static str> {
            let fields = task.fields();
            fields
                .iter()
                .filter(|f| f.in_list)
                .map(|f| f.label)
                .collect()
        };
        assert_eq!(listed(&task), ["Priority", "Status", "Due"]);
        let priority = field(&task, "priority");
        assert_eq!(priority.value.short(), "high");
        assert_eq!(priority.tone, Tone::Danger);

        task.set_status(Status::Done);
        assert_eq!(listed(&task), ["Priority", "Completed"]);

        // Mixed rows show the union of their columns, in field order.
        let open = Task::new("b");
        let rows = [task.fields(), open.fields()];
        let columns: Vec<_> = field::list_columns(&rows)
            .into_iter()
            .map(|i| rows[0][i].label)
            .collect();
        assert_eq!(columns, ["Priority", "Status", "Due", "Completed"]);
    }

    #[test]
    fn overdue_due_dates_are_flagged() {
        use jiff::SignedDuration;
        let yesterday = Timestamp::now() - SignedDuration::from_hours(24);
        let task = Task::new("a").with_due(yesterday);
        assert_eq!(field(&task, "due").tone, Tone::Danger);
    }

    fn field(task: &Task, key: &str) -> Field {
        task.fields().into_iter().find(|f| f.key == key).unwrap()
    }

    #[test]
    fn every_editable_field_can_be_set() {
        // Setting each editable field to its own shown value must work,
        // which keeps `fields` and `set` in step.
        let mut task = Task::new("a").with_due(Timestamp::now());
        for field in task.fields().into_iter().filter(Field::is_editable) {
            let shown = field.value.short();
            task.set(field.key, &shown)
                .unwrap_or_else(|err| panic!("{}: {err}", field.key));
        }
    }

    #[test]
    fn set_cleans_and_checks_input() {
        let mut task = Task::new("a");
        task.set("description", "  buy\n milk ").unwrap();
        assert_eq!(task.description, "buy milk");
        task.set("priority", "H").unwrap();
        assert_eq!(task.priority, Priority::High);
        task.set("status", "done").unwrap();
        assert!(task.time_completed.is_some());
        task.set("due", "none").unwrap();
        assert_eq!(task.due, None);

        assert!(matches!(
            task.set("description", " "),
            Err(Error::EmptyField(_))
        ));
        assert!(matches!(
            task.set("priority", "urgent"),
            Err(Error::InvalidPriority(_))
        ));
        assert!(matches!(
            task.set("created", "today"),
            Err(Error::ReadOnlyField("Created"))
        ));
        assert!(matches!(
            task.set("colour", "red"),
            Err(Error::UnknownField(_))
        ));
    }

    #[test]
    fn changes_list_only_requested_fields_that_changed() {
        let before = Task::new("a");
        let mut after = before.clone();
        after.set("priority", "high").unwrap();
        after.set("status", "done").unwrap(); // also sets Completed
        let changes = after.changes_from(&before, &["priority", "status", "due"]);
        let shown: Vec<_> = changes.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["priority: low → high", "status: todo → done"]);
    }

    #[test]
    fn urgency_orders_by_due_then_priority() {
        let now = Timestamp::now();
        let in_days = |days: i64| now + SignedDuration::from_hours(days * 24);
        let mut tasks = [
            Task::new("no due, high").with_priority(Priority::High),
            Task::new("later, high")
                .with_priority(Priority::High)
                .with_due(in_days(5)),
            Task::new("soon, low").with_due(in_days(1)),
            Task::new("no due, low"),
            Task::new("soon, high")
                .with_priority(Priority::High)
                .with_due(in_days(1)),
        ];
        tasks.sort_by(Task::cmp_urgency);
        let order: Vec<_> = tasks.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(
            order,
            [
                "soon, high",
                "soon, low",
                "later, high",
                "no due, high",
                "no due, low"
            ]
        );
    }
}
