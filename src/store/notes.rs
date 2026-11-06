//! Markdown notes attached to tasks, for notes too long or rich for the
//! context field (code, lists, links…).
//!
//! A task in a plain bucket keeps its notes at `<store dir>/notes/<task
//! id>.md`. A task in a private bucket keeps them inside the encrypted
//! bucket instead; to edit them, [`Store::begin_notes`] decrypts them into a
//! private temporary file and [`Store::end_notes`] takes them back and
//! deletes it. Either way the task's `mdpath` is set, and the notes go away
//! with the task.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::Store;
use crate::task::Task;
use crate::utils::error::{Error, Result};

/// A task's notes, opened for editing at `path` (see [`Store::begin_notes`]).
#[derive(Debug)]
pub struct NotesEdit {
    pub path: PathBuf,
    task: Uuid,
    /// For private notes: the temporary folder holding the decrypted copy.
    scratch: Option<PathBuf>,
}

impl Store {
    /// The task's notes file on disk, if it has one. Notes of tasks in
    /// private buckets aren't on disk, so they have none.
    pub fn md_path(&self, id: Uuid) -> Result<Option<PathBuf>> {
        let entry = self.find(id)?;
        if self.is_private(entry.bucket.header.id) {
            return Ok(None);
        }
        Ok(entry.task.mdpath.as_ref().map(|path| self.dir.join(path)))
    }

    /// Attaches a markdown file to a task in a plain bucket, creating it
    /// (headed with the task's description) if it doesn't exist yet. Returns
    /// its path and whether it was created. Call [`Store::save`] to keep the
    /// link. Tasks in private buckets have no file; see
    /// [`Store::begin_notes`].
    pub fn attach_md(&mut self, id: Uuid) -> Result<(PathBuf, bool)> {
        if self.is_private(self.find(id)?.bucket.header.id) {
            return Err(Error::PrivateNotes);
        }
        let relative = notes_path(id);
        let path = self.dir.join(&relative);
        let created = !path.exists();
        if created {
            let heading = format!("# {}\n\n", self.find(id)?.task.description);
            write_file(&path, &heading)?;
        }
        self.mark_notes(id)?;
        Ok((path, created))
    }

    /// Opens a task's notes for editing, creating them if needed: the notes
    /// file itself for a plain task; for a task in a private bucket, a
    /// decrypted copy in a new temporary folder only this user can read.
    /// Call [`Store::end_notes`] when the editor is done, then save.
    pub fn begin_notes(&mut self, id: Uuid) -> Result<NotesEdit> {
        let bucket = self.bucket_of(id)?;
        if !self.is_private(bucket) {
            let (path, _) = self.attach_md(id)?;
            return Ok(NotesEdit {
                path,
                task: id,
                scratch: None,
            });
        }
        let text = match self.buckets[&bucket].notes(id) {
            Some(text) => text.to_string(),
            None => format!("# {}\n\n", self.find(id)?.task.description),
        };
        let scratch = private_folder()?;
        let path = scratch.join(format!("{id}.md"));
        if let Err(err) = write_file(&path, &text) {
            let _ = fs::remove_dir_all(&scratch);
            return Err(err);
        }
        self.put_notes(id, text)?;
        Ok(NotesEdit {
            path,
            task: id,
            scratch: Some(scratch),
        })
    }

    /// Finishes editing notes opened with [`Store::begin_notes`]. For a
    /// private task, reads the edited copy back into the encrypted bucket
    /// and deletes the temporary folder (with anything the editor left in
    /// it), even if reading fails.
    pub fn end_notes(&mut self, edit: NotesEdit) -> Result<()> {
        let Some(scratch) = edit.scratch else {
            return Ok(());
        };
        let text = fs::read_to_string(&edit.path).map_err(|source| Error::Io {
            path: edit.path.clone(),
            source,
        });
        let _ = fs::remove_dir_all(&scratch);
        self.put_notes(edit.task, text?)
    }

    /// Deletes a task's notes (the file, or the encrypted copy) and detaches
    /// them. Returns whether there were any.
    pub fn remove_md(&mut self, id: Uuid) -> Result<bool> {
        if self.find(id)?.task.mdpath.is_none() {
            return Ok(false);
        }
        self.take_notes(id)?;
        let task = self.task_mut(id)?;
        task.mdpath = None;
        task.header = task.header.update_edited();
        Ok(true)
    }

