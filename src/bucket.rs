use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::task::{Status, Task};
use crate::utils::header::Header;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bucket {
    pub header: Header,
    pub name: String,
    tasks: BTreeMap<Uuid, Task>,
}

impl Bucket {
    /// Creates an empty bucket with a fresh id.
    pub fn new(name: impl Into<String>) -> Bucket {
        Bucket {
            header: Header::new(),
            name: name.into(),
            tasks: BTreeMap::new(),
        }
    }

    /// Inserts a task and returns its id.
    pub fn add(&mut self, task: Task) -> Uuid {
        let id = task.header.id;
        self.tasks.insert(id, task);
        id
    }

    /// Looks up a task by id.
    pub fn get(&self, id: Uuid) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Looks up a task by id for editing in place.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// Removes a task by id, returning it if it existed.
    pub fn remove(&mut self, id: Uuid) -> Option<Task> {
        self.tasks.remove(&id)
    }

    /// Keeps only the tasks matching `keep`; returns how many were removed.
    pub fn retain(&mut self, mut keep: impl FnMut(&Task) -> bool) -> usize {
        let before = self.tasks.len();
        self.tasks.retain(|_, task| keep(task));
        before - self.tasks.len()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Number of tasks that are not done.
    pub fn open_count(&self) -> usize {
        self.iter().filter(|t| !t.is_done()).count()
    }

    /// All tasks, in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    /// Tasks whose description contains `query` (case-insensitive).
    pub fn search(&self, query: &str) -> impl Iterator<Item = &Task> {
        let query = query.to_lowercase();
        self.iter()
            .filter(move |t| t.description.to_lowercase().contains(&query))
    }

    /// Tasks with the given status.
    pub fn with_status(&self, status: Status) -> impl Iterator<Item = &Task> {
        self.iter().filter(move |t| t.status == status)
    }
}
