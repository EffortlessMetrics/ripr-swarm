//! A workspace assertion macro defined in another file names a limit instead
//! of an unnamed gap (#12886).
//!
//! `check_eq!(Widget::new().read(), 4)` forwards its operands to a std
//! assertion, but the macro lives in `macros.rs`, so the call never reached
//! the extractor as an assertion and the finding read as a gap with no oracle.
//! The finding now names `rust_macro_wrapped_assertion_unresolved` and credits
//! nothing. Each control changes one thing about the shape and keeps the gap.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const LIMIT: &str = "rust_macro_wrapped_assertion_unresolved";
const FORWARDS: &str = "($a:expr, $b:expr) => {{ std::assert_eq!($a, $b); }};";

/// How the test file reaches the macro.
#[derive(Clone, Copy)]
enum Route {
    /// `#[macro_use] mod macros;` in the crate root; test module inline.
    MacroUse,
    /// `#[macro_export]` plus a top-level `use crate::check_eq;` in an
    /// out-of-line `tests.rs`.
    ExportAndUse,
    /// Nothing brings the macro into scope.
    Unreached,
}

struct Case<'a> {
    route: Route,
    /// The macro's arm, matcher and transcriber.
    arm: &'a str,
    /// The test statement.
    statement: &'a str,
    /// Extra items in the test file, before the test.
    prelude: &'a str,
}

impl Default for Case<'_> {
    fn default() -> Self {
        Self {
            route: Route::MacroUse,
            arm: FORWARDS,
            statement: "check_eq!(Widget::new().read(), 4);",
            prelude: "",
        }
    }
}

struct Workspace {
    root: PathBuf,
    diff: PathBuf,
}

fn production(read_body: &str) -> String {
    format!(
        "pub struct Widget;
pub struct Other;
pub fn gauge() -> u32 {{ 4 }}
impl Widget {{
    pub fn new() -> Self {{ Widget }}
    pub fn read(&self) -> u32 {{ {read_body} }}
}}
impl Other {{
    pub fn new() -> Self {{ Other }}
    pub fn read(&self) -> u32 {{ 4 }}
}}
"
    )
}

fn whole_file_diff(path: &str, before: &str, after: &str) -> String {
    let before: Vec<&str> = before.lines().collect();
    let after: Vec<&str> = after.lines().collect();
    let mut body = String::new();
    for (old, new) in before.iter().zip(&after) {
        if old == new {
            body.push_str(&format!(" {old}\n"));
        } else {
            body.push_str(&format!("-{old}\n+{new}\n"));
        }
    }
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{n} +1,{n} @@\n{body}",
        n = before.len()
    )
}

