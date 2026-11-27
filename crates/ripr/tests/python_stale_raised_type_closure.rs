//! #13622: `ripr continue` must not report a Python gap closed when the new
//! test's only crediting assertion pins the old raised type.
//!
//! The production change replaces `raise ValueError("negative quantity")` with
//! `raise TypeError("negative quantity")`. A test written during the repair
//! that still expects `pytest.raises(ValueError, match=...)` fails against the
//! new code, so the after phase refuses closure, names both types and the
//! next step, and keeps the attempt retryable. The exposure class is not
//! changed (RIPR-SPEC-0238 example 14); only the repair outcome refuses.
//! Negative controls: the new-type forms close the gap.

#![cfg(feature = "lang-python")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

const BASE_SOURCE: &str = "def validate_qty(qty):\n    if qty < 0:\n        raise ValueError(\"negative quantity\")\n    return qty\n";
const HEAD_SOURCE: &str = "def validate_qty(qty):\n    if qty < 0:\n        raise TypeError(\"negative quantity\")\n    return qty\n";
/// A smoke call with no strong oracle: the gap is open and the printed
/// gap-bound start is admitted (`no_strong_oracle`).
const BASE_TESTS: &str =
    "from shop.core import validate_qty\n\n\ndef test_positive():\n    validate_qty(3)\n";

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("mkdir {relative}: {error}"))?;
    }
    std::fs::write(&path, text).map_err(|error| format!("write {relative}: {error}"))
}

/// A two-commit pytest repository: the base raises `ValueError`, the head
/// raises `TypeError`, and the only test is a smoke call that never enters
/// the raise.
fn repository(label: &str) -> Result<(TempRoot, PathBuf), String> {
    repository_with(label, BASE_SOURCE, HEAD_SOURCE)
}

