//! Bounded reads for user-named CLI inputs (#4480).
//!
//! The CLI reads `--diff`, `--diff -` (stdin), JSON artifact flags such as
//! `outcome --before/--after`, `--suppression-policy`, and `ripr.toml` fully
//! into memory. An unbounded `read_to_string` on a character device
//! (`--diff /dev/zero`) or an accidentally huge file (a multi-GB log passed as
//! `--diff`) grows memory until the process is terminated instead of failing.
//!
//! These helpers are drop-in replacements for `std::fs::read_to_string` /
//! `std::fs::read` and return `std::io::Result`, so every call site keeps its
//! existing error wording (which names the flag or path) and its exit class:
//! the CLI maps a returned `Err(String)` to exit 2 (docs/EXIT_CODES.md).
//!
//! The bound is enforced while reading (`take(limit + 1)`), not from
//! metadata: a character device or FIFO reports length 0, and a file that
//! grows between a metadata check and the read would bypass a pre-check.
//! Non-regular files are deliberately not refused: `--diff <(git diff ...)`
//! passes a FIFO, and stdin (`-`) is a pipe; both are legitimate and are
//! bounded by the same cap instead.

use std::io::Read;
use std::path::Path;

/// Byte cap for a single user-named CLI input.
///
/// Real diffs and repo-exposure artifacts for large repositories reach tens of
/// megabytes; 256 MiB is far above any legitimate input while still failing
/// closed on an unbounded one. It matches the LSP's `MAX_LSP_ARTIFACT_BYTES`
/// and the CLI's `MAX_AGENT_VERIFY_SNAPSHOT_BYTES` (#2921), so the same file
/// is accepted or refused consistently across surfaces.
pub(crate) const MAX_CLI_INPUT_BYTES: u64 = 256 * 1024 * 1024;

/// Bounded replacement for `std::fs::read_to_string` on a user-named input.
pub(crate) fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    read_to_string_with_limit(path, MAX_CLI_INPUT_BYTES)
}

/// Byte cap for repository metadata TOML (`ripr.toml`, `Cargo.toml`,
/// `rust-toolchain.toml`) that ripr parses into a `toml` document.
///
/// The `toml` lexer materializes a token vector before parsing, so a file made
/// of newlines needs a few dozen times its own size in memory: a 200 MB
/// `ripr.toml` still under [`MAX_CLI_INPUT_BYTES`] aborted the process on
/// allocation (#11563). Real manifests and configs are kilobytes; 2 MiB leaves
/// generous headroom while bounding the worst case near 60 MB.
pub(crate) const MAX_METADATA_TOML_BYTES: u64 = 2 * 1024 * 1024;

/// Bounded replacement for `std::fs::read_to_string` on repository metadata
/// TOML that a hostile checkout controls.
///
/// Unlike [`read_to_string`] (which deliberately admits FIFOs for
/// `--diff <(git diff)`), this refuses anything that is not a regular file
/// before opening it, so a FIFO planted as `ripr.toml` or `Cargo.toml` cannot
/// block the run forever, and caps the read at [`MAX_METADATA_TOML_BYTES`].
pub(crate) fn read_metadata_toml(path: impl AsRef<Path>) -> std::io::Result<String> {
    read_metadata_toml_with_limit(path, MAX_METADATA_TOML_BYTES)
}

pub(crate) fn read_metadata_toml_with_limit(
    path: impl AsRef<Path>,
    limit: u64,
) -> std::io::Result<String> {
    let path = path.as_ref();
    // `metadata` follows symlinks, so a link to a FIFO or device is refused too.
    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "is a directory; expected a regular TOML file",
        ));
    }
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "is not a regular file (FIFO, device or socket); ripr reads repository metadata only from regular files",
        ));
    }
    if metadata.len() > limit {
        return Err(metadata_oversize_error(limit));
    }
    let bytes =
        read_reader_with_limit_as(std::fs::File::open(path)?, limit, metadata_oversize_error)?;
    String::from_utf8(bytes).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("stream did not contain valid UTF-8 ({err})"),
        )
    })
}

