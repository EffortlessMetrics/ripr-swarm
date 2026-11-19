//! The source tree `ripr` is packaged and published from (#14198).
//!
//! crates.io refuses a `.crate` above its default 10 MiB upload limit. The
//! `include` list in `crates/ripr/Cargo.toml` already leaves out out-of-line
//! unit-test files, but about a third of the remaining source is inline
//! `#[cfg(test)] mod tests { .. }` blocks, and that share grows with every
//! test added. Moving those blocks into files is a 400k-line diff.
//!
//! Instead the package is built from a staged copy of the workspace: every
//! tracked file the build reads is copied, and in `crates/ripr/src` each inline
//! module whose attributes include exactly `#[cfg(test)]` is removed with its
//! attributes and doc comments. A build without `cfg(test)` never compiles
//! such a module, so the library, the binary and every feature build are the
//! same with or without it. The repository checkout keeps every test.
//!
//! The strip is fail-closed: a file that does not parse before or after the
//! edit stops the stage, and the rehearsal then compiles the packaged crate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ra_ap_syntax::ast::{self, HasAttrs};
use ra_ap_syntax::{AstNode, Edition, SourceFile, TextRange};

/// The crate whose inline test modules are stripped in the stage.
const STRIPPED_SOURCE_ROOT: &str = "crates/ripr/src";

/// Workspace directories copied into the stage. Cargo loads every member's
/// manifest and targets, so all members are copied; nothing else is read.
const STAGED_DIRECTORIES: &[&str] = &[
    ".cargo",
    "crates/ripr",
    "crates/ripr-contract",
    "xtask",
    "tools/repo-policy",
];

/// What staging did, for the rehearsal log.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct StageReport {
    pub(crate) copied_files: usize,
    pub(crate) stripped_files: usize,
    pub(crate) stripped_modules: usize,
    pub(crate) stripped_bytes: usize,
}

