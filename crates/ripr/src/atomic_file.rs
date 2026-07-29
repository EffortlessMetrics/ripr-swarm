//! Atomic replacement for small on-disk artifacts and cache entries.

use std::io::Write;
use std::path::Path;

static TEMP_FILE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Write bytes through a same-directory temporary file, flush them to disk,
/// then atomically replace the destination. The short temporary name keeps
/// the operation usable when the destination filename is near a platform's
/// maximum filename length.
pub(crate) fn write(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let dir = parent.unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|err| {
        format!(
            "failed to create {label} directory {}: {err}",
            dir.display()
        )
    })?;
    if path.file_name().is_none() {
        return Err(format!(
            "atomic write path {} has no file name",
            path.display()
        ));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = dir.join(format!(
        ".ripr-atomic-{}-{nanos}-{sequence}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&tmp_path).map_err(|err| {
            format!(
                "failed to create {label} temp file {}: {err}",
                tmp_path.display()
            )
        })?;
        file.write_all(bytes).map_err(|err| {
            format!(
                "failed to write {label} temp file {}: {err}",
                tmp_path.display()
            )
        })?;
        file.sync_all().map_err(|err| {
            format!(
                "failed to fsync {label} temp file {}: {err}",
                tmp_path.display()
            )
        })?;
        drop(file);
        std::fs::rename(&tmp_path, path)
            .map_err(|err| format!("failed to finalize {label} {}: {err}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}
