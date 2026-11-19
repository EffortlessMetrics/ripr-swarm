//! Public render dispatch anchors GitHub `file=` and SARIF uris at the Git
//! checkout top level when the analyzed root is a workspace member below it
//! (#15367). The renderer unit tests inject an anchor; these controls go
//! through `render_check`, so they also fail if dispatch stops discovering it.
use ripr::{CheckInput, CheckOutput, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct FixtureDir(PathBuf);
impl FixtureDir {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-anchor-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The smallest directory Git's discovery accepts as a repository (`HEAD`,
/// `objects/`, `refs/`), so the control needs no Git subprocess of its own.
fn mark_git_top_level(top: &Path) -> Result<(), std::io::Error> {
    let git = top.join(".git");
    std::fs::create_dir_all(git.join("objects"))?;
    std::fs::create_dir_all(git.join("refs").join("heads"))?;
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n")?;
    Ok(())
}

fn check(root: &Path, patch: &str) -> Result<CheckOutput, Box<dyn std::error::Error>> {
    let patch_path = root.join("change.patch");
    std::fs::write(&patch_path, patch)?;
    let output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(patch_path),
        ..CheckInput::default()
    })?;
    assert!(
        !output.findings.is_empty(),
        "setup must produce an actual finding"
    );
    Ok(output)
}

fn github_files(output: &CheckOutput) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let text = render_check(output, &OutputFormat::Github)?;
    let mut files: Vec<String> = text
        .lines()
        .filter_map(|line| line.split("file=").nth(1))
        .filter_map(|rest| rest.split([',', ':']).next())
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn sarif_uris(output: &CheckOutput) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut uris = Vec::new();
    for format in [OutputFormat::Sarif, OutputFormat::SarifCompact] {
        let document: Value = serde_json::from_str(&render_check(output, &format)?)?;
        let results = document["runs"][0]["results"]
            .as_array()
            .ok_or("missing results")?;
        for result in results {
            let uri = result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                .as_str()
                .ok_or("missing uri")?;
            uris.push(uri.to_string());
        }
    }
    uris.sort();
    uris.dedup();
    Ok(uris)
}

#[cfg(feature = "lang-rust")]
#[test]
fn member_root_rust_findings_name_files_from_the_checkout_top_level()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = FixtureDir::new("rust")?;
    let top = dir.path();
    let member = top.join("crates").join("app");
    std::fs::create_dir_all(member.join("src"))?;
    std::fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = \"anchor_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    std::fs::write(
        member.join("src/lib.rs"),
        "pub fn accepts(value: i32) -> bool {\n    value > 0\n}\n",
    )?;
    let patch = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn accepts(value: i32) -> bool {\n-    value >= 0\n+    value > 0\n }\n";

    // Control: outside any work tree the location stays root-relative.
    let unanchored = check(&member, patch)?;
    assert_eq!(github_files(&unanchored)?, vec!["src/lib.rs".to_string()]);
    assert_eq!(sarif_uris(&unanchored)?, vec!["src/lib.rs".to_string()]);

    mark_git_top_level(top)?;
    let anchored = check(&member, patch)?;
    assert_eq!(
        github_files(&anchored)?,
        vec!["crates/app/src/lib.rs".to_string()]
    );
    assert_eq!(
        sarif_uris(&anchored)?,
        vec!["crates/app/src/lib.rs".to_string()]
    );
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn member_root_python_findings_name_files_from_the_checkout_top_level()
-> Result<(), Box<dyn std::error::Error>> {
    // Preview-language findings carry root-relative files (`src/calc.py`),
    // unlike root-joined Rust findings; both must anchor the same way.
    let dir = FixtureDir::new("python")?;
    let top = dir.path();
    let member = top.join("pkg");
    std::fs::create_dir_all(member.join("src"))?;
    std::fs::create_dir_all(member.join("tests"))?;
    std::fs::write(member.join("pyproject.toml"), "[project]\nname = \"p\"\n")?;
    std::fs::write(
        member.join("src/calc.py"),
        "def big(x):\n    return x >= 3\n",
    )?;
    std::fs::write(
        member.join("tests/test_calc.py"),
        "from src.calc import big\n\n\ndef test_big():\n    assert big(5)\n",
    )?;
    let patch = "diff --git a/src/calc.py b/src/calc.py\n--- a/src/calc.py\n+++ b/src/calc.py\n@@ -1,2 +1,2 @@\n def big(x):\n-    return x > 3\n+    return x >= 3\n";

    let unanchored = check(&member, patch)?;
    assert_eq!(github_files(&unanchored)?, vec!["src/calc.py".to_string()]);

    mark_git_top_level(top)?;
    let anchored = check(&member, patch)?;
    assert_eq!(
        github_files(&anchored)?,
        vec!["pkg/src/calc.py".to_string()]
    );
    assert_eq!(sarif_uris(&anchored)?, vec!["pkg/src/calc.py".to_string()]);
    Ok(())
}