impl Workspace {
    fn create(case: &Case<'_>) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-cross-file-macro-{}-{}",
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
            "[workspace]\nmembers = [\"a\"]\nresolver = \"2\"\n",
        )?;
        write(
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\ndoctest = false\n",
        )?;
        let export = match case.route {
            Route::ExportAndUse => "#[macro_export]\n",
            _ => "",
        };
        let macros = format!("{export}macro_rules! check_eq {{\n    {}\n}}\n", case.arm);
        let test_body = format!(
            "{}\n#[test]\nfn reads() {{\n    {}\n}}\n",
            case.prelude, case.statement
        );
        let (head, tests) = match case.route {
            Route::MacroUse => (
                "#[macro_use]\nmod macros;\n",
                format!("#[cfg(test)]\nmod tests {{\n    use super::*;\n{test_body}}}\n"),
            ),
            Route::ExportAndUse => ("mod macros;\n#[cfg(test)]\nmod tests;\n", String::new()),
            Route::Unreached => (
                "mod macros;\n",
                format!("#[cfg(test)]\nmod tests {{\n    use super::*;\n{test_body}}}\n"),
            ),
        };
        write("a/src/macros.rs", &macros)?;
        if matches!(case.route, Route::ExportAndUse) {
            write(
                "a/src/tests.rs",
                &format!("use super::*;\nuse crate::check_eq;\n{test_body}"),
            )?;
        }
        let before = format!("{head}{}{tests}", production("4"));
        let after = format!("{head}{}{tests}", production("5"));
        write("a/src/lib.rs", &after)?;
        let diff = root.join("change.diff");
        write(
            "change.diff",
            &whole_file_diff("a/src/lib.rs", &before, &after),
        )?;
        Ok(Self { root, diff })
    }

    fn finding(&self) -> Result<Value, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--mode", "ready", "--quiet", "--format", "json"])
            .env("RIPR_CACHE_DIR", self.root.join("cache"))
            .arg("--diff")
            .arg(&self.diff)
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
        let findings = document
            .get("findings")
            .and_then(Value::as_array)
            .ok_or_else(|| "instrument: no findings array".to_string())?;
        let mut on_owner = findings.iter().filter(|finding| {
            finding
                .pointer("/probe/expression")
                .and_then(Value::as_str)
                .is_some_and(|expression| expression == "5")
        });
        match (on_owner.next(), on_owner.next()) {
            (Some(finding), None) => Ok(finding.clone()),
            other => Err(format!(
                "premise: one finding on the mutated return, got {other:?}"
            )),
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn judge(case: &Case<'_>) -> Result<(String, Option<String>), String> {
    let finding = Workspace::create(case)?.finding()?;
    let class = finding
        .get("classification")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing classification".to_string())?
        .to_string();
    let limit = finding
        .get("static_limit_kind")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((class, limit))
}

fn assert_gap(case: &Case<'_>) -> Result<(), String> {
    let (class, limit) = judge(case)?;
    if class == "reachable_unrevealed" && limit.is_none() {
        Ok(())
    } else {
        Err(format!("expected the unnamed gap, got {class} {limit:?}"))
    }
}

#[test]
fn a_macro_use_wrapper_that_forwards_its_operands_names_the_limit() -> Result<(), String> {
    let (class, limit) = judge(&Case::default())?;
    assert_eq!(class, "static_unknown", "never exposed, never a gap");
    assert_eq!(limit.as_deref(), Some(LIMIT));
    Ok(())
}

#[test]
fn an_exported_wrapper_named_by_use_names_the_limit() -> Result<(), String> {
    let (class, limit) = judge(&Case {
        route: Route::ExportAndUse,
        ..Case::default()
    })?;
    assert_eq!(class, "static_unknown");
    assert_eq!(limit.as_deref(), Some(LIMIT));
    Ok(())
}

#[test]
fn a_wrapper_that_ignores_its_operands_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        arm: "($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }};",
        ..Case::default()
    })
}

#[test]
fn a_wrapper_that_forwards_to_a_weak_assertion_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        arm: "($a:expr, $b:expr) => {{ std::assert!($a > 0 || $b > 0); }};",
        ..Case::default()
    })
}

#[test]
fn a_wrong_receiver_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        statement: "check_eq!(Other::new().read(), 4);",
        ..Case::default()
    })
}

#[test]
fn a_same_named_free_function_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        prelude: "fn read() -> u32 { 4 }",
        statement: "check_eq!(read(), 4);",
        ..Case::default()
    })
}

#[test]
fn a_local_rebinding_of_the_receiver_type_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        prelude: "struct Widget; impl Widget { fn new() -> Self { Widget } fn read(&self) -> u32 { 4 } }",
        ..Case::default()
    })
}

#[test]
fn a_wrapper_no_import_reaches_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        route: Route::Unreached,
        ..Case::default()
    })
}

#[test]
fn a_test_local_macro_of_the_same_name_keeps_the_gap() -> Result<(), String> {
    assert_gap(&Case {
        statement: "macro_rules! check_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; }\n    check_eq!(Widget::new().read(), 4);",
        ..Case::default()
    })
}
