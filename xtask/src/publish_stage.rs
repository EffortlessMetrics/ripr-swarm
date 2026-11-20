//! The source tree `ripr` is packaged and published from (#14198).
//!
//! crates.io refuses a `.crate` above its default 10 MiB upload limit. The
//! `include` list in `crates/ripr/Cargo.toml` already leaves out out-of-line
//! unit-test files, but about a third of the remaining source is inline
//! `#[cfg(test)] mod tests { .. }` blocks, and that share grows with every
//! test added. Moving those blocks into files is a 400k-line diff.
//!
//! Instead the package is built from a publish stage: a sparse local clone of
//! the workspace checked out at `HEAD`, holding the workspace members and the
//! root files, in which `crates/ripr/src` has each inline module whose
//! attributes include exactly `#[cfg(test)]` removed with its attributes and
//! doc comments. A build without `cfg(test)` never compiles such a module, so
//! the library, the binary and every feature build are the same with or
//! without it. The repository checkout keeps every test.
//!
//! Because the stage is a Git checkout of `HEAD`, `cargo package` still
//! records the source commit in `.cargo_vcs_info.json`, marked dirty by the
//! strip. The stage also writes [`PUBLISH_STAGE_MARKER`] holding that commit;
//! a dirty record whose commit equals the marker's is a publish-stage package,
//! which `build.rs` and the release gate identify by its commit
//! (`crates/ripr/src/build_commit_record.rs`).
//!
//! The strip is fail-closed: a file that does not parse before or after the
//! edit stops the stage, and the rehearsal then compiles the packaged crate.

use std::fs;
use std::path::Path;
use std::process::Command;

use ra_ap_syntax::ast::{self, HasAttrs};
use ra_ap_syntax::{AstNode, Edition, SourceFile, TextRange};

/// The crate whose inline test modules are stripped in the stage.
const STRIPPED_SOURCE_ROOT: &str = "crates/ripr/src";

/// The marker file, relative to the `ripr` package root, that names the
/// commit a publish stage was checked out at. Packaged by the `include` list.
pub(crate) const PUBLISH_STAGE_MARKER: &str = ".ripr-publish-stage";

/// Workspace directories checked out in the stage, besides the root files.
/// Cargo loads every member's manifest and targets, so all members are
/// checked out; nothing else is read.
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
    pub(crate) commit: String,
    pub(crate) stripped_files: usize,
    pub(crate) stripped_modules: usize,
    pub(crate) stripped_bytes: usize,
}

/// Check out `HEAD` of `workspace` into `stage` (which must not exist) as a
/// sparse local clone, then strip inline test modules from the staged
/// `crates/ripr/src` and write the marker. Only committed content is staged:
/// an uncommitted edit in the checkout is not packaged.
pub(crate) fn stage_workspace(workspace: &Path, stage: &Path) -> Result<StageReport, String> {
    if stage.exists() {
        return Err(format!(
            "publish stage {} already exists; refusing to stage over it",
            stage.display()
        ));
    }
    let commit = git_output(workspace, &["rev-parse", "--verify", "HEAD"])?;
    let workspace_arg = workspace.to_string_lossy().into_owned();
    let stage_arg = stage.to_string_lossy().into_owned();
    let parent = stage
        .parent()
        .ok_or_else(|| format!("publish stage {} has no parent", stage.display()))?;
    fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    git_output(
        parent,
        &[
            "clone",
            "--quiet",
            // Stage the committed bytes on every host: a global
            // `core.autocrlf=true` would otherwise rewrite the checkout to
            // CRLF and `verify_stage` compares against `git show` (LF).
            // `core.eol=lf` covers `text=auto` attributes, which otherwise
            // check out with the native (CRLF) ending on Windows.
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.eol=lf",
            "--shared",
            "--no-checkout",
            workspace_arg.as_str(),
            stage_arg.as_str(),
        ],
    )?;
    let mut sparse = vec!["sparse-checkout", "set", "--cone"];
    sparse.extend_from_slice(STAGED_DIRECTORIES);
    git_output(stage, &sparse)?;
    git_output(stage, &["checkout", "--quiet", "--detach", commit.as_str()])?;

    let mut report = StageReport {
        commit: commit.clone(),
        ..StageReport::default()
    };
    strip_tree(stage, &stage.join(STRIPPED_SOURCE_ROOT), &mut report)?;
    if report.stripped_files == 0 {
        // The ripr source always holds inline test modules; staging none
        // means the strip did not run on it, not that there was nothing to do.
        return Err(format!(
            "no inline #[cfg(test)] module was stripped under {STRIPPED_SOURCE_ROOT}; the stage would package every test"
        ));
    }
    let marker = stage.join("crates/ripr").join(PUBLISH_STAGE_MARKER);
    fs::write(&marker, format!("{commit}\n"))
        .map_err(|e| format!("cannot write {}: {e}", marker.display()))?;
    let verified = verify_stage(stage)?;
    if verified != report.stripped_files {
        return Err(format!(
            "publish stage verification saw {verified} stripped files, the strip wrote {}",
            report.stripped_files
        ));
    }
    Ok(report)
}

