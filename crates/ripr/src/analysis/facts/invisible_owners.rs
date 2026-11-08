//! Whether every possible owner of an out-of-line module file is visible to
//! the composition pass (#13736 review).
//!
//! The parser producer captures only top-level `mod name;` declarations of
//! files it parsed. A declaration inside an inline module, inside a macro
//! token tree (`cfg_if!`, a local `macro_rules!` invocation), in a
//! lexical-fallback or non-UTF-8 file, or in a file the index did not take in
//! (a partial or package-narrowed run) can compile the same child file
//! without the owner the pass saw. A verdict that removes production probes
//! (the Kani-only verdict) is granted only when such an owner is ruled out.
//!
//! The check is textual and fail-closed, over every `.rs` file of the child's
//! package on disk (nested packages, `target` and hidden directories
//! excluded): the comment- and string-masked count of `mod <name>;` must
//! equal the captured declarations of that name, the count of `#[path]`
//! literals naming the child's file must equal the captured ones, and no
//! file may hold a macro-generated (`mod $`) declaration, a non-literal
//! `#[path]` or a `cfg_attr`-introduced `path`. Unreadable files, unresolved
//! includes and an oversized package all answer "not visible".

use super::model::{ModulePathTarget, RustIndex};
use crate::analysis::extract::mask_comments_and_strings;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Upper bound on the package files read for one verdict; larger packages
/// fail closed rather than make composition unbounded.
const MAX_SCANNED_FILES: usize = 20_000;

/// Directory-walk depth bound, mirroring the composition chain bound.
const MAX_SCAN_DEPTH: usize = 32;

/// Masked declaration text of one package.
#[derive(Default)]
struct PackageText {
    /// `mod <name>;` occurrences by name.
    out_of_line_modules: BTreeMap<String, usize>,
    /// `#[path = "..."]` literals by final path component.
    path_file_names: BTreeMap<String, usize>,
}

pub(super) struct InvisibleOwners<'index> {
    index: &'index RustIndex,
    workspace_root: &'index Path,
    packages: BTreeMap<PathBuf, Option<PackageText>>,
}

impl<'index> InvisibleOwners<'index> {
    pub(super) fn new(index: &'index RustIndex, workspace_root: &'index Path) -> Self {
        Self {
            index,
            workspace_root,
            packages: BTreeMap::new(),
        }
    }

    /// True only when every declaration that could own `child` (declared as
    /// `mod <name>;`) is one the index captured.
    pub(super) fn all_owners_visible(&mut self, child: &Path, name: &str) -> bool {
        if !self.index.include_limitations.is_empty() {
            return false;
        }
        let Some(package) = nearest_manifest_dir(self.workspace_root, child) else {
            return false;
        };
        let Some(child_file_name) = child.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let mut captured_modules = 0usize;
        let mut captured_paths = 0usize;
        for (file, facts) in &self.index.files {
            if !file.starts_with(&package)
                || nearest_manifest_dir(self.workspace_root, file).as_ref() != Some(&package)
            {
                continue;
            }
            if self.index.non_utf8_sources.contains(file) {
                return false;
            }
            if facts.used_lexical_fallback {
                // No captured declarations: its text is counted on disk, so
                // any declaration it holds shows up as a mismatch.
                continue;
            }
            for declaration in &facts.module_declarations {
                if declaration.name == name {
                    captured_modules += 1;
                }
                match &declaration.path_target {
                    ModulePathTarget::Unknown => return false,
                    ModulePathTarget::Literal(literal)
                        if final_component(literal) == child_file_name =>
                    {
                        captured_paths += 1;
                    }
                    _ => {}
                }
            }
        }
        let workspace_root = self.workspace_root;
        let Some(text) = self
            .packages
            .entry(package.clone())
            .or_insert_with(|| scan_package(workspace_root, &package))
        else {
            return false;
        };
        text.out_of_line_modules.get(name).copied().unwrap_or(0) == captured_modules
            && text
                .path_file_names
                .get(child_file_name)
                .copied()
                .unwrap_or(0)
                == captured_paths
    }
}

/// The nearest ancestor directory of `file` (relative to `workspace_root`)
/// holding a `Cargo.toml`.
fn nearest_manifest_dir(workspace_root: &Path, file: &Path) -> Option<PathBuf> {
    let mut cursor = file.parent()?.to_path_buf();
    for _ in 0..=MAX_SCAN_DEPTH {
        if workspace_root.join(&cursor).join("Cargo.toml").is_file() {
            return Some(cursor);
        }
        if !cursor.pop() {
            return None;
        }
    }
    None
}

