//! Test-edit disclosure for a two-sided diff. This is not test admission,
//! evidence strength, recommendation selection, or current-line placement.

use super::diff::{ChangedFile, reconstruct_old_source};
use super::facts::{
    RustIndex, imported_test_alias_crate, member_crates::MemberCrates,
    normalize_reconstructed_source_roles,
};
use super::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use super::workspace::{SourceRole, SourceRoleContext, classify_with};
use crate::config::RiprConfig;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffTestChanges {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) unresolved: bool,
}

/// Read changed Rust files within the remaining admission budget, at the
/// requested head. Reconstruct
/// their old source from the actual diff so removed coordinates never resolve
/// against a different function in the current source. Keep uncertainty out of
/// the affirmative `unchanged_tests` claim without changing analysis status.
pub(crate) fn diff_test_changes(
    root: &Path,
    head: &str,
    diff_text: &str,
    config: &RiprConfig,
    remaining_files: usize,
    remaining_bytes: u64,
) -> DiffTestChanges {
    let parsed = super::diff::parse_unified_diff_with_metadata(diff_text);
    // Whole-file deletions have no new-side ChangedFile. The shared parser
    // retains their count, not test-role attribution: do not infer unchanged
    // tests or invent a confirmed test path from that missing evidence.
    let mut changes = DiffTestChanges {
        unresolved: parsed.deleted_file_count > 0,
        ..DiffTestChanges::default()
    };
    let mut budget = DisclosureBudget {
        files: remaining_files,
        bytes: remaining_bytes,
    };
    let layout = SourceRoleContext::empty();
    for file in parsed.changed_files {
        if super::cancellation::checkpoint().is_err() {
            changes.unresolved = true;
            break;
        }
        if file.added_lines.is_empty() && file.removed_lines.is_empty() {
            continue;
        }
        if classify_with(&file.path, &layout) == SourceRole::TestEvidence {
            changes.files.push(file.path);
            continue;
        }
        if file
            .path
            .extension()
            .is_none_or(|extension| extension != "rs")
        {
            continue;
        }
        if !file
            .path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            changes.unresolved = true;
            continue;
        }
        let Some(path) = file.path.to_str() else {
            changes.unresolved = true;
            continue;
        };
        // Git paths are already slash-separated. A literal Unix backslash
        // belongs to the object name and must not be rewritten for display.
        let object = format!("{head}:{path}");
        if budget.files == 0 {
            changes.unresolved = true;
            continue;
        }
        budget.files -= 1;
        let Some(size) = blob_size(root, &object) else {
            changes.unresolved = true;
            continue;
        };
        let Ok(output_limit) = usize::try_from(size.max(1)) else {
            changes.unresolved = true;
            continue;
        };
        if size > budget.bytes {
            changes.unresolved = true;
            continue;
        }
        budget.bytes -= size;
        let source = crate::git::run_git_output_with_deadline_and_limit(
            root,
            &["show", "--no-textconv", &object],
            Duration::from_mins(1),
            output_limit,
        )
        .ok()
        .filter(|output| output.status.success() && output.stdout.len() as u64 == size)
        .and_then(|output| String::from_utf8(output.stdout).ok());
        let Some(source) = source else {
            changes.unresolved = true;
            continue;
        };
        let added = touches_test(root, head, &file, &source, false, config, &mut budget);
        let removed = if file.removed_lines.is_empty() {
            Some(false)
        } else {
            reconstruct_old_source(&source, &file)
                .and_then(|old| touches_test(root, head, &file, &old, true, config, &mut budget))
        };
        if added == Some(true) || removed == Some(true) {
            changes.files.push(file.path);
        } else if added.is_none() || removed.is_none() {
            changes.unresolved = true;
        }
    }
    changes.files.sort();
    changes.files.dedup();
    changes
}

/// Remaining Git-blob admission for changed files and wrapper-alias companions.
struct DisclosureBudget {
    files: usize,
    bytes: u64,
}

fn blob_size(root: &Path, object: &str) -> Option<u64> {
    let kind = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-t", object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !kind.status.success() || kind.stdout != b"blob\n" {
        return None;
    }
    let size = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-s", object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !size.status.success() {
        return None;
    }
    std::str::from_utf8(&size.stdout).ok()?.trim().parse().ok()
}

