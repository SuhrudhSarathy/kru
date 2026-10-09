//! Terminal UI. Holds only view state (selection, focus, prompts); every
//! change to buckets and tasks goes through the same [`kru::Store`] methods
//! the CLI uses.

mod app;
mod event;
mod ui;

use anyhow::Result;
use kru::Store;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{Event, KeyEventKind, read};

use app::App;

pub fn run(store: Store) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, App::new(store));
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, mut app: App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &app))?;
        if let Event::Key(key) = read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = event::action_for(key, &app.mode)
        {
            app.update(action);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::app::{Action, App, Focus, Mode};
    use super::{event, ui};
    use kru::Store;
    use kru::task::{Priority, Status};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.update(Action::Input(c));
        }
        app.update(Action::Submit);
    }

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 18)).unwrap();
        terminal.draw(|frame| ui::render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Feeds a key through the same mapping the event loop uses.
    fn press(app: &mut App, code: KeyCode) {
        if let Some(action) = event::action_for(KeyEvent::from(code), &app.mode) {
            app.update(action);
        }
    }

    fn task(app: &App, num: usize) -> kru::task::Task {
        app.store.entry(num).unwrap().task.clone()
    }

    #[test]
    fn task_changes_need_confirmation() {
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        assert_eq!(app.focus, Focus::Buckets);

        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::Add);
        type_text(&mut app, "work");
        app.update(Action::FocusTasks);
        for description in ["Buy milk", "Fix bike"] {
            app.update(Action::Add);
            type_text(&mut app, description);
        }
        app.update(Action::Up);

        // Enter does nothing, in normal mode and in a confirmation.
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::Normal));
        press(&mut app, KeyCode::Char(' '));
        assert!(matches!(app.mode, Mode::Confirm(_)));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::Confirm(_)));
        assert_eq!(task(&app, 1).status, Status::ToDo);

        // n cancels, y applies.
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(task(&app, 1).status, Status::ToDo);
        press(&mut app, KeyCode::Char('p'));
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(task(&app, 1).priority, Priority::Medium);

        // A bad due date keeps the prompt open; a good one asks first.
        app.update(Action::EditDue);
        type_text(&mut app, "not a date");
        assert!(app.message.as_ref().unwrap().is_error);
        assert!(matches!(app.mode, Mode::Input { .. }));
        app.update(Action::Cancel);
        app.update(Action::EditDue);
        type_text(&mut app, "tomorrow");
        assert!(task(&app, 1).due.is_none());
        press(&mut app, KeyCode::Char('y'));
        assert!(task(&app, 1).due.is_some());

        // Unknown keys raise an error and change nothing.
        press(&mut app, KeyCode::Char('z'));
        let message = app.message.as_ref().unwrap();
        assert!(message.is_error && message.text.contains('z'));

        // Done tasks move to the bottom pane, which spans every bucket.
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(app.open_tasks().len(), 1);
        assert_eq!(app.done_tasks().len(), 1);
        app.update(Action::FocusBuckets);
        app.update(Action::Down); // select "work"
        let screen_text = screen(&app);
        assert!(
            screen_text.contains("Done · all buckets (1)"),
            "{screen_text}"
        );
        assert!(screen_text.contains("Buy milk"), "{screen_text}");

        // Changes were saved: a fresh store sees them.
        let reloaded = Store::open(&dir).unwrap();
        assert_eq!(reloaded.entry(1).unwrap().task.status, Status::Done);

        // Reopen from the done pane.
        app.update(Action::FocusPrev);
        assert_eq!(app.focus, Focus::Done);
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.done_tasks().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
