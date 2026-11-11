//! Keeping a store's data in a file in a synced folder.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;
use zeroize::Zeroizing;

use crate::codec::{Codec, StoreError};
use crate::fs::{conflict_copies, write_atomic};
use crate::merge::Merge;

/// Somewhere a store's data is kept and kept in step with other devices.
pub trait Store {
    type Data;

    /// Reads the data as it now stands.
    fn load(&mut self) -> Result<Self::Data, StoreError>;

    /// Writes `data`, first merging in anything another device wrote since
    /// this store last read or wrote, so neither side's edits are lost.
    fn save(&mut self, data: &mut Self::Data) -> Result<(), StoreError>;

    /// Merges in whatever other devices wrote, and writes the result back
    /// if that left the stored copy out of date.
    fn sync(&mut self, data: &mut Self::Data) -> Result<SyncOutcome, StoreError>;
}

/// What a [`Store::sync`] did.
#[derive(Debug, Default)]
pub struct SyncOutcome {
    /// The data in memory changed, so the app should show it again.
    pub changed: bool,
    /// The store's file was rewritten.
    pub wrote: bool,
    /// Conflict copies merged in and then deleted.
    pub merged: Vec<PathBuf>,
    /// Conflict copies that couldn't be read, left where they are: written
    /// under another key, by a newer build, or damaged.
    pub unreadable: Vec<(PathBuf, StoreError)>,
}

/// A store kept as one file, `<name>.<ext>`, in a folder that a sync client
/// (iCloud Drive, OneDrive, Dropbox, Syncthing) moves between devices.
///
/// Those clients only move whole files, so two devices writing offline
/// leave two versions: the main file, and a conflict copy beside it. The
/// store merges both back together. It remembers the bytes it last read or
/// wrote, which is how it notices another device's write.
///
/// The data can be any [`Merge`] type. To keep several collections in one
/// file, make it a struct with one [`RecordSet`](crate::RecordSet) field
/// per collection, plus an [`Unknown`](crate::Unknown) so collections added
/// by a newer build survive.
pub struct FolderStore<C> {
    dir: PathBuf,
    name: String,
    ext: String,
    skip: Vec<String>,
    max_len: u64,
    codec: C,
    last_seen: Option<Vec<u8>>,
}

impl<C> FolderStore<C>
where
    C: Codec,
    C::Data: Merge + Serialize,
{
    /// A store kept in `dir/<name>.<ext>`. Files named `<name>-backup…` are
    /// never taken for conflict copies; add more with [`Self::skip`].
    pub fn new(dir: impl Into<PathBuf>, name: &str, ext: &str, codec: C) -> Self {
        FolderStore {
            dir: dir.into(),
            name: name.into(),
            ext: ext.into(),
            skip: vec![format!("{name}-backup")],
            max_len: 64 << 20,
            codec,
            last_seen: None,
        }
    }

    /// Never treats files whose names start with `prefix` (compared
    /// case-insensitively) as conflict copies.
    pub fn skip(mut self, prefix: &str) -> Self {
        self.skip.push(prefix.into());
        self
    }

    /// Refuses files longer than `bytes` as damaged. 64 MiB by default.
    pub fn max_len(mut self, bytes: u64) -> Self {
        self.max_len = bytes;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The main file.
    pub fn path(&self) -> PathBuf {
        self.dir.join(format!("{}.{}", self.name, self.ext))
    }

    pub fn exists(&self) -> bool {
        self.path().is_file()
    }

    pub fn codec(&self) -> &C {
        &self.codec
    }

    /// Swaps the codec, to write under a new key. The next save writes with
    /// it; a file another device wrote meanwhile must then open with it.
    pub fn set_codec(&mut self, codec: C) {
        self.codec = codec;
    }

    /// Forgets what was last read or written, so the next save or sync
    /// reads the file again before writing.
    pub fn forget(&mut self) {
        self.last_seen = None;
    }

    /// The conflict copies beside the main file now.
    pub fn conflict_copies(&self) -> Vec<PathBuf> {
        let skip: Vec<&str> = self.skip.iter().map(String::as_str).collect();
        conflict_copies(&self.dir, &self.name, &self.ext, &skip)
    }

    /// Writes `data` as a new store. Refuses if the file already exists.
    pub fn create(&mut self, data: &C::Data) -> Result<(), StoreError> {
        if self.path().exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "the store already exists").into());
        }
        fs::create_dir_all(&self.dir)?;
        self.write(data)
    }

    /// Reads the main file, or `None` if it isn't there.
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, StoreError> {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take(self.max_len + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.max_len {
            return Err(StoreError::Corrupt("the file is larger than the store allows".into()));
        }
        Ok(Some(bytes))
    }

    fn changed_since_seen(&self, bytes: &[u8]) -> bool {
        self.last_seen.as_deref() != Some(bytes)
    }

    fn write(&mut self, data: &C::Data) -> Result<(), StoreError> {
        let bytes = self.codec.encode(data)?;
        write_atomic(&self.path(), &bytes)?;
        self.last_seen = Some(bytes);
        Ok(())
    }
}

