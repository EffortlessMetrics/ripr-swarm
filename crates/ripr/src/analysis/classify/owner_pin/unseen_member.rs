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
//! placement off for the whole workspace as before: a symlink, an
//! unreadable or non-UTF-8 file, a member past the file or byte budget, a
//! manifest target outside the member, a non-literal or nested `#[path]`,
//! `#![path]`, `r#path` or a `cfg_attr`-introduced path, an `include!` that
//! is not one literal or that leaves the member, and a mounted file that
//! itself declares `mod name;`, `#[path]` or `include!`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{
    ModulePathTarget, RustIndex, cfg_attr_may_introduce_path,
    declared_crate_root_paths_from_manifest, mask_comments_and_strings, module_path_targets,
    own_path_literals, source_may_declare_file_module, whole_word_offsets,
};
use crate::analysis::extract::string_literal_spans;

/// The most `.rs` files one unindexed member may hold before its reading
/// fails closed.
const MEMBER_FILES: usize = 8192;

/// The most source bytes one unindexed member may hold before its reading
/// fails closed. Large enough for a generated or test-heavy automation crate,
/// small enough that the read stays a bounded part of one run.
const MEMBER_BYTES: usize = 64 * 1024 * 1024;

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
    manifest_targets_stay_inside(member_dir)?;
    let mut pending = member_sources(member_dir)?;
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
        let source = read_bounded(&path, &mut bytes_read)?;
        let reading = file_reading(&path, &source, member_dir)?;
        if reading.resolves_paths {
            resolving.insert(path);
        }
        // An included member file that is not `.rs` was not listed, so it is
        // read here under the same rule.
        included.extend(reading.included.iter().cloned());
        pending.extend(reading.included);
        for target in reading.mounted {
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

/// Every crate target path the member's manifest names must resolve inside
/// the member: a target rooted elsewhere compiles another package's file as
/// a crate root.
fn manifest_targets_stay_inside(member_dir: &Path) -> Option<()> {
    let manifest = match std::fs::read_to_string(member_dir.join("Cargo.toml")) {
        Ok(manifest) => manifest,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(()),
        Err(_) => return None,
    };
    let mut declared = declared_crate_root_paths_from_manifest(&manifest, member_dir);
    let build = toml::from_str::<toml::Value>(&manifest)
        .ok()
        .and_then(|value| {
            value
                .get("package")?
                .get("build")?
                .as_str()
                .map(|build| member_dir.join(build.trim()))
        });
    declared.extend(build);
    for path in declared {
        match path.canonicalize() {
            Ok(target) if target.starts_with(member_dir) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return None,
        }
    }
    Some(())
}

/// Every `.rs` file under `member_dir`, skipping the member's own `target/`.
/// `None` for a symlink, an unlistable directory, or past [`MEMBER_FILES`].
fn member_sources(member_dir: &Path) -> Option<Vec<PathBuf>> {
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
                files.push(path);
                if files.len() > MEMBER_FILES {
                    return None;
                }
            }
        }
    }
    Some(files)
}

/// The UTF-8 text of `path`, counted against the member's byte budget.
fn read_bounded(path: &Path, bytes_read: &mut usize) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    *bytes_read = bytes_read.checked_add(bytes.len())?;
    if *bytes_read > MEMBER_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// What one member file compiles beyond itself.
struct FileReading {
    /// Member files an `include!` pastes in.
    included: Vec<PathBuf>,
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
    let mut mounted = Vec::new();
    let resolves_paths = attributes > 0 || !included.is_empty();
    if attributes == 0 {
        return Some(FileReading {
            included,
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
                if !target.starts_with(member_dir) {
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
