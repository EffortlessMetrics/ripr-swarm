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
    let seam = reversed_seam(
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?;
    assert_eq!(
        field(&seam, "/grip_class"),
        Some("strongly_gripped"),
        "{seam}"
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

#[test]
fn a_reassigned_mut_local_is_not_read_from_its_initializer() -> Result<(), String> {
    // The call runs with `express = true`: the initializer must not hold
    // the guard.
    let seam = TempRepo::create(
        SOURCE,
        "let mut express = false;\n    express = true;\n    assert_eq!(shipping_cost(100, 5000, express), 200);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );

    // The call runs with 5001: the initializer must not sit on the boundary.
    let seam = TempRepo::create(
        SOURCE,
        "let mut cents = 5000;\n    cents += 1;\n    assert_eq!(shipping_cost(100, cents, false), 0);",
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
fn an_unasserted_guard_holding_call_does_not_activate_a_separately_asserted_one()
-> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let _ = shipping_cost(100, 5000, false);\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
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
fn a_bound_result_of_the_guard_holding_call_is_observed_through_its_binding() -> Result<(), String>
{
    let seam = TempRepo::create(
        SOURCE,
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!(free, 0);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");

    let seam = TempRepo::create(
        SOURCE,
        "let mut free = shipping_cost(100, 5000, false);\n    free = 0;\n    assert_eq!(free, 0);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "a reassigned binding observes nothing: {seam}"
    );
    Ok(())
}

const TWO_GUARDS_WRAPPED: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, member: bool, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && member
        && !express
    {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

#[test]
fn a_second_guard_wrapped_under_a_guarded_head_is_read() -> Result<(), String> {
    let seam = TempRepo::create(
        TWO_GUARDS_WRAPPED,
        "assert_eq!(shipping_cost(100, 5000, true, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true, true), 200);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(
        summary.contains("`express = false`") && summary.contains("`member = true`"),
        "both guards are named: {summary}"
    );

    let seam = TempRepo::create(
        TWO_GUARDS_WRAPPED,
        "assert_eq!(shipping_cost(100, 5000, true, false), 0);\n    assert_eq!(shipping_cost(100, 4999, true, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}

#[test]
fn a_sibling_statements_binding_is_not_the_guard_holding_calls_result() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let got = shipping_cost(100, 5000, true); let _ = shipping_cost(100, 5000, false);\n    assert_eq!(got, 200);",
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
fn a_binding_named_only_in_an_assertion_message_is_not_observed() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!(shipping_cost(100, 5000, true), 200, \"{free}\");",
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
fn a_shadowed_binding_or_local_is_never_read_from_its_first_value() -> Result<(), String> {
    for body in [
        // The asserted `r` is the second, guard-missing call.
        "let r = shipping_cost(100, 5000, false);\n    let r = shipping_cost(100, 4000, true);\n    assert_eq!(r, 200);",
        // The guard shadowed to `true` before the call.
        "let express = false;\n    let express = !express;\n    assert_eq!(shipping_cost(100, 5000, express), 200);",
        // The boundary shadowed off 5000 before the call.
        "let cents = 5000;\n    let cents = cents + 1;\n    assert_eq!(shipping_cost(100, cents, false), 0);",
        // A loop variable shadows the guard.
        "let express = false;\n    for express in [true] {\n        assert_eq!(shipping_cost(100, 5000, express), 200);\n    }",
    ] {
        let seam = TempRepo::create(SOURCE, body)?.comparison_seam()?;
        assert_eq!(
            field(&seam, "/evidence/activate"),
            Some("unknown"),
            "{body}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn a_commented_wrapped_guard_is_still_read() -> Result<(), String> {
    let source = WRAPPED.replace("&& !express\n", "&& !express // express never ships free\n");
    assert!(source.contains("// express never ships free"));
    let seam = TempRepo::create(
        &source,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
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
fn a_binding_named_only_in_an_operand_comment_is_not_observed() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!(shipping_cost(100, 5000, true), 200 /* free */);",
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
fn a_stable_guard_local_read_on_a_result_binding_line_holds() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let express = false;\n    let free = shipping_cost(100, 5000, express);\n    assert_eq!(free, 0);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&seam, "/evidence/activate"), Some("yes"), "{seam}");
    Ok(())
}

#[test]
fn a_parameter_shadowing_the_binding_or_a_buried_call_is_not_observed() -> Result<(), String> {
    for body in [
        "let r = shipping_cost(100, 5000, false);\n    fn check(r: u32) { assert_eq!(r, 0); }\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
        "let r = shipping_cost(100, 5000, false);\n    [(1u32, 0u32)].iter().for_each(|(_, r)| assert_eq!(*r, 0));\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
        "assert_eq!(shipping_cost(100, 5000, false) * 0, 0);\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
        "assert_eq!(std::cmp::min(shipping_cost(100, 5000, false), 0), 0);\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
    ] {
        let seam = TempRepo::create(SOURCE, body)?.comparison_seam()?;
        assert_eq!(
            field(&seam, "/evidence/activate"),
            Some("unknown"),
            "{body}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn a_bound_result_inside_a_larger_operand_is_not_observed() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!(free * 0, 0);\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
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
fn a_parenthesised_whole_operand_is_still_observed() -> Result<(), String> {
    for test in [
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!((free), 0);",
        "assert_eq!((shipping_cost(100, 5000, false)), 0);",
    ] {
        let seam = TempRepo::create(SOURCE, test)?.comparison_seam()?;
        assert_eq!(
            field(&seam, "/evidence/activate"),
            Some("yes"),
            "{test}: {seam}"
        );
    }
    let seam = TempRepo::create(
        SOURCE,
        "let free = shipping_cost(100, 5000, false);\n    assert_eq!((free) * (0), 0);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "parentheses around parts are not a whole operand: {seam}"
    );
    Ok(())
}

#[test]
fn a_binding_shadowed_by_a_wrapped_let_pattern_is_not_observed() -> Result<(), String> {
    let seam = TempRepo::create(
        SOURCE,
        "let r = shipping_cost(100, 5000, false);\n    let (\n        r,\n        _x,\n    ) = (shipping_cost(100, 5000, true), 1);\n    assert_eq!(r, 200);",
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
fn a_call_to_a_same_named_function_in_another_module_is_not_the_owner() -> Result<(), String> {
    let repo = TempRepo::create(
        SOURCE,
        "assert_eq!(rival::shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 5000, true), 200);",
    )?;
    std::fs::write(
        repo.root.join("tests/shipping.rs"),
        "use guarded_repo_seam::shipping_cost;\n\nmod rival {\n    pub fn shipping_cost(_w: u32, _c: u32, _e: bool) -> u32 { 0 }\n}\n\n#[test]\nfn pins() {\n    assert_eq!(rival::shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 5000, true), 200);\n}\n",
    )
    .map_err(|error| format!("write rival test: {error}"))?;
    let seam = repo.comparison_seam()?;
    assert_eq!(
        field(&seam, "/evidence/activate"),
        Some("unknown"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn a_rebound_compared_parameter_or_an_earlier_decision_is_not_reached() -> Result<(), String> {
    // Each row's expected value is what the source really returns, so no
    // contradicted assertion can be what refuses the credit.
    for (source, below) in [
        (
            // `order_cents + 1` puts 4999 on the boundary too: both rows return 0.
            SOURCE.replace(
                "    if order_cents >= FREE_OVER_CENTS && !express {",
                "    let order_cents = order_cents + 1;\n    if order_cents >= FREE_OVER_CENTS && !express {",
            ),
            0,
        ),
        (
            SOURCE.replace(
                "    if order_cents >= FREE_OVER_CENTS && !express {",
                "    if order_cents == FREE_OVER_CENTS {\n        return 0;\n    }\n    if order_cents >= FREE_OVER_CENTS && !express {",
            ),
            100,
        ),
    ] {
        let seam = TempRepo::create(
            &source,
            &format!(
                "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), {below});"
            ),
        )?
        .comparison_seam()?;
        assert_eq!(
            field(&seam, "/evidence/activate"),
            Some("unknown"),
            "{source}: {seam}"
        );
        assert!(missing_values(&seam).is_empty(), "never actionable: {seam}");
        let summary = field(&seam, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
        assert!(
            summary.contains("`express = false`"),
            "the guarded reading refuses the rows and names the guard: {summary}"
        );
    }
    Ok(())
}

#[test]
fn a_while_head_reads_its_guard() -> Result<(), String> {
    let source = SOURCE.replace(
        "    if order_cents >= FREE_OVER_CENTS && !express {",
        "    while order_cents >= FREE_OVER_CENTS && !express {",
    );
    let missing = TempRepo::create(
        &source,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
    )?
    .comparison_seam()?;
    assert_eq!(
        field(&missing, "/evidence/activate"),
        Some("unknown"),
        "{missing}"
    );
    let summary = field(&missing, "/evidence_record/evidence_path/activate/summary").unwrap_or("");
    assert!(summary.contains("`express = false`"), "{summary}");
    let held = TempRepo::create(
        &source,
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 100);",
    )?
    .comparison_seam()?;
    assert_eq!(field(&held, "/evidence/activate"), Some("yes"), "{held}");
    Ok(())
}

#[test]
fn a_head_comment_does_not_swallow_the_wrapped_guard() -> Result<(), String> {
    let source = WRAPPED.replace(
        "    if order_cents >= FREE_OVER_CENTS\n",
        "    if order_cents >= FREE_OVER_CENTS // threshold\n",
    );
    assert!(source.contains("// threshold"));
    let seam = TempRepo::create(
        &source,
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n    assert_eq!(shipping_cost(100, 4999, true), 200);",
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
fn statement_predicates_require_a_boundary_row_that_holds_every_guard() -> Result<(), String> {
    use ripr::{CheckInput, Mode, OutputFormat, ProbeFamily, check_workspace};

    for (predicate, supported) in [
        (
            "let free = order_cents >= FREE_OVER_CENTS && !express;\n    free",
            true,
        ),
        ("return order_cents >= FREE_OVER_CENTS && !express;", true),
        (
            "let free = order_cents >= FREE_OVER_CENTS\n        && !express;\n    free",
            true,
        ),
        (
            "return order_cents >= FREE_OVER_CENTS\n        && !express;",
            true,
        ),
        (
            "if order_cents >= FREE_OVER_CENTS && !express && ready() { true } else { false }",
            false,
        ),
        (
            "if order_cents >= FREE_OVER_CENTS && !express\n        && ready()\n    { true } else { false }",
            false,
        ),
    ] {
        let source = format!(
            "pub const FREE_OVER_CENTS: u32 = 5000;\nfn ready() -> bool {{ true }}\npub fn shipping_cost(order_cents: u32, express: bool) -> bool {{\n    {predicate}\n}}\n"
        );
        let head = predicate.lines().next().ok_or("empty predicate fixture")?;
        let old = head.replace(">=", ">");
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4 +4 @@\n-    {old}\n+    {head}\n"
        );
        for express in [true, false] {
            let observed = !express;
            let repo = TempRepo::create(
                &source,
                &format!(
                    "assert_eq!(shipping_cost(5000, {express}), {observed});\n    assert_eq!(shipping_cost(4999, {express}), false);"
                ),
            )?;
            let seam = repo.comparison_seam()?;
            let reached = supported && observed;
            let expected = if reached {
                "strongly_gripped"
            } else {
                "activation_unknown"
            };
            assert_eq!(
                field(&seam, "/grip_class"),
                Some(expected),
                "{predicate}, express={express}: {seam}"
            );
            let diff = repo.root.join("diff.patch");
            std::fs::write(&diff, &patch).map_err(|error| error.to_string())?;
            let report = check_workspace(CheckInput {
                root: repo.root.clone(),
                diff_file: Some(diff),
                mode: Mode::Ready,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let findings: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| {
                    finding.probe.family == ProbeFamily::Predicate
                        && finding.probe.location.line == 4
                })
                .collect();
            let [finding] = findings.as_slice() else {
                return Err(format!(
                    "expected one changed predicate for {predicate}: {findings:?}"
                ));
            };
            assert_eq!(
                finding.ripr.infect.state.as_str(),
                if reached { "yes" } else { "unknown" },
                "{predicate}, express={express}: {finding:?}"
            );
        }
    }
    Ok(())
}
