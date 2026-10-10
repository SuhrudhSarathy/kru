//! The buckets pane.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::style;
use crate::tui::app::{App, Focus};

/// Buckets with their open-task counts; `*` marks the active one.
pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Buckets;
    let block = style::pane("Buckets", focused);
    if app.store.buckets().next().is_none() {
        let hint = Paragraph::new("No buckets yet.\nPress a to add one.")
            .dark_gray()
            .block(block);
        frame.render_widget(hint, area);
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
            ListItem::new(Line::from(vec![
                Span::raw(marker).green(),
                Span::raw(bucket.name.as_str()),
                Span::raw(format!(" ({})", bucket.open_count())).dark_gray(),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(style::highlight(focused));
    let mut state = ListState::default().with_selected(Some(app.bucket_index));
    frame.render_stateful_widget(list, area, &mut state);
}
