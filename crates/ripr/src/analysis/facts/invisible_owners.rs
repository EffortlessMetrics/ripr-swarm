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
//! `#[path]` or a `cfg_attr`-introduced `path`. Unreadable files, an
//! oversized package and an unresolved `include!` in the package all answer
//! "not visible", except an `include!` of a literal `.rs` file of the same
//! package inside an inline module whose text declares no `mod <name>;`: that
//! file's text is already in the package scan.

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
    manifest_dirs: BTreeMap<PathBuf, Option<PathBuf>>,
}

impl<'index> InvisibleOwners<'index> {
    pub(super) fn new(index: &'index RustIndex, workspace_root: &'index Path) -> Self {
        Self {
            index,
            workspace_root,
            packages: BTreeMap::new(),
            manifest_dirs: BTreeMap::new(),
        }
    }

    /// The package directory of `file`, memoized per containing directory.
    fn package_of(&mut self, file: &Path) -> Option<PathBuf> {
        let directory = file.parent()?.to_path_buf();
        if let Some(known) = self.manifest_dirs.get(&directory) {
            return known.clone();
        }
        let found = nearest_manifest_dir(self.workspace_root, file);
        self.manifest_dirs.insert(directory, found.clone());
        found
    }

    /// True when every unresolved include of `package` is a literal `.rs`
    /// file of the same package, pasted into an inline module, whose text
    /// declares no `mod <name>;`.
    fn includes_accounted(&mut self, package: &Path, name: &str) -> bool {
        let index = self.index;
        for limitation in &index.include_limitations {
            let Some(parent_package) = self.package_of(&limitation.parent) else {
                return false;
            };
            if parent_package != package {
                continue;
            }
            if limitation.reason_code != "rust_include_nested_module_context" {
                return false;
            }
            let Some(literal) = include_literal(&limitation.expression) else {
                return false;
            };
            let Some(target) = limitation
                .parent
                .parent()
                .and_then(|directory| normalize_relative(&directory.join(literal)))
            else {
                return false;
            };
            if self.package_of(&target).as_deref() != Some(package) {
                return false;
            }
            let Ok(source) = std::fs::read_to_string(self.workspace_root.join(&target)) else {
                return false;
            };
            let mut text = PackageText::default();
            if record_declarations(&source, &mut text).is_none()
                || text.out_of_line_modules.contains_key(name)
            {
                return false;
            }
        }
        true
    }

    /// True only when every declaration that could own `child` (declared as
    /// `mod <name>;`) is one the index captured.
    pub(super) fn all_owners_visible(&mut self, child: &Path, name: &str) -> bool {
        let Some(package) = self.package_of(child) else {
            return false;
        };
        if !self.includes_accounted(&package, name) {
            return false;
        }
        let Some(child_file_name) = child.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let mut captured_modules = 0usize;
        let mut captured_paths = 0usize;
        let index = self.index;
        for (file, facts) in &index.files {
            if !file.starts_with(&package) || self.package_of(file).as_ref() != Some(&package) {
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

/// The string literal of `include!("x.rs")` when it names a `.rs` file.
fn include_literal(expression: &str) -> Option<&str> {
    let start = expression.find('"')? + 1;
    let end = start + expression[start..].find('"')?;
    let literal = &expression[start..end];
    let rest = &expression[end + 1..];
    (literal.ends_with(".rs") && !literal.contains('\\') && !rest.contains('"')).then_some(literal)
}

/// `path` with `.` and `..` folded; `None` when it leaves its root.
fn normalize_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(out)
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
        }
    }
    // A `path = ..` introduced by `cfg_attr(..)`: the effective target
    // depends on configuration. Named format arguments (`path = p`) outside
    // a `cfg_attr` are ordinary code.
    for (at, _) in masked.match_indices("cfg_attr") {
        if at > 0 && ident(bytes[at - 1]) {
            continue;
        }
        let Some(open) = masked[at + 8..].find('(').map(|offset| at + 8 + offset) else {
            continue;
        };
        if !masked[at + 8..open].trim().is_empty() {
            continue;
        }
        let mut depth = 0usize;
        let mut close = masked.len();
        for (offset, byte) in bytes[open..].iter().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        let inner = &masked[open..close];
        let inner_bytes = inner.as_bytes();
        let introduces_path = inner.match_indices("path").any(|(at, _)| {
            let before_ok = at == 0 || !ident(inner_bytes[at - 1]);
            let after_ok = inner_bytes.get(at + 4).is_none_or(|byte| !ident(*byte));
            let rest = inner[at + 4..].trim_start();
            before_ok && after_ok && rest.starts_with('=') && !rest.starts_with("==")
        });
        if introduces_path {
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
        assert!(
            record("fn f(p: &str) -> String { format!(\"{path}\", path = p) }\n").is_some(),
            "a named format argument is not a cfg_attr path"
        );
    }
}
