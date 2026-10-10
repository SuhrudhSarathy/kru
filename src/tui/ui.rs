//! Rendering. A pure function of `&App`, split by screen element:
//!
//! - `buckets`, `tasks`: the panes
//! - `footer`: message and key hints
//! - `expanded`: the blown-up view of one task
//! - `dialog`: text prompts and confirmations
//! - `style`, `wrap`: shared helpers

mod buckets;
mod dialog;
mod expanded;
mod footer;
mod style;
mod tasks;
mod wrap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use super::app::{App, Mode};

/// Draws the whole screen: buckets and open tasks on top, done tasks below,
/// the footer, then any overlay (expanded task, prompt, confirmation).
pub fn render(frame: &mut Frame, app: &App) {
    let [main, done, footer] = Layout::vertical([
        Constraint::Fill(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());
    let [buckets, open] =
        Layout::horizontal([Constraint::Length(28), Constraint::Min(0)]).areas(main);

    buckets::render(frame, app, buckets);
    tasks::render_open(frame, app, open);
    tasks::render_done(frame, app, done);
    footer::render(frame, app, footer);

    if let Some(entry) = app.expanded_task() {
        expanded::render(frame, &entry);
    }
    match &app.mode {
        Mode::Normal => {}
        Mode::Input { prompt, buffer } => dialog::render_input(frame, app, *prompt, buffer),
        Mode::Confirm(pending) => dialog::render_confirm(frame, pending),
    }
}
