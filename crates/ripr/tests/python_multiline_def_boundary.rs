//! #15292: a Python owner whose `def` spans several lines (`max_age: int | None
//! = None,` on a continuation line) is not "rebinding" its parameter. The
//! continuation lines used to read as annotated assignments in the owner body,
//! which made the compared parameter look rebound, left the boundary
//! activation unknown, and kept the oracle verdict: a `>` to `>=` flip that no
//! test places on the boundary read `exposed`. The control places the boundary
//! and stays `exposed`.

#![cfg(feature = "lang-python")]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

const OWNER_OLD: &str = "def expired(\n    now: int,\n    max_age: int | None = None,\n    strict: bool = False,\n) -> int:\n    age = now - 100\n    if age > max_age:\n        return 1\n    return 0\n";
const OFF_BOUNDARY_TEST: &str = "from fx.core import expired\n\n\ndef test_expired():\n    assert expired(105, max_age=10) == 0\n    assert expired(111, max_age=10) == 1\n";

/// A boundary test needs the compared operand to be a parameter: the control
/// compares two parameters, one of which the test places on the other.
const CONTROL_OLD: &str = "def over(\n    age: int,\n    max_age: int | None = None,\n) -> int:\n    if age > max_age:\n        return 1\n    return 0\n";
const CONTROL_TEST: &str = "from fx.core import over\n\n\ndef test_over():\n    assert over(10, max_age=10) == 0\n    assert over(11, max_age=10) == 1\n";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

impl Repo {
    fn create(old: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-python-multiline-def-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self(root);
        repo.write(".gitignore", "__pycache__/\ntarget/\n")?;
        repo.write("ripr.toml", "[languages]\nenabled = [\"python\"]\n")?;
        repo.write(
            "pyproject.toml",
            "[project]\nname = \"fx\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\npythonpath = [\"src\"]\n",
        )?;
        repo.write("src/fx/__init__.py", "")?;
        repo.write("src/fx/core.py", old)?;
        repo.write("tests/test_t.py", test)?;
        let root = &repo.0;
        fixture_git_ok(root, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(root, &["config", "commit.gpgsign", "false"])?;
        fixture_git_ok(root, &["config", "user.name", "fixture"])?;
        fixture_git_ok(root, &["config", "user.email", "fixture@example.invalid"])?;
        fixture_git_ok(root, &["add", "-A"])?;
        fixture_git_ok(root, &["commit", "-q", "-m", "base"])?;
        repo.write("src/fx/core.py", &old.replace("> max_age", ">= max_age"))?;
        fixture_git_ok(root, &["commit", "-q", "-am", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, text).map_err(|error| error.to_string())
    }

    fn classification(&self) -> Result<String, String> {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(["check", "--base", "HEAD~1", "--format", "json"])
            .output()
            .map_err(|error| error.to_string())?;
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
        let findings = value
            .get("findings")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "check json must carry findings".to_string())?;
        let [finding] = findings.as_slice() else {
            return Err(format!("exactly one finding expected: {findings:?}"));
        };
        finding
            .get("classification")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("finding must carry a classification: {finding}"))
    }
}

#[test]
fn multiline_def_boundary_no_test_places_reads_weak() -> Result<(), String> {
    let repo = Repo::create(OWNER_OLD, OFF_BOUNDARY_TEST)?;
    let class = repo.classification()?;
    if class != "weakly_exposed" {
        return Err(format!(
            "no test places age == max_age, so the flip must not read exposed; got {class}"
        ));
    }
    Ok(())
}

#[test]
fn multiline_def_boundary_a_test_places_stays_exposed() -> Result<(), String> {
    let repo = Repo::create(CONTROL_OLD, CONTROL_TEST)?;
    let class = repo.classification()?;
    if class != "exposed" {
        return Err(format!(
            "over(10, max_age=10) places the boundary, so the flip stays exposed; got {class}"
        ));
    }
    Ok(())
}
