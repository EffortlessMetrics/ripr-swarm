//! #14116: which indexed files an unindexed workspace member may compile.
//!
//! Draft/Fast indexes omit members that are not the changed package or its
//! reverse path-dependents. Such a member is an unseen compilation *target*
//! of the files it mounts through `#[path]` or `include!`: those files are
//! compiled in a crate ripr does not see (the #10149 generated-B case), so
//! they cannot be charged to the one root the index records.
//!
//! The member is read as a whole: every `.rs` file under its directory,
//! except its own `target/`. A file read that way needs no module edge to be
//! seen, so a default-layout `mod name;`, a `#[path]` that stays inside the
//! member, and an `include!` of a member file compile nothing ripr has not
//! read. Each top-level `#[path = "literal"] mod name;` that resolves outside
//! the member names one *mounted* file, which [`super::TargetRoots`] withholds
//! from placement while every other file keeps it.
//!
//! Anything this reading cannot bound answers `None`, which switches
//! placement off for the whole workspace as before: a symlink, a file that
//! is not a regular file, an unreadable or non-UTF-8 file, a member whose
//! read closure passes the file or byte budget (charged before each read), a
//! manifest target outside the member, a non-literal or nested `#[path]`,
//! `#![path]`, `r#path` or a `cfg_attr`-introduced path, an `include!` that
//! is not one literal or that leaves the member, and a mounted file that
//! itself declares `mod name;`, `#[path]` or `include!`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{
    ModulePathTarget, RustIndex, cfg_attr_may_introduce_path, mask_comments_and_strings,
    module_path_targets, own_path_literals, source_may_declare_file_module, whole_word_offsets,
};
use crate::analysis::extract::string_literal_spans;

/// The most `.rs` files one unindexed member may hold before its reading
/// fails closed.
const MEMBER_FILES: usize = 8192;

/// The most source bytes one unindexed member may hold before its reading
/// fails closed. Large enough for a generated or test-heavy automation crate,
/// small enough that the read stays a bounded part of one run.
const MEMBER_BYTES: usize = 64 * 1024 * 1024;

/// The largest member `Cargo.toml` read; a larger or non-regular manifest
/// fails closed.
const MANIFEST_BYTES: usize = 1024 * 1024;

/// The indexed files, relative to the workspace root, that some unindexed
/// member may compile into a crate ripr does not see. Empty when no member
/// is unindexed or none mounts a file outside itself. `None` when a member's
/// effect cannot be bounded, so no file may be placed.
pub(super) fn unseen_member_mounts(index: &RustIndex) -> Option<BTreeSet<PathBuf>> {
    let Some(root) = index
        .workspace_authority
        .as_ref()
        .map(|authority| authority.root.as_path())
    else {
        return Some(BTreeSet::new());
    };
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut mounted = BTreeSet::new();
    for member_dir in &index.workspace_member_dirs {
        let member_canon = member_dir
            .canonicalize()
            .unwrap_or_else(|_| member_dir.to_path_buf());
        let Some(relative) = member_canon
            .strip_prefix(&root_canon)
            .or_else(|_| member_canon.strip_prefix(root))
            .ok()
        else {
            continue;
        };
        let indexed = index.files().iter().any(|(path, _)| {
            let path = path.as_path();
            let path = path.strip_prefix("./").unwrap_or(path);
            path.starts_with(relative)
        });
        if indexed {
            continue;
        }
        for file in member_mounts(&member_canon)? {
            // A mount outside the repository is no indexed file.
            if let Ok(relative) = file.strip_prefix(&root_canon) {
                mounted.insert(relative.to_path_buf());
            }
        }
    }
    Some(mounted)
}

/// The canonical files outside `member_dir` (canonical) that the member
/// mounts through a top-level literal `#[path]`, or `None` when its sources
/// cannot be bounded.
fn member_mounts(member_dir: &Path) -> Option<BTreeSet<PathBuf>> {
    member_mounts_within(member_dir, MEMBER_FILES)
}

