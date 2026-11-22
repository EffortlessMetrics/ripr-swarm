//! #15302: the printed pytest verify route runs from the Python project that
//! owns the test file, not blindly from the repository root.
//!
//! Run from the root, `python -m pytest py/tests/test_price.py::test_discount`
//! cannot import the project's own package (`found no collectors`), yet was
//! labelled high confidence. The Python producer now names the nested project
//! directory (`suggested_verify_cwd`) and spells the command relative to it;
//! every surface carries that one decision. A root-level project is the
//! control: no cwd, root-relative command, unchanged.

// Every test here is `not(windows)`; the whole target is unconfigured on
// Windows so its helpers are not dead code there under `-D warnings`.
#![cfg(all(feature = "lang-python", not(windows)))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

const PRICE_BASE: &str =
    "def discount(price, qty):\n    if qty >= 10:\n        return price * 0.9\n    return price\n";
const PRICE_HEAD: &str =
    "def discount(price, qty):\n    if qty >= 5:\n        return price * 0.9\n    return price\n";
const TEST: &str =
    "from pkg.price import discount\n\n\ndef test_discount():\n    assert discount(10, 10) > 0\n";
const PYPROJECT: &str = "[project]\nname = \"pkg\"\nversion = \"0.0.0\"\n";
const CONFIG: &str = "[languages]\nenabled = [\"python\"]\n";

fn write(root: &Path, file: &str, text: &str) -> Result<(), String> {
    let path = root.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create {file}: {error}"))?;
    }
    std::fs::write(&path, text).map_err(|error| format!("write {file}: {error}"))
}

/// A git repository with a base commit and a head commit that moves the
/// `qty` boundary of `<project>/pkg/price.py`. `project` is `""` for a
/// root-level project or a nested directory such as `py`.
fn materialize(project: &str, extra: &[(&str, &str)]) -> Result<(TempDirGuard, PathBuf), String> {
    let launch = std::env::temp_dir().join(format!(
        "ripr-nested-verify-cwd-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let guard = TempDirGuard(launch.clone());
    let root = launch.join("repo");
    let at = |file: &str| {
        if project.is_empty() {
            file.to_string()
        } else {
            format!("{project}/{file}")
        }
    };
    std::fs::create_dir_all(&root).map_err(|error| format!("create root: {error}"))?;
    write(&root, "ripr.toml", CONFIG)?;
    write(&root, "README.md", "monorepo\n")?;
    write(&root, &at("pyproject.toml"), PYPROJECT)?;
    write(&root, &at("pkg/__init__.py"), "")?;
    write(&root, &at("pkg/price.py"), PRICE_BASE)?;
    write(&root, &at("tests/test_price.py"), TEST)?;
    for (file, text) in extra {
        write(&root, file, text)?;
    }
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    write(&root, &at("pkg/price.py"), PRICE_HEAD)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "head"])?;
    Ok((guard, root))
}

fn check(root: &Path, format: &str) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--quiet", "--base", "HEAD~1", "--format", format])
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "check --format {format} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn weak_python_finding(json: &str) -> Result<serde_json::Value, String> {
    let document: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("check JSON: {error}"))?;
    let findings = document["findings"]
        .as_array()
        .ok_or_else(|| format!("no findings array:\n{json}"))?;
    findings
        .iter()
        .find(|finding| {
            finding["classification"] == "weakly_exposed" && finding["language"] == "python"
        })
        .cloned()
        .ok_or_else(|| format!("no weakly exposed python finding:\n{json}"))
}

fn evidence_line<'a>(finding: &'a serde_json::Value, prefix: &str) -> Option<&'a str> {
    finding["evidence"]
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find_map(|line| line.strip_prefix(prefix))
}

#[cfg(not(windows))]
#[test]
fn nested_project_verify_runs_from_the_project_directory_on_every_surface() -> Result<(), String> {
    let (_guard, root) = materialize("py", &[])?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;

    let command = "python -m pytest tests/test_price.py::test_discount";
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some(command),
        "{json}"
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_cwd: "),
        Some("py")
    );
    assert_eq!(finding["repair_placement"]["verify_command"], command);
    assert_eq!(finding["repair_placement"]["verify_cwd"], "py");
    assert_eq!(finding["python_repair_card"]["verify"]["command"], command);
    assert_eq!(finding["python_repair_card"]["verify"]["cwd"], "py");
    // The test node stays root-relative; only the verify route is project-relative.
    assert_eq!(
        finding["repair_placement"]["suggested_test_node_id"],
        "py/tests/test_price.py::test_discount"
    );

    let human = check(&root, "human-full")?;
    let root_text = root.to_string_lossy();
    let verify = human
        .lines()
        .find(|line| line.contains("verify (Bash):"))
        .ok_or_else(|| format!("no verify line:\n{human}"))?;
    assert!(
        verify.contains(&format!("cd -P -- {root_text}/./py && {command}"))
            || verify.contains(&format!("cd -P -- '{root_text}/./py' && {command}")),
        "{verify}"
    );
    assert!(
        !verify.contains("py/tests/test_price.py"),
        "the root-relative path must not survive: {verify}"
    );

    let github = check(&root, "github")?;
    assert!(
        github.contains(&format!("verify `{command}` from `py/`")),
        "{github}"
    );
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn nested_project_resolves_its_own_venv_relative_to_the_project_directory() -> Result<(), String> {
    let site = "py/.venv/lib/python3.13/site-packages";
    let (_guard, root) = materialize(
        "py",
        &[
            ("py/.venv/bin/python", ""),
            ("py/.venv/pyvenv.cfg", "version = 3.13.1\n"),
            (&format!("{site}/pytest-9.2.0.dist-info/METADATA"), ""),
            (&format!("{site}/pytest/__init__.py"), ""),
        ],
    )?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some(".venv/bin/python -m pytest tests/test_price.py::test_discount"),
        "{json}"
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_cwd: "),
        Some("py")
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_venv_pytest: "),
        Some("present")
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command_confidence: "),
        Some("high")
    );
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn root_level_project_keeps_the_root_relative_route_without_a_cwd() -> Result<(), String> {
    let (_guard, root) = materialize("", &[])?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;
    let command = "python -m pytest tests/test_price.py::test_discount";
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some(command),
        "{json}"
    );
    assert_eq!(evidence_line(&finding, "suggested_verify_cwd: "), None);
    assert!(finding["repair_placement"].get("verify_cwd").is_none());
    assert!(finding["python_repair_card"]["verify"].get("cwd").is_none());

    let human = check(&root, "human-full")?;
    let verify = human
        .lines()
        .find(|line| line.contains("verify (Bash):"))
        .ok_or_else(|| format!("no verify line:\n{human}"))?;
    assert!(verify.contains(&format!("&& {command})")), "{verify}");
    Ok(())
}

