//! An owner call that only an untrusted inner macro holds gives the owner no
//! exact credit (#16188, RIPR-SPEC-0197).
//!
//! `assert_eq!(check_ok!(pick(0)), Ok(Mode::A))` with a test-local
//! `macro_rules! check_ok { ($e:expr) => { Ok(Mode::A) }; }` never evaluates
//! `pick(0)`, so a mutated `Ok(Mode::B)` survives `cargo test`. The repo view
//! must not read that seam `strongly_gripped`. The same assertion with the
//! call written plainly, inside a standard evaluating macro (`vec!`,
//! `format!`), or also outside the inner macro keeps its credit; a local
//! `macro_rules! vec` that shadows the standard name loses it.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CHECK_OK: &str = "macro_rules! check_ok { ($e:expr) => { Ok(Mode::A) }; }";

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// One package whose `pick` returns `Ok(Mode::A)` and whose test `picks_a`
    /// defines `macros` before running `assertion`.
    fn create(macros: &str, assertion: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-macro-held-owner-call-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write = |path: &str, text: &str| -> Result<(), String> {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(&file, text).map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write(
            "src/lib.rs",
            &format!(
                "#[derive(Debug, PartialEq)]
pub enum Mode {{ A, B }}

pub fn pick(n: u32) -> Result<Mode, ()> {{
    if n > 10 {{
        return Err(());
    }}
    Ok(Mode::A)
}}

#[cfg(test)]
mod tests {{
    use super::*;
    {macros}
    #[test]
    fn picks_a() {{
        {assertion}
    }}
}}
"
            ),
        )?;
        Ok(Self { root })
    }

    /// (grip class, oracle kind of `picks_a`, its relation reason) on the
    /// `Ok(Mode::A)` return seam of `pick`.
    fn ok_seam(&self) -> Result<(String, String, String), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args([
                "--mode",
                "ready",
                "--quiet",
                "--format",
                "repo-exposure-json",
            ])
            .env("RIPR_CACHE_DIR", self.root.join("cache"))
            .output()
            .map_err(|error| format!("run ripr failed: {error}"))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(format!(
                "instrument: ripr exited {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let document: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("instrument: stdout is not JSON: {error}"))?;
        let seams = document
            .pointer("/seams")
            .and_then(Value::as_array)
            .ok_or("instrument: no seams array")?;
        let seam = seams
            .iter()
            .find(|seam| {
                seam.pointer("/owner").and_then(Value::as_str) == Some("src/lib.rs::pick")
                    && seam.pointer("/expression").and_then(Value::as_str) == Some("Ok(Mode::A)")
            })
            .ok_or_else(|| format!("premise: no `Ok(Mode::A)` seam in {seams:?}"))?;
        let test = seam
            .pointer("/related_tests")
            .and_then(Value::as_array)
            .and_then(|tests| {
                tests
                    .iter()
                    .find(|test| test.pointer("/name").and_then(Value::as_str) == Some("picks_a"))
            })
            .ok_or_else(|| format!("premise: test `picks_a` is not related to {seam}"))?;
        let text = |value: &Value, pointer: &str| {
            value
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("instrument: {pointer} missing in {value}"))
        };
        Ok((
            text(seam, "/grip_class")?,
            text(test, "/oracle_kind")?,
            text(test, "/relation_reason")?,
        ))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn an_owner_call_only_an_untrusted_macro_holds_gives_no_exact_credit() -> Result<(), String> {
    let vec_shadow = "macro_rules! vec { ($($t:tt)*) => { 0 }; }";
    // (label, test-local macros, assertion, expected grip class, oracle kind)
    let other_method = format!(
        "{CHECK_OK}\n    struct Other;\n    impl Other {{ fn pick(&self, _: u32) -> Result<Mode, ()> {{ Ok(Mode::A) }} }}"
    );
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        (
            "a local macro holds the only owner call",
            CHECK_OK,
            "assert_eq!(check_ok!(pick(0)), Ok(Mode::A));",
            "propagation_unknown",
            "unknown",
        ),
        (
            "a local macro shadowing vec! holds the owner call",
            vec_shadow,
            "assert_eq!(vec![pick(0)], vec![Ok(Mode::A)]);",
            "propagation_unknown",
            "unknown",
        ),
        (
            "a shadowing vec! declared with trivia before the bang",
            "macro_rules /* shadow */ ! vec { ($($t:tt)*) => { 0 }; }",
            "assert_eq!(vec![pick(0)], vec![Ok(Mode::A)]);",
            "propagation_unknown",
            "unknown",
        ),
        (
            "a comment that only mentions macro_rules! vec",
            "// macro_rules! vec is not needed here",
            "assert_eq!(vec![pick(0)], vec![Ok(Mode::A)]);",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "a method of the same name on another receiver is not the owner",
            other_method.as_str(),
            "assert_eq!(check_ok!(pick(0)), Other.pick(0));",
            "propagation_unknown",
            "unknown",
        ),
        (
            "plain owner call",
            "",
            "assert_eq!(pick(0), Ok(Mode::A));",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "the standard vec! evaluates the owner call",
            "",
            "assert_eq!(vec![pick(0)], vec![Ok(Mode::A)]);",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "the standard format! evaluates the owner call",
            "",
            "assert_eq!(format!(\"{:?}\", pick(0)), \"Ok(A)\");",
            "strongly_gripped",
            "whole_object_equality",
        ),
        (
            "an owner call outside the local macro still runs",
            CHECK_OK,
            "assert_eq!(check_ok!(pick(0)), pick(0));",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "the owner passed as a function value outside the local macro",
            CHECK_OK,
            "assert_eq!(check_ok!(pick(0)), [0u32].map(pick)[0]);",
            "strongly_gripped",
            "exact_value",
        ),
    ];
    for (label, macros, assertion, grip, oracle) in cases {
        let workspace = Workspace::create(macros, assertion)?;
        let (observed_grip, observed_oracle, relation) = workspace.ok_seam()?;
        if relation != "direct_owner_call" {
            return Err(format!(
                "{label}: premise: `picks_a` calls pick directly, got {relation}"
            ));
        }
        if (observed_grip.as_str(), observed_oracle.as_str()) != (*grip, *oracle) {
            return Err(format!(
                "{label}: expected ({grip}, {oracle}), got ({observed_grip}, {observed_oracle})"
            ));
        }
    }
    Ok(())
}

#[test]
fn an_owner_result_local_only_an_untrusted_macro_uses_gives_no_exact_credit() -> Result<(), String>
{
    let field_macros = format!("{CHECK_OK}\n    struct Holder {{ r: u32 }}");
    // (label, test-local macros, test body, expected grip class, oracle kind)
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        (
            "the owner result is bound, then only the local macro uses it",
            CHECK_OK,
            "let r = pick(0);\n        assert_eq!(check_ok!(r), Ok(Mode::A));",
            "propagation_unknown",
            "unknown",
        ),
        (
            "the bound result is asserted plainly",
            CHECK_OK,
            "let r = pick(0);\n        assert_eq!(r, Ok(Mode::A));",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "the bound result is also asserted outside the local macro",
            CHECK_OK,
            "let r = pick(0);\n        assert_eq!(check_ok!(r), Ok(Mode::A));\n        assert_eq!(r, Ok(Mode::A));",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "a field of another value that shares the local's name is not a use",
            &field_macros,
            "let r = pick(0);\n        let holder = Holder { r: 1 };\n        let _ = holder.r;\n        assert_eq!(check_ok!(r), Ok(Mode::A));",
            "propagation_unknown",
            "unknown",
        ),
        (
            "a local borrowed elsewhere does not undo a held owner call",
            CHECK_OK,
            "let r = pick(0);\n        let _ = &r;\n        assert_eq!(check_ok!((pick(0), r)), Ok(Mode::A));",
            "propagation_unknown",
            "unknown",
        ),
        (
            "a format capture of the local is read outside the local macro",
            CHECK_OK,
            "let r = pick(0);\n        assert_eq!(format!(\"{r:?}{:?}\", check_ok!(r)), \"Ok(A)Ok(A)\");",
            "strongly_gripped",
            "whole_object_equality",
        ),
        (
            "a typed local read again through a clone is observed",
            CHECK_OK,
            "let r: Result<Mode, ()> = pick(0);\n        let s = r.clone();\n        assert_eq!(check_ok!(r), s);",
            "strongly_gripped",
            "exact_value",
        ),
        (
            "the standard vec! evaluates the bound result",
            "",
            "let r = pick(0);\n        assert_eq!(vec![r], vec![Ok(Mode::A)]);",
            "strongly_gripped",
            "exact_value",
        ),
    ];
    for (label, macros, body, grip, oracle) in cases {
        let workspace = Workspace::create(macros, body)?;
        let (observed_grip, observed_oracle, relation) = workspace.ok_seam()?;
        if relation != "direct_owner_call" {
            return Err(format!(
                "{label}: premise: `picks_a` calls pick directly, got {relation}"
            ));
        }
        if (observed_grip.as_str(), observed_oracle.as_str()) != (*grip, *oracle) {
            return Err(format!(
                "{label}: expected ({grip}, {oracle}), got ({observed_grip}, {observed_oracle})"
            ));
        }
    }
    Ok(())
}