/// [`member_mounts`] with `file_cap` as the most distinct files it may read.
fn member_mounts_within(member_dir: &Path, file_cap: usize) -> Option<BTreeSet<PathBuf>> {
    let roots = manifest_targets(member_dir)?;
    let mut pending = member_sources(member_dir, file_cap)?;
    // A declared root that is not `.rs` was not listed; read it too.
    pending.extend(roots);
    if pending.is_empty() {
        // The member exists and nothing it compiles could be read.
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut mounted = BTreeSet::new();
    let mut included = BTreeSet::new();
    let mut resolving = BTreeSet::new();
    let mut bytes_read = 0usize;
    while let Some(path) = pending.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        // The file cap covers the distinct closure read here (listed files,
        // manifest roots, includes, in-member modules and mounts), not only
        // the initial `.rs` walk.
        if seen.len() + mounted.len() > file_cap {
            return None;
        }
        let source = read_bounded(&path, &mut bytes_read)?;
        let reading = file_reading(&path, &source, member_dir)?;
        if reading.resolves_paths {
            resolving.insert(path);
        }
        // An included member file that is not `.rs` was not listed, so it is
        // read here under the same rule.
        included.extend(reading.included.iter().cloned());
        pending.extend(reading.included);
        // So is an in-member `#[path]` module whose file is not `.rs`.
        pending.extend(reading.modules);
        for target in reading.mounted {
            if seen.contains(&target) || mounted.contains(&target) {
                continue;
            }
            if seen.len() + mounted.len() >= file_cap {
                return None;
            }
            let source = read_bounded(&target, &mut bytes_read)?;
            // A mounted file's own `mod name;`, `#[path]` or `include!`
            // resolves beside it, outside the member, into files this
            // reading would have to follow across the workspace.
            if !own_path_literals(&source, false).is_some_and(|literals| literals.is_empty())
                || source_may_declare_file_module(&source)
            {
                return None;
            }
            mounted.insert(target);
        }
    }
    // Pasted text resolves its own `#[path]` and `include!` from the
    // includer's context, which this reading does not follow.
    included.is_disjoint(&resolving).then_some(mounted)
}

/// Every crate target path the member's manifest names (`[lib]`, `[[bin]]`,
/// `[[test]]`, `[[bench]]`, `[[example]]` `path`, and `package.build`),
/// resolved from the member directory. Each must lie inside the member: a
/// target rooted elsewhere compiles another package's file as a crate root.
/// `None` for an escaping target, a non-string path, or an unreadable or
/// unparsable manifest. A target that does not exist compiles nothing.
fn manifest_targets(member_dir: &Path) -> Option<Vec<PathBuf>> {
    let manifest_path = member_dir.join("Cargo.toml");
    match std::fs::symlink_metadata(&manifest_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    }
    let manifest = String::from_utf8(read_regular(&manifest_path, MANIFEST_BYTES)?).ok()?;
    let value = toml::from_str::<toml::Value>(&manifest).ok()?;
    let mut declared = Vec::new();
    for key in ["lib", "bin", "test", "bench", "example"] {
        let entries = match value.get(key) {
            Some(toml::Value::Array(entries)) => entries.iter().collect::<Vec<_>>(),
            Some(table @ toml::Value::Table(_)) => vec![table],
            Some(_) => return None,
            None => continue,
        };
        for entry in entries {
            match entry.get("path") {
                Some(toml::Value::String(path)) => declared.push(path.clone()),
                Some(_) => return None,
                None => {}
            }
        }
    }
    match value
        .get("package")
        .and_then(|package| package.get("build"))
    {
        Some(toml::Value::String(path)) => declared.push(path.clone()),
        Some(toml::Value::Boolean(_)) | None => {}
        Some(_) => return None,
    }
    let mut targets = Vec::new();
    for path in declared {
        // Cargo joins the declared path verbatim, whitespace included.
        match member_dir.join(&path).canonicalize() {
            Ok(target) if target.starts_with(member_dir) => targets.push(target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return None,
        }
    }
    Some(targets)
}

/// Every `.rs` file under `member_dir`, skipping the member's own `target/`.
/// `None` for a symlink, an unlistable directory, a `.rs` entry that is not a
/// regular file, or past [`MEMBER_FILES`].
fn member_sources(member_dir: &Path, file_cap: usize) -> Option<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![member_dir.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).ok()? {
            let entry = entry.ok()?;
            let kind = entry.file_type().ok()?;
            let path = entry.path();
            if kind.is_symlink() {
                return None;
            }
            if kind.is_dir() {
                if !(directory == member_dir && entry.file_name() == "target") {
                    directories.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                if !kind.is_file() {
                    return None;
                }
                files.push(path);
                if files.len() > file_cap {
                    return None;
                }
            }
        }
    }
    Some(files)
}

/// The UTF-8 text of the regular file at `path`, counted against the
/// member's byte budget before it is read: at most the remaining budget plus
/// one byte is consumed from the file, so a file past the budget (or one that
/// grows while it is read) fails closed without its remainder being read. The
/// buffer grows with the bytes consumed; its exact capacity is not bounded
/// here beyond that. A symlink, a
/// directory, a FIFO, a device or any other non-regular file is refused.
fn read_bounded(path: &Path, bytes_read: &mut usize) -> Option<String> {
    let remaining = MEMBER_BYTES.checked_sub(*bytes_read)?;
    let bytes = read_regular(path, remaining)?;
    *bytes_read += bytes.len();
    String::from_utf8(bytes).ok()
}

