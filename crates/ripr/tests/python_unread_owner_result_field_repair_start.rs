//! The nightly `python/builder` repair-start case: a changed `self.<field>`
//! write in a self-returning builder method whose only test binds the result
//! (`r = Req(url).timeout(10)`) and reads only `r.url`. `ripr agent repair
//! --phase before` starts on that gap, and the repaired test
//! (`assert r.timeout_value == 20`) reads `exposed`, so the attempt can close.
//! A test that already reads the field, or a second related test, gets no
//! start.

#![cfg(feature = "lang-python")]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

const GAP: &str =
    "gap:python:src/fx/core.py:Req.timeout:field_value:field_construction:self.timeout_value=t*2";
const CORE_OLD: &str = "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t\n        return self\n";
const UNREAD_TEST: &str = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n";
const REPAIRED_TEST: &str = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.timeout_value == 20\n";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

impl Repo {
    /// `CORE_OLD` and `test` committed on `main`, then the `t * 2` change
    /// committed on top.
    fn create(test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-python-unread-field-{}-{}",
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
        repo.write("src/fx/core.py", CORE_OLD)?;
        repo.write("tests/test_t.py", test)?;
        let root = &repo.0;
        fixture_git_ok(root, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(root, &["config", "commit.gpgsign", "false"])?;
        fixture_git_ok(root, &["config", "user.name", "fixture"])?;
        fixture_git_ok(root, &["config", "user.email", "fixture@example.invalid"])?;
        fixture_git_ok(root, &["add", "-A"])?;
        fixture_git_ok(root, &["commit", "-q", "-m", "base"])?;
        repo.write(
            "src/fx/core.py",
            &CORE_OLD.replace("self.timeout_value = t\n", "self.timeout_value = t * 2\n"),
        )?;
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

    fn ripr(&self, args: &[&str]) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|error| error.to_string())
    }

    fn repair_before(&self) -> Result<(Option<i32>, String), String> {
        let output = self.ripr(&[
            "agent", "repair", "--root", ".", "--gap-id", GAP, "--base", "HEAD~1", "--phase",
            "before",
        ])?;
        Ok((output.status.code(), text(&output)))
    }

    fn finding(&self) -> Result<serde_json::Value, String> {
        let output = self.ripr(&["check", "--base", "HEAD~1", "--format", "json"])?;
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
        value
            .get("findings")
            .and_then(serde_json::Value::as_array)
            .and_then(|findings| {
                findings.iter().find(|finding| {
                    finding.get("canonical_gap_id").and_then(|id| id.as_str()) == Some(GAP)
                })
            })
            .cloned()
            .ok_or_else(|| format!("check must report {GAP}: {}", text(&output)))
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn str_at<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

#[test]
fn unread_field_of_a_bound_builder_result_starts_repair_and_can_close() -> Result<(), String> {
    let repo = Repo::create(UNREAD_TEST)?;
    let finding = repo.finding()?;
    if str_at(&finding, "classification") != Some("weakly_exposed")
        || str_at(&finding, "oracle_alignment") != Some("orthogonal")
    {
        return Err(format!(
            "the unread case keeps its orthogonal weak reading: {finding}"
        ));
    }
    let (code, repair) = repo.repair_before()?;
    if code != Some(0) || !repair.contains("repair started") {
        return Err(format!("repair must start (exit {code:?}):\n{repair}"));
    }
    // The repair the card asks for reads exposed, so the attempt can close.
    repo.write("tests/test_t.py", REPAIRED_TEST)?;
    let after = repo.finding()?;
    if str_at(&after, "classification") != Some("exposed")
        || str_at(&after, "alignment_reason") != Some("strong_oracle_observes_owner_result_field")
    {
        return Err(format!("the repaired test must read exposed: {after}"));
    }
    Ok(())
}

#[test]
fn a_test_that_reads_the_field_gets_no_start() -> Result<(), String> {
    let repo = Repo::create(
        "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.timeout_value > 0\n",
    )?;
    let (code, repair) = repo.repair_before()?;
    if code == Some(0) || repair.contains("repair started") {
        return Err(format!(
            "a field read must not start a repair (exit {code:?}):\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn two_related_tests_get_no_start() -> Result<(), String> {
    let repo = Repo::create(&format!(
        "{UNREAD_TEST}\n\ndef test_builder_sets_url_again():\n    s = Req('http://y').timeout(3)\n    assert s.url == 'http://y'\n"
    ))?;
    let (code, repair) = repo.repair_before()?;
    if code == Some(0) || repair.contains("repair started") {
        return Err(format!(
            "two related tests must not start a repair (exit {code:?}):\n{repair}"
        ));
    }
    Ok(())
}
