//! Advisory output leaf acquisition. This is not ancestor or hard-link confinement.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replace `path` with `bytes` so a reader sees the old file or the complete
/// new one, never a truncated or half-written file. The bytes go to an
/// exclusively created temporary file beside the destination, which is
/// flushed and then renamed over it. An interrupted run, a full disk or a
/// failed write leaves the previous file untouched and removes the temporary
/// file where the process survives to do so. An existing destination that
/// is not a regular file is refused, as before.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_with(path, |file| file.write_all(bytes))
}

/// [`write`] for output produced incrementally: `fill` streams into the
/// temporary file, which is published only when `fill` succeeds.
pub(crate) fn write_with(
    path: &Path,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    validate_destination(path)?;
    let Some(name) = path.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path has no file name",
        ));
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    // Hidden, and distinct per process and call, so concurrent writers of
    // one destination never share a temporary file.
    let temp_path = parent.join(format!(
        ".{}.ripr-{}-{nanos}-{sequence}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let published = (|| {
        let mut temp = create_exclusive(&temp_path)?;
        if let Ok(metadata) = fs::metadata(path)
            && metadata.is_file()
        {
            temp.set_permissions(metadata.permissions())?;
        }
        fill(&mut temp)?;
        temp.sync_all()?;
        drop(temp);
        fs::rename(&temp_path, path)
    })();
    if published.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    published
}

pub(crate) fn create_exclusive(path: &Path) -> io::Result<File> {
    open_new(path)
}

/// Rename does not follow the destination leaf, but refuse existing nonregular
/// destinations rather than silently replacing them.
pub(crate) fn validate_destination(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output destination is not a regular file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// `create_new` (O_CREAT|O_EXCL, CREATE_NEW) never opens an existing path,
/// symlink or otherwise, so every platform can create the temporary file
/// safely. The platform flags below are hardening where their values are
/// known, not the safety boundary.
fn open_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // O_NOFOLLOW | O_NONBLOCK: reject links; do not wait for a FIFO reader.
        options.custom_flags(0x0002_0000 | 0x0000_0800);
    }
    #[cfg(all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(0x0000_0100 | 0x0000_0004);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // OPEN_REPARSE_POINT; deny write/delete sharing while validating/writing.
        options.custom_flags(0x0020_0000).share_mode(0x0000_0001);
    }
    let file = options.open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "opened output is not a regular file",
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::write;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir(label: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-file-write-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
        Ok(dir)
    }

    fn leftovers(dir: &Path) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for entry in fs::read_dir(dir).map_err(|err| format!("read_dir: {err}"))? {
            let name = entry
                .map_err(|err| format!("dir entry: {err}"))?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if name.ends_with(".tmp") {
                names.push(name);
            }
        }
        Ok(names)
    }

    #[test]
    fn replaces_a_longer_file_completely_and_leaves_no_temp() -> Result<(), String> {
        let dir = fresh_dir("replace")?;
        let path = dir.join("report.json");
        fs::write(&path, "x".repeat(4096)).map_err(|err| format!("seed: {err}"))?;
        write(&path, b"{}\n").map_err(|err| format!("write: {err}"))?;
        assert_eq!(
            fs::read(&path).map_err(|err| format!("read: {err}"))?,
            b"{}\n"
        );
        assert_eq!(leftovers(&dir)?, Vec::<String>::new());
        write(&dir.join("nested/dir/out.md"), b"# ok\n")
            .map_err(|err| format!("nested write: {err}"))?;
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_refused_destination_is_left_untouched() -> Result<(), String> {
        let dir = fresh_dir("refused")?;
        let blocker = dir.join("report.json");
        fs::create_dir(&blocker).map_err(|err| format!("seed dir: {err}"))?;
        fs::write(blocker.join("keep"), b"keep").map_err(|err| format!("seed keep: {err}"))?;
        let Err(error) = write(&blocker, b"new") else {
            return Err("a directory destination was replaced".to_string());
        };
        assert!(
            error.to_string().contains("not a regular file"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read(blocker.join("keep")).map_err(|err| format!("read keep: {err}"))?,
            b"keep"
        );
        assert_eq!(leftovers(&dir)?, Vec::<String>::new());
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_destination_is_refused_and_its_target_kept() -> Result<(), String> {
        let dir = fresh_dir("symlink")?;
        let target = dir.join("target.json");
        fs::write(&target, b"original").map_err(|err| format!("seed: {err}"))?;
        let link = dir.join("report.json");
        std::os::unix::fs::symlink(&target, &link).map_err(|err| format!("symlink: {err}"))?;
        if write(&link, b"new").is_ok() {
            return Err("a symlinked destination was written".to_string());
        }
        assert_eq!(
            fs::read(&target).map_err(|err| format!("read: {err}"))?,
            b"original"
        );
        assert_eq!(leftovers(&dir)?, Vec::<String>::new());
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn replacing_a_file_keeps_its_permissions() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = fresh_dir("mode")?;
        let path = dir.join("report.md");
        fs::write(&path, b"old").map_err(|err| format!("seed: {err}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))
            .map_err(|err| format!("chmod: {err}"))?;
        write(&path, b"new").map_err(|err| format!("write: {err}"))?;
        let mode = fs::metadata(&path)
            .map_err(|err| format!("stat: {err}"))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640);
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
