//! File handling for a store kept in a synced folder: writing without ever
//! leaving a half-written file, and finding the extra copies sync clients
//! leave behind when two devices change the file at once.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `contents` to `path` atomically: write to a temp file in the same
/// directory, fsync, then rename over the target. A crash mid-write can
/// never corrupt the only copy. The file is created owner-readable only
/// (0600) on Unix.
///
/// The temp file is `path` with `.tmp` appended, so it never looks like a
/// conflict copy to [`conflict_copies`].
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut tmp_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    // A sync client uploading the previous version can hold the file open
    // for a moment, which makes the rename fail on Windows. Give it a few tries.
    let mut attempt = 0;
    loop {
        match fs::rename(&tmp, path) {
            Ok(()) => return sync_dir(path),
            Err(_) if attempt < 4 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Makes a rename in `path`'s directory durable. Windows has no directory
/// handles to sync; its renames are journaled.
fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
        fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// This installation's device id, kept in `dir/device-id`; created on first
/// call. `dir` must be local to the device (an app-data folder), never the
/// synced folder: two devices sharing an id could make clashing timestamps.
pub fn device_id(dir: &Path) -> io::Result<crate::DeviceId> {
    let path = dir.join("device-id");
    if let Ok(text) = fs::read_to_string(&path) {
        match u64::from_str_radix(text.trim(), 16) {
            Ok(id) if id != crate::LEGACY_DEVICE => return Ok(id),
            _ => {} // Unreadable: replace it
        }
    }
    let id = random_device_id()?;
    fs::create_dir_all(dir)?;
    write_atomic(&path, format!("{id:016x}\n").as_bytes())?;
    Ok(id)
}

/// A fresh random device id, never [`LEGACY_DEVICE`](crate::LEGACY_DEVICE).
pub fn random_device_id() -> io::Result<crate::DeviceId> {
    loop {
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        let id = u64::from_le_bytes(bytes);
        if id != crate::LEGACY_DEVICE {
            return Ok(id);
        }
    }
}

/// The extra copies sync clients leave beside `<stem>.<ext>` when two
/// devices changed it at once: "vault (Will's conflicted copy).enc"
/// (Dropbox), "vault-LAPTOP.enc" (OneDrive), "vault.sync-conflict-….enc"
/// (Syncthing), "vault 2.enc" (iCloud), for a stem of "vault" and an
/// extension of "enc". Files whose names start with one of `skip`, such as
/// exported backups named "vault-backup…", are left alone. Names are
/// compared case-insensitively.
pub fn conflict_copies(dir: &Path, stem: &str, ext: &str, skip: &[&str]) -> Vec<PathBuf> {
    let stem = stem.to_ascii_lowercase();
    let main = format!("{stem}.{}", ext.to_ascii_lowercase());
    let suffix = format!(".{}", ext.to_ascii_lowercase());
    let skip: Vec<String> = skip.iter().map(|s| s.to_ascii_lowercase()).collect();

    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    read.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
            let name = name.to_ascii_lowercase();
            p.is_file()
                && name != main
                && name.starts_with(&stem)
                && name.ends_with(&suffix)
                && !skip.iter().any(|s| name.starts_with(s.as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("folder_crdt_tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn device_id_is_created_once_then_reused() {
        let dir = temp_dir("device_id");
        let first = device_id(&dir).unwrap();
        assert_ne!(first, crate::LEGACY_DEVICE);
        assert_eq!(device_id(&dir).unwrap(), first);

        // A damaged file is replaced rather than trusted
        fs::write(dir.join("device-id"), "not hex").unwrap();
        let replaced = device_id(&dir).unwrap();
        assert_ne!(replaced, crate::LEGACY_DEVICE);
        assert_eq!(device_id(&dir).unwrap(), replaced);
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp_file() {
        let dir = temp_dir("write_atomic");
        let path = dir.join("vault.enc");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!dir.join("vault.enc.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn conflict_copies_finds_each_sync_clients_copies_and_nothing_else() {
        let dir = temp_dir("conflict_copies");
        let copies = [
            "vault (Will's conflicted copy).enc",
            "vault-LAPTOP.enc",
            "vault.sync-conflict-20261110-120000-ABCDEFG.enc",
            "vault 2.enc",
            "VAULT 3.ENC",
        ];
        let others = ["vault.enc", "vault-backup.enc", "vault-backup 2.enc", "vault.enc.tmp", "notes.enc", "vault.txt"];
        for name in copies.iter().chain(&others) {
            fs::write(dir.join(name), "x").unwrap();
        }
        fs::create_dir_all(dir.join("vault 4.enc")).unwrap();

        let mut found: Vec<String> = conflict_copies(&dir, "vault", "enc", &["vault-backup"])
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        let mut expected: Vec<String> = copies.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(found, expected);
        let _ = fs::remove_dir_all(&dir);
    }
}
