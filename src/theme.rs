//! Colours shared by the CLI and the TUI, so a task looks the same in both.
//!
//! Task fields say what their values mean with a [`Tone`]; [`tone`] picks
//! the colour for each. The choice is made here once, as a [`Colour`], and
//! each frontend converts it into its own colour type
//! (`comfy_table::Color` for the CLI, `ratatui::style::Color` for the TUI)
//! via `From`.

use kru::task::Tone;

/// A terminal colour, independent of the library drawing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Colour {
    Red,
    Yellow,
    Green,
    Cyan,
    Magenta,
    DarkGrey,
}

/// Task numbers (`#3`).
pub const NUMBER: Colour = Colour::Cyan;

/// The marker for the active bucket.
pub const ACTIVE: Colour = Colour::Green;

/// The colour for a field's tone; `None` means the terminal's default.
pub fn tone(tone: Tone) -> Option<Colour> {
    match tone {
        Tone::Neutral => None,
        Tone::Muted => Some(Colour::DarkGrey),
        Tone::Info => Some(Colour::Cyan),
        Tone::Progress => Some(Colour::Magenta),
        Tone::Warning => Some(Colour::Yellow),
        Tone::Danger => Some(Colour::Red),
        Tone::Success => Some(Colour::Green),
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
            Colour::DarkGrey => Self::DarkGrey,
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
            Colour::DarkGrey => Self::DarkGray,
        }
    }
}
