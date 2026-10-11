use uuid::Uuid;

use super::{Entry, Store};
use crate::task::{Change, Task};
use crate::utils::error::{Error, Result};
use crate::utils::sanitize;

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

    /// Resolves a task reference typed by a user or agent: a task number
    /// (`3`), a full id, or the start of an id (`01a125dc-bcef`),
    /// case-insensitive. A reference of only digits is always a number.
    /// Fails if nothing matches or an id prefix matches several tasks.
    ///
    /// Numbers can shift as tasks are added, moved or removed; ids never
    /// change.
    pub fn resolve(&self, reference: &str) -> Result<Uuid> {
        let reference = reference.trim();
        if let Ok(num) = reference.parse::<usize>() {
            return self.id_of(num);
        }
        let prefix = reference.to_lowercase();
        let mut matches = self
            .entries()
            .map(|entry| entry.task.header.id)
            .filter(|id| !prefix.is_empty() && id.to_string().starts_with(&prefix));
        match (matches.next(), matches.count()) {
            (Some(id), 0) => Ok(id),
            (Some(_), others) => Err(Error::AmbiguousTask {
                reference: reference.to_string(),
                count: others + 1,
            }),
            (None, _) => Err(Error::NoTaskMatches(reference.to_string())),
        }
    }

    /// Adds a task to the named bucket, or to the active bucket if `None`.
    /// Fails if the bucket does not exist or the description is empty once
    /// cleaned.
    pub fn add_task(&mut self, bucket: Option<&str>, mut task: Task) -> Result<Uuid> {
        task.description = sanitize::text(&task.description, "description")?;
        let bucket_id = match bucket {
            Some(name) => self.bucket_id(name)?,
            None => self.state.active.ok_or(Error::NoActiveBucket)?,
        };
        let bucket = self.buckets.get_mut(&bucket_id).expect("id is valid");
        Ok(bucket.add(task))
    }

    /// Edits a task's fields from user input, given as `(field key, input)`
    /// pairs such as `("due", "fri")` (see [`Task::set`]).
    ///
    /// All or nothing: if any input is invalid, nothing changes. Returns the
    /// fields that actually changed; if none did, the task is left alone
    /// (its edited time included).
    pub fn edit_task<K, V>(&mut self, id: Uuid, edits: &[(K, V)]) -> Result<Vec<Change>>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let (mut edited, changes) = self.edited(id, edits)?;
        if !changes.is_empty() {
            edited.header = edited.header.update_edited();
            *self.task_mut(id)? = edited;
        }
        Ok(changes)
    }

    /// What [`Store::edit_task`] would change, without changing anything.
    /// Fails the same way on invalid input.
    pub fn preview_edit<K, V>(&self, id: Uuid, edits: &[(K, V)]) -> Result<Vec<Change>>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        Ok(self.edited(id, edits)?.1)
    }

    /// A copy of the task with `edits` applied, and what changed.
    fn edited<K, V>(&self, id: Uuid, edits: &[(K, V)]) -> Result<(Task, Vec<Change>)>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let original = self.find(id)?.task;
        let mut edited = original.clone();
        for (key, input) in edits {
            edited.set(key.as_ref(), input.as_ref())?;
        }
        let keys: Vec<&str> = edits.iter().map(|(key, _)| key.as_ref()).collect();
        let changes = edited.changes_from(original, &keys);
        Ok((edited, changes))
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
        let task = self.buckets.get_mut(&from).unwrap().remove(id).unwrap();
        self.delete_md_of(&task);
        self.tombstones.tasks.insert(id, jiff::Timestamp::now());
        Ok(task)
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
    pub(super) fn task_mut(&mut self, id: Uuid) -> Result<&mut Task> {
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
    use crate::task::Priority;

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
    fn tasks_resolve_by_number_or_id() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        let with_id = |description: &str, id: &str| {
            let mut task = Task::new(description);
            task.header.id = id.parse().unwrap();
            task
        };
        let first = "01a125dc-bcef-7784-8fc5-000000000001";
        let second = "01a125dc-bcef-7784-8fc5-100000000002";
        store.add_task(None, with_id("a", first)).unwrap();
        store.add_task(None, with_id("b", second)).unwrap();
        let id = |s: &str| s.parse::<Uuid>().unwrap();

        assert_eq!(store.resolve("2").unwrap(), id(second));
        assert_eq!(store.resolve(second).unwrap(), id(second));
        assert_eq!(store.resolve(&second.to_uppercase()).unwrap(), id(second));
        assert_eq!(
            store.resolve(" 01a125dc-bcef-7784-8fc5-1 ").unwrap(),
            id(second)
        );

        assert!(matches!(
            store.resolve("01a125dc"),
            Err(Error::AmbiguousTask { count: 2, .. })
        ));
        assert!(matches!(
            store.resolve("3"),
            Err(Error::TaskNumberNotFound(3))
        ));
        assert!(matches!(store.resolve("zz"), Err(Error::NoTaskMatches(_))));
        assert!(matches!(store.resolve(""), Err(Error::NoTaskMatches(_))));
    }

    #[test]
    fn edit_move_and_remove() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("work").unwrap();
        let id = store.add_task(None, Task::new("a")).unwrap();

        let changes = store
            .edit_task(id, &[("priority", "high"), ("status", "done")])
            .unwrap();
        assert_eq!(changes.len(), 2);
        let task = store.find(id).unwrap().task;
        assert_eq!(task.priority, Priority::High);
        assert!(task.time_completed.is_some());

        store.move_task(id, "work").unwrap();
        assert_eq!(store.find(id).unwrap().bucket.name, "work");

        store.remove_task(id).unwrap();
        assert!(store.find(id).is_err());
    }

    #[test]
    fn edits_are_all_or_nothing_and_skip_no_ops() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        let id = store.add_task(None, Task::new("  water\tplants ")).unwrap();
        assert_eq!(store.find(id).unwrap().task.description, "water plants");

        // A bad input anywhere means nothing changes.
        let result = store.edit_task(id, &[("priority", "high"), ("due", "someday")]);
        assert!(matches!(result, Err(Error::InvalidDue(_))));
        assert_eq!(store.find(id).unwrap().task.priority, Priority::Low);

        // Setting a value it already has changes nothing, not even the edited time.
        let edited = store.find(id).unwrap().task.header.time_last_edited;
        assert!(
            store
                .edit_task(id, &[("priority", "low")])
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.find(id).unwrap().task.header.time_last_edited, edited);

        // A preview reports the change without making it.
        let preview = store.preview_edit(id, &[("priority", "h")]).unwrap();
        assert_eq!(preview[0].to_string(), "priority: low → high");
        assert_eq!(store.find(id).unwrap().task.priority, Priority::Low);

        assert!(matches!(
            store.add_task(None, Task::new(" \n ")),
            Err(Error::EmptyField("description"))
        ));
    }
}
