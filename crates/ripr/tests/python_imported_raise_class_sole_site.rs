//! #15356: a raise swapped from a builtin to an imported exception class,
//! `raise ValueError(...)` to `raise InvalidVersion(...)`, is told apart by a
//! plain `with pytest.raises(InvalidVersion): owner(...)` when the changed line
//! is the class's only construction site and the test imports the class from
//! the module that defines it. A context expecting the base class passes
//! against both raises, and the same spelling imported from another module is
//! a different class: both stay a gap.

#![cfg(feature = "lang-python")]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

const ERRORS: &str = "class InvalidVersion(ValueError):\n    pass\n";
const OWNER_OLD: &str = "from verlib.errors import InvalidVersion\n\n\ndef parse(text):\n    if not text:\n        raise ValueError(\"empty version\")\n    return text\n";
const OLD_RAISE: &str = "raise ValueError(\"empty version\")";
const NEW_RAISE: &str = "raise InvalidVersion(\"empty version\")";

const BASE_CLASS_TEST: &str = "import pytest\n\nfrom verlib.core import parse\n\n\ndef test_empty():\n    with pytest.raises(ValueError):\n        parse(\"\")\n";
const DEFINING_MODULE_TEST: &str = "import pytest\n\nfrom verlib.core import parse\nfrom verlib.errors import InvalidVersion\n\n\ndef test_empty():\n    with pytest.raises(InvalidVersion):\n        parse(\"\")\n";
const OTHER_MODULE_TEST: &str = "import pytest\n\nfrom other.errors import InvalidVersion\nfrom verlib.core import parse\n\n\ndef test_empty():\n    with pytest.raises(InvalidVersion):\n        parse(\"\")\n";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

impl Repo {
    /// `extra` are further production files committed in the base.
    fn create(test: &str, extra: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-python-raise-sole-site-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self(root);
        repo.write(".gitignore", "__pycache__/\ntarget/\n")?;
        repo.write("ripr.toml", "[languages]\nenabled = [\"python\"]\n")?;
        repo.write(
            "pyproject.toml",
            "[project]\nname = \"verlib\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\npythonpath = [\"src\"]\n",
        )?;
        repo.write("src/verlib/__init__.py", "")?;
        repo.write("src/verlib/errors.py", ERRORS)?;
        repo.write("src/verlib/core.py", OWNER_OLD)?;
        for (name, text) in extra {
            repo.write(name, text)?;
        }
        repo.write("tests/test_parse.py", test)?;
        let root = &repo.0;
        fixture_git_ok(root, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(root, &["config", "commit.gpgsign", "false"])?;
        fixture_git_ok(root, &["config", "user.name", "fixture"])?;
        fixture_git_ok(root, &["config", "user.email", "fixture@example.invalid"])?;
        fixture_git_ok(root, &["add", "-A"])?;
        fixture_git_ok(root, &["commit", "-q", "-m", "base"])?;
        if !OWNER_OLD.contains(OLD_RAISE) {
            return Err("the owner fixture must carry the old raise".to_string());
        }
        repo.write(
            "src/verlib/core.py",
            &OWNER_OLD.replace(OLD_RAISE, NEW_RAISE),
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
        if !finding.to_string().contains("verlib/core.py::parse") {
            return Err(format!("the finding must be owned by `parse`: {finding}"));
        }
        finding
            .get("classification")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("finding must carry a classification: {finding}"))
    }
}

#[test]
fn base_class_context_does_not_tell_the_raises_apart() -> Result<(), String> {
    let class = Repo::create(BASE_CLASS_TEST, &[])?.classification()?;
    if class != "weakly_exposed" {
        return Err(format!(
            "pytest.raises(ValueError) passes against both raises; got {class}"
        ));
    }
    Ok(())
}

#[test]
fn defining_module_class_context_on_the_sole_raise_site_is_exposed() -> Result<(), String> {
    let class = Repo::create(DEFINING_MODULE_TEST, &[])?.classification()?;
    if class != "exposed" {
        return Err(format!(
            "pytest.raises(InvalidVersion) from verlib.errors fails against the old ValueError; got {class}"
        ));
    }
    Ok(())
}

#[test]
fn same_spelling_from_another_module_stays_a_gap() -> Result<(), String> {
    let other: [(&str, &str); 2] = [
        ("src/other/__init__.py", ""),
        (
            "src/other/errors.py",
            "class InvalidVersion(Exception):\n    pass\n",
        ),
    ];
    let class = Repo::create(OTHER_MODULE_TEST, &other)?.classification()?;
    if class != "weakly_exposed" {
        return Err(format!(
            "other.errors.InvalidVersion is not the raised class; got {class}"
        ));
    }
    Ok(())
}

#[test]
fn a_second_production_use_of_the_class_stays_a_gap() -> Result<(), String> {
    let other: [(&str, &str); 1] = [(
        "src/verlib/strict.py",
        "from verlib.errors import InvalidVersion\n\n\ndef strict(text):\n    raise InvalidVersion(text)\n",
    )];
    let class = Repo::create(DEFINING_MODULE_TEST, &other)?.classification()?;
    if class != "weakly_exposed" {
        return Err(format!(
            "another module also constructs InvalidVersion, so the changed line is not its sole site; got {class}"
        ));
    }
    Ok(())
}
