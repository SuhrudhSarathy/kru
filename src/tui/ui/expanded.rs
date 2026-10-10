//! The expanded view: one task blown up over most of the screen, with its
//! full description and every field.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use kru::Entry;
use kru::utils::due;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use super::dialog::popup_area;
use super::style;
use super::wrap::wrap_display;
use crate::theme;

pub(super) fn render(frame: &mut Frame, entry: &Entry) {
    let screen = frame.area();
    let width = (screen.width * 4 / 5).max(50).min(screen.width);

    // Wrap the description to the inner width (borders + padding = 4).
    let description = wrap_display(
        &entry.task.description,
        usize::from(width.saturating_sub(4)).max(1),
    );
    let details = details(entry);

    // Description, a separator line, then the fields; borders add 2.
    let content = description.len() + 1 + details.len();
    let height = (content as u16 + 2).min(screen.height);
    let area = popup_area(screen, width, height);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(format!(" Task #{} · {} ", entry.num, entry.bucket.name))
        .border_style(style::border(true))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [description_area, fields_area] = Layout::vertical([
        Constraint::Length(description.len() as u16),
        Constraint::Min(0),
    ])
    .areas(inner);

    let title_style = if entry.task.is_done() {
        Style::new().bold().crossed_out().dark_gray()
    } else {
        Style::new().bold()
    };
    let lines: Vec<Line> = description
        .into_iter()
        .map(|line| Line::styled(line, title_style))
        .collect();
    frame.render_widget(Paragraph::new(lines), description_area);
    frame.render_widget(
        Paragraph::new(details).block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Style::new().dark_gray()),
        ),
        fields_area,
    );
}

/// One `label  value` line per field.
fn details(entry: &Entry) -> Vec<Line<'static>> {
    let task = entry.task;
    let now = Timestamp::now();

    let due = match task.due {
        Some(at) => {
            let text = format!("{}  ({})", due::format(at), relative(at, now));
            if task.is_overdue(now) {
                Span::styled(text, style::fg(theme::OVERDUE))
            } else {
                Span::raw(text)
            }
        }
        None => Span::raw("—").dark_gray(),
    };
    let when = |at: Timestamp| Span::raw(format!("{}  ({})", local(at), relative(at, now)));

    let mut rows = vec![
        (
            "Status",
            Span::styled(task.status.to_string(), style::status(task.status)),
        ),
        (
            "Priority",
            Span::styled(task.priority.to_string(), style::priority(task.priority)),
        ),
        ("Due", due),
        ("Bucket", Span::raw(entry.bucket.name.clone())),
        ("Created", when(task.header.time_created)),
        ("Edited", when(task.header.time_last_edited)),
    ];
    if let Some(completed) = task.time_completed {
        rows.push(("Completed", when(completed)));
    }
    rows.push(("Id", Span::raw(task.header.id.to_string()).dark_gray()));

    rows.into_iter()
        .map(|(label, value)| {
            Line::from(vec![Span::raw(format!("{label:<11}")).dark_gray(), value])
        })
        .collect()
}

/// A timestamp in local time, e.g. `2026-10-10 14:05`.
fn local(at: Timestamp) -> String {
    at.to_zoned(TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

/// How far `at` is from `now`, rounded to the nearest unit,
/// e.g. `in 3 days` or `2 hours ago`.
fn relative(at: Timestamp, now: Timestamp) -> String {
    let secs = at.duration_since(now).as_secs();
    let (unit, size) = match secs.unsigned_abs() {
        s if s < 60 => return "just now".to_string(),
        s if s < 3_570 => ("minute", 60),
        s if s < 84_600 => ("hour", 3_600),
        _ => ("day", 86_400),
    };
    let amount = (secs.unsigned_abs() + size / 2) / size;
    let plural = if amount == 1 { "" } else { "s" };
    if secs > 0 {
        format!("in {amount} {unit}{plural}")
    } else {
        format!("{amount} {unit}{plural} ago")
    }
}

#[cfg(test)]
mod tests {
    use super::relative;
    use jiff::{SignedDuration, Timestamp};

    #[test]
    fn relative_times() {
        let now = Timestamp::now();
        let at = |hours: i64| now + SignedDuration::from_hours(hours);
        assert_eq!(relative(now, now), "just now");
        assert_eq!(relative(at(1), now), "in 1 hour");
        assert_eq!(relative(at(-5), now), "5 hours ago");
        assert_eq!(relative(at(72), now), "in 3 days");
        assert_eq!(relative(at(-24), now), "1 day ago");
        let almost_3_days = now + SignedDuration::from_hours(72) - SignedDuration::from_secs(1);
        assert_eq!(relative(almost_3_days, now), "in 3 days");
    }
}
