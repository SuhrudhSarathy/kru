//! Maps key presses to [`Action`]s depending on the current mode.
//!
//! Returns `None` for keys that are deliberately ignored (Enter outside of
//! text input) and [`Action::InvalidKey`] for keys with no meaning.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{Action, Mode};

/// The action for `key` in the current mode, if any.
pub fn action_for(key: KeyEvent, mode: &Mode) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    let action = match mode {
        Mode::Normal => match key.code {
            KeyCode::Enter => return None,
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('k') | KeyCode::Up => Action::Up,
            KeyCode::Char('j') | KeyCode::Down => Action::Down,
            KeyCode::Char('h') | KeyCode::Left => Action::FocusBuckets,
            KeyCode::Char('l') | KeyCode::Right => Action::FocusTasks,
            KeyCode::Tab => Action::FocusNext,
            KeyCode::BackTab => Action::FocusPrev,
            KeyCode::Char('a') => Action::Add,
            KeyCode::Char('e') => Action::Edit,
            KeyCode::Char('x') | KeyCode::Delete => Action::Remove,
            KeyCode::Char('u') => Action::UseBucket,
            KeyCode::Char(' ') => Action::ToggleDone,
            KeyCode::Char('s') => Action::CycleStatus,
            KeyCode::Char('p') => Action::CyclePriority,
            KeyCode::Char('d') => Action::EditDue,
            KeyCode::Char('m') => Action::MoveTask,
            _ => invalid(key),
        },
        Mode::Input { .. } => match key.code {
            KeyCode::Enter => Action::Submit,
            KeyCode::Esc => Action::Cancel,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Char(c) => Action::Input(c),
            _ => invalid(key),
        },
        // Only `y` confirms, so a stray Enter can never apply a change.
        Mode::Confirm(_) => match key.code {
            KeyCode::Enter => return None,
            KeyCode::Char('y') => Action::Submit,
            KeyCode::Char('n') | KeyCode::Esc => Action::Cancel,
            _ => invalid(key),
        },
    };
    Some(action)
}

/// An [`Action::InvalidKey`] naming the key, e.g. `z` or `Control+x`.
fn invalid(key: KeyEvent) -> Action {
    let name = match key.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        code => code.to_string(),
    };
    let modifiers = key.modifiers - KeyModifiers::SHIFT;
    if modifiers.is_empty() {
        Action::InvalidKey(name)
    } else {
        Action::InvalidKey(format!("{modifiers}+{name}"))
    }
}
