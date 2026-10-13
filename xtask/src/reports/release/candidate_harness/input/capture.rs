//! One initial retained-byte capture; no lookup, identity scheme or artifact store.
use super::{Snapshot, read_snapshot};
use std::fs::{File, OpenOptions, Permissions};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct CapturedFile {
    path: PathBuf,
    snapshot: Snapshot,
}

impl CapturedFile {
    pub(crate) fn revalidate(&self, expected: &[u8]) -> Result<(), String> {
        let current = inspect(&self.path)?;
        if current != self.snapshot {
            return Err("frozen executable snapshot changed after capture".to_string());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "frozen executable has no parent".to_string())?;
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "frozen executable name is not UTF-8".to_string())?;
        let bytes = read_snapshot(parent, name, expected.len() as u64)?;
        if bytes != expected || inspect(&self.path)? != self.snapshot {
            return Err("frozen executable bytes changed after capture".to_string());
        }
        Ok(())
    }
}

fn inspect(path: &Path) -> Result<Snapshot, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("inspect frozen executable: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("frozen executable must have one owned link".to_string());
        }
    }
    Snapshot::capture(&metadata)
}

/// The writer receives retained bytes, never a mutable compiler-output path.
/// Publication is the returned handle, after complete readback. Observation is
/// unlocked: this does not authenticate hostile filesystem races or durability.
pub(crate) fn capture_retained_executable_with(
    bytes: &[u8],
    permissions: Permissions,
    path: &Path,
    before_create: impl FnOnce() -> Result<(), String>,
    writer: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> Result<CapturedFile, String> {
    before_create()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("capture destination creation refused: {error}"))?;
    let created = Snapshot::capture(&file.metadata().map_err(|error| {
        format!(
            "capture ownership unavailable; partial retained at {}: {error}",
            path.display()
        )
    })?)
    .map_err(|error| {
        format!(
            "capture ownership unavailable; partial retained at {}: {error}",
            path.display()
        )
    })?;
    // `#[cfg(unix)]` arms inside this closure constrain nothing on Windows,
    // so the closure's error type needs this explicit annotation to compile
    // there (inherited from main, #7498; behavior unchanged on both).
    let result: Result<CapturedFile, String> = (|| {
        writer(&mut file, bytes).map_err(|error| format!("write retained executable: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("flush retained executable: {error}"))?;
        file.set_permissions(permissions)
            .map_err(|error| format!("restore retained executable permissions: {error}"))?;
        let snapshot = inspect(path)?;
        #[cfg(unix)]
        if snapshot.identity != created.identity {
            return Err("capture path no longer owns the created file".to_string());
        }
        let captured = CapturedFile {
            path: path.to_path_buf(),
            snapshot,
        };
        captured.revalidate(bytes)?;
        Ok(captured)
    })();
    drop(file);
    match result {
        Ok(captured) => Ok(captured),
        Err(error) => {
            let cleanup = remove_owned_partial(path, &created);
            Err(format!(
                "initial executable capture failed: {error}; {cleanup}"
            ))
        }
    }
}

fn remove_owned_partial(path: &Path, created: &Snapshot) -> String {
    #[cfg(unix)]
    {
        match inspect(path) {
            Ok(current) if current.identity == created.identity => {
                match std::fs::remove_file(path) {
                    Ok(()) => "owned partial removed".to_string(),
                    Err(error) => format!("owned partial retained; cleanup failed: {error}"),
                }
            }
            _ => "partial ownership changed or unavailable; occupant retained".to_string(),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = created;
        format!(
            "partial retained at {}; native file ownership unavailable for deletion",
            path.display()
        )
    }
}