/// `data` in a canonical form, to tell whether two copies hold the same.
/// Wiped afterwards, since the data may be secret.
fn digest<T: Serialize>(data: &T) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    serde_json::to_vec(data).map(Zeroizing::new).map_err(|e| StoreError::Corrupt(e.to_string()))
}

impl<C> Store for FolderStore<C>
where
    C: Codec,
    C::Data: Merge + Serialize,
{
    type Data = C::Data;

    fn load(&mut self) -> Result<C::Data, StoreError> {
        let bytes = self
            .read(&self.path())?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the store's file is missing"))?;
        let data = self.codec.decode(&bytes)?;
        self.last_seen = Some(bytes);
        Ok(data)
    }

    /// If the file is gone, `data` is written as it stands: it holds
    /// everything the store did.
    fn save(&mut self, data: &mut C::Data) -> Result<(), StoreError> {
        if let Some(bytes) = self.read(&self.path())? {
            if self.changed_since_seen(&bytes) {
                // A file this codec can't open is never overwritten: under a
                // new key, that would undo a password change made elsewhere.
                data.merge(self.codec.decode(&bytes)?);
            }
        }
        self.write(data)
    }

    /// Merges the main file, if another device changed it, and every
    /// readable conflict copy into `data`, writes the result back if the
    /// file no longer matches it, and only then deletes the merged copies.
    /// A missing main file is left for the next sync: a sync client may be
    /// swapping it.
    fn sync(&mut self, data: &mut C::Data) -> Result<SyncOutcome, StoreError> {
        let mut outcome = SyncOutcome::default();
        let Some(bytes) = self.read(&self.path())? else { return Ok(outcome) };

        let remote_changed = self.changed_since_seen(&bytes);
        let copies = self.conflict_copies();
        if !remote_changed && copies.is_empty() {
            return Ok(outcome);
        }

        let before = digest(data)?;
        // What the file holds, to decide whether it needs rewriting. Unchanged
        // since we read or wrote it, it holds `data` as it stood then.
        let on_disk = if remote_changed {
            let theirs = self.codec.decode(&bytes)?;
            let on_disk = digest(&theirs)?;
            data.merge(theirs);
            on_disk
        } else {
            before.clone()
        };

        for copy in copies {
            match self.read(&copy).and_then(|b| {
                b.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound).into())
                    .and_then(|b| self.codec.decode(&b))
            }) {
                Ok(theirs) => {
                    data.merge(theirs);
                    outcome.merged.push(copy);
                }
                Err(e) => outcome.unreadable.push((copy, e)),
            }
        }

        let after = digest(data)?;
        outcome.changed = *after != *before;
        if *after != *on_disk || !outcome.merged.is_empty() {
            self.write(data)?;
            outcome.wrote = true;
        } else {
            // Already in step: just note that this version has been seen
            self.last_seen = Some(bytes);
        }
        // Only now that their contents are safely in the main file
        for copy in &outcome.merged {
            let _ = fs::remove_file(copy);
        }
        Ok(outcome)
    }
}
