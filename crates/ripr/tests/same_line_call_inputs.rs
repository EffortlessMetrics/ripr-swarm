//! RIPR-SPEC-0001: each same-line free-owner call contributes its own activation row (#13373).

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
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

/// The finding on the changed `n >= 10` -> `n > 9` line of `owner`, a crate
/// body whose boundary line is the one holding `n > 9`.
fn finding(owner: &str, test: &str) -> Result<Finding, String> {
    finding_with_before(owner, test, "n >= 10")
}

fn finding_with_before(owner: &str, test: &str, before: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-same-line-call-{}-{sequence}",
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
        "[package]\nname = \"band\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", owner)?;
    write("tests/it.rs", test)?;
    let changed_line = owner
        .lines()
        .position(|line| line.contains("n > 9"))
        .ok_or("owner has no `n > 9` line")?
        + 1;
    let lines: Vec<&str> = owner.lines().collect();
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("n > 9", before))
            } else {
                format!(" {line}\n")
            }
        })
        .collect();
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n{hunk}",
        n = lines.len()
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
    let mut predicates = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == changed_line
                && finding.probe.family == ripr::domain::ProbeFamily::Predicate
        })
        .collect::<Vec<_>>();
    if predicates.len() != 1 {
        return Err(format!(
            "expected one predicate on line {changed_line}, got {}",
            predicates.len()
        ));
    }
    predicates
        .pop()
        .ok_or_else(|| "predicate disappeared".to_string())
}

#[test]
fn same_line_second_boundary_is_an_input_row() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    for calls in [
        "assert_eq!(band(10), 2); assert_eq!(band(9), 1);",
        "assert_eq!(band(9), 1); assert_eq!(band(10), 2);",
        "assert_eq!(band(10), 2);\n    assert_eq!(band(9), 1);",
        "assert_eq!((band(10), band(9)), (2, 1));",
        "assert_eq!(band::band(10), 2); assert_eq!(band::band(9), 1);",
        "assert_eq!(band(10), 2); assert_eq!(band(9), 1); assert_eq!(band(9), 1);",
    ] {
        let import = if calls.contains("band::band") {
            ""
        } else {
            "use band::band;\n"
        };
        let test = format!("{import}#[test]\nfn sides() {{\n    {calls}\n}}\n");
        let found = finding(owner, &test)?;
        assert!(
            found.activation.missing_discriminators.is_empty(),
            "{calls}: {found:#?}"
        );
        assert_eq!(
            found
                .activation
                .observed_values
                .iter()
                .filter(|fact| fact.value == "n = 9")
                .count(),
            1,
            "{calls}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn nested_call_keeps_inner_input_and_outer_input_unreadable() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let found = finding(
        owner,
        "use band::band;\n#[test]\nfn nested() { assert_eq!(band(band(9)), 1); }\n",
    )?;
    assert!(
        found
            .activation
            .observed_values
            .iter()
            .any(|fact| fact.value == "n = 9"),
        "{found:#?}"
    );
    assert!(
        found.activation.missing_discriminators.is_empty(),
        "{found:#?}"
    );
    assert!(
        !found
            .activation
            .observed_values
            .iter()
            .any(|fact| fact.value == "n = 1"),
        "the outer result must not be evaluated: {found:#?}"
    );
    Ok(())
}

#[test]
fn same_named_method_strings_and_comments_supply_no_free_owner_input() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let found = finding(
        owner,
        r#"use band::band;
struct Holder;
impl Holder { fn band(&self, _: u8) -> u8 { 1 } }
#[test]
fn decoys() {
    let holder = Holder;
    assert_eq!(band(10), 2); assert_eq!(holder.band(9), 1); let _ = "band(9)"; /* band(9) */
}
"#,
    )?;
    assert!(
        !found
            .activation
            .observed_values
            .iter()
            .any(|fact| fact.value == "n = 9"),
        "{found:#?}"
    );
    assert!(
        !found.activation.missing_discriminators.is_empty(),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn mixed_owner_and_shadowed_calls_do_not_close_the_boundary() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    for test in [
        "use band::band;\nmod rival { pub fn band(_: u8) -> u8 { 1 } }\n#[test]\nfn mixed() { assert_eq!(band(10), 2); assert_eq!(rival::band(9), 1); }\n",
        "use band::band;\n#[test]\nfn shadow() { assert_eq!(band(10), 2); let band = |_: u8| 1; assert_eq!(band(9), 1); }\n",
    ] {
        let found = finding(owner, test)?;
        assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
        assert!(
            !found
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.value == "n = 9"),
            "{found:#?}"
        );
    }
    Ok(())
}