fn repository_with(
    label: &str,
    base_source: &str,
    head_source: &str,
) -> Result<(TempRoot, PathBuf), String> {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-13622-{label}-{}-{counter}",
        std::process::id()
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|error| format!("clear temp: {error}"))?;
    }
    let guard = TempRoot(dir.clone());
    let root = dir.join("repo");
    write(
        &root,
        "pyproject.toml",
        "[project]\nname = \"shop\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\ntestpaths = [\"tests\"]\n",
    )?;
    write(&root, "shop/__init__.py", "")?;
    write(&root, "shop/core.py", base_source)?;
    write(&root, "tests/test_core.py", BASE_TESTS)?;
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
        ("user.name", "fixture"),
        ("user.email", "fixture@example.invalid"),
    ] {
        fixture_git_ok(&root, &["config", key, value])?;
    }
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    write(&root, "shop/core.py", head_source)?;
    fixture_git_ok(&root, &["commit", "-q", "-am", "raise TypeError"])?;
    Ok((guard, root))
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn combined(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn need(text: &str, needle: &str, label: &str) -> Result<(), String> {
    if text.contains(needle) {
        return Ok(());
    }
    Err(format!("{label}: expected `{needle}` in:\n{text}"))
}

/// Starts the printed gap-bound repair and returns the attempt id.
fn start_attempt(root: &Path) -> Result<String, String> {
    let root_arg = root.to_string_lossy().into_owned();
    let check = ripr(root, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    let check_text = combined(&check);
    let at = check_text
        .find("ripr agent repair --gap-id ")
        .ok_or_else(|| format!("check printed no gap-bound start:\n{check_text}"))?;
    let tokens: Vec<&str> = check_text[at..].split_whitespace().collect();
    let value_after = |flag: &str| -> Result<String, String> {
        let position = tokens
            .iter()
            .position(|token| *token == flag)
            .ok_or_else(|| format!("start lacks {flag}:\n{check_text}"))?;
        tokens
            .get(position + 1)
            .map(|token| token.trim_matches(|c| c == '\'' || c == '"').to_string())
            .ok_or_else(|| format!("start lacks a {flag} value:\n{check_text}"))
    };
    let gap = value_after("--gap-id")?;
    let base = value_after("--base")?;
    if !gap.starts_with("gap:python:shop/core.py:validate_qty:") {
        return Err(format!("unexpected gap `{gap}`:\n{check_text}"));
    }
    let before = ripr(
        root,
        &[
            "agent", "repair", "--gap-id", &gap, "--base", &base, "--root", &root_arg, "--phase",
            "before",
        ],
    )?;
    let before_text = combined(&before);
    if !before.status.success() {
        return Err(format!("before phase failed:\n{before_text}"));
    }
    before_text
        .lines()
        .find_map(|line| line.trim().strip_prefix("Next, after the test edit: "))
        .and_then(|command| {
            command
                .split_whitespace()
                .skip_while(|token| *token != "--attempt")
                .nth(1)
                .map(|token| token.trim_matches(|c| c == '\'' || c == '"').to_string())
        })
        .ok_or_else(|| format!("before printed no continue command:\n{before_text}"))
}

fn commit_test(root: &Path, test: &str, message: &str) -> Result<(), String> {
    write(
        root,
        "tests/test_core.py",
        &format!("import pytest\n{BASE_TESTS}\n\n{test}"),
    )?;
    fixture_git_ok(root, &["commit", "-q", "-am", message])
}

fn continue_attempt(root: &Path, attempt: &str) -> Result<Output, String> {
    let root_arg = root.to_string_lossy().into_owned();
    ripr(
        root,
        &["continue", "--root", &root_arg, "--attempt", attempt],
    )
}

const STALE_TEST: &str = "def test_negative():\n    with pytest.raises(ValueError, match=\"negative quantity\"):\n        validate_qty(-1)\n";

#[test]
fn stale_old_type_test_is_not_reported_closed_and_names_both_types() -> Result<(), String> {
    let (_guard, root) = repository("stale")?;
    let attempt = start_attempt(&root)?;
    commit_test(&root, STALE_TEST, "stale old-type test")?;
    let after = continue_attempt(&root, &attempt)?;
    let text = combined(&after);
    if after.status.code() != Some(3) {
        return Err(format!(
            "a stale old-type test must be a typed refusal (exit 3), got {:?}:\n{text}",
            after.status.code()
        ));
    }
    if text.contains("closed: weakly_exposed -> exposed") || text.contains("\"closed\": true") {
        return Err(format!("the stale test was reported closed:\n{text}"));
    }
    need(&text, "pins the old raised type `ValueError`", "old type")?;
    need(
        &text,
        "is expected to fail against the new `TypeError` raise",
        "new type",
    )?;
    need(&text, "pytest.raises(TypeError, ...)", "next step")?;
    need(&text, "remains `awaiting_edit`", "attempt stays retryable")?;
    if root.join("target/ripr/reports/agent-receipt.json").exists() {
        return Err("a receipt was issued for a stale old-type test".to_string());
    }
    // The corrected new-type test then closes the same attempt.
    commit_test(
        &root,
        "def test_negative():\n    with pytest.raises(TypeError, match=\"negative quantity\"):\n        validate_qty(-1)\n",
        "new-type test",
    )?;
    let retry = continue_attempt(&root, &attempt)?;
    let retry_text = combined(&retry);
    if !retry.status.success() {
        return Err(format!(
            "the corrected test must close the gap:\n{retry_text}"
        ));
    }
    need(
        &retry_text,
        "closed: weakly_exposed -> exposed",
        "corrected test closes",
    )
}

#[test]
fn new_type_forms_close_the_gap() -> Result<(), String> {
    for (label, test) in [
        (
            "raises-new",
            "def test_negative():\n    with pytest.raises(TypeError, match=\"negative quantity\"):\n        validate_qty(-1)\n",
        ),
        (
            "excinfo-type",
            "def test_negative():\n    with pytest.raises(TypeError, match=\"negative quantity\") as excinfo:\n        validate_qty(-1)\n    assert excinfo.type is TypeError\n",
        ),
    ] {
        let (_guard, root) = repository(label)?;
        let attempt = start_attempt(&root)?;
        commit_test(&root, test, label)?;
        let after = continue_attempt(&root, &attempt)?;
        let text = combined(&after);
        if !after.status.success() {
            return Err(format!("{label}: the new-type test must close:\n{text}"));
        }
        need(&text, "closed: weakly_exposed -> exposed", label)?;
        if text.contains("pins the old raised type") {
            return Err(format!("{label}: refused as stale:\n{text}"));
        }
    }
    Ok(())
}

/// `QtyError(ValueError)`: the old-type assertion passes against the old and
/// the new raise alike, so it is no stale pin, and it does not close the gap
/// either: it cannot tell the two raises apart (#15356).
#[test]
fn old_type_assertion_of_a_subclass_raise_is_not_stale_and_does_not_close() -> Result<(), String> {
    let base = format!("class QtyError(ValueError):\n    pass\n\n\n{BASE_SOURCE}");
    let head = format!(
        "class QtyError(ValueError):\n    pass\n\n\n{}",
        HEAD_SOURCE.replace("TypeError", "QtyError")
    );
    let (_guard, root) = repository_with("subclass", &base, &head)?;
    let attempt = start_attempt(&root)?;
    commit_test(&root, STALE_TEST, "old-type test of a subclass raise")?;
    let after = continue_attempt(&root, &attempt)?;
    let text = combined(&after);
    if after.status.code() != Some(3)
        || text.contains("pins the old raised type")
        || text.contains("closed: weakly_exposed -> exposed")
    {
        return Err(format!(
            "a base-class test of a subclass raise must leave the gap open, got {:?}:\n{text}",
            after.status.code()
        ));
    }
    need(&text, "pytest.raises(QtyError)", "names the new class")
}

/// Review follow-up: a message built with an f-string does not change which
/// class is raised, so the stale old-type test is still refused.
#[test]
fn stale_old_type_test_is_refused_when_the_raise_message_is_an_f_string() -> Result<(), String> {
    let base = BASE_SOURCE.replace(
        "ValueError(\"negative quantity\")",
        "ValueError(f\"negative quantity {qty}\")",
    );
    let head = HEAD_SOURCE.replace(
        "TypeError(\"negative quantity\")",
        "TypeError(f\"negative quantity {qty}\")",
    );
    let (_guard, root) = repository_with("fstring", &base, &head)?;
    let attempt = start_attempt(&root)?;
    commit_test(
        &root,
        STALE_TEST,
        "stale old-type test of an f-string raise",
    )?;
    let after = continue_attempt(&root, &attempt)?;
    let text = combined(&after);
    if after.status.code() != Some(3) || text.contains("closed: weakly_exposed -> exposed") {
        return Err(format!(
            "an f-string raise with a stale old-type test must be refused, got {:?}:\n{text}",
            after.status.code()
        ));
    }
    need(&text, "pins the old raised type `ValueError`", "old type")?;
    need(
        &text,
        "is expected to fail against the new `TypeError` raise",
        "new type",
    )
}

/// #13753: the `expected_exception=` keyword and a tuple of the old class are
/// the same stale old-type pin. A tuple that also names the new class or a
/// broader class passes against the old and the new raise alike, so it is no
/// stale pin and does not close the gap either (#15356).
#[test]
fn keyword_and_tuple_old_type_tests_are_refused_unless_the_tuple_names_the_new_type()
-> Result<(), String> {
    for (label, expected, refused) in [
        ("keyword", "expected_exception=ValueError", true),
        ("tuple", "(ValueError, ValueError)", true),
        ("tuple-new", "(ValueError, TypeError)", false),
        ("tuple-broad", "(ValueError, Exception)", false),
    ] {
        let (_guard, root) = repository(label)?;
        let attempt = start_attempt(&root)?;
        commit_test(
            &root,
            &format!(
                "def test_negative():\n    with pytest.raises({expected}, match=\"negative quantity\"):\n        validate_qty(-1)\n"
            ),
            label,
        )?;
        let after = continue_attempt(&root, &attempt)?;
        let text = combined(&after);
        let stale = text.contains("pins the old raised type `ValueError`");
        if refused != (after.status.code() == Some(3) && stale) {
            return Err(format!(
                "{label}: refused={refused}, got {:?}:\n{text}",
                after.status.code()
            ));
        }
        if !refused {
            // Not a stale pin, but the gap stays open: the tuple accepts
            // the old and the new raise alike.
            if after.status.code() != Some(3)
                || stale
                || text.contains("closed: weakly_exposed -> exposed")
            {
                return Err(format!(
                    "{label}: must leave the gap open without a stale pin, got {:?}:\n{text}",
                    after.status.code()
                ));
            }
            need(&text, "still reads `weakly_exposed`", label)?;
        }
    }
    Ok(())
}

/// Review follow-up: `excinfo.type is New` under a broad `pytest.raises` names
/// the new class, so it is never refused as a stale old-type pin.
#[test]
fn broad_raises_with_excinfo_type_new_is_not_refused_as_stale() -> Result<(), String> {
    let (_guard, root) = repository("excinfo-only")?;
    let attempt = start_attempt(&root)?;
    commit_test(
        &root,
        "def test_negative():\n    with pytest.raises(Exception) as excinfo:\n        validate_qty(-1)\n    assert excinfo.type is TypeError\n",
        "excinfo type test",
    )?;
    let after = continue_attempt(&root, &attempt)?;
    let text = combined(&after);
    eprintln!("excinfo-only outcome {:?}:\n{text}", after.status.code());
    if text.contains("pins the old raised type") {
        return Err(format!(
            "excinfo.type is TypeError was refused as stale:\n{text}"
        ));
    }
    Ok(())
}