fn final_component(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Reads every `.rs` file of the package; `None` on any doubt.
fn scan_package(workspace_root: &Path, package: &Path) -> Option<PackageText> {
    let root = workspace_root.join(package);
    let mut text = PackageText::default();
    let mut pending = vec![(root.clone(), 0usize)];
    let mut scanned = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_SCAN_DEPTH {
            return None;
        }
        if directory != root && directory.join("Cargo.toml").is_file() {
            // Another package: its files cannot declare this package's
            // modules without a `#[path]`, which is outside this check.
            continue;
        }
        let entries = std::fs::read_dir(&directory).ok()?;
        for entry in entries {
            let entry = entry.ok()?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_str()?;
            let kind = entry.file_type().ok()?;
            if kind.is_dir() {
                if !file_name.starts_with('.') && file_name != "target" {
                    pending.push((path, depth + 1));
                }
            } else if file_name.ends_with(".rs") {
                scanned += 1;
                if scanned > MAX_SCANNED_FILES {
                    return None;
                }
                let source = std::fs::read_to_string(&path).ok()?;
                record_declarations(&source, &mut text)?;
            }
        }
    }
    Some(text)
}

/// Adds the declaration text of one file; `None` for a shape this check
/// cannot account for.
fn record_declarations(source: &str, text: &mut PackageText) -> Option<()> {
    if !source.contains("mod") && !source.contains("path") {
        return Some(());
    }
    let masked = mask_comments_and_strings(source);
    let bytes = masked.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    for (at, _) in masked.match_indices("mod") {
        if at > 0 && ident(bytes[at - 1]) {
            continue;
        }
        let rest = &masked[at + 3..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        if rest.starts_with('$') {
            return None;
        }
        let name_len = rest.bytes().take_while(|byte| ident(*byte)).count();
        if name_len == 0 {
            continue;
        }
        if rest[name_len..].trim_start().starts_with(';') {
            *text
                .out_of_line_modules
                .entry(rest[..name_len].to_string())
                .or_default() += 1;
        }
    }
    for (at, _) in masked.match_indices("path") {
        let before_ok = at == 0 || !ident(bytes[at - 1]);
        let after_ok = bytes.get(at + 4).is_none_or(|byte| !ident(*byte));
        if !before_ok || !after_ok {
            continue;
        }
        let after = masked[at + 4..].trim_start();
        if !after.starts_with('=') || after.starts_with("==") {
            continue;
        }
        let opener = masked[..at].trim_end();
        if opener.ends_with("#[") {
            // The literal itself is masked; read it from the source, whose
            // byte offsets the mask preserves.
            let value = source[at + 4..]
                .trim_start()
                .strip_prefix('=')?
                .trim_start();
            let literal = value.strip_prefix('"')?;
            let end = literal.find('"')?;
            *text
                .path_file_names
                .entry(final_component(&literal[..end]).to_string())
                .or_default() += 1;
        } else if opener.ends_with(',') || opener.ends_with('(') {
            // `path = ..` as an argument (`cfg_attr(.., path = "..")`): the
            // effective target depends on configuration.
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(source: &str) -> Option<PackageText> {
        let mut text = PackageText::default();
        record_declarations(source, &mut text)?;
        Some(text)
    }

    #[test]
    fn counts_out_of_line_declarations_and_path_literals() -> Result<(), String> {
        let text = record(
            "#[cfg(kani)] mod proofs; mod helpers;\n// mod proofs;\nconst S: &str = \"mod proofs;\";\nmod inline { mod proofs; }\n#[path = \"dir/proofs.rs\"]\nmod renamed;\n",
        )
        .ok_or("plain declarations are accountable")?;
        assert_eq!(text.out_of_line_modules.get("proofs"), Some(&2));
        assert_eq!(text.out_of_line_modules.get("helpers"), Some(&1));
        assert_eq!(text.out_of_line_modules.get("renamed"), Some(&1));
        assert_eq!(text.out_of_line_modules.get("inline"), None);
        assert_eq!(text.path_file_names.get("proofs.rs"), Some(&1));
        Ok(())
    }

    #[test]
    fn unaccountable_shapes_fail_closed() {
        assert!(record("macro_rules! m { ($n:ident) => { mod $n; } }\n").is_none());
        assert!(record("#[cfg_attr(kani, path = \"proofs.rs\")]\nmod proofs;\n").is_none());
        assert!(record("#[path = concat!(\"a\", \".rs\")]\nmod proofs;\n").is_none());
        assert!(record("fn module_path() {}\nlet path = 1;\n").is_some());
    }
}
