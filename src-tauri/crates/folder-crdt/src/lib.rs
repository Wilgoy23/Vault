//! Keeps records edited offline on several devices in step, using a folder
//! that an existing sync client (iCloud Drive, OneDrive, Dropbox, Syncthing)
//! already moves between them.
//!
//! Everything that syncs implements [`Merge`], a join that gives the same
//! result whatever order copies arrive in. Records are built from the field
//! types in [`field`], stamped with [`Hlc`] timestamps, and kept in a
//! [`RecordSet`]; ordered lists use [`Sequence`]. Put an [`Unknown`] in each
//! record so fields from newer builds survive a save from an older one, and
//! a [`format::Header`] on each file so an older build refuses what it
//! can't read.
//!
//! [`vault`] holds Vault's own records, built on these types. Nothing here
//! touches disk except [`fs`].

#![forbid(unsafe_code)]

pub mod field;
pub mod format;
pub mod fs;
mod hlc;
mod merge;
mod record_set;
pub mod sequence;
mod unknown;
pub mod vault;

pub use field::{AddWinsSet, BoundedSet, Counter, DeviceId, Immutable, Lww, LwwHistory, Max, Tag};
pub use hlc::{Hlc, HlcClock, LEGACY_DEVICE};
pub use merge::{check_laws, merged, Merge};
pub use record_set::{DeletePolicy, DeleteWins, EditRevives, Record, RecordSet};
pub use sequence::{ItemId, Position, Sequence};
pub use unknown::Unknown;