/// The bytes of the regular file at `path` when it holds at most `limit`
/// bytes. Consumes at most `limit + 1` bytes, so an oversized or growing file
/// is refused without its remainder being read; a non-regular file is
/// refused before open.
fn read_regular(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    if !std::fs::symlink_metadata(path).ok()?.file_type().is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(limit).ok()?.checked_add(1)?)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

/// What one member file compiles beyond itself.
struct FileReading {
    /// Member files an `include!` pastes in.
    included: Vec<PathBuf>,
    /// Member files a top-level `#[path]` names as modules.
    modules: Vec<PathBuf>,
    /// Files outside the member a top-level `#[path]` mounts.
    mounted: Vec<PathBuf>,
    /// Whether the file names any `#[path]` or `include!` target.
    resolves_paths: bool,
}

fn file_reading(path: &Path, source: &str, member_dir: &Path) -> Option<FileReading> {
    let directory = path.parent()?;
    let masked = mask_comments_and_strings(source);
    let compact: String = masked.chars().filter(|ch| !ch.is_whitespace()).collect();
    // Spellings the `#[path` count below cannot see.
    if compact.contains("#![path")
        || compact.contains("r#path")
        || cfg_attr_may_introduce_path(&compact)
    {
        return None;
    }
    let mut included = Vec::new();
    for offset in whole_word_offsets(&masked, "include") {
        let after = offset + "include".len();
        if !masked
            .get(after..)
            .is_some_and(|rest| rest.trim_start().starts_with('!'))
        {
            continue;
        }
        let literal = include_literal(source, &masked, after)?;
        let target = directory.join(literal).canonicalize().ok()?;
        if !target.starts_with(member_dir) {
            return None;
        }
        included.push(target);
    }
    let attributes = compact.matches("#[path").count();
    let mut modules = Vec::new();
    let mut mounted = Vec::new();
    let resolves_paths = attributes > 0 || !included.is_empty();
    if attributes == 0 {
        return Some(FileReading {
            included,
            modules,
            mounted,
            resolves_paths,
        });
    }
    let mut literals = 0;
    for target in module_path_targets(source)? {
        match target {
            ModulePathTarget::Default => {}
            ModulePathTarget::Unknown => return None,
            ModulePathTarget::Literal(literal) => {
                literals += 1;
                let target = directory.join(literal).canonicalize().ok()?;
                if !target.is_file() {
                    return None;
                }
                if target.starts_with(member_dir) {
                    modules.push(target);
                } else {
                    mounted.push(target);
                }
            }
        }
    }
    // A `#[path` the top-level declarations do not account for sits on a
    // nested module or another item, whose base directory this reading does
    // not resolve.
    (literals == attributes).then_some(FileReading {
        included,
        modules,
        mounted,
        resolves_paths,
    })
}