/// Copy the tracked files of `workspace` that a package build reads into
/// `stage` (which must not exist), then strip inline test modules from the
/// staged `crates/ripr/src`. Tracked files are read from the working tree, so
/// an uncommitted edit to a tracked file is staged, as `cargo package
/// --allow-dirty` would package it; untracked files are not.
pub(crate) fn stage_workspace(workspace: &Path, stage: &Path) -> Result<StageReport, String> {
    if stage.exists() {
        return Err(format!(
            "publish stage {} already exists; refusing to stage over it",
            stage.display()
        ));
    }
    let mut report = StageReport::default();
    for relative in tracked_files(workspace)? {
        if !is_staged(&relative) {
            continue;
        }
        let from = workspace.join(&relative);
        // A tracked file deleted in the working tree is not packaged either.
        if !from.is_file() {
            continue;
        }
        let to = stage.join(&relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        fs::copy(&from, &to).map_err(|e| format!("cannot stage {}: {e}", from.display()))?;
        report.copied_files += 1;
        if relative.starts_with(STRIPPED_SOURCE_ROOT)
            && relative.extension().is_some_and(|ext| ext == "rs")
        {
            let text = fs::read_to_string(&to)
                .map_err(|e| format!("cannot read {}: {e}", to.display()))?;
            let stripped = strip_inline_test_modules(&text)
                .map_err(|e| format!("{}: {e}", relative.display()))?;
            if stripped.modules > 0 {
                fs::write(&to, &stripped.text)
                    .map_err(|e| format!("cannot write {}: {e}", to.display()))?;
                report.stripped_files += 1;
                report.stripped_modules += stripped.modules;
                report.stripped_bytes += text.len().saturating_sub(stripped.text.len());
            }
        }
    }
    if report.stripped_files == 0 {
        // The ripr source always holds inline test modules; staging none
        // means the strip did not run on it, not that there was nothing to do.
        return Err(format!(
            "no inline #[cfg(test)] module was stripped under {STRIPPED_SOURCE_ROOT}; the stage would package every test"
        ));
    }
    Ok(report)
}

fn tracked_files(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--cached"])
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("git ls-files failed to start: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed in {}: {}",
            workspace.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|e| format!("git ls-files printed a non-UTF-8 path: {e}"))?;
    Ok(text
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// Root-level files (the workspace manifest, lockfile and toolchain pin) and
/// everything under the staged directories.
fn is_staged(relative: &Path) -> bool {
    relative.components().count() == 1
        || STAGED_DIRECTORIES
            .iter()
            .any(|directory| relative.starts_with(directory))
}

/// One source file after the strip.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stripped {
    pub(crate) text: String,
    pub(crate) modules: usize,
}

/// Remove every inline module (`mod name { .. }`) carrying the attribute
/// `#[cfg(test)]`, together with its other attributes, its doc comments and
/// the line break after it. Out-of-line declarations (`mod name;`) are kept:
/// their files are left out by the package `include` list instead. A module
/// nested in a stripped module goes with it. Source that does not parse, before
/// or after the edit, is an error rather than a guess.
pub(crate) fn strip_inline_test_modules(source: &str) -> Result<Stripped, String> {
    let parsed = SourceFile::parse(source, Edition::Edition2024);
    if let Some(error) = parsed.errors().first() {
        return Err(format!("does not parse, so it is not stripped: {error}"));
    }
    let mut ranges: Vec<TextRange> = Vec::new();
    for module in parsed
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
    {
        if module.item_list().is_none() || !is_cfg_test(&module) {
            continue;
        }
        let range = module.syntax().text_range();
        // Descendants come in source order, so an enclosing module was seen first.
        if ranges
            .last()
            .is_some_and(|outer| outer.contains_range(range))
        {
            continue;
        }
        ranges.push(range);
    }
    let modules = ranges.len();
    let mut text = source.to_string();
    for range in ranges.iter().rev() {
        let start = usize::from(range.start());
        let end = line_break_end(source, usize::from(range.end()));
        text.replace_range(start..end, "");
    }
    if modules > 0 {
        let reparsed = SourceFile::parse(&text, Edition::Edition2024);
        if let Some(error) = reparsed.errors().first() {
            return Err(format!(
                "no longer parses after its inline test modules were stripped: {error}"
            ));
        }
        let left = reparsed
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Module::cast)
            .any(|module| module.item_list().is_some() && is_cfg_test(&module));
        if left {
            return Err("still holds an inline #[cfg(test)] module after the strip".to_string());
        }
    }
    Ok(Stripped { text, modules })
}

fn is_cfg_test(module: &ast::Module) -> bool {
    module.attrs().any(|attr| {
        let text: String = attr
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        text == "#[cfg(test)]"
    })
}

/// `end` moved past one `\n` or `\r\n` that directly follows it.
fn line_break_end(source: &str, end: usize) -> usize {
    let rest = &source[end..];
    if rest.starts_with("\r\n") {
        end + 2
    } else if rest.starts_with('\n') {
        end + 1
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `git` with fixed arguments in a test's own temporary repository.
    fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| e.to_string())?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("git {args:?} failed"))
    }

    #[test]
    fn strips_an_inline_test_module_with_its_attributes_and_docs() -> Result<(), String> {
        let source = "pub fn keep() -> u8 {\n    1\n}\n\n/// Unit tests.\n#[cfg(test)]\n#[allow(clippy::unwrap_used)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn one() {\n        assert_eq!(keep(), 1);\n    }\n}\n";
        let stripped = strip_inline_test_modules(source)?;
        assert_eq!(stripped.modules, 1);
        assert_eq!(stripped.text, "pub fn keep() -> u8 {\n    1\n}\n\n");
        Ok(())
    }

    #[test]
    fn a_brace_line_inside_a_test_raw_string_does_not_end_the_module() -> Result<(), String> {
        // A line-based strip stops at the first column-0 `}` and leaves the
        // rest of the raw string behind; the parser does not.
        let source = "pub fn keep() {}\n\n#[cfg(test)]\nmod tests {\n    const FIXTURE: &str = r#\"\nfn fixture() {\n}\nmod inner {\n\"#;\n}\n\npub fn after() {}\n";
        let stripped = strip_inline_test_modules(source)?;
        assert_eq!(stripped.modules, 1);
        assert_eq!(stripped.text, "pub fn keep() {}\n\n\npub fn after() {}\n");
        Ok(())
    }

    #[test]
    fn keeps_out_of_line_declarations_and_other_cfgs() -> Result<(), String> {
        let source = "#[cfg(test)]\nmod tests;\n\n#[cfg(any(test, feature = \"x\"))]\nmod shared {\n    pub fn helper() {}\n}\n\n#[cfg(not(test))]\nmod live {}\n\n#[cfg(feature = \"test\")]\nmod featured {}\n";
        let stripped = strip_inline_test_modules(source)?;
        assert_eq!(stripped.modules, 0);
        assert_eq!(stripped.text, source);
        Ok(())
    }

    #[test]
    fn strips_a_nested_test_module_once_and_keeps_its_production_parent() -> Result<(), String> {
        let source = "mod outer {\n    pub fn live() {}\n\n    #[cfg( test )]\n    mod tests {\n        #[cfg(test)]\n        mod deeper {}\n    }\n}\n";
        let stripped = strip_inline_test_modules(source)?;
        assert_eq!(stripped.modules, 1);
        assert_eq!(
            stripped.text,
            "mod outer {\n    pub fn live() {}\n\n    }\n"
        );
        Ok(())
    }

    #[test]
    fn source_that_does_not_parse_is_refused() {
        let refused = strip_inline_test_modules("#[cfg(test)]\nmod tests {\n");
        assert!(matches!(refused, Err(e) if e.contains("does not parse")));
    }

    #[test]
    fn stages_tracked_member_files_and_strips_only_ripr_source() -> Result<(), String> {
        let io = |e: std::io::Error| e.to_string();
        let root = std::env::temp_dir().join(format!("ripr-stage-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let workspace = root.join("workspace");
        let test_module =
            "pub fn live() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
        for (path, body) in [
            ("Cargo.toml", "[workspace]\n"),
            ("crates/ripr/src/lib.rs", test_module),
            ("crates/ripr-contract/src/lib.rs", test_module),
            ("docs/guide.md", "not staged\n"),
        ] {
            let file = workspace.join(path);
            fs::create_dir_all(file.parent().ok_or("no parent")?).map_err(io)?;
            fs::write(&file, body).map_err(io)?;
        }
        git(&workspace, &["init", "-q"])?;
        git(&workspace, &["add", "."])?;
        // Untracked: never staged.
        fs::write(workspace.join("crates/ripr/src/scratch.rs"), "junk").map_err(io)?;

        let stage = root.join("stage");
        let report = stage_workspace(&workspace, &stage)?;
        assert_eq!(
            (
                report.copied_files,
                report.stripped_files,
                report.stripped_modules
            ),
            (3, 1, 1)
        );
        assert_eq!(
            fs::read_to_string(stage.join("crates/ripr/src/lib.rs")).map_err(io)?,
            "pub fn live() {}\n\n"
        );
        // Only ripr is stripped; the contract crate keeps its tests.
        assert_eq!(
            fs::read_to_string(stage.join("crates/ripr-contract/src/lib.rs")).map_err(io)?,
            test_module
        );
        assert!(!stage.join("docs/guide.md").exists());
        assert!(!stage.join("crates/ripr/src/scratch.rs").exists());
        // An existing stage is never staged over.
        assert!(
            matches!(stage_workspace(&workspace, &stage), Err(e) if e.contains("already exists"))
        );
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn a_stage_that_strips_nothing_is_refused() -> Result<(), String> {
        let io = |e: std::io::Error| e.to_string();
        let root = std::env::temp_dir().join(format!("ripr-stage-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let workspace = root.join("workspace");
        let lib = workspace.join("crates/ripr/src/lib.rs");
        fs::create_dir_all(lib.parent().ok_or("no parent")?).map_err(io)?;
        fs::write(&lib, "pub fn live() {}\n").map_err(io)?;
        git(&workspace, &["init", "-q"])?;
        git(&workspace, &["add", "."])?;
        let refused = stage_workspace(&workspace, &root.join("stage"));
        assert!(
            matches!(refused, Err(e) if e.contains("no inline #[cfg(test)] module was stripped"))
        );
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
