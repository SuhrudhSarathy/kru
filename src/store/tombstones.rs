//! Tombstones: when each deleted task and bucket was deleted. Syncing needs
//! them to tell "deleted here" from "never existed here", and to let a
//! newer edit on another device win over an older delete.
//!
//! Kept in `tombstones.json`; a missing file means there are none.

use std::collections::BTreeMap;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How long a tombstone is kept. A device that hasn't synced for longer
/// may bring a task deleted elsewhere back.
pub const TOMBSTONE_RETENTION: SignedDuration = SignedDuration::from_hours(90 * 24);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstones {
    #[serde(default)]
    pub tasks: BTreeMap<Uuid, Timestamp>,
    #[serde(default)]
    pub buckets: BTreeMap<Uuid, Timestamp>,
}

impl Tombstones {
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.buckets.is_empty()
    }

    /// Adds `other`'s tombstones, keeping the later time where both have one.
    pub fn absorb(&mut self, other: &Tombstones) {
        for (mine, theirs) in [
            (&mut self.tasks, &other.tasks),
            (&mut self.buckets, &other.buckets),
        ] {
            for (&id, &at) in theirs {
                let kept = mine.entry(id).or_insert(at);
                *kept = (*kept).max(at);
            }
        }
    }

    /// Drops tombstones older than `retention`. Returns whether any were.
    pub fn expire(&mut self, retention: SignedDuration) -> bool {
        let cutoff = Timestamp::now()
            .checked_sub(retention)
            .unwrap_or(Timestamp::MIN);
        let before = self.tasks.len() + self.buckets.len();
        self.tasks.retain(|_, at| *at >= cutoff);
        self.buckets.retain(|_, at| *at >= cutoff);
        self.tasks.len() + self.buckets.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorb_keeps_the_latest_time() {
        let id = Uuid::now_v7();
        let early = Timestamp::from_second(1_000).unwrap();
        let late = Timestamp::from_second(2_000).unwrap();
        let mut mine = Tombstones::default();
        mine.tasks.insert(id, late);
        let mut theirs = Tombstones::default();
        theirs.tasks.insert(id, early);
        theirs.buckets.insert(id, early);
        mine.absorb(&theirs);
        assert_eq!(mine.tasks[&id], late);
        assert_eq!(mine.buckets[&id], early);
    }

    #[test]
    fn old_tombstones_expire() {
        let mut stones = Tombstones::default();
        stones
            .tasks
            .insert(Uuid::now_v7(), Timestamp::from_second(0).unwrap());
        stones.tasks.insert(Uuid::now_v7(), Timestamp::now());
        assert!(stones.expire(TOMBSTONE_RETENTION));
        assert_eq!(stones.tasks.len(), 1);
        assert!(!stones.expire(TOMBSTONE_RETENTION));
    }
}
