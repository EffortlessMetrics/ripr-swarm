//! Owned boundary-gap fixture for conformance rows (#6307).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const FIXTURE_SEAM: &str = "67fc764ba37d77bd";
const FIXTURE_CARGO_TOML: &str = "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n";
const FIXTURE_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const FIXTURE_WEAK_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";
pub const BOUNDARY_TEST_APPEND: &str = "\n#[test]\nfn at_threshold_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

pub struct BoundaryFixture {
    pub root: PathBuf,
    decoy: PathBuf,
}

const FIXTURE_LIB_WEAKENED: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";

impl BoundaryFixture {
    pub fn new(label: &str) -> Result<Self, String> {
        let root = unique_temp_workspace(label);
        populate_boundary(&root)?;
        let decoy = unique_temp_workspace(&format!("{label}-decoy"));
        std::fs::create_dir_all(&decoy).map_err(|error| format!("decoy dir: {error}"))?;
        Ok(Self { root, decoy })
    }

    /// Uncommitted predicate weakening so `--worktree` check rows have a finding.
    pub fn with_worktree_gap(self) -> Result<Self, String> {
        std::fs::write(self.root.join("src/lib.rs"), FIXTURE_LIB_WEAKENED)
            .map_err(|error| format!("write weakened lib: {error}"))?;
        Ok(self)
    }

    pub fn root_arg(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    pub fn foreign_cwd(&self) -> &Path {
        &self.decoy
    }
}

impl Drop for BoundaryFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.decoy);
    }
}

fn unique_temp_workspace(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ripr-nac-{label}-{stamp}-{pid}-{counter}"))
}

fn run_git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn populate_boundary(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), FIXTURE_LIB).map_err(|e| e.to_string())?;
    std::fs::write(root.join("tests/pricing.rs"), FIXTURE_WEAK_TEST).map_err(|e| e.to_string())?;
    run_git(root, &["init"])?;
    std::fs::write(root.join(".git/info/exclude"), "/target/\n").map_err(|e| e.to_string())?;
    run_git(root, &["config", "core.autocrlf", "false"])?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    run_git(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-m",
            "fixture source",
        ],
    )?;
    Ok(())
}

pub fn run_ripr(cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

#[path = "../common/printed_words.rs"]
mod printed_words;

pub fn split_ripr_command(command: &str) -> Result<Vec<String>, String> {
    let tokens = printed_words::printed_words(command)?;
    if tokens.first().map(String::as_str) != Some("ripr") {
        return Err(format!("expected ripr command, got `{command}`"));
    }
    Ok(tokens)
}

pub fn attempt_ids(root: &Path) -> Vec<String> {
    let dir = root.join("target/ripr/repair-attempts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .filter(|name| name.starts_with("repair-attempt-"))
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}
