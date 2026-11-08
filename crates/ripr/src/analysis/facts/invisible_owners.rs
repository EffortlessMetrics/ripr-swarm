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
//! Every read goes through the index's own snapshot: the per-file text is the
//! source the index holds (committed bytes for a dirty path in committed
//! mode), the package file set comes from the workspace discovery authority
//! the run itself uses, and manifests and include targets are read through
//! `committed_source`. Nothing reads working-tree bytes behind the snapshot.
//!
//! Fail-closed rules for the child's package (nested packages excluded):
//!
//! - no committed source is missing from the working tree, every indexed
//!   file is parsed (no lexical fallback) and UTF-8, and every discovered
//!   `.rs` file the index did not take in (a build script, an example, a
//!   package-narrowed file) is read through the snapshot and masked: it
//!   refuses the verdict when it declares `mod <name>;` anywhere or carries a
//!   `#[path]` literal naming the child's file (`build.rs` owning
//!   `src/build_commit_record.rs` is exactly this shape), and refuses the
//!   whole package when its text is unreadable or unaccountable;
//! - no indexed file holds a macro-generated `mod $name;`, a `cfg_attr`
//!   that introduces `path`, or a non-literal `#[path]`;
//! - **per file**, the comment- and string-masked count of `mod <name>;`
//!   equals that file's captured declarations of `name`, and the count of
//!   `#[path]` literals naming the child's file equals the captured ones, so
//!   one file's surplus can never cancel another's deficit;
//! - every unresolved `include!` of the package is an inline-module include
//!   of a literal `.rs` file of the package whose text declares no
//!   `mod <name>;`.
//!
//! Cost: one discovery listing per package and one masked pass over each
//! indexed file that mentions the module or child file name (or could hide a
//! declaration), cached for the composition pass. Only packages holding a
//! `#[cfg(kani)]` module edge are examined, and each further edge is
//! answered from the caches.

use super::model::{ModulePathTarget, RustIndex};
use crate::analysis::committed_source::{self, CommittedSourceRead};
use crate::analysis::extract::mask_comments_and_strings;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Directory-walk depth bound for manifest lookups, mirroring the
/// composition chain bound.
const MAX_SCAN_DEPTH: usize = 32;

/// Masked declaration text of one file.
#[derive(Default)]
struct FileText {
    /// `mod <name>;` occurrences by name.
    out_of_line_modules: BTreeMap<String, usize>,
    /// `#[path = "..."]` literals by final path component.
    path_file_names: BTreeMap<String, usize>,
}

/// What one package allows, computed once per composition pass.
struct PackageOwners {
    /// The package's indexed files.
    files: Vec<PathBuf>,
    /// Masked text of discovered package files the index did not take in.
    unindexed: Vec<FileText>,
    /// Module names some unresolved nested-module include could declare.
    include_declared: BTreeSet<String>,
}

pub(super) struct InvisibleOwners<'index> {
    index: &'index RustIndex,
    workspace_root: &'index Path,
    /// Directory -> package directory (nearest snapshot manifest).
    manifest_dirs: BTreeMap<PathBuf, Option<PathBuf>>,
    /// Package -> its indexed files, built on first use.
    indexed_by_package: Option<BTreeMap<PathBuf, Vec<PathBuf>>>,
    /// Package -> verdict inputs; `None` refuses every edge of the package.
    packages: BTreeMap<PathBuf, Option<PackageOwners>>,
    /// File -> masked declaration text; `None` for an unaccountable shape.
    texts: BTreeMap<PathBuf, Option<FileText>>,
}

impl<'index> InvisibleOwners<'index> {
    pub(super) fn new(index: &'index RustIndex, workspace_root: &'index Path) -> Self {
        Self {
            index,
            workspace_root,
            manifest_dirs: BTreeMap::new(),
            indexed_by_package: None,
            packages: BTreeMap::new(),
            texts: BTreeMap::new(),
        }
    }

    /// True only when every declaration that could own `child` (declared as
    /// `mod <name>;`) is one the index captured.
    pub(super) fn all_owners_visible(&mut self, child: &Path, name: &str) -> bool {
        let Some(package) = self.package_of(child) else {
            return false;
        };
        let Some(child_file_name) = child.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        if !self.packages.contains_key(&package) {
            let owners = self.package_owners(&package);
            self.packages.insert(package.clone(), owners);
        }
        let files = match self.packages.get(&package) {
            Some(Some(owners))
                if !owners.include_declared.contains(name)
                    && !owners.unindexed.iter().any(|text| {
                        text.out_of_line_modules.contains_key(name)
                            || text.path_file_names.contains_key(child_file_name)
                    }) =>
            {
                owners.files.clone()
            }
            _ => return false,
        };
        let index = self.index;
        for file in files {
            let Some(facts) = index.files.get(&file) else {
                return false;
            };
            if !facts.source.contains(name) && !facts.source.contains(child_file_name) {
                continue;
            }
            let captured_modules = facts
                .module_declarations
                .iter()
                .filter(|declaration| declaration.name == name)
                .count();
            let captured_paths = facts
                .module_declarations
                .iter()
                .filter(|declaration| {
                    matches!(&declaration.path_target, ModulePathTarget::Literal(literal)
                        if final_component(literal) == child_file_name)
                })
                .count();
            let source = facts.source.clone();
            let Some(text) = self.texts.entry(file).or_insert_with(|| file_text(&source)) else {
                return false;
            };
            let count = |map: &BTreeMap<String, usize>, key: &str| map.get(key).copied();
            if count(&text.out_of_line_modules, name).unwrap_or(0) != captured_modules
                || count(&text.path_file_names, child_file_name).unwrap_or(0) != captured_paths
            {
                return false;
            }
        }
        true
    }

