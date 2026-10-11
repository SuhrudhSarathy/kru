//! Editing a task in the expanded view.
//!
//! A cursor moves over the task's editable fields (from
//! [`Task::fields`](kru::task::Task::fields)); choices change in place with
//! h/l, text and times are typed in place. Changes are staged in a draft
//! and saved together, after one confirmation, through
//! [`kru::Store::edit_task`].

use kru::task::{Change, Editing, Field, Task, Value};
use uuid::Uuid;

use super::confirm::Op;
use super::{Action, App, Mode, Result, UiError};

/// Editing one task: a cursor on an editable field, and staged changes.
#[derive(Debug)]
pub struct Editor {
    pub id: Uuid,
    /// Key of the field under the cursor.
    pub cursor: &'static str,
    /// Staged `(field key, input)` pairs, already checked; saved together.
    pub draft: Vec<(&'static str, String)>,
    /// Text being typed into the cursor's field, if any.
    pub typing: Option<String>,
    /// Close the expanded view when done (it was closed before editing).
    close_after: bool,
}

impl App {
    /// The edited task with its staged changes applied, while editing.
    pub fn draft_task(&self) -> Option<Task> {
        let Mode::Edit(editor) = &self.mode else {
            return None;
        };
        let mut task = self.store.find(editor.id).ok()?.task.clone();
        for (key, input) in &editor.draft {
            // Each input was checked when staged.
            let _ = task.set(key, input);
        }
        Some(task)
    }

    /// What the staged changes would change, while editing.
    pub fn draft_changes(&self) -> Vec<Change> {
        let (Mode::Edit(editor), Some(drafted)) = (&self.mode, self.draft_task()) else {
            return Vec::new();
        };
        let Ok(original) = self.store.find(editor.id) else {
            return Vec::new();
        };
        let keys: Vec<&str> = editor.draft.iter().map(|(key, _)| *key).collect();
        drafted.changes_from(original.task, &keys)
    }

    /// Starts editing the target task in the expanded view, with the cursor
    /// on field `key` (or the first editable field). `typing` starts typing
    /// into that field straight away.
    pub(super) fn open_editor(&mut self, key: Option<&str>, typing: bool) -> Result<()> {
        let (id, _) = self.require_selected()?;
        let fields = editable(self.require_task()?);
        let cursor = match key {
            Some(key) => fields.iter().find(|f| f.key == key),
            None => fields.first(),
        }
        .ok_or_else(|| UiError("Nothing to edit".to_string()))?;
        let typing =
            (typing && !matches!(cursor.value, Value::Choice { .. })).then(|| cursor.value.short());

        self.mode = Mode::Edit(Editor {
            id,
            cursor: cursor.key,
            draft: Vec::new(),
            typing,
            close_after: self.expanded.is_none(),
        });
        self.expanded = Some(id);
        Ok(())
    }

    /// Keys while editing: j/k move between fields, h/l change a choice,
    /// Enter types into a text or time field, Esc finishes. While typing,
    /// Enter keeps the text, Alt+Enter adds a line break (text fields only;
    /// single-line fields turn it into a space when saved) and Esc drops
    /// the text.
    pub(super) fn on_edit(&mut self, action: Action) -> Result<()> {
        if action == Action::Newline {
            let field = self.cursor_field()?;
            if !matches!(field.value, Value::Text(_)) {
                return Err(UiError(format!("{} can't have line breaks", field.label)));
            }
            if let Mode::Edit(Editor {
                typing: Some(text), ..
            }) = &mut self.mode
            {
                text.push('\n');
            }
            return Ok(());
        }
        let Mode::Edit(editor) = &mut self.mode else {
            return Ok(());
        };
        if let Some(text) = &mut editor.typing {
            match action {
                Action::Input(c) => text.push(c),
                Action::Backspace => {
                    text.pop();
                }
                Action::Cancel => editor.typing = None,
                Action::Submit => {
                    let (key, input) = (editor.cursor, text.clone());
                    // On invalid input the error shows and typing goes on.
                    self.stage(key, input)?;
                    if let Mode::Edit(editor) = &mut self.mode {
                        editor.typing = None;
                    }
                }
                Action::InvalidKey(key) => {
                    self.error(format!("`{key}` can't be used while typing"))
                }
                _ => {}
            }
            return Ok(());
        }

        match action {
            Action::Up => self.move_cursor(-1),
            Action::Down => self.move_cursor(1),
            Action::Left => self.cycle_draft(-1)?,
            Action::Right => self.cycle_draft(1)?,
            Action::Submit => {
                let field = self.cursor_field()?;
                if matches!(field.value, Value::Choice { .. }) {
                    self.cycle_draft(1)?;
                } else if let Mode::Edit(editor) = &mut self.mode {
                    editor.typing = Some(field.value.short());
                }
            }
            Action::Cancel => self.finish_editing()?,
            Action::InvalidKey(key) => self.error(format!("`{key}` does nothing while editing")),
            _ => {}
        }
        Ok(())
    }