/// The one plain string literal of the `include!` whose `!` follows byte
/// `after`: `include!("path")`, `include!["path"]` or `include!{"path"}`.
fn include_literal<'a>(source: &'a str, masked: &str, after: usize) -> Option<&'a str> {
    let rest = masked.get(after..)?;
    let bang = after + rest.find('!')?;
    let rest = masked.get(bang + 1..)?;
    let trimmed = rest.trim_start();
    let open = bang + 1 + (rest.len() - trimmed.len());
    let close = match trimmed.chars().next()? {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let (start, end, raw) = string_literal_spans(source)
        .into_iter()
        .find(|(start, _, _)| *start > open)?;
    if raw || !masked.get(open + 1..start)?.trim().is_empty() {
        return None;
    }
    let literal = source.get(start + 1..end)?;
    if literal.contains('\\') {
        return None;
    }
    let tail = masked.get(end + 1..)?.trim_start();
    let tail = tail.strip_prefix(',').map_or(tail, str::trim_start);
    tail.starts_with(close).then_some(literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-unseen-member-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        dir.canonicalize().map_err(|e| e.to_string())
    }

    #[test]
    fn reads_refuse_non_regular_and_oversized_files_before_loading_them() -> Result<(), String> {
        let dir = member("read")?;
        let result = (|| -> Result<(), String> {
            let file = dir.join("src/lib.rs");
            std::fs::write(&file, "pub fn a() {}\n").map_err(|e| e.to_string())?;
            let len = "pub fn a() {}\n".len();
            assert_eq!(read_regular(&file, len).map(|b| b.len()), Some(len));
            assert_eq!(
                read_regular(&file, len - 1),
                None,
                "one byte over the limit"
            );
            assert_eq!(
                read_regular(&dir.join("src"), usize::MAX - 1),
                None,
                "a directory"
            );
            // The member budget is charged before the read: with fewer bytes
            // left than the file holds, it is refused and nothing is charged.
            let mut spent = MEMBER_BYTES - (len - 1);
            assert_eq!(read_bounded(&file, &mut spent), None);
            assert_eq!(spent, MEMBER_BYTES - (len - 1));
            let mut spent = MEMBER_BYTES - len;
            assert!(read_bounded(&file, &mut spent).is_some());
            assert_eq!(spent, MEMBER_BYTES);
            #[cfg(unix)]
            {
                let link = dir.join("src/link.rs");
                std::os::unix::fs::symlink(&file, &link).map_err(|e| e.to_string())?;
                assert_eq!(read_regular(&link, usize::MAX - 1), None, "a symlink");
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_non_regular_rs_entry_refuses_the_member_walk() -> Result<(), String> {
        let dir = member("fifo")?;
        let result = (|| -> Result<(), String> {
            std::fs::write(dir.join("src/lib.rs"), "pub fn a() {}\n").map_err(|e| e.to_string())?;
            assert_eq!(member_sources(&dir, 8).map(|files| files.len()), Some(1));
            nix::unistd::mkfifo(&dir.join("src/pipe.rs"), nix::sys::stat::Mode::S_IRWXU)
                .map_err(|e| e.to_string())?;
            assert_eq!(member_sources(&dir, 8), None, "a FIFO named .rs");
            assert_eq!(member_mounts_within(&dir, 8), None);
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn the_file_cap_counts_the_whole_source_closure() -> Result<(), String> {
        let dir = member("closure")?;
        let result = (|| -> Result<(), String> {
            std::fs::write(
                dir.join("src/lib.rs"),
                "mod other;\ninclude!(\"part.in\");\n",
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(dir.join("src/other.rs"), "pub fn b() {}\n")
                .map_err(|e| e.to_string())?;
            std::fs::write(dir.join("src/part.in"), "pub fn c() {}\n")
                .map_err(|e| e.to_string())?;
            // Two `.rs` files fit a cap of two; the included `part.in` makes
            // the closure three files.
            assert_eq!(member_sources(&dir, 2).map(|files| files.len()), Some(2));
            assert_eq!(member_mounts_within(&dir, 2), None, "closure past the cap");
            assert_eq!(member_mounts_within(&dir, 3), Some(BTreeSet::new()));
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_whitespace_bearing_declared_root_is_read_as_cargo_spells_it() -> Result<(), String> {
        let dir = member("spaced-root")?;
        let outside = dir.with_file_name(format!(
            "ripr-unseen-member-spaced-outside-{}.rs",
            std::process::id()
        ));
        let result = (|| -> Result<(), String> {
            std::fs::write(&outside, "pub fn o() {}\n").map_err(|e| e.to_string())?;
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname = \"m\"\nversion = \"0.0.0\"\n[lib]\npath = \"src/root.in \"\n",
            )
            .map_err(|e| e.to_string())?;
            let mount = format!(
                "#[path = \"../../{}\"]\nmod outside;\n",
                outside.file_name().and_then(|n| n.to_str()).ok_or("name")?
            );
            // Only the exact spelling, trailing space included, exists.
            std::fs::write(dir.join("src/root.in "), mount).map_err(|e| e.to_string())?;
            assert!(!dir.join("src/root.in").exists());
            let target = outside.canonicalize().map_err(|e| e.to_string())?;
            assert_eq!(
                member_mounts_within(&dir, 8),
                Some(BTreeSet::from([target])),
                "the declared root's mount is seen"
            );
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&outside);
        result
    }

    #[test]
    fn an_oversized_or_non_regular_manifest_refuses() -> Result<(), String> {
        let dir = member("manifest")?;
        let result = (|| -> Result<(), String> {
            assert_eq!(manifest_targets(&dir), Some(Vec::new()), "no manifest");
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname = \"m\"\nversion = \"0.0.0\"\n",
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(manifest_targets(&dir), Some(Vec::new()));
            let padded = format!(
                "[package]\nname = \"m\"\nversion = \"0.0.0\"\n#{}\n",
                "x".repeat(MANIFEST_BYTES)
            );
            std::fs::write(dir.join("Cargo.toml"), padded).map_err(|e| e.to_string())?;
            assert_eq!(manifest_targets(&dir), None, "manifest past its cap");
            std::fs::remove_file(dir.join("Cargo.toml")).map_err(|e| e.to_string())?;
            std::fs::create_dir(dir.join("Cargo.toml")).map_err(|e| e.to_string())?;
            assert_eq!(manifest_targets(&dir), None, "a directory named Cargo.toml");
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}
