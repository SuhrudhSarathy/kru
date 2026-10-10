//! Styles shared by the panes and overlays. Task colours come from
//! [`crate::theme`], which the CLI uses too.

use kru::task::Tone;
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::theme::{self, Colour};

/// A bordered block, highlighted when focused.
pub(super) fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::bordered()
        .title(format!(" {title} "))
        .border_style(border(focused))
}

/// Border colour: cyan when focused, grey otherwise.
pub(super) fn border(focused: bool) -> Style {
    if focused {
        Style::new().cyan()
    } else {
        Style::new().dark_gray()
    }
}

/// Row highlight: strong in the focused pane, subtle elsewhere.
pub(super) fn highlight(focused: bool) -> Style {
    if focused {
        Style::new().reversed()
    } else {
        Style::new().bold()
    }
}

/// Text in a theme colour.
pub(super) fn fg(colour: Colour) -> Style {
    Style::new().fg(colour.into())
}

/// Text coloured for what a field's value means.
pub(super) fn tone(tone: Tone) -> Style {
    theme::tone(tone).map_or(Style::new(), fg)
}