fn touches_test(
    root: &Path,
    head: &str,
    changed: &ChangedFile,
    source: &str,
    old: bool,
    config: &RiprConfig,
    budget: &mut DisclosureBudget,
) -> Option<bool> {
    let lines = if old {
        &changed.removed_lines
    } else {
        &changed.added_lines
    };
    if lines.is_empty() {
        return Some(false);
    }
    let facts = RaRustSyntaxAdapter
        .summarize_file(&changed.path, source)
        .ok()?;
    if facts.used_lexical_fallback {
        return None;
    }
    let needs_alias = facts
        .functions
        .iter()
        .any(|function| function.attrs.iter().any(|attr| is_wrapper_test_attr(attr)));
    let mut index = RustIndex::default();
    index.insert_file(changed.path.clone(), facts, true);
    if needs_alias {
        insert_alias_support(&mut index, root, head, &changed.path, source, budget)?;
    }
    normalize_reconstructed_source_roles(&mut index, root, config.analysis().test_harnesses())
        .ok()?;
    Some(lines.iter().any(|line| {
        index.functions().iter().any(|function| {
            function.file == changed.path
                && function.source_role.is_evidence_role()
                // A proof harness is verification source, not a test.
                && function.source_role != crate::analysis::facts::FunctionSourceRole::ProofHarness
                && function.start_line <= line.line
                && line.line <= function.end_line
        })
    }))
}

fn is_wrapper_test_attr(attr: &str) -> bool {
    let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
    let Some(body) = compact.strip_prefix("#[") else {
        return false;
    };
    let Some(body) = body.strip_suffix(']') else {
        return false;
    };
    matches!(
        body.split('(').next().unwrap_or(body),
        "crate::test" | "self::test"
    )
}

/// Load the crate root and path-bound wrapper crate so `#[crate::test]` in a
/// module can use the same alias authority as a full index (#10141 item 3).
/// Registry imports, workspace-inherited deps, and unreadable companions stay
/// uncredited. Exhausting the disclosure budget while a wrapper attribute is
/// present is unresolved rather than a false `unchanged_tests`.
fn insert_alias_support(
    index: &mut RustIndex,
    root: &Path,
    head: &str,
    changed: &Path,
    changed_source: &str,
    budget: &mut DisclosureBudget,
) -> Option<()> {
    index.member_crates = MemberCrates::new(root);
    let mut loaded = BTreeSet::from([changed.to_path_buf()]);
    let mut alias_crates = Vec::new();
    if let Some(name) = imported_test_alias_crate(changed_source) {
        alias_crates.push(name);
    }
    for crate_root in crate_root_paths(changed) {
        if !loaded.insert(crate_root.clone()) {
            continue;
        }
        match head_blob(root, head, &crate_root, budget)? {
            None => continue,
            Some(source) => {
                if let Some(name) = imported_test_alias_crate(&source) {
                    alias_crates.push(name);
                }
                let facts = RaRustSyntaxAdapter
                    .summarize_file(&crate_root, &source)
                    .ok()?;
                if facts.used_lexical_fallback {
                    return None;
                }
                index.insert_file(crate_root, facts, true);
            }
        }
    }
    for name in alias_crates {
        let Some(lib) = path_bound_library_root(root, head, changed, &name) else {
            continue;
        };
        if !loaded.insert(lib.clone()) {
            index.package_names.insert(name.clone());
            index.package_names.insert(name.replace('-', "_"));
            continue;
        }
        let Some(source) = head_blob(root, head, &lib, budget)? else {
            continue;
        };
        let facts = RaRustSyntaxAdapter.summarize_file(&lib, &source).ok()?;
        if facts.used_lexical_fallback {
            return None;
        }
        index.insert_file(lib, facts, true);
        index.package_names.insert(name.clone());
        index.package_names.insert(name.replace('-', "_"));
    }
    Some(())
}

/// Default crate roots (`src/lib.rs`, `src/main.rs`), matching
/// `LocalTestAliases`. Custom `[lib] path` remains an unresolved route.
fn crate_root_paths(file: &Path) -> Vec<PathBuf> {
    let Some(src) = file
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "src"))
    else {
        return vec![file.to_path_buf()];
    };
    if file.starts_with(src.join("bin")) {
        return vec![file.to_path_buf()];
    }
    ["lib.rs", "main.rs"]
        .into_iter()
        .map(|name| src.join(name))
        .collect()
}