    /// The package directory of `file`: its nearest ancestor holding a
    /// `Cargo.toml` at the snapshot, memoized per directory.
    fn package_of(&mut self, file: &Path) -> Option<PathBuf> {
        let directory = file.parent()?.to_path_buf();
        if let Some(known) = self.manifest_dirs.get(&directory) {
            return known.clone();
        }
        let mut cursor = directory.clone();
        let mut found = None;
        for _ in 0..=MAX_SCAN_DEPTH {
            if manifest_present(self.workspace_root, &cursor) {
                found = Some(cursor);
                break;
            }
            if !cursor.pop() {
                break;
            }
        }
        self.manifest_dirs.insert(directory, found.clone());
        found
    }

    /// Package-wide inputs, or `None` when the package admits an owner the
    /// producer cannot see.
    fn package_owners(&mut self, package: &Path) -> Option<PackageOwners> {
        let index = self.index;
        let root = self.workspace_root;
        let missing = committed_source::committed_workspace_paths_missing_on_disk(root);
        if missing
            .iter()
            .any(|path| Path::new(path).starts_with(package))
        {
            return None;
        }
        if self.indexed_by_package.is_none() {
            let mut grouped: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
            for file in index.files.keys() {
                if let Some(owner) = self.package_of(file) {
                    grouped.entry(owner).or_default().push(file.clone());
                }
            }
            self.indexed_by_package = Some(grouped);
        }
        let files = self
            .indexed_by_package
            .as_ref()
            .and_then(|grouped| grouped.get(package))
            .cloned()
            .unwrap_or_default();
        for file in &index.non_utf8_sources {
            if self.package_of(file).as_deref() == Some(package) {
                return None;
            }
        }
        // The package's file set as the run discovers it. A discovered file
        // the index did not take in is read through the snapshot and kept as
        // masked text, so a declaration in it can still refuse the verdict.
        let discovered =
            crate::analysis::workspace::discover_rust_files(&root.join(package)).ok()?;
        let mut unindexed = Vec::new();
        for relative in discovered {
            let file = package.join(relative);
            if self.package_of(&file).as_deref() != Some(package) || index.files.contains_key(&file)
            {
                continue;
            }
            let Some(bytes) = committed_source::read_source_bytes(root, &file).ok()? else {
                // Absent at the selected snapshot: not an owner there.
                continue;
            };
            unindexed.push(file_text(&String::from_utf8(bytes).ok()?)?);
        }
        for file in &files {
            let facts = index.files.get(file)?;
            if facts.used_lexical_fallback
                || facts
                    .module_declarations
                    .iter()
                    .any(|declaration| declaration.path_target == ModulePathTarget::Unknown)
            {
                return None;
            }
            let source = &facts.source;
            let may_hide = (source.contains('$') && source.contains("mod"))
                || (source.contains("cfg_attr") && source.contains("path"))
                || source.contains("#[path");
            if may_hide {
                let source = source.clone();
                if self
                    .texts
                    .entry(file.clone())
                    .or_insert_with(|| file_text(&source))
                    .is_none()
                {
                    return None;
                }
            }
        }
        let include_declared = self.include_declared(package)?;
        Some(PackageOwners {
            files,
            unindexed,
            include_declared,
        })
    }

    /// Module names declared by the package's unresolved nested-module
    /// includes; `None` when one of them is not a literal `.rs` file of the
    /// package, or for any other kind of unresolved include in the package.
    fn include_declared(&mut self, package: &Path) -> Option<BTreeSet<String>> {
        let index = self.index;
        let mut declared = BTreeSet::new();
        for limitation in &index.include_limitations {
            let parent_package = self.package_of(&limitation.parent)?;
            if parent_package != package {
                continue;
            }
            if limitation.reason_code != "rust_include_nested_module_context" {
                return None;
            }
            let literal = include_literal(&limitation.expression)?;
            let target = limitation
                .parent
                .parent()
                .and_then(|directory| normalize_relative(&directory.join(literal)))?;
            if self.package_of(&target).as_deref() != Some(package) {
                return None;
            }
            let source = match index.files.get(&target) {
                Some(facts) => facts.source.to_string(),
                None => {
                    let bytes = committed_source::read_source_bytes(self.workspace_root, &target)
                        .ok()??;
                    String::from_utf8(bytes).ok()?
                }
            };
            let text = file_text(&source)?;
            declared.extend(text.out_of_line_modules.into_keys());
        }
        Some(declared)
    }
}

/// Whether `directory/Cargo.toml` exists at the snapshot.
fn manifest_present(workspace_root: &Path, directory: &Path) -> bool {
    let relative = directory.join("Cargo.toml");
    match committed_source::lookup(workspace_root, &relative) {
        CommittedSourceRead::Committed(_) => true,
        CommittedSourceRead::AbsentAtHead => false,
        CommittedSourceRead::Worktree => {
            let full = workspace_root.join(&relative);
            crate::analysis::input_boundary::observe_module_read(workspace_root, &full, false);
            full.is_file()
        }
    }
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

/// Masked declaration text of one file; `None` on an unaccountable shape.
fn file_text(source: &str) -> Option<FileText> {
    let mut text = FileText::default();
    record_declarations(source, &mut text)?;
    Some(text)
}

/// Adds the declaration text of one file; `None` for a shape this check
/// cannot account for.
fn record_declarations(source: &str, text: &mut FileText) -> Option<()> {
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

    fn record(source: &str) -> Option<FileText> {
        file_text(source)
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
