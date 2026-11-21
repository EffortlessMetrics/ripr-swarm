//! #15299: a Cargo workspace below the analysis root keeps the package
//! identity of its `crates/<name>` members.
//!
//! The repository root has no `Cargo.toml`; each workspace lives in its own
//! directory (`w1/`, `w2/`), the layout of a mixed-language monorepo. The
//! changed owner `clamp_score` has a same-named `fn` in a sibling crate, so
//! the test's call must be told apart by package. Every case asserts its
//! generated fixture first (the finding sits on the changed line of the
//! intended file) before any claim about the reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MEMBERS: &str = "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n";
const OWNER: &str = "pub fn clamp_score(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
const TEST: &str = "#[test]\nfn clamps_at_limit() {\n    assert_eq!(alpha::clamp_score(100), 100);\n    assert_eq!(alpha::clamp_score(101), 100);\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn package(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
}

/// One workspace `dir` with crates `alpha` and `beta` that both define
/// `clamp_score`; `alpha_test` adds `crates/alpha/tests/a.rs`.
fn workspace(dir: &str, alpha_test: bool) -> Vec<(String, String)> {
    let mut files = vec![
        (format!("{dir}/Cargo.toml"), MEMBERS.to_string()),
        (format!("{dir}/crates/alpha/Cargo.toml"), package("alpha")),
        (format!("{dir}/crates/alpha/src/lib.rs"), OWNER.to_string()),
        (format!("{dir}/crates/beta/Cargo.toml"), package("beta")),
        (format!("{dir}/crates/beta/src/lib.rs"), OWNER.to_string()),
    ];
    if alpha_test {
        files.push((format!("{dir}/crates/alpha/tests/a.rs"), TEST.to_string()));
    }
    files
}

/// The finding for the changed `x >= 100` (was `x > 100`) in `changed`.
fn finding(files: &[(String, String)], changed: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-nested-workspace-package-identity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| format!("dir {path:?}: {error}"))?;
        std::fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))
    };
    for (path, text) in files {
        write(path, text)?;
    }
    let diff = format!(
        "diff --git a/{changed} b/{changed}\nindex 0000000..1111111 100644\n--- a/{changed}\n+++ b/{changed}\n@@ -1,3 +1,3 @@\n pub fn clamp_score(x: i32) -> i32 {{\n-    if x > 100 {{ 100 }} else {{ x }}\n+    if x >= 100 {{ 100 }} else {{ x }}\n }}\n"
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check: {error}"))?;
    let found = output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == 2 && finding.probe.expression.contains("x >= 100")
        })
        .ok_or_else(|| format!("no finding for the changed `x >= 100` in {changed}"))?;
    let file = normalized(&found.probe.location.file);
    if !file.ends_with(&format!("/{changed}")) && file != changed {
        return Err(format!("finding sits in {file}, not {changed}"));
    }
    Ok(found)
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn related_files(found: &Finding) -> Vec<String> {
    let mut files: Vec<String> = found
        .related_tests
        .iter()
        .map(|test| normalized(&test.file))
        .collect();
    files.sort();
    files.dedup();
    files
}

#[test]
fn a_member_of_a_workspace_below_the_root_keeps_its_own_test() -> Result<(), String> {
    let found = finding(&workspace("w2", true), "w2/crates/alpha/src/lib.rs")?;
    assert_eq!(
        found.class,
        ExposureClass::Exposed,
        "evidence: {:?}",
        found.evidence
    );
    assert_eq!(related_files(&found), vec!["w2/crates/alpha/tests/a.rs"]);
    Ok(())
}

#[test]
fn the_same_crate_path_in_two_workspaces_names_two_packages() -> Result<(), String> {
    // `w1/crates/alpha` has no test; `w2/crates/alpha` does.
    let mut files = workspace("w1", false);
    files.extend(workspace("w2", true));

    let tested = finding(&files, "w2/crates/alpha/src/lib.rs")?;
    assert_eq!(
        tested.class,
        ExposureClass::Exposed,
        "evidence: {:?}",
        tested.evidence
    );
    assert_eq!(related_files(&tested), vec!["w2/crates/alpha/tests/a.rs"]);

    // The other workspace's test calls its own `alpha`, never this one.
    let no_test = finding(&files, "w1/crates/alpha/src/lib.rs")?;
    assert_ne!(no_test.class, ExposureClass::Exposed);
    assert_eq!(related_files(&no_test), Vec::<String>::new());
    Ok(())
}