fn strip_tree(stage: &Path, dir: &Path, report: &mut StageReport) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    entries.sort_by_key(fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if kind.is_dir() {
            strip_tree(stage, &path, report)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let relative = path.strip_prefix(stage).unwrap_or(&path);
            let stripped = strip_inline_test_modules(&text)
                .map_err(|e| format!("{}: {e}", relative.display()))?;
            if stripped.modules > 0 {
                fs::write(&path, &stripped.text)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
                report.stripped_files += 1;
                report.stripped_modules += stripped.modules;
                report.stripped_bytes += text.len().saturating_sub(stripped.text.len());
            }
        }
    }
    Ok(())
}

/// Run `git` with fixed arguments in `dir` and return its trimmed stdout.
fn git_output(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_raw(dir, args).map(|text| text.trim().to_string())
}

/// Run `git` with fixed arguments in `dir` and return its stdout unchanged.
/// An inherited `GIT_DIR`-style variable (set by a hook or wrapper) would
/// point every command here at the caller's repository instead of `dir`, and
/// the sparse checkout would then rewrite the real checkout, so they are
/// cleared.
fn git_raw(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .map_err(|e| format!("git {} failed to start: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| format!("git {} printed non-UTF-8 output: {e}", args.join(" ")))
}

/// Prove that `stage` is its `HEAD` with only the publish-stage edits: every
/// change Git reports is either the marker (untracked, holding `HEAD`) or a
/// modified `.rs` file under `crates/ripr/src` whose content is exactly the
/// strip of that file at `HEAD`. Anything else (a hand edit, a rewritten
/// lockfile, a stray file) is an error, because the marker would otherwise
/// identify such a package as a clean build of the commit. Returns the number
/// of stripped files.
pub(crate) fn verify_stage(stage: &Path) -> Result<usize, String> {
    let head = git_output(stage, &["rev-parse", "--verify", "HEAD"])?;
    let status = git_raw(
        stage,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let marker_path = format!("crates/ripr/{PUBLISH_STAGE_MARKER}");
    let mut marker_seen = false;
    let mut stripped = 0;
    for entry in status.split('\0').filter(|entry| !entry.is_empty()) {
        let (code, path) = entry
            .split_at_checked(3)
            .ok_or_else(|| format!("publish stage has an unreadable status entry `{entry}`"))?;
        match code {
            "?? " if path == marker_path => {
                let text = fs::read_to_string(stage.join(path))
                    .map_err(|e| format!("cannot read {path}: {e}"))?;
                if text != format!("{head}\n") {
                    return Err(format!("publish-stage marker does not name HEAD {head}"));
                }
                marker_seen = true;
            }
            " M " if path.starts_with(STRIPPED_SOURCE_ROOT) && path.ends_with(".rs") => {
                let original = git_raw(stage, &["show", &format!("HEAD:{path}")])?;
                let expected =
                    strip_inline_test_modules(&original).map_err(|e| format!("{path}: {e}"))?;
                let actual = fs::read_to_string(stage.join(path))
                    .map_err(|e| format!("cannot read {path}: {e}"))?;
                if expected.modules == 0 || expected.text != actual {
                    return Err(format!(
                        "publish stage file {path} is not its HEAD content with inline test modules stripped"
                    ));
                }
                stripped += 1;
            }
            _ => {
                return Err(format!(
                    "publish stage differs from HEAD beyond the strip: `{}` {path}",
                    code.trim_end()
                ));
            }
        }
    }
    if !marker_seen {
        return Err("publish stage has no marker".to_string());
    }
    Ok(stripped)
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

    /// A temporary repository with `files` committed, plus `untracked`.
    fn committed_workspace(
        root: &Path,
        files: &[(&str, &str)],
        untracked: &[(&str, &str)],
    ) -> Result<std::path::PathBuf, String> {
        let io = |e: std::io::Error| e.to_string();
        let _ = fs::remove_dir_all(root);
        let workspace = root.join("workspace");
        for (path, body) in files.iter().chain(untracked) {
            let file = workspace.join(path);
            fs::create_dir_all(file.parent().ok_or("no parent")?).map_err(io)?;
            fs::write(&file, body).map_err(io)?;
        }
        git_output(&workspace, &["init", "-q"])?;
        git_output(&workspace, &["config", "core.autocrlf", "false"])?;
        for (path, _) in files {
            git_output(&workspace, &["add", "--", path])?;
        }
        git_output(
            &workspace,
            &[
                "-c",
                "user.name=stage-test",
                "-c",
                "user.email=stage-test@example.invalid",
                "commit",
                "-q",
                "-m",
                "fixture",
            ],
        )?;
        Ok(workspace)
    }

    #[test]
    fn strips_an_inline_test_module_with_its_attributes_and_docs() -> Result<(), String> {
        let source = "pub fn keep() -> u8 {\n    1\n}\n\n/// Unit tests.\n#[cfg(test)]\n#[doc(hidden)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn one() {\n        assert_eq!(keep(), 1);\n    }\n}\n";
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
    fn stages_head_of_the_members_strips_only_ripr_source_and_marks_the_commit()
    -> Result<(), String> {
        let io = |e: std::io::Error| e.to_string();
        let root = std::env::temp_dir().join(format!("ripr-stage-test-{}", std::process::id()));
        let test_module =
            "pub fn live() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
        let workspace = committed_workspace(
            &root,
            &[
                ("Cargo.toml", "[workspace]\n"),
                ("crates/ripr/src/lib.rs", test_module),
                ("crates/ripr/src/nested/deep.rs", test_module),
                ("crates/ripr-contract/src/lib.rs", test_module),
                ("docs/guide.md", "not staged\n"),
            ],
            &[("crates/ripr/src/scratch.rs", "untracked\n")],
        )?;
        // An uncommitted edit is not staged: the stage is HEAD.
        fs::write(workspace.join("Cargo.toml"), "edited\n").map_err(io)?;
        let head = git_output(&workspace, &["rev-parse", "HEAD"])?;

        let stage = root.join("stage");
        let report = stage_workspace(&workspace, &stage)?;
        assert_eq!(
            (
                report.commit.as_str(),
                report.stripped_files,
                report.stripped_modules
            ),
            (head.as_str(), 2, 2)
        );
        assert_eq!(git_output(&stage, &["rev-parse", "HEAD"])?, head);
        assert_eq!(
            fs::read_to_string(stage.join("Cargo.toml")).map_err(io)?,
            "[workspace]\n"
        );
        for stripped in ["crates/ripr/src/lib.rs", "crates/ripr/src/nested/deep.rs"] {
            assert_eq!(
                fs::read_to_string(stage.join(stripped)).map_err(io)?,
                "pub fn live() {}\n\n"
            );
        }
        // Only ripr is stripped; the contract crate keeps its tests.
        assert_eq!(
            fs::read_to_string(stage.join("crates/ripr-contract/src/lib.rs")).map_err(io)?,
            test_module
        );
        assert!(!stage.join("docs/guide.md").exists());
        assert!(!stage.join("crates/ripr/src/scratch.rs").exists());
        assert_eq!(
            fs::read_to_string(stage.join("crates/ripr").join(PUBLISH_STAGE_MARKER)).map_err(io)?,
            format!("{head}\n")
        );
        assert_eq!(verify_stage(&stage)?, 2);
        // Any other difference from HEAD fails verification: a hand edit of a
        // stripped file, an edit outside the stripped source, a stray file.
        let lib = stage.join("crates/ripr/src/lib.rs");
        fs::write(&lib, "pub fn live() {}\npub fn extra() {}\n").map_err(io)?;
        assert!(matches!(verify_stage(&stage), Err(e) if e.contains("is not its HEAD content")));
        fs::write(&lib, "pub fn live() {}\n\n").map_err(io)?;
        assert_eq!(verify_stage(&stage)?, 2);
        fs::write(stage.join("Cargo.toml"), "[workspace]\nmembers = []\n").map_err(io)?;
        assert!(matches!(verify_stage(&stage), Err(e) if e.contains("beyond the strip")));
        fs::write(stage.join("Cargo.toml"), "[workspace]\n").map_err(io)?;
        fs::write(stage.join("crates/ripr/src/stray.rs"), "pub fn s() {}\n").map_err(io)?;
        assert!(matches!(verify_stage(&stage), Err(e) if e.contains("beyond the strip")));
        fs::remove_file(stage.join("crates/ripr/src/stray.rs")).map_err(io)?;
        let marker = stage.join("crates/ripr").join(PUBLISH_STAGE_MARKER);
        fs::write(&marker, "0000000000000000000000000000000000000000\n").map_err(io)?;
        assert!(matches!(verify_stage(&stage), Err(e) if e.contains("does not name HEAD")));
        fs::remove_file(&marker).map_err(io)?;
        assert!(matches!(verify_stage(&stage), Err(e) if e.contains("no marker")));
        // An existing stage is never staged over.
        assert!(
            matches!(stage_workspace(&workspace, &stage), Err(e) if e.contains("already exists"))
        );
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn a_stage_that_strips_nothing_is_refused() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!("ripr-stage-empty-{}", std::process::id()));
        let workspace = committed_workspace(
            &root,
            &[("crates/ripr/src/lib.rs", "pub fn live() {}\n")],
            &[],
        )?;
        let refused = stage_workspace(&workspace, &root.join("stage"));
        assert!(
            matches!(refused, Err(e) if e.contains("no inline #[cfg(test)] module was stripped"))
        );
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
