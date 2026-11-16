//! #13037 follow-up: the repo-wide seam view (`--format repo-exposure-json`,
//! the evidence `ripr repair` reads) splits `order_cents >= FREE_OVER_CENTS
//! && !express` into a comparison seam and drops the guard. It credited that
//! seam `strongly_gripped` from calls with `express = true`, which cannot
//! tell `>` from `>=`, while `ripr check` read the same change as unclear,
//! so the check screen printed a conflicting-evidence advisory. The seam's
//! boundary now needs a call that sits on the boundary value while holding
//! every guard, as in `check`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, test_body: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-guarded-repo-seam-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write = |path: &Path, text: &str| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
        };
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"guarded_repo_seam\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write(&root.join("src/lib.rs"), source)?;
        write(
            &root.join("tests/shipping.rs"),
            &format!(
                "use guarded_repo_seam::shipping_cost;\n\n#[test]\nfn pins() {{\n    {test_body}\n}}\n"
            ),
        )?;
        Ok(Self { root })
    }

    fn comparison_seam(&self) -> Result<Value, String> {
        self.comparison_seam_for("order_cents >= FREE_OVER_CENTS")
    }

    fn comparison_seam_for(&self, expression: &str) -> Result<Value, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--mode", "ready", "--format", "repo-exposure-json"])
            .output()
            .map_err(|error| format!("run ripr: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("parse repo exposure: {error}"))?;
        let mut seams = report
            .get("seams")
            .and_then(Value::as_array)
            .ok_or("report has no seams")?
            .iter()
            .filter(|seam| {
                seam.get("kind").and_then(Value::as_str) == Some("predicate_boundary")
                    && seam.get("expression").and_then(Value::as_str) == Some(expression)
            });
        match (seams.next(), seams.next()) {
            (Some(seam), None) => Ok(seam.clone()),
            other => Err(format!(
                "premise: exactly one comparison seam, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn field<'a>(seam: &'a Value, path: &str) -> Option<&'a str> {
    seam.pointer(path).and_then(Value::as_str)
}

fn missing_values(seam: &Value) -> Vec<String> {
    seam.get("missing_discriminators")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("value").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn boundary_calls_that_miss_the_guard_do_not_grip_the_comparison() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    assert_eq!(
        field(&seam, "/grip_class"),
        Some("activation_unknown"),
        "{seam}"
    );
    assert!(missing_values(&seam).is_empty(), "never actionable: {seam}");
    let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(
        summary.contains("`express = false`"),
        "the guard is named: {summary}"
    );
    Ok(())
}

#[test]
fn a_guard_holding_boundary_call_grips_the_comparison() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    assert_eq!(
        field(&seam, "/grip_class"),
        Some("strongly_gripped"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn a_mut_guard_is_not_read_from_the_test_argument() -> Result<(), String> {
    let source = SOURCE.replace("express: bool", "mut express: bool");
    let seam = TempRepo::create(
        &source,
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn guard_and_boundary_must_hold_on_the_same_call() -> Result<(), String> {
    // One call sits on the boundary with the guard missing, another holds
    // the guard off the boundary: neither alone reaches the boundary.
    let seam = TempRepo::create(
        SOURCE,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn a_call_naming_the_boundary_constant_holds_the_boundary() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "assert_eq!(shipping_cost(100, guarded_repo_seam::FREE_OVER_CENTS, false), 0);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}

#[test]
fn an_immutable_local_on_the_boundary_holds_the_boundary() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let cents = 5000;\n    assert_eq!(shipping_cost(100, cents, false), 0);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}

#[test]
fn an_immutable_local_off_the_boundary_does_not_hold_it() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let cents = 7000;\n    assert_eq!(shipping_cost(100, cents, false), 0);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn an_immutable_local_guard_holds_the_guard() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let express = false;\n    assert_eq!(shipping_cost(100, 5000, express), 0);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}

const REVERSED: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if FREE_OVER_CENTS <= order_cents && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

fn reversed_seam(test_body: &str) -> Result<Value, String> {
    TempRepo::create(REVERSED, test_body)?.comparison_seam_for("FREE_OVER_CENTS <= order_cents")
}

#[test]
fn a_constant_on_the_left_is_held_by_a_guard_holding_boundary_call() -> Result<(), String> {
    // The guard gate orients a left-hand constant as `check` does, so a
    // guard-holding boundary call never reads as missing the guard. (The
    // seam view does not yet resolve a left-hand constant at all, an
    // existing fail-closed limitation tracked separately.)
    let seam = reversed_seam(
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?;
    let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(
        !summary.contains("`express = false`"),
        "the guard is held: {summary}"
    );
    Ok(())
}

#[test]
fn a_constant_on_the_left_still_needs_the_guard() -> Result<(), String> {
    let seam = reversed_seam(
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
    )?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(
        summary.contains("`express = false`"),
        "the guard is named: {summary}"
    );
    Ok(())
}

const WRAPPED: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS
        && !express
    {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

#[test]
fn a_wrapped_guard_is_read_from_the_line_below() -> Result<(), String> {
    // rustfmt puts a long condition's guard on its own line; `check` reads
    // it there, so the seam view must too.
    let seam = TempRepo::create(
        WRAPPED,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(
        summary.contains("`express = false`"),
        "the wrapped guard is named: {summary}"
    );

    let seam = TempRepo::create(
        WRAPPED,
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}