#[test]
fn second_computed_input_keeps_the_boundary_unresolved() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    for calls in [
        "assert_eq!(band(10), 2); assert_eq!(band(base - 1), 1);",
        "assert_eq!(band(base - 1), 1); assert_eq!(band(10), 2);",
    ] {
        let test = format!("use band::band;\n#[test]\nfn computed() {{ let base = 0; {calls} }}\n");
        let found = finding(owner, &test)?;
        assert_eq!(
            found.ripr.infect.state,
            ripr::domain::StageState::Unknown,
            "{calls}: {found:#?}"
        );
        assert!(
            found.activation.missing_discriminators.is_empty(),
            "{calls}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn nested_observer_masking_and_far_call_orders_refuse_credit() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    for calls in [
        "assert_eq!(band(band(9)), 1);",
        "assert_eq!(band(10), 2); assert_eq!(band(band(9)), 1);",
        "assert_eq!(band(band(9)), 1); assert_eq!(band(10), 2);",
        "assert_eq!(band(band(band(9))), 1);",
        "assert_eq!(band(10), 2); assert_eq!(band (band(9)), 1);",
        "assert_eq!(band /* outer */ (band(9)), 1);",
        "assert_eq!(band(10), 2); assert_eq!(band /* outer */ (band(9)), 1);",
        "assert_eq!(band /* outer */ (band(9)), 1); assert_eq!(band(10), 2);",
        "assert_eq!((band(10), band(band(9))), (2, 1));",
        "assert_eq!(\n        band(band(9)),\n        1\n    );",
    ] {
        let test = format!("use band::band;\n#[test]\nfn masking() {{ {calls} }}\n");
        let found = finding_with_before(owner, &test, "n >= 9")?;
        assert_eq!(
            found.class,
            ripr::ExposureClass::WeaklyExposed,
            "{calls}: {found:#?}"
        );
        assert!(
            found
                .evidence
                .iter()
                .any(|text| text.contains("same_test_pairing_missing")),
            "{calls}: {found:#?}"
        );
        assert_ne!(
            found.class,
            ripr::ExposureClass::Exposed,
            "{calls}: {found:#?}"
        );
        assert!(
            found.activation.missing_discriminators.is_empty(),
            "must not invent a missing boundary: {calls}: {found:#?}"
        );
        assert!(
            found
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.value == "n = 9"),
            "the genuine inner input remains visible: {calls}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn nested_observer_refusal_keeps_a_separately_observed_direct_boundary() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    for test in [
        "use band::band;\n#[test]\nfn observed() { assert_eq!(band(band(9)), 1); assert_eq!(band(9), 1); }\n",
        "use band::band;\n#[test]\nfn observed() { assert_eq!(band(9), 1); assert_eq!(band(band(9)), 1); }\n",
        "use band::band;\n#[test]\nfn observed() { assert_eq!(band(10), 2); assert_eq!([band(9), band(10)], [1, 2]); }\n",
        "use band::band;\n#[test]\nfn observed() { assert_eq!(band(10), 2); assert_eq!((band(9)), 1); }\n",
        "use band::band;\n#[test]\nfn masking() { assert_eq!(band(band(9)), 1); }\n#[test]\nfn direct() { assert_eq!(band(9), 1); }\n",
    ] {
        let found = finding_with_before(owner, test, "n >= 9")?;
        assert_eq!(
            found.class,
            ripr::ExposureClass::Exposed,
            "{test}: {found:#?}"
        );
        assert!(
            found.activation.missing_discriminators.is_empty(),
            "{test}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn masked_arithmetic_observer_does_not_promote_a_second_boundary_call() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let test = "use band::band;\n#[test]\nfn masking() { assert_eq!(band(10), 2); assert_eq!(band(9) * 0, 0); }\n";
    let found = finding_with_before(owner, test, "n >= 9")?;
    assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn ignored_wrapper_observer_does_not_promote_a_second_boundary_call() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let test = "use band::band;\nfn ignore(_: u8) -> u8 { 0 }\n#[test]\nfn masking() { assert_eq!(band(10), 2); assert_eq!(ignore(band(9)), 0); }\n";
    let found = finding_with_before(owner, test, "n >= 9")?;
    assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn nested_observer_turbofish_keeps_same_owner_association_refused() -> Result<(), String> {
    let owner = "pub fn band<T>(n: u8, _context: T) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let test = "use band::band;\n#[test]\nfn masking() { assert_eq!(band(10, ()), 2); assert_eq!(band::<()>(band(9, ()), ()), 1); }\n";
    let found = finding_with_before(owner, test, "n >= 9")?;
    assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
    assert!(
        found.activation.missing_discriminators.is_empty(),
        "{found:#?}"
    );
    assert!(
        found
            .activation
            .observed_values
            .iter()
            .any(|fact| fact.value == "n = 9"),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn message_argument_does_not_observe_a_second_boundary_call() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let test =
        "use band::band;\n#[test]\nfn masking() { assert_eq!(band(10), 2, \"{}\", band(9)); }\n";
    let found = finding_with_before(owner, test, "n >= 9")?;
    assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
    assert!(
        found.activation.missing_discriminators.is_empty(),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn unproved_negation_does_not_observe_a_second_boundary_call() -> Result<(), String> {
    for owner in [
        "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n",
        "pub fn band(n: u8) -> Mask {\n    if n > 9 { Mask(2) } else { Mask(1) }\n}\npub struct Mask(u8);\nimpl std::ops::Not for Mask {\n    type Output = u8;\n    fn not(self) -> u8 { 0 }\n}\n",
    ] {
        let test = "use band::band;\n#[test]\nfn masked() { let _ = band(10); assert_eq!(!band(9), 0); }\n";
        let found = finding_with_before(owner, test, "n >= 9")?;
        assert_ne!(found.class, ripr::ExposureClass::Exposed, "{found:#?}");
        assert!(
            found.activation.missing_discriminators.is_empty(),
            "{found:#?}"
        );
    }
    Ok(())
}
