//! Terminal UI. Holds only view state (selection, focus, prompts); every
//! change to buckets and tasks goes through the same [`kru::Store`] methods
//! the CLI uses.

mod app;
mod event;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use kru::Store;
use kru::config::Config;
use kru::sync::{self, Report};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{Event, KeyEventKind, poll, read};

use app::App;

/// How often the TUI syncs in the background when `autosync` is on.
const AUTOSYNC_EVERY: Duration = Duration::from_secs(5 * 60);

/// How long to wait for a key before checking on background work.
const TICK: Duration = Duration::from_millis(250);

/// Opens the TUI and blocks until the user quits.
/// The terminal is restored even if the loop returns an error.
///
/// If the store syncs with other devices, it syncs when the TUI opens and
/// when it closes, on `S`, and every few minutes with `autosync` on.
pub fn run(store: Store) -> Result<()> {
    let config = Config::load()?;
    let editor = config.editor();
    let dir = store.dir().to_path_buf();
    let syncs = sync::is_set_up(&dir);

    let mut app = if syncs {
        eprint!("Syncing… ");
        let (message, failed) = describe(sync::run(&dir, false));
        eprintln!("{message}");
        let mut app = App::new(Store::open(&dir)?);
        app.notify(message, failed);
        app
    } else {
        App::new(store)
    };
    let background = (syncs && config.autosync()).then(|| autosync(dir.clone()));

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, &editor, &dir, background.as_ref());
    ratatui::restore();

    if syncs {
        eprint!("Syncing… ");
        eprintln!("{}", describe(sync::run(&dir, false)).0);
    }
    result
}

/// Draws, waits briefly for a key, applies the matching action; repeats
/// until quit. In between, it hands the terminal to the editor when a file
/// should be opened, and syncs when asked or when a background sync is done.
fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    editor: &str,
    dir: &Path,
    background: Option<&Receiver<kru::Result<Report>>>,
) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;
        if poll(TICK)?
            && let Event::Key(key) = read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = event::action_for(key, &app.mode)
        {
            app.update(action);
        }
        if let Some(path) = app.open_file.take() {
            ratatui::restore();
            let result = crate::editor::open(editor, &path).map_err(|err| format!("{err:#}"));
            *terminal = ratatui::init();
            app.editor_closed(result);
        }
        if std::mem::take(&mut app.sync_requested) {
            if !sync::is_set_up(dir) {
                app.notify("Sync isn't set up: run `kru sync init <remote-url>`", true);
                continue;
            }
            app.notify("Syncing…", false);
            terminal.draw(|frame| ui::render(frame, app))?;
            let (message, failed) = describe(sync::run(dir, false));
            app.reload();
            app.notify(message, failed);
        }
        if let Some(result) = background.and_then(|rx| rx.try_recv().ok()) {
            let (message, failed) = describe(result);
            app.reload();
            app.notify(message, failed);
        }
    }
    Ok(())
}

/// Syncs every [`AUTOSYNC_EVERY`] on a background thread; results arrive on
/// the returned channel. The thread stops when the channel is dropped.
fn autosync(dir: PathBuf) -> Receiver<kru::Result<Report>> {
    let (results, receiver) = mpsc::channel();
    thread::spawn(move || {
        loop {
            thread::sleep(AUTOSYNC_EVERY);
            if results.send(sync::run(&dir, false)).is_err() {
                break;
            }
        }
    });
    receiver
}

/// A one-line summary of a sync for the footer, and whether it failed.
fn describe(result: kru::Result<Report>) -> (String, bool) {
    match result {
        Ok(report) => {
            let mut message = if report.pulled == 0 && report.pushed == 0 {
                "Already in sync".to_string()
            } else {
                format!("Synced: pulled {}, pushed {}", report.pulled, report.pushed)
            };
            if !report.conflicts.is_empty() {
                message.push_str(&format!(
                    "; resolved {} conflict(s)",
                    report.conflicts.len()
                ));
            }
            (message, false)
        }
        Err(err) => (format!("Sync failed: {err}"), true),
    }
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
        press(&mut app, KeyCode::Char('j')); // Context
        press(&mut app, KeyCode::Char('j')); // Priority
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
    fn alt_enter_adds_line_breaks_while_typing() {
        use ratatui::crossterm::event::KeyModifiers;
        let alt_enter = |app: &mut App| {
            let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
            if let Some(action) = event::action_for(key, &app.mode) {
                app.update(action);
            }
        };
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::FocusTasks);
        app.update(Action::Add);
        type_text(&mut app, "Fix the sink");

        // Context: Alt+Enter breaks the line, Enter keeps the text.
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('j')); // Context
        press(&mut app, KeyCode::Enter);
        for c in "Leak under the left tap".chars() {
            app.update(Action::Input(c));
        }
        alt_enter(&mut app);
        assert!(matches!(&app.mode, Mode::Edit(editor) if editor.typing.is_some()));
        type_text(&mut app, "Plumber: 555-0100");
        save(&mut app);
        assert_eq!(
            task(&app, 1).ctx,
            "Leak under the left tap\nPlumber: 555-0100"
        );

        // Dates can't hold line breaks.
        app.update(Action::EditDue);
        alt_enter(&mut app);
        let message = app.message.as_ref().unwrap();
        assert!(message.is_error && message.text.contains("line breaks"));
        app.update(Action::Cancel);
        press(&mut app, KeyCode::Esc);

        // In a prompt, Alt+Enter explains itself instead of submitting.
        app.update(Action::Add);
        for c in "Half".chars() {
            app.update(Action::Input(c));
        }
        alt_enter(&mut app);
        assert!(matches!(&app.mode, Mode::Input { buffer, .. } if buffer == "Half"));
        assert!(app.message.as_ref().unwrap().is_error);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_attaches_and_opens_the_markdown_file() {
        let dir = std::env::temp_dir().join(format!("kru-tui-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(Store::open(&dir).unwrap());
        app.update(Action::Add);
        type_text(&mut app, "home");
        app.update(Action::FocusTasks);
        app.update(Action::Add);
        type_text(&mut app, "Fix the sink");

        // o creates the file and asks the event loop to open it.
        press(&mut app, KeyCode::Char('o'));
        let path = app.open_file.take().expect("a file to open");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Fix the sink\n\n"
        );
        let id = task(&app, 1).header.id;
        assert_eq!(task(&app, 1).mdpath, Some(format!("notes/{id}.md")));

        // The expanded view shows where it is.
        press(&mut app, KeyCode::Char('x'));
        let screen_text = screen(&app);
        assert!(screen_text.contains("Markdown"), "{screen_text}");

        app.editor_closed(Err("the editor `nope` exited".into()));
        assert!(app.message.as_ref().unwrap().is_error);

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
