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

/// Opens the TUI and blocks until the user quits.
/// The terminal is restored even if the loop returns an error.
pub fn run(store: Store) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, App::new(store));
    ratatui::restore();
    result
}

/// Draws, waits for a key, applies the matching action; repeats until quit.
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
        // s opens the editor on Status; l changes it in place, and Esc asks
        // to save.
        press(&mut app, KeyCode::Char('s'));
        assert!(matches!(&app.mode, Mode::Edit(editor) if editor.cursor == "status"));
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(task(&app, 1).status, Status::ToDo);
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Confirm(_)));
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(task(&app, 1).status, Status::InProgress);

        // Priority is the exception: it changes without asking.
        press(&mut app, KeyCode::Char('p'));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(task(&app, 1).priority, Priority::Medium);

        // d starts typing a due date; a bad one is refused and typing goes
        // on, a good one is saved after Esc and y.
        app.update(Action::EditDue);
        type_text(&mut app, "not a date");
        assert!(app.message.as_ref().unwrap().is_error);
        assert!(matches!(&app.mode, Mode::Edit(editor) if editor.typing.is_some()));
        for _ in 0.."not a date".len() {
            app.update(Action::Backspace);
        }
        type_text(&mut app, "tomorrow");
        assert!(task(&app, 1).due.is_none());
        save(&mut app);
        assert!(task(&app, 1).due.is_some());
        assert!(
            app.expanded.is_none(),
            "opened from the list, so closed after"
        );

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

    #[test]
    fn open_tasks_are_sorted_and_selection_follows_edits() {
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::FocusTasks);
        for description in ["Water plants", "Pay rent"] {
            app.update(Action::Add);
            type_text(&mut app, description);
        }
        let order = |app: &App| -> Vec<String> {
            let tasks = app.open_tasks();
            tasks.iter().map(|e| e.task.description.clone()).collect()
        };
        assert_eq!(order(&app), ["Water plants", "Pay rent"]);

        // Raising the second task's priority moves it to the top, and the
        // selection moves with it.
        assert_eq!(app.task_index, 1);
        press(&mut app, KeyCode::Char('p'));
        assert_eq!(order(&app), ["Pay rent", "Water plants"]);
        assert_eq!(app.selected_task().unwrap().task.description, "Pay rent");

        // A due date beats priority.
        press(&mut app, KeyCode::Char('j'));
        app.update(Action::EditDue);
        type_text(&mut app, "tomorrow");
        save(&mut app);
        assert_eq!(order(&app), ["Water plants", "Pay rent"]);
        assert_eq!(
            app.selected_task().unwrap().task.description,
            "Water plants"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Esc to finish editing, then y to save.
    fn save(app: &mut App) {
        press(app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Confirm(_)), "{:?}", app.mode);
        press(app, KeyCode::Char('y'));
    }

    #[test]
    fn e_edits_fields_in_the_expanded_view() {
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::FocusTasks);
        app.update(Action::Add);
        type_text(&mut app, "Buy milk");

        // e opens the task in edit mode, on the first editable field.
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(&app.mode, Mode::Edit(editor) if editor.cursor == "description"));
        let screen_text = screen(&app);
        assert!(screen_text.contains("Editing task #1"), "{screen_text}");
        assert!(screen_text.contains("Created"), "{screen_text}");

        // Text is typed in place, starting from the current value.
        press(&mut app, KeyCode::Enter);
        for _ in 0.."milk".len() {
            app.update(Action::Backspace);
        }
        type_text(&mut app, "oat   milk");

        // Choices change in place with h/l; going back and forth is fine.
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Char('h'));
        let staged: Vec<_> = app
            .draft_changes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            staged,
            [
                "description: Buy milk → Buy oat milk",
                "priority: low → medium"
            ]
        );
        let screen_text = screen(&app);
        assert!(screen_text.contains("(was low)"), "{screen_text}");

        // Nothing is saved until Esc and y, and then all at once.
        assert_eq!(task(&app, 1).description, "Buy milk");
        save(&mut app);
        assert_eq!(task(&app, 1).description, "Buy oat milk");
        assert_eq!(task(&app, 1).priority, Priority::Medium);
        assert!(app.expanded.is_none());

        // Finishing without changes just closes; n discards changes.
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Normal) && app.expanded.is_none());
        press(&mut app, KeyCode::Char('s'));
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(task(&app, 1).status, Status::ToDo);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expanded_view_and_remove_key() {
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::FocusTasks);
        for description in ["Call the plumber about the kitchen tap", "Fix bike"] {
            app.update(Action::Add);
            type_text(&mut app, description);
        }
        app.update(Action::Up);

        // x opens the blown-up view of the selected task.
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.expanded_task().unwrap().num, 1);
        let screen_text = screen(&app);
        assert!(screen_text.contains("Task #1 · home"), "{screen_text}");
        assert!(screen_text.contains("Created"), "{screen_text}");

        // Editing from the expanded view keeps it open afterwards; j/k
        // then move the view along.
        press(&mut app, KeyCode::Char('s'));
        press(&mut app, KeyCode::Char('l'));
        save(&mut app);
        assert_eq!(task(&app, 1).status, Status::InProgress);
        assert_eq!(app.expanded_task().unwrap().num, 1);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.expanded_task().unwrap().num, 2);

        // Esc closes the view first, and only quits when nothing is open.
        press(&mut app, KeyCode::Esc);
        assert!(app.expanded.is_none() && !app.should_quit);

        // r removes (after confirming); x no longer does.
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(app.store.entries().count(), 1);

        press(&mut app, KeyCode::Esc);
        assert!(app.should_quit);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
