//! Markdown files attached to tasks, for notes too long or rich for the
//! context field (code, lists, links…). Each lives at
//! `<store dir>/notes/<task id>.md` and goes away with its task.

use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use super::Store;
use crate::task::Task;
use crate::utils::error::{Error, Result};

impl Store {
    /// The task's markdown file, if one is attached.
    pub fn md_path(&self, id: Uuid) -> Result<Option<PathBuf>> {
        let task = self.find(id)?.task;
        Ok(task.mdpath.as_ref().map(|path| self.dir.join(path)))
    }

    /// Attaches a markdown file to a task, creating it (headed with the
    /// task's description) if it doesn't exist yet. Returns its path and
    /// whether it was created. Call [`Store::save`] to keep the link.
    pub fn attach_md(&mut self, id: Uuid) -> Result<(PathBuf, bool)> {
        let relative = format!("notes/{id}.md");
        let path = self.dir.join(&relative);
        let created = !path.exists();
        if created {
            let folder = self.dir.join("notes");
            fs::create_dir_all(&folder).map_err(|source| Error::Io {
                path: folder,
                source,
            })?;
            let heading = format!("# {}\n\n", self.find(id)?.task.description);
            fs::write(&path, heading).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        }
        let task = self.task_mut(id)?;
        if task.mdpath.as_deref() != Some(relative.as_str()) {
            task.mdpath = Some(relative);
            task.header = task.header.update_edited();
        }
        Ok((path, created))
    }

    /// Deletes a task's markdown file and detaches it. Returns whether
    /// there was one.
    pub fn remove_md(&mut self, id: Uuid) -> Result<bool> {
        let Some(path) = self.md_path(id)? else {
            return Ok(false);
        };
        match fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::Io { path, source: err });
            }
            _ => {}
        }
        let task = self.task_mut(id)?;
        task.mdpath = None;
        task.header = task.header.update_edited();
        Ok(true)
    }

    /// Deletes the markdown file of a task that is being removed. Best
    /// effort: a file that can't be deleted doesn't stop the removal.
    pub(super) fn delete_md_of(&self, task: &Task) {
        if let Some(path) = &task.mdpath {
            let _ = fs::remove_file(self.dir.join(path));
        }
    }
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
