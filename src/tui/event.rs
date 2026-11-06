//! Maps key presses to [`Action`]s depending on the current mode.
//!
//! Returns `None` for keys that are deliberately ignored (Enter outside of
//! text input) and [`Action::InvalidKey`] for keys with no meaning.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{Action, Mode, Motion};

/// The action for `key` in the current mode, if any.
pub fn action_for(key: KeyEvent, mode: &Mode) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    let action = match mode {
        Mode::Normal => match key.code {
            KeyCode::Enter => return None,
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Esc => Action::Back,
            KeyCode::Char('k') | KeyCode::Up => Action::Up,
            KeyCode::Char('j') | KeyCode::Down => Action::Down,
            KeyCode::Char('h') | KeyCode::Left => Action::FocusBuckets,
            KeyCode::Char('l') | KeyCode::Right => Action::FocusTasks,
            KeyCode::Tab => Action::FocusNext,
            KeyCode::BackTab => Action::FocusPrev,
            KeyCode::Char('a') => Action::Add,
            KeyCode::Char('e') => Action::Edit,
            KeyCode::Char('r') | KeyCode::Delete => Action::Remove,
            KeyCode::Char('x') => Action::Expand,
            KeyCode::Char('u') => Action::UseBucket,
            KeyCode::Char(' ') => Action::ToggleDone,
            KeyCode::Char('s') => Action::CycleStatus,
            KeyCode::Char('p') => Action::CyclePriority,
            KeyCode::Char('d') => Action::EditDue,
            KeyCode::Char('m') => Action::MoveTask,
            KeyCode::Char('o') => Action::OpenMd,
            KeyCode::Char('S') => Action::Sync,
            KeyCode::Char('P') => Action::Unlock,
            KeyCode::Char('L') => Action::Lock,
            _ => invalid(key),
        },
        Mode::Input { .. } => typing(key),
        Mode::Edit(editor) if editor.typing.is_some() => typing(key),
        Mode::Edit(_) => match key.code {
            KeyCode::Char('k') | KeyCode::Up => Action::Up,
            KeyCode::Char('j') | KeyCode::Down => Action::Down,
            KeyCode::Char('h') | KeyCode::Left => Action::Left,
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => Action::Right,
            KeyCode::Enter => Action::Submit,
            KeyCode::Esc => Action::Cancel,
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

/// The action for `key` while typing into a text field.
fn typing(key: KeyEvent) -> Action {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let command = key.modifiers.contains(KeyModifiers::SUPER);
    match key.code {
        KeyCode::Enter if alt || key.modifiers.contains(KeyModifiers::SHIFT) => Action::Newline,
        KeyCode::Enter => Action::Submit,
        KeyCode::Esc => Action::Cancel,
        KeyCode::Backspace if alt || control => Action::Delete(Motion::WordLeft),
        KeyCode::Backspace if command => Action::Delete(Motion::LineStart),
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::Delete(Motion::Right),
        KeyCode::Left if alt || control => Action::Move(Motion::WordLeft),
        KeyCode::Left if command => Action::Move(Motion::LineStart),
        KeyCode::Left => Action::Move(Motion::Left),
        KeyCode::Right if alt || control => Action::Move(Motion::WordRight),
        KeyCode::Right if command => Action::Move(Motion::LineEnd),
        KeyCode::Right => Action::Move(Motion::Right),
        KeyCode::Up => Action::Move(Motion::Up),
        KeyCode::Down => Action::Move(Motion::Down),
        KeyCode::Home => Action::Move(Motion::LineStart),
        KeyCode::End => Action::Move(Motion::LineEnd),
        KeyCode::Char(c) if control && !alt => match c {
            // Ctrl+J is the line break that works in every terminal.
            'j' => Action::Newline,
            'a' => Action::Move(Motion::LineStart),
            'e' => Action::Move(Motion::LineEnd),
            'w' => Action::Delete(Motion::WordLeft),
            'u' => Action::Delete(Motion::LineStart),
            'k' => Action::Delete(Motion::LineEnd),
            _ => invalid(key),
        },
        // How terminals send Option+Left and Option+Right.
        KeyCode::Char('b') if alt && !control => Action::Move(Motion::WordLeft),
        KeyCode::Char('f') if alt && !control => Action::Move(Motion::WordRight),
        // Control and Alt together is how AltGr can arrive: it types.
        KeyCode::Char(c) if !command && alt == control => Action::Input(c),
        _ => invalid(key),
    }
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
