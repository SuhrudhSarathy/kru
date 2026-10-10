//! Colours shared by the CLI and the TUI, so a task looks the same in both.
//!
//! The choices are made here once, as a [`Colour`]; each frontend converts
//! it into its own colour type (`comfy_table::Color` for the CLI,
//! `ratatui::style::Color` for the TUI) via `From`.

use kru::task::{Priority, Status};

/// A terminal colour, independent of the library drawing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Colour {
    Red,
    Yellow,
    Green,
    Cyan,
    Magenta,
}

/// Task numbers (`#3`).
pub const NUMBER: Colour = Colour::Cyan;

/// Due dates that have passed on open tasks.
pub const OVERDUE: Colour = Colour::Red;

/// The marker for the active bucket.
pub const ACTIVE: Colour = Colour::Green;

pub fn priority(priority: Priority) -> Colour {
    match priority {
        Priority::High => Colour::Red,
        Priority::Medium => Colour::Yellow,
        Priority::Low => Colour::Cyan,
    }
}

pub fn status(status: Status) -> Colour {
    match status {
        Status::ToDo => Colour::Cyan,
        Status::InProgress => Colour::Magenta,
        Status::Done => Colour::Green,
    }
}

impl From<Colour> for comfy_table::Color {
    fn from(colour: Colour) -> Self {
        match colour {
            Colour::Red => Self::Red,
            Colour::Yellow => Self::Yellow,
            Colour::Green => Self::Green,
            Colour::Cyan => Self::Cyan,
            Colour::Magenta => Self::Magenta,
        }
    }
}

impl From<Colour> for ratatui::style::Color {
    fn from(colour: Colour) -> Self {
        match colour {
            Colour::Red => Self::Red,
            Colour::Yellow => Self::Yellow,
            Colour::Green => Self::Green,
            Colour::Cyan => Self::Cyan,
            Colour::Magenta => Self::Magenta,
        }
    }
}
