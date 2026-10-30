//! Skip helper for tests whose stimulus is a permission denial (#8867).
//!
//! Mode bits (`chmod 000`, read-only directories) do not deny a process that
//! bypasses them, typically euid 0 in agent and container sandboxes. The probe
//! asks the filesystem directly instead of guessing from the uid, so the test
//! still runs wherever the denial is real. Where it is not, the skip names the
//! reason on stderr; a silent pass would read as a green control that never ran.
//! The notice writes to the process stderr handle directly: libtest captures
//! `eprintln!` output of passing tests, which would hide the skip.

use std::io::Write as _;
use std::path::Path;

/// Prints a visible skip notice and returns `true` when `denied` is false.
///
/// Call after building the stimulus: `if skip_unless_denied(name, denied) { return Ok(()); }`.
pub(crate) fn skip_unless_denied(test: &str, denied: bool) -> bool {
    if !denied {
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPED {test}: this process bypasses file mode bits (typically euid 0), \
             so the permission-denial stimulus cannot occur"
        );
    }
    !denied
}

/// Classifies a probe result: `PermissionDenied` is the denial, success is
/// no denial, and any other error is a broken probe that must fail the test
/// rather than satisfy it.
pub(crate) fn denied_by_permission<T>(result: std::io::Result<T>) -> Result<bool, String> {
    match result {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(true),
        Err(error) => Err(format!(
            "access probe failed for a non-permission reason: {error}"
        )),
    }
}

/// True when `file` cannot be read by this process. A caller whose denial is a
/// mode-000 parent directory should probe the leaf with `symlink_metadata`
/// through [`denied_by_permission`] instead.
pub(crate) fn file_read_denied(file: &Path) -> Result<bool, String> {
    denied_by_permission(std::fs::read(file))
}

/// True when this process cannot create an entry inside `dir`.
pub(crate) fn dir_creation_denied(dir: &Path) -> Result<bool, String> {
    let probe = dir.join(".ripr-access-probe");
    let denied = denied_by_permission(std::fs::create_dir(&probe))?;
    if !denied {
        let _ = std::fs::remove_dir(&probe);
    }
    Ok(denied)
}
