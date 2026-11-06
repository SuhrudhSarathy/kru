//! Private buckets (see [`crate::privacy`]): creating them, unlocking them
//! with a password for this session, and locking them again.
//!
//! An unlocked private bucket is an ordinary bucket in the store, so
//! listing, numbering and editing work on it unchanged. Saving re-seals it
//! if it changed. Locked ones are invisible: no name, no tasks.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::Store;
use crate::bucket::Bucket;
use crate::privacy::{self, Kdf, Key};
use crate::utils::error::{Error, Result};

/// A private bucket unlocked in this session.
#[derive(Debug)]
pub(super) struct Unlocked {
    key: Key,
    /// The bucket as it was when last sealed.
    sealed_as: Bucket,
}

impl Store {
    /// Creates a private bucket protected by `password`. It starts unlocked.
    pub fn add_private_bucket(&mut self, name: &str, password: &str) -> Result<&Bucket> {
        let id = self.add_bucket(name)?.header.id;
        let name = self.buckets[&id].name.clone();
        self.make_private_with(&name, password, Kdf::new()?)?;
        Ok(&self.buckets[&id])
    }

    /// Makes an existing bucket private, protected by `password`. It stays
    /// unlocked for this session. Fails if any of its tasks has a notes
    /// file, since those can't be encrypted yet.
    pub fn make_private(&mut self, name: &str, password: &str) -> Result<()> {
        self.make_private_with(name, password, Kdf::new()?)
    }

    pub(crate) fn make_private_with(&mut self, name: &str, password: &str, kdf: Kdf) -> Result<()> {
        let id = self.bucket_id(name)?;
        if self.is_private(id) {
            return Ok(());
        }
        let bucket = self.buckets.get_mut(&id).expect("id comes from the map");
        if bucket.iter().any(|task| task.mdpath.is_some()) {
            return Err(Error::PrivateNotes);
        }
        bucket.header = bucket.header.update_edited();
        let key = Key::derive(password, &kdf)?;
        let sealed = privacy::seal(bucket, &key)?;
        let sealed_as = bucket.clone();
        self.sealed.insert(id, sealed);
        self.unlocked.insert(id, Unlocked { key, sealed_as });
        Ok(())
    }

    /// Tries `password` on every locked private bucket and unlocks those it
    /// opens, for this session. Returns how many it unlocked; fails with
    /// [`Error::WrongPassword`] if there were locked buckets and it opened
    /// none.
    pub fn unlock(&mut self, password: &str) -> Result<usize> {
        let locked: Vec<Uuid> = self
            .sealed
            .keys()
            .filter(|id| !self.unlocked.contains_key(id))
            .copied()
            .collect();
        let mut opened = 0;
        for id in &locked {
            let sealed = &self.sealed[id];
            let key = Key::derive(password, &sealed.sealed.kdf)?;
            match privacy::open(sealed, &key) {
                Ok(bucket) => {
                    self.buckets.insert(*id, bucket.clone());
                    self.unlocked.insert(
                        *id,
                        Unlocked {
                            key,
                            sealed_as: bucket,
                        },
                    );
                    opened += 1;
                }
                Err(Error::WrongPassword) => {}
                Err(err) => return Err(err),
            }
        }
        if opened == 0 && !locked.is_empty() {
            return Err(Error::WrongPassword);
        }
        Ok(opened)
    }

    /// Locks every unlocked private bucket again, keeping its changes
    /// (sealed; written on the next save).
    pub fn lock_all(&mut self) -> Result<()> {
        self.reseal()?;
        for id in std::mem::take(&mut self.unlocked).into_keys() {
            self.buckets.remove(&id);
        }
        Ok(())
    }

    /// Whether the bucket is private (locked or not).
    pub fn is_private(&self, bucket: Uuid) -> bool {
        self.sealed.contains_key(&bucket)
    }

    /// How many private buckets are locked.
    pub fn locked_count(&self) -> usize {
        self.sealed
            .keys()
            .filter(|id| !self.unlocked.contains_key(id))
            .count()
    }

    /// Re-seals unlocked private buckets that changed since they were last
    /// sealed. Called before saving.
    pub(super) fn reseal(&mut self) -> Result<()> {
        for (id, unlocked) in self.unlocked.iter_mut() {
            if let Some(bucket) = self.buckets.get(id)
                && *bucket != unlocked.sealed_as
            {
                self.sealed
                    .insert(*id, privacy::seal(bucket, &unlocked.key)?);
                unlocked.sealed_as = bucket.clone();
            }
        }
        Ok(())
    }

