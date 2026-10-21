//! End-to-end witness that a returned bare comparison is read at its boundary:
//! a `return_value` finding for `x > 100` changed to `x > 101` must not read
//! `exposed` when no related test input separates the two thresholds, the same
//! gap the `predicate` finding on that line already names.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Findings on line 2 of a one-function crate whose `src/lib.rs` line 2
/// changed from `old` to `new`, checked against the integration `test`.
fn findings(new: &str, old: &str, test: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-returned-comparison-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"thr\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(
        "src/lib.rs",
        &format!("pub fn thr(x: i32) -> bool {{\n{new}\n}}\n"),
    )?;
    write("tests/it.rs", test)?;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn thr(x: i32) -> bool {{\n-{old}\n+{new}\n }}\n"
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect())
}

fn family<'a>(findings: &'a [Finding], name: &str) -> Result<&'a Finding, String> {
    findings
        .iter()
        .find(|finding| finding.probe.family.as_str() == name)
        .ok_or_else(|| format!("no {name} finding in {findings:?}"))
}

const NEW: &str = "    x > 101";
const OLD: &str = "    x > 100";

#[test]
fn inputs_far_from_the_threshold_do_not_expose_the_returned_comparison() -> Result<(), String> {
    let findings = findings(
        NEW,
        OLD,
        "use thr::thr;\n\n#[test]\nfn low() {\n    assert!(!thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr(500));\n}\n",
    )?;
    let predicate = family(&findings, "predicate")?;
    assert_eq!(
        predicate.class,
        ExposureClass::WeaklyExposed,
        "{predicate:?}"
    );
    let return_value = family(&findings, "return_value")?;
    assert_ne!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    assert!(
        return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_input_at_the_changed_boundary_keeps_the_returned_comparison_exposed() -> Result<(), String> {
    // Control: `thr(101)` is false changed and true before, so the boundary
    // input separates the versions.
    let findings = findings(
        NEW,
        OLD,
        "use thr::thr;\n\n#[test]\nfn at() {\n    assert!(!thr(101));\n}\n\n#[test]\nfn above() {\n    assert!(thr(102));\n}\n",
    )?;
    let return_value = family(&findings, "return_value")?;
    assert_eq!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn a_returned_compound_predicate_keeps_the_return_value_reading() -> Result<(), String> {
    // Control: only a bare comparison is read at its boundary; a compound
    // expression keeps the reading it had.
    let findings = findings(
        "    x > 101 && x < 900",
        "    x > 100 && x < 900",
        "use thr::thr;\n\n#[test]\nfn low() {\n    assert!(!thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr(500));\n}\n",
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        !return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_operator_change_keeps_the_return_value_reading() -> Result<(), String> {
    // Control: `x == 100` to `x > 101` is not a threshold move. `thr(100)` is
    // true before and false after, so the off-boundary input separates the
    // versions and the `x == 101` boundary gap must not be claimed.
    let findings = findings(
        "    x > 101",
        "    x == 100",
        "use thr::thr;\n\n#[test]\nfn at_old() {\n    assert!(!thr(100));\n}\n",
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        !return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}

const FAR_INPUTS: &str = "use thr::thr;\n\n#[test]\nfn low() {\n    assert!(!thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr(500));\n}\n";

#[test]
fn an_explicit_return_is_read_at_its_boundary_like_the_tail_form() -> Result<(), String> {
    // #9116: `return x > 100;` to `return x > 101;` is the same threshold move
    // as the implicit tail, so far inputs must not expose it.
    let findings = findings("    return x > 101;", "    return x > 100;", FAR_INPUTS)?;
    // Fixture construction: the changed line is a parser-backed return_value
    // probe that still carries the explicit `return`.
    let return_value = family(&findings, "return_value")?;
    assert!(
        return_value.probe.expression.contains("return"),
        "{return_value:?}"
    );
    assert_ne!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    assert!(
        return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_explicit_return_with_a_boundary_input_stays_exposed() -> Result<(), String> {
    // Positive control: `thr(101)` separates the versions.
    let findings = findings(
        "    return x > 101;",
        "    return x > 100;",
        "use thr::thr;\n\n#[test]\nfn at() {\n    assert!(!thr(101));\n}\n\n#[test]\nfn above() {\n    assert!(thr(102));\n}\n",
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        return_value.probe.expression.contains("return"),
        "{return_value:?}"
    );
    assert_eq!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_explicit_return_operator_change_keeps_the_return_value_reading() -> Result<(), String> {
    // Control: `== 100` to `> 101` is not a threshold move.
    let findings = findings(
        "    return x > 101;",
        "    return x == 100;",
        "use thr::thr;\n\n#[test]\nfn at_old() {\n    assert!(!thr(100));\n}\n",
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        !return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_explicit_return_of_a_compound_value_keeps_the_return_value_reading() -> Result<(), String> {
    // Control: the explicit-return strip does not broaden to `&&`/`||`.
    let findings = findings(
        "    return x > 101 && x < 900;",
        "    return x > 100 && x < 900;",
        FAR_INPUTS,
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        !return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        "{return_value:?}"
    );
    Ok(())
}
