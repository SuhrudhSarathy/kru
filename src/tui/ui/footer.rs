//! The footer: the latest message and the keys available right now.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::tui::app::{App, Focus, Mode};

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    let message = match &app.message {
        Some(message) if message.is_error => Line::from(message.text.as_str()).red(),
        Some(message) => Line::from(message.text.as_str()).green(),
        None => Line::default(),
    };
    let keys = Line::from(key_hints(app)).dark_gray();
    frame.render_widget(Paragraph::new(vec![message, keys]), area);
}

/// Keys that act on a task, wherever a task is selected or expanded.
/// A macro so it can be joined with `concat!` at compile time.
macro_rules! task_keys {
    () => {
        "e edit · s status · p priority · d due · m move · r remove"
    };
}

fn key_hints(app: &App) -> &'static str {
    match (&app.mode, app.focus) {
        (Mode::Input { .. }, _) => "enter save · esc cancel",
        (Mode::Edit(editor), _) if editor.typing.is_some() => "enter keep · esc undo typing",
        (Mode::Edit(_), _) => {
            "j/k field · h/l or space change option · enter type text · esc finish"
        }
        (Mode::Confirm(_), _) => "y confirm · n/esc cancel",
        (Mode::Normal, _) if app.expanded.is_some() => concat!(
            "x/esc close · j/k next task · space done/reopen · ",
            task_keys!()
        ),
        (Mode::Normal, Focus::Buckets) => {
            "j/k move · a add · e rename · r remove · u use · tab next pane · q quit"
        }
        (Mode::Normal, Focus::Tasks) => concat!(
            "j/k move · x expand · a add · space done · ",
            task_keys!(),
            " · tab next pane · q quit"
        ),
        (Mode::Normal, Focus::Done) => concat!(
            "j/k move · x expand · space reopen · ",
            task_keys!(),
            " · tab next pane · q quit"
        ),
    }
}