fn read_reader_with_limit_as(
    reader: impl Read,
    limit: u64,
    oversize: fn(u64) -> std::io::Error,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(oversize(limit));
    }
    Ok(bytes)
}

fn metadata_oversize_error(limit: u64) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "TOML file exceeds the {limit} byte metadata limit ({} MiB); ripr parses repository metadata fully into memory, so split or trim the file",
            limit / (1024 * 1024)
        ),
    )
}

/// Bounded replacement for `std::fs::read` on a user-named input.
pub(crate) fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    read_with_limit(path, MAX_CLI_INPUT_BYTES)
}

/// Bounded byte read of an already-open stream, for callers that decode
/// non-UTF-8 input themselves (`--diff -`, #4584).
pub(crate) fn read_reader(reader: impl Read) -> std::io::Result<Vec<u8>> {
    read_reader_with_limit(reader, MAX_CLI_INPUT_BYTES)
}

/// `limit` is a parameter so tests can exercise the cap without
/// materializing 256 MiB; production callers use [`read_to_string`].
pub(crate) fn read_to_string_with_limit(
    path: impl AsRef<Path>,
    limit: u64,
) -> std::io::Result<String> {
    read_reader_to_string_with_limit(open_within_limit(path.as_ref(), limit)?, limit)
}

pub(crate) fn read_with_limit(path: impl AsRef<Path>, limit: u64) -> std::io::Result<Vec<u8>> {
    read_reader_with_limit(open_within_limit(path.as_ref(), limit)?, limit)
}

/// Open `path`, rejecting a regular file whose reported length already
/// exceeds `limit` without reading it. This is only a fast path: devices and
/// FIFOs report length 0, so the read itself stays capped.
fn open_within_limit(path: &Path, limit: u64) -> std::io::Result<std::fs::File> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if metadata.is_file() && metadata.len() > limit {
        return Err(oversize_error(limit));
    }
    Ok(file)
}

pub(crate) fn read_reader_to_string_with_limit(
    reader: impl Read,
    limit: u64,
) -> std::io::Result<String> {
    let bytes = read_reader_with_limit(reader, limit)?;
    // Same wording as `std::fs::read_to_string`, plus the offending offset.
    String::from_utf8(bytes).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("stream did not contain valid UTF-8 ({err})"),
        )
    })
}

pub(crate) fn read_reader_with_limit(reader: impl Read, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(read_cap(limit)).read_to_end(&mut bytes)?;
    if exceeds_limit(bytes.len(), limit) {
        return Err(oversize_error(limit));
    }
    Ok(bytes)
}

/// Portable phrasing for a failed user-named input read (#9851): a missing
/// file reads `not found` in plain words — never a raw `(os error N)` or
/// host-specific `strerror` text — mirroring the sibling `outcome` command's
/// established vocabulary. Other failures keep the OS detail, because no
/// portable phrasing exists for them.
pub(crate) fn read_failure_detail(err: &std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::NotFound {
        "not found".to_string()
    } else {
        err.to_string()
    }
}

/// Bounded read of a required user-named input with the shared
/// missing-input diagnostic (#9851): the message names the attempted read
/// and the path (display form, forward slashes), says `not found` in plain
/// words for a missing file, and appends `recovery` — the command that
/// produces the artifact, in the sibling commands' vocabulary — when one
/// exists.
pub(crate) fn read_required_input(
    attempted: &str,
    path: &Path,
    recovery: Option<&str>,
) -> Result<String, String> {
    read_to_string(path).map_err(|err| {
        let mut message = format!(
            "{attempted} {} failed: {}",
            crate::output::path::display_path(path),
            read_failure_detail(&err)
        );
        if let Some(recovery) = recovery {
            message.push_str(". ");
            message.push_str(recovery);
        }
        message
    })
}