    /// Removes a task's notes from wherever they're kept and returns them.
    pub(super) fn take_notes(&mut self, id: Uuid) -> Result<Option<String>> {
        if self.find(id)?.task.mdpath.is_none() {
            return Ok(None);
        }
        let bucket = self.bucket_of(id)?;
        if self.is_private(bucket) {
            let bucket = self.buckets.get_mut(&bucket).expect("found above");
            let text = bucket.notes(id).map(str::to_string);
            bucket.set_notes(id, None);
            return Ok(text);
        }
        let path = self.dir.join(notes_path(id));
        let text = fs::read_to_string(&path).ok();
        let _ = fs::remove_file(&path);
        Ok(text)
    }

    /// Stores a task's notes where its bucket keeps them: encrypted inside
    /// a private bucket, or in the notes file.
    pub(super) fn put_notes(&mut self, id: Uuid, text: String) -> Result<()> {
        let bucket = self.bucket_of(id)?;
        if self.is_private(bucket) {
            let bucket = self.buckets.get_mut(&bucket).expect("found above");
            if bucket.notes(id) != Some(text.as_str()) {
                bucket.set_notes(id, Some(text));
            }
        } else {
            write_file(&self.dir.join(notes_path(id)), &text)?;
        }
        self.mark_notes(id)
    }

    /// Reads the notes files of a bucket's tasks: task id → (path, text).
    pub(super) fn read_notes_files(
        &self,
        bucket: Uuid,
    ) -> Result<BTreeMap<Uuid, (PathBuf, String)>> {
        let mut files = BTreeMap::new();
        for task in self.buckets[&bucket].iter() {
            if task.mdpath.is_some() {
                let path = self.dir.join(notes_path(task.header.id));
                if let Ok(text) = fs::read_to_string(&path) {
                    files.insert(task.header.id, (path, text));
                }
            }
        }
        Ok(files)
    }

    /// Records on the task that it has notes.
    fn mark_notes(&mut self, id: Uuid) -> Result<()> {
        let relative = notes_path(id);
        let task = self.task_mut(id)?;
        if task.mdpath.as_deref() != Some(relative.as_str()) {
            task.mdpath = Some(relative);
            task.header = task.header.update_edited();
        }
        Ok(())
    }

    /// Deletes the markdown file of a task that is being removed. Best
    /// effort: a file that can't be deleted doesn't stop the removal.
    /// (Private notes go with the bucket's copy of the task.)
    pub(super) fn delete_md_of(&self, task: &Task) {
        if let Some(path) = &task.mdpath {
            let _ = fs::remove_file(self.dir.join(path));
        }
    }
}

/// Where a task's notes file lives, relative to the store directory.
fn notes_path(task: Uuid) -> String {
    format!("notes/{task}.md")
}

fn write_file(path: &Path, text: &str) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(io)?;
    }
    fs::write(path, text).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// A new, empty folder in the temp directory that only this user can open,
/// for decrypted notes while they're edited.
fn private_folder() -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("kru-notes-{}", Uuid::now_v7()));
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    builder.create(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_util::TempDir;

    #[test]
    fn markdown_files_attach_and_go_with_their_task() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        let id = store.add_task(None, Task::new("Fix the sink")).unwrap();
        assert_eq!(store.md_path(id).unwrap(), None);

        let (path, created) = store.attach_md(id).unwrap();
        assert!(created);
        assert_eq!(path, dir.0.join(format!("notes/{id}.md")));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Fix the sink\n\n");
        assert_eq!(store.md_path(id).unwrap(), Some(path.clone()));

        // Attaching again keeps what was written.
        fs::write(&path, "my notes").unwrap();
        assert!(!store.attach_md(id).unwrap().1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "my notes");

        // The link survives a reload; removing the task deletes the file.
        store.save().unwrap();
        let mut store = Store::open(&dir.0).unwrap();
        assert_eq!(store.md_path(id).unwrap(), Some(path.clone()));
        store.remove_task(id).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn markdown_files_can_be_removed() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        let id = store.add_task(None, Task::new("a")).unwrap();
        let (path, _) = store.attach_md(id).unwrap();

        assert!(store.remove_md(id).unwrap());
        assert!(!path.exists());
        assert_eq!(store.md_path(id).unwrap(), None);
        assert!(!store.remove_md(id).unwrap());
    }
}