    /// After a save merged in another process's changes, re-opens unlocked
    /// private buckets from their (possibly newer) sealed versions, and
    /// forgets those that were deleted or can no longer be opened.
    pub(super) fn reopen_unlocked(&mut self) {
        let unlocked = std::mem::take(&mut self.unlocked);
        let mut kept = BTreeMap::new();
        for (id, mut entry) in unlocked {
            let reopened = self
                .sealed
                .get(&id)
                .and_then(|sealed| privacy::open(sealed, &entry.key).ok());
            match reopened {
                Some(bucket) => {
                    self.buckets.insert(id, bucket.clone());
                    entry.sealed_as = bucket;
                    kept.insert(id, entry);
                }
                None => {
                    self.buckets.remove(&id);
                }
            }
        }
        self.unlocked = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::tests::cheap_kdf;
    use crate::store::test_util::TempDir;
    use crate::task::Task;

    fn private_store(dir: &TempDir) -> (Store, Uuid) {
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_bucket("diary").unwrap();
        store
            .add_task(Some("diary"), Task::new("a secret"))
            .unwrap();
        store.add_task(Some("home"), Task::new("buy milk")).unwrap();
        store.make_private_with("diary", "pw", cheap_kdf()).unwrap();
        let id = store.bucket("diary").unwrap().header.id;
        store.save().unwrap();
        (store, id)
    }

    #[test]
    fn private_buckets_are_encrypted_on_disk_and_hidden_until_unlocked() {
        let dir = TempDir::new();
        let (_, id) = private_store(&dir);
        let file = std::fs::read_to_string(dir.0.join(format!("buckets/{id}.json"))).unwrap();
        assert!(!file.contains("diary") && !file.contains("a secret"));

        let mut store = Store::open(&dir.0).unwrap();
        assert_eq!(store.buckets().count(), 1, "only home is visible");
        assert_eq!(store.entries().count(), 1);
        assert_eq!(store.locked_count(), 1);
        assert!(store.bucket("diary").is_err());

        assert!(matches!(store.unlock("wrong"), Err(Error::WrongPassword)));
        assert_eq!(store.unlock("pw").unwrap(), 1);
        assert_eq!(store.bucket("diary").unwrap().len(), 1);
        assert_eq!(store.locked_count(), 0);
    }

    #[test]
    fn changes_to_an_unlocked_bucket_are_sealed_on_save() {
        let dir = TempDir::new();
        let (_, _) = private_store(&dir);
        let mut store = Store::open(&dir.0).unwrap();
        store.unlock("pw").unwrap();
        store
            .add_task(Some("diary"), Task::new("another secret"))
            .unwrap();
        store.save().unwrap();

        // Saving again without changes doesn't re-encrypt (no churn in git).
        let before = store.snapshot();
        store.save().unwrap();
        assert_eq!(store.snapshot(), before);

        let mut reopened = Store::open(&dir.0).unwrap();
        reopened.unlock("pw").unwrap();
        assert_eq!(reopened.bucket("diary").unwrap().len(), 2);
    }

    #[test]
    fn locking_keeps_changes_and_hides_the_bucket() {
        let dir = TempDir::new();
        let (mut store, _) = private_store(&dir);
        store
            .add_task(Some("diary"), Task::new("late secret"))
            .unwrap();
        store.lock_all().unwrap();
        assert_eq!(store.buckets().count(), 1);
        store.save().unwrap();

        let mut reopened = Store::open(&dir.0).unwrap();
        reopened.unlock("pw").unwrap();
        assert_eq!(reopened.bucket("diary").unwrap().len(), 2);
    }

    #[test]
    fn a_locked_active_bucket_is_kept_but_refuses_tasks() {
        let dir = TempDir::new();
        let (mut store, id) = private_store(&dir);
        store.set_active_bucket("diary").unwrap();
        store.save().unwrap();

        let mut locked = Store::open(&dir.0).unwrap();
        assert!(matches!(
            locked.add_task(None, Task::new("x")),
            Err(Error::BucketLocked(_))
        ));
        locked.unlock("pw").unwrap();
        assert_eq!(locked.active_bucket().unwrap().header.id, id);
    }

    #[test]
    fn notes_files_are_refused_in_private_buckets() {
        let dir = TempDir::new();
        let (mut store, _) = private_store(&dir);
        let secret = store
            .entries()
            .find(|e| e.task.description == "a secret")
            .unwrap()
            .task
            .header
            .id;
        assert!(matches!(store.attach_md(secret), Err(Error::PrivateNotes)));

        let milk = store
            .entries()
            .find(|e| e.task.description == "buy milk")
            .unwrap()
            .task
            .header
            .id;
        store.attach_md(milk).unwrap();
        assert!(matches!(
            store.move_task(milk, "diary"),
            Err(Error::PrivateNotes)
        ));
        assert!(matches!(
            store.make_private_with("home", "pw", cheap_kdf()),
            Err(Error::PrivateNotes)
        ));
    }

    #[test]
    fn removing_a_private_bucket_forgets_it() {
        let dir = TempDir::new();
        let (mut store, id) = private_store(&dir);
        store.remove_bucket("diary", true).unwrap();
        store.save().unwrap();
        assert!(!dir.0.join(format!("buckets/{id}.json")).exists());
        assert_eq!(Store::open(&dir.0).unwrap().locked_count(), 0);
    }
}
