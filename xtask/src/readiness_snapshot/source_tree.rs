use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::identity::git_output;

const MAX_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_BATCH_FILES: usize = 64;

struct TrackedFile {
    path: String,
    oid: String,
}

/// Hash actual tracked bytes independently of Git's timestamp/index stat cache.
pub(super) fn tracked_bytes_match(root: &Path, commit: &str) -> Result<bool, String> {
    let root_text = root.to_str().ok_or("source root is not UTF-8")?;
    let tree = git_output(&[
        "-C",
        root_text,
        "ls-tree",
        "-r",
        "-z",
        "--full-tree",
        commit,
    ])?;
    if !tree.status.success() {
        return Err("could not read the committed source tree".to_string());
    }
    let mut files = Vec::new();
    let mut directories = BTreeSet::new();
    for record in tree
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("invalid committed tree record")?;
        let header = std::str::from_utf8(&record[..tab]).map_err(|_| "invalid tree metadata")?;
        let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || fields[1] != "blob"
            || !matches!(fields[0], "100644" | "100755")
            || !valid_oid(fields[2])
        {
            return Err(
                "source tree requires regular files; links and unknown modes are unsupported"
                    .to_string(),
            );
        }
        let relative = std::str::from_utf8(&record[tab + 1..])
            .map_err(|_| "tracked source paths must be UTF-8")?;
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty() || !relative.components().all(|part| {
            matches!(part, Component::Normal(name) if !name.to_string_lossy().eq_ignore_ascii_case(".git"))
        }) {
            return Err("tracked source path is outside the supported source tree".to_string());
        }
        check_directories(root, relative, &mut directories)?;
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot inspect tracked source file: {e}"))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err("tracked source must be a regular file, without symlinks".to_string());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if (metadata.permissions().mode() & 0o100 != 0) != (fields[0] == "100755") {
                return Ok(false);
            }
        }
        files.push(TrackedFile {
            path: path
                .to_str()
                .ok_or("tracked source path is not UTF-8")?
                .to_string(),
            oid: fields[2].to_string(),
        });
    }
    if files.is_empty() {
        return Err("cannot establish source cleanliness without tracked files".to_string());
    }
    let prefix = ["-C", root_text, "hash-object", "--no-filters", "--"];
    let prefix_bytes = prefix.iter().map(|arg| arg.len() + 1).sum::<usize>();
    let mut batch = Vec::new();
    let mut argument_bytes = prefix_bytes;
    for file in &files {
        let bytes = file.path.len() + 1;
        if prefix_bytes + bytes > MAX_ARGUMENT_BYTES {
            return Err("tracked path exceeds the bounded Git hash argument budget".to_string());
        }
        if !batch.is_empty()
            && (batch.len() == MAX_BATCH_FILES || argument_bytes + bytes > MAX_ARGUMENT_BYTES)
        {
            if !batch_matches(&prefix, &batch)? {
                return Ok(false);
            }
            batch.clear();
            argument_bytes = prefix_bytes;
        }
        batch.push(file);
        argument_bytes += bytes;
    }
    batch_matches(&prefix, &batch)
}

fn check_directories(
    root: &Path,
    relative: &Path,
    checked: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let mut current = root.to_path_buf();
    let parent = relative
        .parent()
        .ok_or("tracked source path has no parent")?;
    for component in std::iter::once(None).chain(parent.components().map(Some)) {
        if let Some(component) = component {
            current.push(component.as_os_str());
        }
        if checked.contains(&current) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|e| format!("cannot inspect tracked source directory: {e}"))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err("tracked source parent is a symlink or non-directory".to_string());
        }
        checked.insert(current.clone());
    }
    Ok(())
}

fn batch_matches(prefix: &[&str], files: &[&TrackedFile]) -> Result<bool, String> {
    let args = prefix
        .iter()
        .copied()
        .chain(files.iter().map(|file| file.path.as_str()))
        .collect::<Vec<_>>();
    let output = git_output(&args)?;
    if !output.status.success() {
        return Err("could not hash tracked source bytes without filters".to_string());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| "invalid Git hash output")?;
    let hashes = text.lines().collect::<Vec<_>>();
    if hashes.len() != files.len() || hashes.iter().any(|oid| !valid_oid(oid)) {
        return Err("Git hash output does not cover the complete source batch".to_string());
    }
    Ok(hashes
        .iter()
        .zip(files)
        .all(|(actual, expected)| *actual == expected.oid))
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}