/// A minimal POSIX venv at `<dir>/.venv`, with or without pytest installed.
fn venv_files(dir: &str, with_pytest: bool) -> Vec<(String, String)> {
    let at = |file: &str| {
        if dir.is_empty() {
            file.to_string()
        } else {
            format!("{dir}/{file}")
        }
    };
    let site = at(".venv/lib/python3.13/site-packages");
    let mut files = vec![
        (at(".venv/bin/python"), String::new()),
        (at(".venv/pyvenv.cfg"), "version = 3.13.1\n".to_string()),
        (format!("{site}/six.py"), String::new()),
    ];
    if with_pytest {
        files.push((
            format!("{site}/pytest-9.2.0.dist-info/METADATA"),
            String::new(),
        ));
        files.push((format!("{site}/pytest/__init__.py"), String::new()));
    }
    files
}

fn materialize_with(
    project: &str,
    extra: &[(String, String)],
) -> Result<(TempDirGuard, PathBuf), String> {
    let borrowed: Vec<(&str, &str)> = extra
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    materialize(project, &borrowed)
}

/// Two environments: the checkout root's venv has pytest, the nested project
/// has none. The route runs from `py/`, so the root venv is not the
/// interpreter; PATH `python` may lack pytest, so the route is not `high`.
#[cfg(not(windows))]
#[test]
fn nested_project_without_a_venv_does_not_inherit_the_root_venv_or_its_confidence()
-> Result<(), String> {
    let (_guard, root) = materialize_with("py", &venv_files("", true))?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some("python -m pytest tests/test_price.py::test_discount"),
        "{json}"
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_cwd: "),
        Some("py")
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command_confidence: "),
        Some("medium"),
        "{json}"
    );
    Ok(())
}

/// Negative control for the interpreter/confidence pairing: the root venv has
/// pytest but the nested project's own venv does not. The project's venv is
/// the interpreter, and its missing pytest (not the root's) sets confidence.
#[cfg(not(windows))]
#[test]
fn nested_project_venv_without_pytest_lowers_confidence_despite_a_root_venv_with_pytest()
-> Result<(), String> {
    let mut extra = venv_files("", true);
    extra.extend(venv_files("py", false));
    let (_guard, root) = materialize_with("py", &extra)?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some(".venv/bin/python -m pytest tests/test_price.py::test_discount"),
        "{json}"
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_cwd: "),
        Some("py")
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_venv_pytest: "),
        Some("missing")
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command_confidence: "),
        Some("low"),
        "{json}"
    );
    Ok(())
}

/// SARIF carries the same card, so the cwd and project-relative command reach
/// code-scanning consumers too.
#[cfg(not(windows))]
#[test]
fn nested_project_sarif_card_names_the_project_cwd() -> Result<(), String> {
    let (_guard, root) = materialize("py", &[])?;
    let sarif = check(&root, "sarif")?;
    let document: serde_json::Value =
        serde_json::from_str(&sarif).map_err(|error| format!("sarif JSON: {error}"))?;
    let card = document["runs"][0]["results"]
        .as_array()
        .ok_or_else(|| format!("no results:\n{sarif}"))?
        .iter()
        .find_map(|result| result["properties"].get("python_repair_card"))
        .ok_or_else(|| format!("no python repair card:\n{sarif}"))?;
    assert_eq!(card["verify"]["cwd"], "py", "{card}");
    assert_eq!(
        card["verify"]["command"],
        "python -m pytest tests/test_price.py::test_discount"
    );
    Ok(())
}

/// A root-level project whose `tests/` holds a lint-only `setup.cfg`: that
/// file is tool configuration, not a nested project, so the route stays at
/// the root where the uninstalled `pkg` is importable.
#[cfg(not(windows))]
#[test]
fn tool_only_setup_cfg_in_tests_does_not_move_the_root_project_cwd() -> Result<(), String> {
    let (_guard, root) = materialize(
        "",
        &[("tests/setup.cfg", "[flake8]\nmax-line-length = 100\n")],
    )?;
    let json = check(&root, "json")?;
    let finding = weak_python_finding(&json)?;
    assert_eq!(
        evidence_line(&finding, "suggested_verify_command: "),
        Some("python -m pytest tests/test_price.py::test_discount"),
        "{json}"
    );
    assert_eq!(
        evidence_line(&finding, "suggested_verify_cwd: "),
        None,
        "{json}"
    );
    Ok(())
}
