use uuid::Uuid;

use super::Store;
use crate::bucket::Bucket;
use crate::utils::error::{Error, Result};

impl Store {
    /// All buckets, in creation order.
    pub fn buckets(&self) -> impl Iterator<Item = &Bucket> {
        self.buckets.values()
    }

    /// Looks up a bucket by name (case-insensitive).
    pub fn bucket(&self, name: &str) -> Result<&Bucket> {
        Ok(&self.buckets[&self.bucket_id(name)?])
    }

    /// The bucket tasks go to when no bucket is named, if one is set.
    pub fn active_bucket(&self) -> Option<&Bucket> {
        self.state.active.and_then(|id| self.buckets.get(&id))
    }

    /// Makes the named bucket the active one.
    pub fn set_active_bucket(&mut self, name: &str) -> Result<()> {
        self.state.active = Some(self.bucket_id(name)?);
        Ok(())
    }

    /// Creates a bucket. The first bucket created becomes the active one.
    pub fn add_bucket(&mut self, name: &str) -> Result<&Bucket> {
        let name = valid_bucket_name(name)?;
        if self.bucket_id(name).is_ok() {
            return Err(Error::BucketExists(name.to_string()));
        }
        let bucket = Bucket::new(name);
        let id = bucket.header.id;
        self.buckets.insert(id, bucket);
        self.state.active.get_or_insert(id);
        Ok(&self.buckets[&id])
    }

    /// Renames a bucket. Fails if another bucket already has the new name.
    pub fn rename_bucket(&mut self, name: &str, new_name: &str) -> Result<()> {
        let id = self.bucket_id(name)?;
        let new_name = valid_bucket_name(new_name)?;
        if self.bucket_id(new_name).is_ok_and(|other| other != id) {
            return Err(Error::BucketExists(new_name.to_string()));
        }
        let bucket = self.buckets.get_mut(&id).expect("id comes from the map");
        bucket.name = new_name.to_string();
        bucket.header = bucket.header.update_edited();
        Ok(())
    }

    /// Removes a bucket and all its tasks. Fails if the bucket still has
    /// open tasks, unless `force` is set.
    pub fn remove_bucket(&mut self, name: &str, force: bool) -> Result<Bucket> {
        let id = self.bucket_id(name)?;
        let bucket = &self.buckets[&id];
        let open = bucket.open_count();
        if open > 0 && !force {
            return Err(Error::BucketNotEmpty {
                name: bucket.name.clone(),
                open,
            });
        }
        if self.state.active == Some(id) {
            self.state.active = None;
        }
        Ok(self.buckets.remove(&id).expect("id comes from the map"))
    }

    /// Id of the bucket with this name (case-insensitive).
    pub(super) fn bucket_id(&self, name: &str) -> Result<Uuid> {
        let wanted = name.trim().to_lowercase();
        self.buckets
            .values()
            .find(|bucket| bucket.name.to_lowercase() == wanted)
            .map(|bucket| bucket.header.id)
            .ok_or_else(|| Error::BucketNotFound(name.trim().to_string()))
    }
}

/// Trims the name and rejects it if empty.
fn valid_bucket_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::EmptyBucketName);
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::io::json_files;
    use crate::store::test_util::TempDir;
    use crate::task::Task;

    #[test]
    fn removing_a_bucket_with_open_tasks_needs_force() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("home").unwrap();
        store.add_task(None, Task::new("a")).unwrap();

        assert!(matches!(
            store.remove_bucket("home", false),
            Err(Error::BucketNotEmpty { open: 1, .. })
        ));
        store.remove_bucket("home", true).unwrap();
        assert!(store.active_bucket().is_none());

        store.save().unwrap();
        assert_eq!(json_files(&dir.0.join("buckets")).unwrap().len(), 0);
    }

    #[test]
    fn bucket_names_are_unique_ignoring_case() {
        let dir = TempDir::new();
        let mut store = Store::open(&dir.0).unwrap();
        store.add_bucket("Home").unwrap();
        store.add_bucket("work").unwrap();

        assert!(matches!(
            store.add_bucket("home"),
            Err(Error::BucketExists(_))
        ));
        assert!(matches!(
            store.rename_bucket("work", "HOME"),
            Err(Error::BucketExists(_))
        ));
        assert!(matches!(
            store.add_bucket("  "),
            Err(Error::EmptyBucketName)
        ));
        store.rename_bucket("home", "house").unwrap();
        assert_eq!(store.bucket("HOUSE").unwrap().name, "house");
    }
}