fn path_bound_library_root(
    root: &Path,
    head: &str,
    consumer: &Path,
    crate_name: &str,
) -> Option<PathBuf> {
    let mut dir = consumer.to_path_buf();
    if !dir.pop() {
        return None;
    }
    loop {
        let manifest = if dir.as_os_str().is_empty() {
            PathBuf::from("Cargo.toml")
        } else {
            dir.join("Cargo.toml")
        };
        match head_metadata_toml(root, head, &manifest)? {
            None => {}
            Some(text) => {
                let table = text.parse::<toml::Table>().ok()?;
                if table.get("package").is_some() {
                    let rel = dep_path_in_table(&table, crate_name)?;
                    let mut package = dir;
                    for part in rel.components() {
                        match part {
                            Component::CurDir => {}
                            Component::ParentDir => {
                                if !package.pop() {
                                    return None;
                                }
                            }
                            Component::Normal(name) => package.push(name),
                            _ => return None,
                        }
                    }
                    package.push("src");
                    package.push("lib.rs");
                    if package
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
                    {
                        return Some(package);
                    }
                    return None;
                }
            }
        }
        if dir.as_os_str().is_empty() {
            return None;
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn dep_path_in_table(manifest: &toml::Table, crate_name: &str) -> Option<PathBuf> {
    let hyphen = crate_name.replace('_', "-");
    let underscore = crate_name.replace('-', "_");
    for table_name in ["dependencies", "dev-dependencies", "dev_dependencies"] {
        let Some(deps) = manifest.get(table_name).and_then(toml::Value::as_table) else {
            continue;
        };
        for key in [crate_name, hyphen.as_str(), underscore.as_str()] {
            let Some(value) = deps.get(key) else {
                continue;
            };
            match value {
                toml::Value::String(_) => return None,
                toml::Value::Table(dep) => {
                    if dep.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                        return None;
                    }
                    let path = dep.get("path").and_then(toml::Value::as_str)?;
                    return Some(PathBuf::from(path));
                }
                _ => return None,
            }
        }
    }
    None
}

/// Manifest text at `head`. Does not consume the source-companion disclosure
/// budget. Outer `None` is a path, oversize, or decode failure; `Some(None)`
/// is a missing object; `Some(Some(text))` is the blob.
fn head_metadata_toml(root: &Path, head: &str, path: &Path) -> Option<Option<String>> {
    let path = path.to_str()?;
    if Path::new(path)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let object = format!("{head}:{path}");
    let kind = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-t", &object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !kind.status.success() || kind.stdout != b"blob\n" {
        return Some(None);
    }
    let size = blob_size(root, &object)?;
    if size > crate::bounded_input::MAX_METADATA_TOML_BYTES {
        return None;
    }
    let Ok(output_limit) = usize::try_from(size.max(1)) else {
        return None;
    };
    let source = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["show", "--no-textconv", &object],
        Duration::from_mins(1),
        output_limit,
    )
    .ok()
    .filter(|output| output.status.success() && output.stdout.len() as u64 == size)
    .and_then(|output| String::from_utf8(output.stdout).ok())?;
    Some(Some(source))
}

/// `None` is a budget or decode failure (unresolved). `Some(None)` is a
/// missing object. `Some(Some(source))` is the blob at `head`.
fn head_blob(
    root: &Path,
    head: &str,
    path: &Path,
    budget: &mut DisclosureBudget,
) -> Option<Option<String>> {
    let path = path.to_str()?;
    if Path::new(path)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let object = format!("{head}:{path}");
    let kind = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-t", &object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !kind.status.success() || kind.stdout != b"blob\n" {
        return Some(None);
    }
    if budget.files == 0 {
        return None;
    }
    budget.files -= 1;
    let size = blob_size(root, &object)?;
    let Ok(output_limit) = usize::try_from(size.max(1)) else {
        return None;
    };
    if size > budget.bytes {
        return None;
    }
    budget.bytes -= size;
    let source = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["show", "--no-textconv", &object],
        Duration::from_mins(1),
        output_limit,
    )
    .ok()
    .filter(|output| output.status.success() && output.stdout.len() as u64 == size)
    .and_then(|output| String::from_utf8(output.stdout).ok())?;
    Some(Some(source))
}

#[cfg(all(test, feature = "lang-rust"))]
mod tests {
    use super::*;
    use crate::config::RiprConfig;
    use crate::testing::fixture_git::{fixture_git_ok, fixture_git_stdout, remove_fixture_tree};
    use std::sync::atomic::{AtomicU64, Ordering};

    const WRAPPER: &str = "#[proc_macro_attribute]\npub fn wrapped_test(_attr: TokenStream, item: TokenStream) -> TokenStream {\n    quote! { #[::tokio::test] #item }\n}\n";
    const ADDED_TEST: &str =
        "#[crate::test]\nfn aliased() {\n    assert_eq!(crate::value(), 1);\n}\n";
    const LOOKALIKE: &str =
        "#[crate::tests]\nfn lookalike() {\n    assert_eq!(crate::value(), 1);\n}\n";
    const BUILTIN: &str = "#[test]\nfn builtin() {\n    assert_eq!(crate::value(), 1);\n}\n";

