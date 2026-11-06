//! A store's syncable contents, independent of where they were read from.

use std::collections::BTreeMap;
use std::path::PathBuf;

use uuid::Uuid;

use super::git::Git;
use crate::bucket::Bucket;
use crate::privacy::Sealed;
use crate::store::Tombstones;
use crate::store::io::{BucketFile, bucket_file, parse_versioned};
use crate::store::migrate::File;
use crate::utils::error::Result;

/// Buckets (with their tasks), private buckets (sealed), and tombstones:
/// everything that syncs. Device-local state, like the active bucket, is
/// not part of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub buckets: BTreeMap<Uuid, Bucket>,
    /// Private buckets, encrypted (see [`crate::privacy`]).
    pub sealed: BTreeMap<Uuid, Sealed>,
    pub tombstones: Tombstones,
}

impl Snapshot {
    /// The store as committed at `rev`, upgraded to the current format.
    pub(crate) fn from_git(git: &Git, rev: &str) -> Result<Snapshot> {
        let listing = git.run(&[
            "ls-tree",
            "-r",
            "--name-only",
            rev,
            "--",
            "buckets",
            "tombstones.json",
        ])?;
        let mut snapshot = Snapshot::default();
        for path in listing.lines() {
            let text = git.file(rev, path)?;
            let origin = PathBuf::from(format!("{rev}:{path}"));
            if path == "tombstones.json" {
                snapshot.tombstones = parse_versioned(&text, &origin, File::Tombstones)?.0;
            } else if path.starts_with("buckets/") && path.ends_with(".json") {
                let value = parse_versioned(&text, &origin, File::Bucket)?.0;
                match bucket_file(value, &origin)? {
                    BucketFile::Plain(bucket) => {
                        snapshot.buckets.insert(bucket.header.id, bucket);
                    }
                    BucketFile::Sealed(sealed) => {
                        snapshot.sealed.insert(sealed.id(), sealed);
                    }
                }
            }
        }
        Ok(snapshot)
    }
}