/// Bytes to read so an over-limit input is detectable: one past `limit`,
/// saturating so `limit == u64::MAX` cannot overflow.
fn read_cap(limit: u64) -> u64 {
    limit.saturating_add(1)
}

/// Whether `read_len` bytes (read under [`read_cap`]) prove the input is over
/// `limit`.
fn exceeds_limit(read_len: usize, limit: u64) -> bool {
    read_len as u64 > limit
}

/// The whole-MiB figure shown in the oversize message, when `limit` is an
/// exact multiple of one MiB.
fn whole_mib(limit: u64) -> Option<u64> {
    const MIB: u64 = 1024 * 1024;
    if limit >= MIB && limit.is_multiple_of(MIB) {
        Some(limit / MIB)
    } else {
        None
    }
}

fn oversize_error(limit: u64) -> std::io::Error {
    let readable = match whole_mib(limit) {
        Some(mib) => format!(" ({mib} MiB)"),
        None => String::new(),
    };
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "input exceeds the {limit} byte input limit{readable}; ripr reads file inputs fully into memory, so pass a finite input under the limit (device paths such as /dev/zero never end)"
        ),
    )
}

#[cfg(kani)]
#[path = "bounded_input_kani.rs"]
mod kani_proofs;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, contents: &[u8]) -> Result<std::path::PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-bounded-input-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        let path = dir.join("input");
        std::fs::write(&path, contents).map_err(|err| format!("write input: {err}"))?;
        Ok(path)
    }

    #[test]
    fn reads_input_at_limit_and_rejects_one_byte_over() -> Result<(), String> {
        let path = temp_file("limit", b"12345")?;
        let text = read_to_string_with_limit(&path, 5).map_err(|err| err.to_string())?;
        assert_eq!(text, "12345");
        let bytes = read_with_limit(&path, 5).map_err(|err| err.to_string())?;
        assert_eq!(bytes, b"12345");

        let err = read_to_string_with_limit(&path, 4)
            .err()
            .ok_or("5-byte file must exceed a 4-byte limit")?;
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string()
                .contains("input exceeds the 4 byte input limit;"),
            "{err}"
        );
        let err = read_with_limit(&path, 4)
            .err()
            .ok_or("5-byte file must exceed a 4-byte limit")?;
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string()
                .contains("input exceeds the 4 byte input limit;"),
            "{err}"
        );
        Ok(())
    }

    #[test]
    fn stream_read_is_bounded_and_reports_mib_limit() -> Result<(), String> {
        // An endless reader stands in for stdin fed from /dev/zero: an
        // unbounded read never returns.
        let err = read_reader_to_string_with_limit(std::io::repeat(b'a'), 16)
            .err()
            .ok_or("endless stream must be refused")?;
        assert!(err.to_string().contains("16 byte input limit"), "{err}");
        assert!(
            oversize_error(MAX_CLI_INPUT_BYTES)
                .to_string()
                .contains("268435456 byte input limit (256 MiB)"),
        );
        Ok(())
    }

    #[test]
    fn stream_byte_read_keeps_non_utf8_bytes() -> Result<(), String> {
        // `--diff -` decodes non-UTF-8 itself (#4584), so the byte read
        // must not reject or rewrite them.
        let bytes = read_reader(&b"+caf\xe9\n"[..]).map_err(|err| err.to_string())?;
        assert_eq!(bytes, b"+caf\xe9\n");
        Ok(())
    }

    #[test]
    fn missing_file_keeps_not_found_kind() -> Result<(), String> {
        let path = std::env::temp_dir().join("ripr-bounded-input-definitely-missing/input");
        let err = read_to_string(&path)
            .err()
            .ok_or("missing file must fail")?;
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        Ok(())
    }

    #[test]
    fn read_required_input_names_path_and_recovery_without_os_errno() -> Result<(), String> {
        let missing =
            std::env::temp_dir().join("ripr-bounded-input-definitely-missing/required-input.json");
        let err = read_required_input(
            "read baseline create source",
            &missing,
            Some("Run `ripr gate evaluate` first, or pass the correct path"),
        )
        .err()
        .ok_or("missing required input must fail")?;
        let expected_path = crate::output::path::display_path(&missing);
        assert!(
            err.starts_with(&format!(
                "read baseline create source {expected_path} failed:"
            )),
            "{err}"
        );
        assert!(err.contains("not found"), "{err}");
        assert!(
            err.contains("Run `ripr gate evaluate` first, or pass the correct path"),
            "{err}"
        );
        // The raw OS errno must not leak for the missing-file class (#9851).
        assert!(!err.contains("os error"), "{err}");
        Ok(())
    }

    #[test]
    fn read_required_input_omits_recovery_when_none_exists() -> Result<(), String> {
        let missing =
            std::env::temp_dir().join("ripr-bounded-input-definitely-missing/bare-input.json");
        let err = read_required_input("read receipt", &missing, None)
            .err()
            .ok_or("missing required input must fail")?;
        assert!(err.contains("failed: not found"), "{err}");
        assert!(!err.contains("os error"), "{err}");
        Ok(())
    }

    #[test]
    fn read_required_input_reads_existing_file() -> Result<(), String> {
        let path = temp_file("required-ok", b"{}")?;
        let text = read_required_input("read baseline create source", &path, None)?;
        assert_eq!(text, "{}");
        Ok(())
    }

    #[test]
    fn read_failure_detail_prefers_plain_not_found() {
        let not_found = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(read_failure_detail(&not_found), "not found");
        let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(read_failure_detail(&other), "denied");
    }

    #[cfg(unix)]
    #[test]
    fn character_device_is_refused_instead_of_read_forever() -> Result<(), String> {
        let err = read_to_string_with_limit(Path::new("/dev/zero"), 1024)
            .err()
            .ok_or("/dev/zero must be refused")?;
        assert!(err.to_string().contains("1024 byte input limit"), "{err}");
        Ok(())
    }

    #[test]
    fn metadata_toml_reads_regular_file_at_limit_and_refuses_one_byte_over() -> Result<(), String> {
        let path = temp_file("meta-limit", b"a = 1\n")?;
        let text = read_metadata_toml_with_limit(&path, 6).map_err(|err| err.to_string())?;
        assert_eq!(text, "a = 1\n");
        let err = read_metadata_toml_with_limit(&path, 5)
            .err()
            .ok_or("6-byte file must exceed a 5-byte limit")?;
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("5 byte metadata limit"), "{err}");
        Ok(())
    }

    #[test]
    fn metadata_toml_refuses_directories_without_reading() -> Result<(), String> {
        let path = temp_file("meta-dir", b"")?;
        let dir = path.parent().ok_or("parent")?;
        let err = read_metadata_toml(dir)
            .err()
            .ok_or("a directory must be refused")?;
        assert!(err.to_string().contains("is a directory"), "{err}");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn metadata_toml_refuses_fifo_and_device_instead_of_blocking() -> Result<(), String> {
        let path = temp_file("meta-fifo", b"")?;
        let fifo = path.with_file_name("fifo");
        let _ = std::fs::remove_file(&fifo);
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map_err(|err| format!("mkfifo: {err}"))?;
        assert!(made.success(), "mkfifo failed");
        // A FIFO with no writer blocks `open` forever; run the read on a
        // thread so a regression fails the test instead of hanging it.
        let (tx, rx) = std::sync::mpsc::channel();
        let probe = fifo.clone();
        std::thread::spawn(move || {
            let _ = tx.send(read_metadata_toml(&probe).map_err(|err| err.to_string()));
        });
        let outcome = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|err| format!("reading a FIFO as metadata TOML blocked: {err}"))?;
        let err = outcome.err().ok_or("a FIFO must be refused")?;
        assert!(err.contains("is not a regular file"), "{err}");
        let err = read_metadata_toml("/dev/zero")
            .err()
            .ok_or("/dev/zero must be refused")?;
        assert!(err.to_string().contains("is not a regular file"), "{err}");
        Ok(())
    }
}
