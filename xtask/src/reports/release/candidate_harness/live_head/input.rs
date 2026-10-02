//! Bounded observed snapshots, not atomic or authenticated filesystem custody.
use super::super::safe_artifact_path;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_RETAINED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Eq, PartialEq)]
struct Snapshot {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl Snapshot {
    fn capture(metadata: &Metadata) -> Result<Self, String> {
        if !metadata.is_file() {
            return Err("direct manifest input is not a regular file".to_string());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|e| format!("input modification time: {e}"))?,
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino()),
        })
    }
}

pub(super) fn read_owned(root: &Path, relative: &str, remaining: u64) -> Result<Vec<u8>, String> {
    if !safe_artifact_path(Path::new(relative)) {
        return Err(
            "direct manifest input must be an ordinary controller-relative path".to_string(),
        );
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("resolve direct manifest input {relative}: {e}"))?;
    if !path.starts_with(root) {
        return Err(format!(
            "direct manifest input escapes controller: {relative}"
        ));
    }
    let limit = remaining.min(MAX_INPUT_BYTES);
    if limit == 0 {
        return Err("direct manifest exceeds 64 MiB aggregate retained-byte budget".to_string());
    }
    let inspect = || -> Result<Snapshot, String> {
        Snapshot::capture(
            &std::fs::metadata(&path)
                .map_err(|e| format!("inspect direct manifest input {relative}: {e}"))?,
        )
    };
    let before = inspect()?;
    if before.len == 0 || before.len > limit {
        return Err(format!(
            "direct manifest input {relative} is empty or exceeds its {limit}-byte budget (16 MiB per file, 64 MiB aggregate)"
        ));
    }
    let mut file =
        File::open(&path).map_err(|e| format!("open direct manifest input {relative}: {e}"))?;
    let opened = Snapshot::capture(
        &file
            .metadata()
            .map_err(|e| format!("inspect opened input: {e}"))?,
    )?;
    if before != opened {
        return Err(format!(
            "direct manifest input changed while opening: {relative}"
        ));
    }
    let bytes = read_limited(&mut file, limit)?;
    let after = Snapshot::capture(
        &file
            .metadata()
            .map_err(|e| format!("inspect read input: {e}"))?,
    )?;
    if opened != after
        || after != inspect()?
        || bytes.len() as u64 != after.len
        || root
            .join(relative)
            .canonicalize()
            .map_err(|e| format!("re-resolve input: {e}"))?
            != path
    {
        return Err(format!(
            "direct manifest input changed while reading: {relative}"
        ));
    }
    Ok(bytes)
}

// The limit applies to bytes actually read, even when a file grows after stat.
// Snapshot metadata is observational: same-size/timestamp-preserving writes,
// path swaps between checks and later mutation cannot be excluded without locks.
fn read_limited(reader: &mut impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read direct manifest input: {e}"))?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(format!(
            "direct manifest input is empty or exceeds its {limit}-byte read budget"
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_read_limit_rejects_growth_without_consuming_the_tail() -> Result<(), String> {
        let mut reader = std::io::Cursor::new(b"123456789".to_vec());
        if read_limited(&mut reader, 4).is_ok() || reader.position() != 5 {
            return Err("limit+1 read consumed an unbounded tail or admitted growth".to_string());
        }
        let bytes = read_limited(&mut std::io::Cursor::new(b"1234"), 4)?;
        if bytes != b"1234" {
            return Err("exact read budget rejected".to_string());
        }
        Ok(())
    }
}