    struct AliasWorkspace {
        root: PathBuf,
    }

    impl AliasWorkspace {
        fn new(label: &str, alias: &str) -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "ripr-review-crate-test-{}-{}-{}",
                label,
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("app/src")).map_err(|err| err.to_string())?;
            std::fs::create_dir_all(root.join("macros/src")).map_err(|err| err.to_string())?;
            std::fs::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"app\", \"macros\"]\nresolver = \"2\"\n",
            )
            .map_err(|err| err.to_string())?;
            std::fs::write(
                root.join("app/Cargo.toml"),
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nmy_macros = { path = \"../macros\" }\n",
            )
            .map_err(|err| err.to_string())?;
            std::fs::write(
                root.join("macros/Cargo.toml"),
                "[package]\nname = \"my_macros\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nproc-macro = true\n",
            )
            .map_err(|err| err.to_string())?;
            std::fs::write(root.join("macros/src/lib.rs"), WRAPPER)
                .map_err(|err| err.to_string())?;
            std::fs::write(
                root.join("app/src/lib.rs"),
                format!("{alias}pub fn value() -> u32 {{ 1 }}\n"),
            )
            .map_err(|err| err.to_string())?;
            std::fs::write(root.join("app/src/user.rs"), "// module\n")
                .map_err(|err| err.to_string())?;
            let fixture = Self { root };
            fixture_git_ok(&fixture.root, &["init", "-q"])?;
            fixture_git_ok(&fixture.root, &["config", "core.autocrlf", "false"])?;
            fixture.commit("before")?;
            Ok(fixture)
        }

        fn write_relative(&self, relative: &str, contents: &str) -> Result<(), String> {
            std::fs::write(self.root.join(relative), contents).map_err(|err| err.to_string())
        }

        fn commit_message(&self, message: &str) -> Result<(), String> {
            fixture_git_ok(
                &self.root,
                &[
                    "-c",
                    "user.name=RIPR fixture",
                    "-c",
                    "user.email=ripr@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qm",
                    message,
                ],
            )
        }

        fn commit(&self, message: &str) -> Result<(), String> {
            fixture_git_ok(&self.root, &["add", "."])?;
            self.commit_message(message)
        }

        fn commit_paths(&self, message: &str, paths: &[&str]) -> Result<(), String> {
            for path in paths {
                fixture_git_ok(&self.root, &["add", "--", path])?;
            }
            self.commit_message(message)
        }

        fn add_module_item(&self, item: &str) -> Result<String, String> {
            self.write_relative("app/src/user.rs", &format!("// module\n{item}"))?;
            self.commit("after")?;
            let diff = fixture_git_stdout(&self.root, &["diff", "--unified=0", "HEAD^", "HEAD"])?;
            if !diff.contains("diff --git a/app/src/user.rs b/app/src/user.rs") {
                return Err(format!("expected a module-only diff, got {diff}"));
            }
            if diff.contains("diff --git a/app/src/lib.rs") || diff.contains("diff --git a/macros/")
            {
                return Err(format!("crate root or wrapper must stay unchanged: {diff}"));
            }
            Ok(diff)
        }

        fn app_manifest(dep_path: &str) -> String {
            format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nmy_macros = {{ path = \"{dep_path}\" }}\n"
            )
        }
    }

    impl Drop for AliasWorkspace {
        fn drop(&mut self) {
            let _ = remove_fixture_tree(&self.root);
        }
    }

    fn changes_for(root: &Path, diff: &str) -> DiffTestChanges {
        diff_test_changes(
            root,
            "HEAD",
            diff,
            &RiprConfig::default(),
            usize::MAX,
            u64::MAX,
        )
    }

    #[test]
    fn added_module_crate_test_is_a_test_edit() -> Result<(), String> {
        let fixture = AliasWorkspace::new("credited", "use my_macros::wrapped_test as test;\n")?;
        let diff = fixture.add_module_item(ADDED_TEST)?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "readable alias workspace must resolve: {changes:?}"
        );
        assert_eq!(
            changes.files,
            [PathBuf::from("app/src/user.rs")],
            "a new #[crate::test] function in a module must count as a test edit: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn added_module_crate_test_without_alias_is_not_a_test_edit() -> Result<(), String> {
        let fixture = AliasWorkspace::new("no-alias", "")?;
        let diff = fixture.add_module_item(ADDED_TEST)?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "missing alias must resolve closed: {changes:?}"
        );
        assert!(
            changes.files.is_empty(),
            "#[crate::test] without a crate-root alias must not become a test edit: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn added_module_crate_tests_lookalike_is_not_a_test_edit() -> Result<(), String> {
        let fixture = AliasWorkspace::new("lookalike", "use my_macros::wrapped_test as test;\n")?;
        let diff = fixture.add_module_item(LOOKALIKE)?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "lookalike must resolve closed: {changes:?}"
        );
        assert!(
            changes.files.is_empty(),
            "#[crate::tests] must not inherit crate::test credit: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn added_module_crate_test_with_foreign_import_is_not_a_test_edit() -> Result<(), String> {
        let fixture =
            AliasWorkspace::new("foreign", "use external_macros::wrapped_test as test;\n")?;
        let diff = fixture.add_module_item(ADDED_TEST)?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "foreign import must resolve closed: {changes:?}"
        );
        assert!(
            changes.files.is_empty(),
            "a namesake wrapper must not credit a foreign import: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn added_module_builtin_test_stays_a_test_edit() -> Result<(), String> {
        let fixture = AliasWorkspace::new("builtin", "")?;
        let diff = fixture.add_module_item(BUILTIN)?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "builtin #[test] must resolve: {changes:?}"
        );
        assert_eq!(
            changes.files,
            [PathBuf::from("app/src/user.rs")],
            "ordinary #[test] in a module must not require alias companions: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn overlapping_companion_test_line_does_not_credit_production_edit() -> Result<(), String> {
        let fixture = AliasWorkspace::new("overlap", "use my_macros::wrapped_test as test;\n")?;
        let crate_root_test = format!(
            "use my_macros::wrapped_test as test;\npub fn value() -> u32 {{ 1 }}\n\n#[test]\nfn crate_root_test() {{\n{}\n}}\n",
            "    let _ = value();\n".repeat(12)
        );
        fixture.write_relative("app/src/lib.rs", &crate_root_test)?;
        fixture.write_relative("app/src/user.rs", &format!("// module\n{ADDED_TEST}"))?;
        fixture.commit("seed wrapper and crate-root test")?;
        let diff = fixture.add_module_item(&format!(
            "{ADDED_TEST}pub fn production() -> u32 {{\n    2\n}}\n"
        ))?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "overlapping companion fixture must resolve: {changes:?}"
        );
        assert!(
            changes.files.is_empty(),
            "a production edit must not inherit a companion test at the same line: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn dirty_worktree_missing_path_still_credits_head_wrapper() -> Result<(), String> {
        let fixture =
            AliasWorkspace::new("dirty-head-ok", "use my_macros::wrapped_test as test;\n")?;
        let diff = fixture.add_module_item(ADDED_TEST)?;
        fixture.write_relative(
            "app/Cargo.toml",
            &AliasWorkspace::app_manifest("../macros-missing"),
        )?;
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "HEAD wrapper path must still resolve through a dirty missing worktree path: {changes:?}"
        );
        assert_eq!(
            changes.files,
            [PathBuf::from("app/src/user.rs")],
            "a dirty missing worktree path must not drop HEAD wrapper credit: {changes:?}"
        );
        Ok(())
    }

    #[test]
    fn dirty_worktree_valid_path_does_not_credit_when_head_path_missing() -> Result<(), String> {
        let fixture =
            AliasWorkspace::new("dirty-head-miss", "use my_macros::wrapped_test as test;\n")?;
        fixture.write_relative(
            "app/Cargo.toml",
            &AliasWorkspace::app_manifest("../macros-missing"),
        )?;
        fixture.commit("head missing wrapper path")?;
        fixture.write_relative("app/Cargo.toml", &AliasWorkspace::app_manifest("../macros"))?;
        fixture.write_relative("app/src/user.rs", &format!("// module\n{ADDED_TEST}"))?;
        fixture.commit_paths("after", &["app/src/user.rs"])?;
        let diff = fixture_git_stdout(&fixture.root, &["diff", "--unified=0", "HEAD^", "HEAD"])?;
        if !diff.contains("diff --git a/app/src/user.rs b/app/src/user.rs") {
            return Err(format!("expected a module-only diff, got {diff}"));
        }
        if diff.contains("diff --git a/app/Cargo.toml") {
            return Err(format!("dirty manifest must stay uncommitted: {diff}"));
        }
        let changes = changes_for(&fixture.root, &diff);
        assert!(
            !changes.unresolved,
            "missing HEAD wrapper path must resolve closed: {changes:?}"
        );
        assert!(
            changes.files.is_empty(),
            "a dirty worktree wrapper path must not credit a missing HEAD dependency: {changes:?}"
        );
        Ok(())
    }
}
