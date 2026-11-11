//! Keeps records edited offline on several devices in step, using a folder
//! that an existing sync client (iCloud Drive, OneDrive, Dropbox, Syncthing)
//! already moves between them.
//!
//! This is phase 1 of the crate: Vault's sync code moved here unchanged.
//! [`vault`] still holds Vault's own types and merge rules; later phases
//! replace it with a generic `RecordSet<T>`. [`fs`] is already generic.

#![forbid(unsafe_code)]

pub mod fs;
pub mod vault;
