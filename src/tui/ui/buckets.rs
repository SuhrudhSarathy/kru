//! The buckets pane.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::style;
use crate::theme;
use crate::tui::app::{App, Focus};

/// Buckets with their open-task counts; `*` marks the active one, 🔒 an
/// unlocked private one. Locked private buckets aren't listed, only counted.
pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Buckets;
    let block = style::pane("Buckets", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let locked = app.store.locked_count();
    let [list_area, locked_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::from(locked > 0)),
    ])
    .areas(inner);
    if locked > 0 {
        let hint = format!("🔒 {locked} locked · P to unlock");
        frame.render_widget(Paragraph::new(hint).dark_gray(), locked_area);
    }

    if app.store.buckets().next().is_none() {
        let hint = Paragraph::new("No buckets yet.\nPress a to add one.").dark_gray();
        frame.render_widget(hint, list_area);
        return;
    }

    let active = app.store.active_bucket().map(|b| b.header.id);
    let items: Vec<ListItem> = app
        .store
        .buckets()
        .map(|bucket| {
            let marker = if Some(bucket.header.id) == active {
                "* "
            } else {
                "  "
            };
            let private = if app.store.is_private(bucket.header.id) {
                " 🔒"
            } else {
                ""
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, style::fg(theme::ACTIVE)),
                Span::raw(bucket.name.as_str()),
                Span::raw(private),
                Span::raw(format!(" ({})", bucket.open_count())).dark_gray(),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_style(style::highlight(focused));
    let mut state = ListState::default().with_selected(Some(app.bucket_index));
    frame.render_stateful_widget(list, list_area, &mut state);
}
