//! Keeps records edited offline on several devices in step, using a folder
//! that an existing sync client (iCloud Drive, OneDrive, Dropbox, Syncthing)
//! already moves between them.
//!
//! Everything that syncs implements [`Merge`], a join that gives the same
//! result whatever order copies arrive in. Records are built from the field
//! types in [`field`] and kept in a [`RecordSet`]; ordered lists use
//! [`Sequence`].
//!
//! [`vault`] still holds Vault's own types and merge rules, moved here
//! unchanged; phase 3 rebuilds them on the generic types. Nothing here
//! touches disk except [`fs`].

#![forbid(unsafe_code)]

pub mod field;
pub mod fs;
mod merge;
mod record_set;
pub mod sequence;
pub mod vault;

pub use field::{AddWinsSet, BoundedSet, Counter, DeviceId, Lww, LwwHistory, Max, Tag};
pub use merge::{check_laws, merged, Merge};
pub use record_set::{DeletePolicy, DeleteWins, EditRevives, Record, RecordSet};
pub use sequence::{ItemId, Position, Sequence};
