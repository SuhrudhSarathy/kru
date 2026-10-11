//! A task describing its own fields, so views can show every field without
//! naming each one.
//!
//! [`Task::fields`](super::Task::fields) lists the fields in display order.
//! Each [`Field`] carries its [`Value`] (text, time or choice), a [`Tone`]
//! saying what the value means, where it should appear, and whether and how
//! it can be edited. Frontends only need to know how to draw and edit the
//! three kinds of value; a new field is one line in `Task::fields` (plus
//! one in [`Task::set`](super::Task::set) if it is editable).

use std::fmt::{self, Display};

use jiff::Timestamp;

use crate::utils::due;

/// What a value means, so frontends can colour it. Not a colour itself:
/// the CLI and TUI theme decides that.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// Nothing special; drawn in the default colour.
    Neutral,
    /// Secondary detail, drawn faintly.
    Muted,
    Info,
    Progress,
    Warning,
    Danger,
    Success,
}

/// An enum that can be a field: its `Display` text, a tone, and every
/// value it can take (offered when editing).
pub trait Choice: Display + Copy + 'static {
    const ALL: &'static [Self];

    fn tone(self) -> Tone;
}

/// How a field can be edited.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Editing {
    ReadOnly,
    /// Editable; interactive frontends ask before applying.
    Confirm,
    /// Editable and applied straight away (e.g. cycling priority).
    Instant,
}

/// A field's value, by kind.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Value {
    Text(String),
    Time(Option<Timestamp>),
    /// The current value and every value it can take.
    Choice {
        value: String,
        options: Vec<String>,
    },
}

impl Value {
    /// Compact form for tables: times as `Sat 2026-10-17 15:48`, nothing
    /// if unset.
    pub fn short(&self) -> String {
        match self {
            Value::Text(text) | Value::Choice { value: text, .. } => text.clone(),
            Value::Time(Some(at)) => due::format(*at),
            Value::Time(None) => String::new(),
        }
    }

    /// Full form for detail views: times with how far away they are, `—`
    /// if unset or empty.
    pub fn long(&self, now: Timestamp) -> String {
        match self {
            Value::Text(text) if text.is_empty() => "—".to_string(),
            Value::Time(Some(at)) => {
                format!("{}  ({})", due::format(*at), due::relative(*at, now))
            }
            Value::Time(None) => "—".to_string(),
            _ => self.short(),
        }
    }
}

/// One field of a task, ready to display and edit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    /// Identifies the field for editing ([`Task::set`](super::Task::set))
    /// and names its CLI flag.
    pub key: &'static str,
    pub label: &'static str,
    pub value: Value,
    pub tone: Tone,
    /// Shown as a column in task tables.
    pub in_list: bool,
    /// Shown in detail views (the TUI's expanded view, `kru task show`).
    pub in_detail: bool,
    pub editing: Editing,
}

impl Field {
    /// A text field: neutral, detail views only, read-only.
    pub fn text(key: &'static str, label: &'static str, text: impl Into<String>) -> Field {
        Field::new(key, label, Value::Text(text.into()), Tone::Neutral)
    }

    /// A time field: neutral, detail views only, read-only.
    pub fn time(key: &'static str, label: &'static str, at: Option<Timestamp>) -> Field {
        Field::new(key, label, Value::Time(at), Tone::Neutral)
    }

    /// An enum field, toned by its value; detail views only, read-only.
    pub fn choice<C: Choice>(key: &'static str, label: &'static str, value: C) -> Field {
        let options = C::ALL.iter().map(ToString::to_string).collect();
        let value_text = value.to_string();
        Field::new(
            key,
            label,
            Value::Choice {
                value: value_text,
                options,
            },
            value.tone(),
        )
    }

    /// Makes the field editable, with confirmation.
    pub fn editable(mut self) -> Field {
        self.editing = Editing::Confirm;
        self
    }

    /// Makes the field editable without confirmation.
    pub fn instant(mut self) -> Field {
        self.editing = Editing::Instant;
        self
    }

    pub fn is_editable(&self) -> bool {
        self.editing != Editing::ReadOnly
    }

    /// Sets whether the field is a table column.
    pub fn listed(mut self, in_list: bool) -> Field {
        self.in_list = in_list;
        self
    }

    /// Sets whether the field appears in detail views.
    pub fn detailed(mut self, in_detail: bool) -> Field {
        self.in_detail = in_detail;
        self
    }

    /// Overrides the tone, e.g. to flag an overdue date.
    pub fn toned(mut self, tone: Tone) -> Field {
        self.tone = tone;
        self
    }

    fn new(key: &'static str, label: &'static str, value: Value, tone: Tone) -> Field {
        Field {
            key,
            label,
            value,
            tone,
            in_list: false,
            in_detail: true,
            editing: Editing::ReadOnly,
        }
    }
}

/// One field changed by an edit, e.g. `priority: low → high`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub key: &'static str,
    pub label: &'static str,
    pub from: Value,
    pub to: Value,
}

impl Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown = |value: &Value| {
            let text = value.short();
            if text.is_empty() {
                "—".to_string()
            } else {
                text
            }
        };
        write!(
            f,
            "{}: {} → {}",
            self.label.to_lowercase(),
            shown(&self.from),
            shown(&self.to)
        )
    }
}

/// Indices of the fields to show as table columns for these rows (each row
/// being one task's [`fields`](super::Task::fields)): every field that at
/// least one row lists, in field order.
pub fn list_columns(rows: &[Vec<Field>]) -> Vec<usize> {
    let count = rows.first().map_or(0, Vec::len);
    (0..count)
        .filter(|&i| rows.iter().any(|fields| fields[i].in_list))
        .collect()
}
