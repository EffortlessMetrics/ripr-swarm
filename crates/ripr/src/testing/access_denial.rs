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

/// True when `file` cannot be read by this process.
pub(crate) fn file_read_denied(file: &Path) -> bool {
    std::fs::read(file).is_err()
}

/// True when this process cannot create an entry inside `dir`.
pub(crate) fn dir_creation_denied(dir: &Path) -> bool {
    let probe = dir.join(".ripr-access-probe");
    match std::fs::create_dir(&probe) {
        Ok(()) => {
            let _ = std::fs::remove_dir(&probe);
            false
        }
        Err(_) => true,
    }
}
