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
    findings_after("", new, old, test)
}

/// [`findings`] with `prefix` (whole lines) ahead of the changed line, which
/// then sits on line `2 + prefix lines`.
fn findings_after(prefix: &str, new: &str, old: &str, test: &str) -> Result<Vec<Finding>, String> {
    findings_wrapped(prefix, "", new, old, test)
}

/// [`findings_after`] with `suffix` (whole lines) after the changed line, for
/// a comparison nested in a block opened by `prefix`.
fn findings_wrapped(
    prefix: &str,
    suffix: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    findings_with_parameter("x: i32", prefix, suffix, new, old, test)
}

/// [`findings_wrapped`] with the owner's parameter declared as `parameter`
/// (`mut x: i32` for an in-place mutation).
fn findings_with_parameter(
    parameter: &str,
    prefix: &str,
    suffix: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    let changed_line = 2 + prefix.lines().count();
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
        &format!("pub fn thr({parameter}) -> bool {{\n{prefix}{new}\n{suffix}}}\n"),
    )?;
    write("tests/it.rs", test)?;
    // Fixture construction: the changed line sits where the diff says it does,
    // so an arm built from `prefix`/`suffix` is the subject being analyzed.
    let source = std::fs::read_to_string(root.join("src/lib.rs"))
        .map_err(|error| format!("read src/lib.rs: {error}"))?;
    let lines: Vec<&str> = source.lines().collect();
    if lines.get(changed_line - 1).copied() != Some(new) {
        return Err(format!(
            "changed line {changed_line} is not {new:?}: {lines:?}"
        ));
    }
    if lines.len() != changed_line + suffix.lines().count() + 1 {
        return Err(format!(
            "unexpected source length for hunk counts: {lines:?}"
        ));
    }
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n pub fn thr({parameter}) -> bool {{\n{ctx}-{old}\n+{new}\n{post} }}\n",
        n = changed_line + 1 + suffix.lines().count(),
        ctx = prefix
            .lines()
            .map(|line| format!(" {line}\n"))
            .collect::<String>(),
        post = suffix
            .lines()
            .map(|line| format!(" {line}\n"))
            .collect::<String>(),
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
        .filter(|finding| finding.probe.location.line == changed_line)
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
fn an_explicit_return_followed_by_a_tab_is_read_at_its_boundary() -> Result<(), String> {
    // Any whitespace after the `return` keyword, not only a space.
    let findings = findings("    return\tx > 101;", "    return\tx > 100;", FAR_INPUTS)?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        return_value.probe.expression.contains("return\t"),
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

const INTERMEDIATE_INPUT: &str = "use thr::thr;\n\n#[test]\nfn between() {\n    assert!(!thr(101));\n}\n\n#[test]\nfn above() {\n    assert!(thr(500));\n}\n";

fn assert_gap_named(findings: &[Finding], named: bool) -> Result<(), String> {
    let return_value = family(findings, "return_value")?;
    assert_eq!(
        return_value
            .ripr
            .infect
            .summary
            .contains("equality-boundary discriminator is missing"),
        named,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_intermediate_input_separates_a_multi_value_tail_threshold_move() -> Result<(), String> {
    // #9147: `thr(101)` is true before (`> 100`) and false after (`> 102`), so
    // it discriminates even though it equals neither threshold.
    let findings = findings("    x > 102", OLD, INTERMEDIATE_INPUT)?;
    assert_gap_named(&findings, false)?;
    let return_value = family(&findings, "return_value")?;
    assert_eq!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_intermediate_input_separates_a_multi_value_explicit_return_threshold_move()
-> Result<(), String> {
    let findings = findings(
        "    return x > 102;",
        "    return x > 100;",
        INTERMEDIATE_INPUT,
    )?;
    let return_value = family(&findings, "return_value")?;
    assert!(
        return_value.probe.expression.contains("return"),
        "{return_value:?}"
    );
    assert_gap_named(&findings, false)?;
    assert_eq!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn far_only_inputs_keep_the_multi_value_threshold_gap() -> Result<(), String> {
    // Negative control: 50 and 500 are on the same side of both thresholds.
    for (new, old) in [
        ("    x > 102", OLD),
        ("    return x > 102;", "    return x > 100;"),
    ] {
        let findings = findings(new, old, FAR_INPUTS)?;
        assert_gap_named(&findings, true)?;
        let return_value = family(&findings, "return_value")?;
        assert_ne!(
            return_value.class,
            ExposureClass::Exposed,
            "{return_value:?}"
        );
    }
    Ok(())
}

#[test]
fn an_input_outside_the_moved_range_keeps_the_multi_value_threshold_gap() -> Result<(), String> {
    // Negative control: 99 and 103 flank the range (100, 102] without
    // landing inside it, so neither separates the versions.
    let findings = findings(
        "    x > 102",
        OLD,
        "use thr::thr;\n\n#[test]\nfn below() {\n    assert!(!thr(99));\n}\n\n#[test]\nfn above() {\n    assert!(thr(103));\n}\n",
    )?;
    assert_gap_named(&findings, true)?;
    Ok(())
}

#[test]
fn a_shadowed_parameter_does_not_take_the_intermediate_input() -> Result<(), String> {
    // The compared `x` is the local 500, not the caller's 101, so 101 says
    // nothing about the moved threshold and the gap stays.
    let findings = findings_after(
        "    let x = 500;\n",
        "    x > 102",
        "    x > 100",
        INTERMEDIATE_INPUT,
    )?;
    let return_value = family(&findings, "return_value")?;
    assert_ne!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn a_mutated_parameter_does_not_take_the_intermediate_input() -> Result<(), String> {
    let findings = findings_after(
        "    x += 5;\n",
        "    x > 102",
        "    x > 100",
        INTERMEDIATE_INPUT,
    )?;
    let return_value = family(&findings, "return_value")?;
    assert_ne!(
        return_value.class,
        ExposureClass::Exposed,
        "{return_value:?}"
    );
    Ok(())
}

#[test]
fn an_equality_literal_move_is_separated_only_by_an_input_equal_to_one_literal()
-> Result<(), String> {
    // `x == 100` to `x == 102`: `thr(100)` is true before and false after, so
    // it separates the versions; 101 equals neither literal and does not.
    let separating = findings(
        "    x == 102",
        "    x == 100",
        "use thr::thr;\n\n#[test]\nfn old() {\n    assert!(!thr(100));\n}\n",
    )?;
    let separating = family(&separating, "return_value")?;
    assert_eq!(separating.class, ExposureClass::Exposed, "{separating:?}");
    let between = findings(
        "    x == 102",
        "    x == 100",
        "use thr::thr;\n\n#[test]\nfn neither() {\n    assert!(!thr(101));\n}\n",
    )?;
    let between = family(&between, "return_value")?;
    assert_ne!(between.class, ExposureClass::Exposed, "{between:?}");
    Ok(())
}

#[test]
fn a_qualified_rival_call_is_not_a_separating_input() -> Result<(), String> {
    // `rival::thr(101)` is not the owner's `thr`; only 50 and 500 reach it.
    let rival = findings(
        "    x > 102",
        OLD,
        "use thr::thr;\n\nmod rival {\n    pub fn thr(x: i32) -> bool {\n        x > 0\n    }\n}\n\n#[test]\nfn low() {\n    assert!(!thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr(500));\n}\n\n#[test]\nfn other() {\n    assert!(rival::thr(101));\n}\n",
    )?;
    assert_gap_named(&rival, true)?;
    let owner = family(&rival, "return_value")?;
    assert_ne!(owner.class, ExposureClass::Exposed, "{owner:?}");
    // Positive control: a plain `thr(101)` does separate.
    let plain = findings("    x > 102", OLD, INTERMEDIATE_INPUT)?;
    assert_gap_named(&plain, false)?;
    Ok(())
}

#[test]
fn pattern_and_loop_rebindings_of_the_parameter_keep_the_gap() -> Result<(), String> {
    // Each prefix rebinds `x` through a pattern, so the compared value is not
    // the caller's 101 and the intermediate input must not separate.
    for prefix in [
        "    let (x, _) = (0, 0);\n",
        "    let [x, ..] = [0, 0];\n",
        "    for x in 0..1 {}\n",
        "    if let Some(x) = Some(0) {}\n",
        "    match Some(0) {\n        Some(x) => {}\n        None => {}\n    }\n",
        "    let _f = |x: i32| x;\n",
    ] {
        let findings = findings_after(prefix, "    x > 102", "    x > 100", INTERMEDIATE_INPUT)?;
        let return_value = family(&findings, "return_value")?;
        assert_ne!(
            return_value.class,
            ExposureClass::Exposed,
            "{prefix:?} {return_value:?}"
        );
        assert_gap_named(&findings, true)?;
    }
    Ok(())
}

#[test]
fn an_unrelated_differently_named_local_keeps_the_intermediate_input_credit() -> Result<(), String>
{
    // Control (#9197): `y` is declared, `x` is untouched, so the #9147 credit
    // holds for the tail and explicit-return forms.
    for (new, old) in [
        ("    x > 102", "    x > 100"),
        ("    return x > 102;", "    return x > 100;"),
    ] {
        let findings = findings_after(
            "    let y = 7;\n    let _ = y + 1;\n",
            new,
            old,
            INTERMEDIATE_INPUT,
        )?;
        assert_gap_named(&findings, false)?;
        let return_value = family(&findings, "return_value")?;
        assert_eq!(
            return_value.class,
            ExposureClass::Exposed,
            "{return_value:?}"
        );
    }
    Ok(())
}

/// `thr(101)` equals the new threshold of `> 100` to `> 101`; `thr(50)` is far.
const AT_NEW_THRESHOLD: &str = "use thr::thr;\n\n#[test]\nfn at() {\n    assert!(thr(101));\n}\n\n#[test]\nfn below() {\n    assert!(!thr(50));\n}\n";

#[test]
fn a_rebound_or_mutated_parameter_does_not_satisfy_the_equality_boundary() -> Result<(), String> {
    // #9197: the observed 101 equals the new threshold only by name. The
    // owner rebinds or mutates `x` first, so the compared value is not 101 and
    // the boundary is unresolved: no missing equality value is named (no
    // input could satisfy it), and no input is listed as the compared value.
    for prefix in [
        "    let x = x * 2;\n",
        "    let mut x = x;\n    x += 5;\n",
        "    let mut x = x;\n    x = x + 5;\n",
        "    let (x, _) = (x + 5, 0);\n",
        "    for x in 0..1 {}\n",
    ] {
        for (new, old) in [
            ("    x > 101", "    x > 100"),
            ("    return x > 101;", "    return x > 100;"),
        ] {
            let findings = findings_after(prefix, new, old, AT_NEW_THRESHOLD)?;
            assert_gap_named(&findings, false).map_err(|e| format!("{prefix:?} {new}: {e}"))?;
            let return_value = family(&findings, "return_value")?;
            assert!(
                return_value.activation.missing_discriminators.is_empty(),
                "{prefix:?} {return_value:?}"
            );
        }
    }

const RIVAL_MOD: &str =
    "mod rival {\n    pub fn thr(x: i32) -> bool {\n        x > 0\n    }\n}\n\n";
const FAR_ONLY: &str = "#[test]\nfn low() {\n    assert!(!thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr(500));\n}\n";

#[test]
fn a_qualified_rival_call_is_not_a_boundary_input() -> Result<(), String> {
    // #9218: the owner is called only with far inputs; `rival::thr(101)` is
    // another function, so it must not supply the boundary-equal input.
    let rival = findings(
        NEW,
        OLD,
        &format!(
            "use thr::thr;\n\n{RIVAL_MOD}{FAR_ONLY}\n#[test]\nfn other() {{\n    assert!(!rival::thr(101));\n}}\n"
        ),
    )?;
    assert_gap_named(&rival, true)?;
    let owner = family(&rival, "return_value")?;
    assert_ne!(owner.class, ExposureClass::Exposed, "{owner:?}");
    // The same holds when the rival module is not defined in the test file.
    let foreign = findings(
        NEW,
        OLD,
        &format!(
            "use thr::thr;\n\n{FAR_ONLY}\n#[test]\nfn other() {{\n    assert!(!other_dep::thr(101));\n}}\n"
        ),
    )?;
    assert_gap_named(&foreign, true)?;
    let owner = family(&foreign, "return_value")?;
    assert_ne!(owner.class, ExposureClass::Exposed, "{owner:?}");
    // Positive control: the plain owner call at the boundary still clears it.
    let plain = findings(
        NEW,
        OLD,
        &format!(
            "use thr::thr;\n\n{RIVAL_MOD}{FAR_ONLY}\n#[test]\nfn at() {{\n    assert!(!thr(101));\n}}\n"
        ),
    )?;
    assert_gap_named(&plain, false)?;
    Ok(())
}

#[test]
fn a_receiver_call_on_a_mut_parameter_makes_the_predicate_boundary_unresolved() -> Result<(), String>
{
    // #9197: `x.add_assign(5)` changes the `mut` parameter in place, so the
    // observed 101 is not the compared value and the predicate reads
    // infection-unknown; a read-only receiver call on an immutable parameter
    // keeps the credit.
    let read = |parameter: &str, prefix: &str| -> Result<(ExposureClass, usize), String> {
        let findings = findings_with_parameter(
            parameter,
            prefix,
            "",
            "    if x > 101 { true } else { false }",
            "    if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&findings, "predicate")?;
        Ok((
            predicate.class.clone(),
            predicate.activation.missing_discriminators.len(),
        ))
    };
    for prefix in ["    x.add_assign(5);\n", "    x\n        .add_assign(5);\n"] {
        let (class, missing) = read("mut x: i32", prefix)?;
        assert_eq!(class, ExposureClass::InfectionUnknown, "{prefix:?}");
        assert_eq!(missing, 0, "{prefix:?}");
    }
    for (parameter, prefix) in [
        ("x: i32", "    let _ = x.abs();\n"),
        ("x: i32", "    let _ = x.min(3);\n"),
    ] {
        let (control, _) = read(parameter, prefix)?;
        assert_ne!(control, ExposureClass::InfectionUnknown, "{prefix:?}");
    }
    Ok(())
}

#[test]
fn an_unrelated_local_keeps_the_equality_boundary_credit() -> Result<(), String> {
    // Control: `y` is declared, `x` is untouched, so `thr(101)` still sits
    // on the new threshold.
    for (new, old) in [
        ("    x > 101", "    x > 100"),
        ("    return x > 101;", "    return x > 100;"),
    ] {
        let findings = findings_after(
            "    let y = 7;\n    let _ = y + 1;\n",
            new,
            old,
            AT_NEW_THRESHOLD,
        )?;
        assert_gap_named(&findings, false)?;
        let return_value = family(&findings, "return_value")?;
        assert_eq!(
            return_value.class,
            ExposureClass::Exposed,
            "{return_value:?}"
        );
    }
    Ok(())
}

#[test]
fn a_rebound_parameter_makes_the_predicate_boundary_unresolved_not_a_named_gap()
-> Result<(), String> {
    // #9197: with the owner rebinding `x`, the observed 101 would satisfy
    // `x > 101` only by name, so the predicate reads infection-unknown with no
    // missing value to name; the unrebound control keeps its reading.
    let read = |prefix: &str| -> Result<(ExposureClass, usize), String> {
        let findings = findings_after(
            prefix,
            "    if x > 101 { true } else { false }",
            "    if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&findings, "predicate")?;
        Ok((
            predicate.class.clone(),
            predicate.activation.missing_discriminators.len(),
        ))
    };
    for prefix in [
        "    let x = x * 2;\n",
        "    let mut x = x;\n    x += 5;\n",
        "    let mut x = x;\n    x = x + 5;\n",
        // A char literal holding a quote must not hide the rebinding.
        "    let _quote = '\"';\n    let x = x + 5;\n",
        "    let _quote = b'\"';\n    let mut x = x;\n    x += 5;\n",
    ] {
        let (class, missing) = read(prefix)?;
        assert_eq!(class, ExposureClass::InfectionUnknown, "{prefix:?}");
        assert_eq!(missing, 0, "{prefix:?}");
    }
    let (control, _) = read("    let y = 7;\n")?;
    assert_ne!(control, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn a_match_binding_split_across_lines_makes_the_boundary_unresolved() -> Result<(), String> {
    // #9197: `x` is bound by the arm pattern `x\n =>` (a value of `y`), not the
    // caller's argument, so the observed 101 must not satisfy `x > 101`.
    let findings = findings_wrapped(
        "    match y {\n        x\n        =>\n",
        "    }\n",
        "            if x > 101 { true } else { false }",
        "            if x > 100 { true } else { false }",
        AT_NEW_THRESHOLD,
    )?;
    let predicate = family(&findings, "predicate")?;
    assert_eq!(
        predicate.class,
        ExposureClass::InfectionUnknown,
        "{predicate:?}"
    );
    assert!(predicate.activation.missing_discriminators.is_empty());
    // Control: the same shape with the scrutinee read, not bound, keeps the
    // equality credit (`x` stays the caller's argument).
    let control = findings_wrapped(
        "    match x {\n        0 =>\n            false,\n        _ =>\n",
        "    }\n",
        "            if x > 101 { true } else { false }",
        "            if x > 100 { true } else { false }",
        AT_NEW_THRESHOLD,
    )?;
    let predicate = family(&control, "predicate")?;
    assert_ne!(
        predicate.class,
        ExposureClass::InfectionUnknown,
        "{predicate:?}"
    );
    Ok(())
}

#[test]
fn a_quote_char_literal_without_a_rebinding_keeps_the_equality_credit() -> Result<(), String> {
    // Control: the quote is lexed as a char, and `x` is never rebound, so
    // `thr(101)` still satisfies `x > 101` by value.
    let findings = findings_after(
        "    let _quote = '\"';\n",
        "    if x > 101 { true } else { false }",
        "    if x > 100 { true } else { false }",
        AT_NEW_THRESHOLD,
    )?;
    let predicate = family(&findings, "predicate")?;
    assert_ne!(
        predicate.class,
        ExposureClass::InfectionUnknown,
        "{predicate:?}"
    );
    Ok(())
}

#[test]
fn a_multiline_tuple_or_array_binding_makes_the_boundary_unresolved() -> Result<(), String> {
    for opening in [
        "    let _ = x;\n    match (y, 0) {\n        (\n            x,\n            _\n        )\n        =>\n",
        "    let _ = x;\n    match [y, 0] {\n        [\n            x,\n            _\n        ]\n        =>\n",
    ] {
        let findings = findings_wrapped(
            opening,
            "    }\n",
            "            if x > 101 { true } else { false }",
            "            if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&findings, "predicate")?;
        assert_eq!(
            predicate.class,
            ExposureClass::InfectionUnknown,
            "{opening:?} {predicate:?}"
        );
        // Control, same multiline shape with `x` left alone: not unresolved,
        // so the assertion above cannot come from an unparsed subject.
        let unbound = opening
            .replace("x,", "a,")
            .replace("\n            x\n", "\n            a\n");
        assert_ne!(unbound, opening);
        let control = findings_wrapped(
            &unbound,
            "    }\n",
            "            if x > 101 { true } else { false }",
            "            if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&control, "predicate")?;
        assert_ne!(
            predicate.class,
            ExposureClass::InfectionUnknown,
            "{unbound:?} {predicate:?}"
        );

fn a_crate_qualified_owner_call_is_still_a_boundary_input() -> Result<(), String> {
    // #9218: a qualification that resolves to the owner's own module keeps
    // supplying its input (`thr::thr` names the owner crate's root `thr`).
    let findings = findings(
        NEW,
        OLD,
        &format!("{FAR_ONLY_QUALIFIED}\n#[test]\nfn at() {{\n    assert!(!thr::thr(101));\n}}\n"),
    )?;
    assert_gap_named(&findings, false)?;
    Ok(())
}

const FAR_ONLY_QUALIFIED: &str = "#[test]\nfn low() {\n    assert!(!thr::thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr::thr(500));\n}\n";

#[test]
fn an_import_bound_owner_crate_path_is_still_a_boundary_input() -> Result<(), String> {
    // #9218: `use thr as t; t::thr(101)` is the owner through a renamed
    // import of its crate, so it keeps clearing the gap; the same spelling
    // of an unbound module keeps it.
    let bound = findings(
        NEW,
        OLD,
        &format!(
            "use thr as t;\n\n{FAR_ONLY_QUALIFIED}\n#[test]\nfn at() {{\n    assert!(!t::thr(101));\n}}\n"
        ),
    )?;
    assert_gap_named(&bound, false)?;
    let unbound = findings(
        NEW,
        OLD,
        &format!("{FAR_ONLY_QUALIFIED}\n#[test]\nfn at() {{\n    assert!(!t::thr(101));\n}}\n"),
    )?;
    assert_gap_named(&unbound, true)?;
    Ok(())
}

/// [`findings`] for an owner `thr` that lives in `src/internal.rs`, with
/// `lib_rs` as the crate root (beside a path dependency `other_dep` that
/// also defines a `thr`).
fn findings_in_internal(
    lib_rs: &str,
    extra: &[(&str, &str)],
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-returned-comparison-internal-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    for dir in ["src", "tests", "other_dep/src"] {
        std::fs::create_dir_all(root.join(dir)).map_err(|error| format!("{dir}: {error}"))?;
    }
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"thr\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nother_dep = { path = \"other_dep\" }\n\n[workspace]\n",
    )?;
    write(
        "other_dep/Cargo.toml",
        "[package]\nname = \"other_dep\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )?;
    write(
        "other_dep/src/lib.rs",
        "pub fn thr(x: i32) -> bool {\n    x > 0\n}\n\npub fn other() {}\n",
    )?;
    write("src/lib.rs", lib_rs)?;
    for (path, text) in extra {
        write(path, text)?;
    }
    write(
        "src/internal.rs",
        &format!("pub fn thr(x: i32) -> bool {{\n{new}\n}}\n"),
    )?;
    write("tests/it.rs", test)?;
    let diff = format!(
        "diff --git a/src/internal.rs b/src/internal.rs\nindex 0000000..1111111 100644\n--- a/src/internal.rs\n+++ b/src/internal.rs\n@@ -1,3 +1,3 @@\n pub fn thr(x: i32) -> bool {{\n-{old}\n+{new}\n }}\n"
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

const INTERNAL_FAR_ONLY: &str = "#[test]\nfn low() {\n    assert!(!thr::internal::thr(50));\n}\n\n#[test]\nfn high() {\n    assert!(thr::internal::thr(500));\n}\n";

fn reexport_case(lib_rs: &str) -> Result<Vec<Finding>, String> {
    reexport_case_with(lib_rs, &[])
}

fn reexport_case_with(lib_rs: &str, extra: &[(&str, &str)]) -> Result<Vec<Finding>, String> {
    findings_in_internal(
        lib_rs,
        extra,
        NEW,
        OLD,
        &format!("{INTERNAL_FAR_ONLY}\n#[test]\nfn at() {{\n    assert!(!thr::thr(101));\n}}\n"),
    )
}

#[test]
fn a_genuine_reexport_of_the_owner_still_supplies_the_boundary_input() -> Result<(), String> {
    // #9218: `pub use internal::thr;` makes `thr::thr(101)` the owner.
    for lib_rs in [
        "pub mod internal;\npub use internal::thr;\n",
        "pub mod internal;\npub use internal::{thr, helper};\n",
        "pub mod internal;\npub use crate::internal::*;\n",
    ] {
        let found = reexport_case(lib_rs)?;
        assert_gap_named(&found, false).map_err(|error| format!("{lib_rs:?}: {error}"))?;
    }
    Ok(())
}

#[test]
fn guard_operators_ranges_and_struct_patterns_still_bind_the_name() -> Result<(), String> {
    // #9197: each arm pattern binds `x` from `y`, however the pattern and its
    // guard are spelled, so the observed 101 must not satisfy `x > 101`.
    for pattern in [
        "        Some(x) if x == 3 =>\n",
        "        Some(x) if x >= 3 =>\n",
        "        Some(\n            x\n        ) if y != 3 =>\n",
        "        (a, x) if a == 1 =>\n",
        "        Some(x)\n            if a == 1 =>\n",
        "        x @ 1..=5 =>\n",
        "        Some(x @ 1..=5) =>\n",
        "        S { f: x } =>\n",
        "        S { x } =>\n",
        "        S { x, .. } =>\n",
        "        Foo::Bar { x: 1, y: x } =>\n",
        "        Or(x)\n            if x == 3 =>\n",
    ] {
        let opening = format!("    match y {{\n{pattern}");
        let findings = findings_wrapped(
            &opening,
            "    }\n",
            "            if x > 101 { true } else { false }",
            "            if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&findings, "predicate")?;
        assert_eq!(
            predicate.class,
            ExposureClass::InfectionUnknown,
            "{pattern:?} {predicate:?}"
        );
    }
    // Control: `x` only used in an earlier arm's result keeps the credit.
    let control = findings_wrapped(
        "    match y {\n        A => x_not_bound(),\n        B =>\n",
        "    }\n",
        "            if x > 101 { true } else { false }",
        "            if x > 100 { true } else { false }",
        AT_NEW_THRESHOLD,
    )?;
    let predicate = family(&control, "predicate")?;
    assert_ne!(
        predicate.class,
        ExposureClass::InfectionUnknown,
        "{predicate:?}"
    );

fn a_dependency_export_of_a_same_named_function_is_not_the_owner() -> Result<(), String> {
    // #9218: the root exports `other_dep`'s `thr`, so `thr::thr(101)` is not
    // the owner whether the export is named, grouped, glob or renamed.
    for lib_rs in [
        "pub mod internal;\npub use other_dep::thr;\n",
        "pub mod internal;\npub use other_dep::{thr, other};\n",
        "pub mod internal;\npub use other_dep::*;\n",
        "pub mod internal;\npub use internal::thr as renamed;\npub use other_dep::thr;\n",
    ] {
        let found = reexport_case(lib_rs)?;
        assert_gap_named(&found, true).map_err(|error| format!("{lib_rs:?}: {error}"))?;
        let owner = family(&found, "return_value")?;
        assert_ne!(owner.class, ExposureClass::Exposed, "{lib_rs:?} {owner:?}");
    }
    Ok(())
}

#[test]
fn a_bare_arm_after_a_nontrivial_earlier_result_still_binds_the_name() -> Result<(), String> {
    // #9197: `x =>` re-binds `x` however the previous arm's result is spelled.
    for result in [
        "a + 1",
        "-1",
        "f(a)?",
        "1_u8..2",
        "if a { 1 } else { 2 }",
        "match z { Z => 1, _ => 2 }",
    ] {
        let opening = format!("    match y {{\n        0 => {result},\n        x =>\n");
        let findings = findings_wrapped(
            &opening,
            "    }\n",
            "            if x > 101 { true } else { false }",
            "            if x > 100 { true } else { false }",
            AT_NEW_THRESHOLD,
        )?;
        let predicate = family(&findings, "predicate")?;
        assert_eq!(
            predicate.class,
            ExposureClass::InfectionUnknown,
            "{result:?} {predicate:?}"
        );
    }
    // Control: the same shape binding `z`, not `x`, keeps the credit.
    let control = findings_wrapped(
        "    match y {\n        0 => a + 1,\n        z =>\n",
        "    }\n",
        "            if x > 101 { true } else { false }",
        "            if x > 100 { true } else { false }",
        AT_NEW_THRESHOLD,
    )?;
    let predicate = family(&control, "predicate")?;
    assert_ne!(
        predicate.class,
        ExposureClass::InfectionUnknown,
        "{predicate:?}"
    );

fn a_reexport_outside_the_crate_roots_top_level_is_not_a_root_export() -> Result<(), String> {
    // #9218: the dependency's `thr` is what the root exports.
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        // A glob of the owner's module inside a nested `mod tests`.
        (
            "pub mod internal;\npub use other_dep::thr;\n#[cfg(test)]\nmod tests {\n    pub use crate::internal::*;\n}\n",
            vec![],
        ),
        // A named import of the owner inside an unrelated nested module.
        (
            "pub mod internal;\npub use other_dep::thr;\nmod unrelated {\n    pub use crate::internal::thr;\n}\n",
            vec![],
        ),
        // The binary root imports the owner's module.
        (
            "pub mod internal;\npub use other_dep::thr;\n",
            vec![(
                "src/main.rs",
                "mod internal;\nuse internal::thr;\nfn main() {\n    let _ = thr(1);\n}\n",
            )],
        ),
        // `crate::thr` in a binary-root test is the binary's, not the lib's.
        (
            "pub mod internal;\npub use internal::thr;\n",
            vec![(
                "src/main.rs",
                "use other_dep::thr;\n#[cfg(test)]\nmod t {\n    #[test]\n    fn at() {\n        assert!(!crate::thr(101));\n    }\n}\nfn main() {\n    let _ = thr(1);\n}\n",
            )],
        ),
        // An explicit dependency export beats the owner's glob.
        (
            "pub mod internal;\npub use crate::internal::*;\npub use other_dep::thr;\n",
            vec![],
        ),
        // A dependency glob beside an owner glob is ambiguous.
        (
            "pub mod internal;\npub use crate::internal::*;\npub use other_dep::*;\n",
            vec![],
        ),
        // A leading `::` names an extern crate, never the local module.
        ("pub mod internal;\npub use ::internal::thr;\n", vec![]),
        // Another item renamed to `thr`.
        (
            "pub mod internal;\npub use other_dep::other as thr;\n",
            vec![],
        ),
        // A private import is no export.
        ("pub mod internal;\nuse internal::thr;\n", vec![]),
    ];
    for (lib_rs, extra) in cases {
        let found = reexport_case_with(lib_rs, &extra)?;
        let owner = family(&found, "return_value")?;
        // With two crate roots naming `internal` the owner's module is
        // ambiguous, so even the far owner calls decline and the boundary
        // reads unresolved instead of missing: only not-Exposed holds.
        assert!(
            !extra.is_empty()
                || owner
                    .ripr
                    .infect
                    .summary
                    .contains("equality-boundary discriminator is missing"),
            "{lib_rs:?} {extra:?}: {owner:?}"
        );
        assert_ne!(owner.class, ExposureClass::Exposed, "{lib_rs:?} {owner:?}");
    }
    Ok(())
}

#[test]
fn an_explicit_owner_export_beats_a_dependency_glob() -> Result<(), String> {
    // #9218 control: the explicit named export wins over the glob.
    for lib_rs in [
        "pub mod internal;\npub use other_dep::*;\npub use internal::thr;\n",
        "pub mod internal;\npub use internal::{helper, thr};\npub use other_dep::*;\n",
    ] {
        let found = reexport_case(lib_rs)?;
        assert_gap_named(&found, false).map_err(|error| format!("{lib_rs:?}: {error}"))?;
    }
    Ok(())
}
