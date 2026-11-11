//! The header that says what wrote a synced file, read before anything
//! else, so a build can refuse a file it would misread instead of damaging
//! it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Marks a file as written through folder-crdt.
pub const MAGIC: &str = "folder-crdt";

/// The version of folder-crdt's own encoding (clocks, field types, record
/// sets). Raised only when a change would make an older build misread a file.
pub const FORMAT: u8 = 1;

/// Who wrote a file and in which version of their schema. Stored unencrypted
/// beside the payload; contains nothing secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub magic: String,
    pub format: u8,
    /// The app's own identifier, so one app never opens another's file.
    pub app: String,
    /// The app's schema version, for the app's own migrations.
    pub schema: u32,
}

impl Header {
    /// A header for a file this build writes now.
    pub fn new(app: &str, schema: u32) -> Self {
        Header { magic: MAGIC.into(), format: FORMAT, app: app.into(), schema }
    }

    /// Checks that this build, at `schema`, can read a file with this header.
    pub fn check(&self, app: &str, schema: u32) -> Result<(), FormatError> {
        if self.magic != MAGIC {
            return Err(FormatError::NotOurs);
        }
        if self.app != app {
            return Err(FormatError::OtherApp(self.app.clone()));
        }
        if self.format > FORMAT || self.schema > schema {
            return Err(FormatError::Newer { format: self.format, schema: self.schema });
        }
        Ok(())
    }
}

/// Why a file can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// Not a folder-crdt file.
    NotOurs,
    /// Written by a different app.
    OtherApp(String),
    /// Written by a newer build. The app should ask the user to update.
    Newer { format: u8, schema: u32 },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::NotOurs => f.write_str("not a folder-crdt file"),
            FormatError::OtherApp(app) => write!(f, "this file belongs to another app ({app})"),
            FormatError::Newer { format, schema } => {
                write!(f, "written by a newer version (format {format}, schema {schema})")
            }
        }
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_newer_files_and_other_apps() {
        let h = Header::new("vault", 2);
        assert_eq!(h.check("vault", 2), Ok(()));
        assert_eq!(h.check("vault", 3), Ok(()));
        assert!(matches!(h.check("vault", 1), Err(FormatError::Newer { .. })));
        assert!(matches!(h.check("chronicle", 2), Err(FormatError::OtherApp(_))));
        let future = Header { format: FORMAT + 1, ..h.clone() };
        assert!(matches!(future.check("vault", 2), Err(FormatError::Newer { .. })));
        let other = Header { magic: "nope".into(), ..h };
        assert_eq!(other.check("vault", 2), Err(FormatError::NotOurs));
    }
}
