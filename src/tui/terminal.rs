//! Entering and leaving the TUI's terminal state.
//!
//! Without the keyboard enhancement, terminals send plain Enter and
//! Alt/Option+Enter as the same byte. Terminals that don't support it still
//! work; they just need Ctrl+J for a line break.

use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;

/// Whether the enhancement is on, and so has to be turned off when leaving.
static ENHANCED: AtomicBool = AtomicBool::new(false);

/// Takes over the terminal.
pub fn enter() -> DefaultTerminal {
    let terminal = ratatui::init();
    let enhanced = supports_keyboard_enhancement().unwrap_or(false)
        && execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok();
    ENHANCED.store(enhanced, Ordering::Relaxed);
    terminal
}

/// Hands the terminal back.
pub fn leave() {
    if ENHANCED.swap(false, Ordering::Relaxed) {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
}
