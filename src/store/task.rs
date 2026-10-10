use uuid::Uuid;

use super::{Entry, Store};
use crate::task::{Task, TaskEdit};
use crate::utils::error::{Error, Result};

impl Store {
    /// All tasks with their numbers. See [`Entry`] for the ordering.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.buckets
            .values()
            .flat_map(|bucket| bucket.iter().map(move |task| (bucket, task)))
            .enumerate()
            .map(|(i, (bucket, task))| Entry {
                num: i + 1,
                bucket,
                task,
            })
    }

    /// Looks up a task by its number.
    pub fn entry(&self, num: usize) -> Result<Entry<'_>> {
        num.checked_sub(1)
            .and_then(|i| self.entries().nth(i))
            .ok_or(Error::TaskNumberNotFound(num))
    }

    /// Looks up a task by id.
    pub fn find(&self, id: Uuid) -> Result<Entry<'_>> {
        self.entries()
            .find(|entry| entry.task.header.id == id)
            .ok_or(Error::TaskNotFound(id))
    }

    /// Resolves a task number to the task's id.
    /// Fails if task with num does not exist
    pub fn id_of(&self, num: usize) -> Result<Uuid> {
        Ok(self.entry(num)?.task.header.id)
    }

    /// Adds a task to the named bucket, or to the active bucket if `None`.
    /// Fails if the bucket does not exist.
    pub fn add_task(&mut self, bucket: Option<&str>, task: Task) -> Result<Uuid> {
        if task.description.trim().is_empty() {
            return Err(Error::EmptyDescription);
        }
        let bucket_id = match bucket {
            Some(name) => self.bucket_id(name)?,
            None => self.state.active.ok_or(Error::NoActiveBucket)?,
        };
        let bucket = self.buckets.get_mut(&bucket_id).expect("id is valid");
        Ok(bucket.add(task))
    }

    /// Applies a partial edit to a task. Rejects an empty description.
    pub fn edit_task(&mut self, id: Uuid, edit: TaskEdit) -> Result<()> {
        if edit
            .description
            .as_deref()
            .is_some_and(|d| d.trim().is_empty())
        {
            return Err(Error::EmptyDescription);
        }
        self.task_mut(id)?.apply(edit);
        Ok(())
    }

    /// Moves a task into another bucket. This renumbers tasks.
    pub fn move_task(&mut self, id: Uuid, to_bucket: &str) -> Result<()> {
        let to = self.bucket_id(to_bucket)?;
        let from = self.bucket_of(id)?;
        if from != to {
            let task = self.buckets.get_mut(&from).unwrap().remove(id).unwrap();
            self.buckets.get_mut(&to).unwrap().add(task);
        }
        Ok(())
    }

    /// Deletes a task and returns it. This renumbers tasks.
    pub fn remove_task(&mut self, id: Uuid) -> Result<Task> {
        let from = self.bucket_of(id)?;
        Ok(self.buckets.get_mut(&from).unwrap().remove(id).unwrap())
    }

    /// Id of the bucket containing the task.
    fn bucket_of(&self, task: Uuid) -> Result<Uuid> {
        self.buckets
            .values()
            .find(|bucket| bucket.get(task).is_some())
            .map(|bucket| bucket.header.id)
            .ok_or(Error::TaskNotFound(task))
    }

    /// Finds a task in any bucket for editing in place.
    fn task_mut(&mut self, id: Uuid) -> Result<&mut Task> {
        self.buckets
            .values_mut()
            .find_map(|bucket| bucket.get_mut(id))
            .ok_or(Error::TaskNotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_util::TempDir;
    use crate::task::{Priority, Status};

    #[test]
    fn tasks_need_an_existing_bucket() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();

        assert!(matches!(
            store.add_task(None, Task::new("a")),
            Err(Error::NoActiveBucket)
        ));
        assert!(matches!(
            store.add_task(Some("home"), Task::new("a")),
            Err(Error::BucketNotFound(_))
        ));

        store.add_bucket("Home").unwrap();
        store.add_task(Some("home"), Task::new("a")).unwrap();
        store.add_task(None, Task::new("b")).unwrap(); // first bucket is active
        assert_eq!(store.bucket("HOME").unwrap().len(), 2);
    }

    #[test]
    fn numbering_spans_buckets_and_survives_reload() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        store.add_task(Some("home"), Task::new("h1")).unwrap();
        store.add_task(Some("work"), Task::new("w1")).unwrap();
        store.add_task(Some("home"), Task::new("h2")).unwrap();
        store.save().unwrap();

        let store = Store::open(&dir.0).unwrap();
        let listed: Vec<_> = store
            .entries()
            .map(|e| (e.num, e.bucket.name.as_str(), e.task.description.as_str()))
            .collect();
        assert_eq!(
            listed,
            [(1, "home", "h1"), (2, "home", "h2"), (3, "work", "w1")]
        );
        assert_eq!(store.active_bucket().unwrap().name, "home");
        assert!(matches!(store.entry(0), Err(Error::TaskNumberNotFound(0))));
        assert!(matches!(store.entry(4), Err(Error::TaskNumberNotFound(4))));
    }

    #[test]
    fn edit_move_and_remove() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        let id = store.add_task(None, Task::new("a")).unwrap();

        store
            .edit_task(
                id,
                TaskEdit {
                    priority: Some(Priority::High),
                    status: Some(Status::Done),
                    ..TaskEdit::default()
                },
            )
            .unwrap();
        let task = store.find(id).unwrap().task;
        assert_eq!(task.priority, Priority::High);
        assert!(task.time_completed.is_some());

        store.move_task(id, "work").unwrap();
        assert_eq!(store.find(id).unwrap().bucket.name, "work");

        store.remove_task(id).unwrap();
        assert!(store.find(id).is_err());
    }
}