    /// Sets a choice field of the target task to its next option, applied
    /// at once if the field is instant (the `p` shortcut).
    pub(super) fn cycle_field(&mut self, key: &str) -> Result<()> {
        let field = self.require_field(key)?;
        let next = next_option(&field, 1)?;
        self.request_edit(key, &next)
    }

    /// Sets field `key` of the target task from `input`: straight away for
    /// instant fields, otherwise after a y/n confirmation. Invalid input is
    /// rejected before anything is asked.
    pub(super) fn request_edit(&mut self, key: &str, input: &str) -> Result<()> {
        let field = self.require_field(key)?;
        let (id, label) = self.require_selected()?;
        let edits = vec![(field.key, input.to_string())];
        let changes = self.store.preview_edit(id, &edits)?;
        if changes.is_empty() {
            self.info(format!("{label}: nothing changed"));
            return Ok(());
        }
        let summary = summarize(&changes);
        if field.editing == Editing::Instant {
            self.store.edit_task(id, &edits)?;
            self.select_task(id);
            return self.commit(format!("{label}: {summary}"));
        }
        let op = Op::EditTask {
            id,
            edits,
            close: false,
        };
        self.confirm(format!("{label}: {summary}?"), op);
        Ok(())
    }

    /// Checks `input` for field `key` against the drafted task and stages it.
    fn stage(&mut self, key: &'static str, input: String) -> Result<()> {
        let mut drafted = self
            .draft_task()
            .ok_or_else(|| UiError("Not editing".to_string()))?;
        drafted.set(key, &input)?;
        if let Mode::Edit(editor) = &mut self.mode {
            editor.draft.retain(|(staged, _)| *staged != key);
            editor.draft.push((key, input));
        }
        Ok(())
    }

    /// Moves the cursor `by` editable fields, staying within the list.
    fn move_cursor(&mut self, by: isize) {
        let Some(task) = self.draft_task() else {
            return;
        };
        let keys: Vec<&'static str> = editable(&task).iter().map(|f| f.key).collect();
        if let Mode::Edit(editor) = &mut self.mode {
            let at = keys.iter().position(|&k| k == editor.cursor).unwrap_or(0);
            let to = at.saturating_add_signed(by).min(keys.len() - 1);
            editor.cursor = keys[to];
        }
    }

    /// Stages the next (`by` = 1) or previous (-1) option of the choice
    /// field under the cursor.
    fn cycle_draft(&mut self, by: isize) -> Result<()> {
        let field = self.cursor_field()?;
        let next = next_option(&field, by)?;
        self.stage(field.key, next)
    }

    /// The drafted field under the cursor.
    fn cursor_field(&self) -> Result<Field> {
        let (Mode::Edit(editor), Some(task)) = (&self.mode, self.draft_task()) else {
            return Err(UiError("Not editing".to_string()));
        };
        task.fields()
            .into_iter()
            .find(|f| f.key == editor.cursor)
            .ok_or_else(|| kru::Error::UnknownField(editor.cursor.to_string()).into())
    }

    /// Leaves edit mode: asks to save if anything changed, otherwise just
    /// closes.
    fn finish_editing(&mut self) -> Result<()> {
        let changes = self.draft_changes();
        let label = self.require_selected()?.1;
        let Mode::Edit(editor) = std::mem::replace(&mut self.mode, Mode::Normal) else {
            return Ok(());
        };
        if changes.is_empty() {
            if editor.close_after {
                self.expanded = None;
            }
            return Ok(());
        }
        let op = Op::EditTask {
            id: editor.id,
            edits: editor.draft,
            close: editor.close_after,
        };
        self.confirm(format!("Save {label}? {}", summarize(&changes)), op);
        Ok(())
    }

    /// The target task's field `key`, if it exists and is editable.
    fn require_field(&self, key: &str) -> Result<Field> {
        let field = self
            .require_task()?
            .fields()
            .into_iter()
            .find(|f| f.key == key)
            .ok_or_else(|| kru::Error::UnknownField(key.to_string()))?;
        if !field.is_editable() {
            return Err(kru::Error::ReadOnlyField(field.label).into());
        }
        Ok(field)
    }
}

/// The task's editable fields, in display order.
fn editable(task: &Task) -> Vec<Field> {
    task.fields()
        .into_iter()
        .filter(Field::is_editable)
        .collect()
}

/// The option `by` steps from a choice field's value, wrapping around.
fn next_option(field: &Field, by: isize) -> Result<String> {
    let Value::Choice { value, options } = &field.value else {
        return Err(UiError(format!(
            "{} has no options; press enter to type",
            field.label
        )));
    };
    let at = options.iter().position(|o| o == value).unwrap_or(0) as isize;
    let len = options.len() as isize;
    Ok(options[(at + by).rem_euclid(len) as usize].clone())
}

/// `priority: low → high; due: — → Fri 2026-10-16`.
fn summarize(changes: &[Change]) -> String {
    let parts: Vec<String> = changes.iter().map(ToString::to_string).collect();
    parts.join("; ")
}
