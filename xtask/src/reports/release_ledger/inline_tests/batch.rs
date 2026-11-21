//! Read the requested immutable blob sides in Git input order. If a batch is
//! unavailable, the caller executes its original per-side reads and diagnostics.

use super::{GIT_TIMEOUT, Side, side_at, side_from_bytes};
use crate::run::{ByteCaptureBudget, capture_bytes_in_dir_with_budget};
use std::path::Path;

// A worker holds at most four files (eight sides) in one batch. Oversized
// blobs keep their original per-file read lifetime on fallback.
const STDOUT_LIMIT: usize = 2 * 1024 * 1024;

pub(super) struct Reader<'a> {
    repo: &'a Path,
    prepared: std::vec::IntoIter<Side>,
}

impl<'a> Reader<'a> {
    pub(super) fn new(repo: &'a Path, requests: &[(&str, &str)]) -> Self {
        Self {
            repo,
            prepared: sides(repo, requests).unwrap_or_default().into_iter(),
        }
    }

    pub(super) fn read(&mut self, revision: Option<&str>, path: &str) -> Result<Side, String> {
        match revision {
            Some(rev) => self
                .prepared
                .next()
                .map_or_else(|| side_at(self.repo, rev, path), Ok),
            None => Ok(Side::empty()),
        }
    }
}

fn sides(repo: &Path, requests: &[(&str, &str)]) -> Option<Vec<Side>> {
    if requests.len() < 2 || requests.iter().any(|(_, path)| path.contains(['\r', '\n'])) {
        return None;
    }
    let mut input = String::new();
    for (rev, path) in requests {
        input.push_str(rev);
        input.push(':');
        input.push_str(path);
        input.push('\n');
    }
    let output = capture_bytes_in_dir_with_budget(
        Path::new("git"),
        &["cat-file".to_string(), "--batch".to_string()],
        (repo, Some(input.as_bytes())),
        &[],
        ByteCaptureBudget {
            timeout: GIT_TIMEOUT,
            stdout_bytes: STDOUT_LIMIT,
            stderr_bytes: 64 * 1024,
        },
        "release ledger inline-test blob batch",
    )
    .ok()?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return None;
    }
    let blobs = decode(&output.stdout, requests.len())?;
    Some(blobs.into_iter().map(side_from_bytes).collect())
}

fn decode(mut bytes: &[u8], count: usize) -> Option<Vec<&[u8]>> {
    let mut blobs = Vec::with_capacity(count);
    for _ in 0..count {
        let header_end = bytes.iter().position(|byte| *byte == b'\n')?;
        let (header, payload) = bytes.split_at_checked(header_end)?;
        let header = std::str::from_utf8(header).ok()?;
        let mut fields = header.split(' ');
        let oid = fields.next()?;
        if !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|b| b.is_ascii_hexdigit())
            || fields.next()? != "blob"
        {
            return None;
        }
        let size = fields.next()?.parse::<usize>().ok()?;
        if fields.next().is_some() {
            return None;
        }
        let (blob, rest) = payload.strip_prefix(b"\n")?.split_at_checked(size)?;
        bytes = rest.strip_prefix(b"\n")?;
        blobs.push(blob);
    }
    bytes.is_empty().then_some(blobs)
}

#[cfg(test)]
mod tests;
