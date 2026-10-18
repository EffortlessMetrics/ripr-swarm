//! Public assertion execution admission controls for MatchArm (#7788).
use ripr::{
    CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily,
    check_workspace, render_check,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn analyze_output(definition: &str, body: &str) -> Result<CheckOutput, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = Scratch(std::env::temp_dir().join(format!(
        "ripr-matcharm-admission-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(root.0.join("src")).map_err(|e| e.to_string())?;
    for (path, text) in [
        (
            "Cargo.toml",
            include_str!("../../../fixtures/match_arm_shadowed_assert/input/Cargo.toml").to_owned(),
        ),
        (
            "src/lib.rs",
            include_str!("../../../fixtures/match_arm_shadowed_assert/input/src/lib.rs").to_owned(),
        ),
        (
            "diff.patch",
            include_str!("../../../fixtures/match_arm_shadowed_assert/diff.patch").to_owned(),
        ),
        (
            "src/tests.rs",
            format!(
                "{definition}\n#[test]\nfn checks_unknown() {{\nlet argv = [\"cargo\", \"test\", \"--locked\", \"-p\", \"alpha\", \"--test\", \"plain_probe\", \"--mystery\"].map(str::to_owned);\nlet actual = super::selection_request(&argv);\n{body}\n}}\n"
            ),
        ),
    ] {
        std::fs::write(root.0.join(path), text).map_err(|e| e.to_string())?;
    }
    let output = check_workspace(CheckInput {
        root: root.0.clone(),
        diff_file: Some(root.0.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<_> = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ProbeFamily::MatchArm)
        .collect();
    assert_eq!(findings.len(), 1, "{output:?}");
    Ok(output)
}
fn analyze(definition: &str, body: &str) -> Result<Finding, String> {
    analyze_output(definition, body)?
        .findings
        .into_iter()
        .find(|finding| finding.probe.family == ProbeFamily::MatchArm)
        .ok_or("MatchArm must be present".to_string())
}
fn check(definition: &str, body: &str, admitted: bool) -> Result<(), String> {
    let finding = analyze(definition, body)?;
    assert_eq!(
        format!("{:?}", finding.ripr.reveal.observe.state) == "Yes",
        admitted,
        "{finding:?}"
    );
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    let related = &finding.related_tests[0];
    if admitted {
        assert_eq!(
            format!("{:?}", related.oracle_strength),
            "Strong",
            "{finding:?}"
        );
    } else {
        assert!(related.oracle.is_none(), "{finding:?}");
        assert_eq!(
            format!("{:?}", related.oracle_strength),
            "None",
            "{finding:?}"
        );
        assert_eq!(
            format!("{:?}", related.oracle_kind),
            "Unknown",
            "{finding:?}"
        );
        assert_eq!(
            format!("{:?}", finding.ripr.reveal.discriminate.state),
            "No",
            "{finding:?}"
        );
    }
    Ok(())
}
#[test]
fn direct_and_eager_helper_assertions_remain_admitted() -> Result<(), String> {
    check(
        "",
        "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));",
        true,
    )?;
    check(
        "fn verify(actual: super::SelectionRequest) { assert!(matches!(actual, super::SelectionRequest::Unresolved(_))); }",
        "verify(actual);",
        true,
    )?;
    check(
        "fn verify() {\nlet argv = [\"cargo\", \"test\", \"--locked\", \"-p\", \"alpha\", \"--test\", \"plain_probe\", \"--mystery\"].map(str::to_owned);\nlet actual = super::selection_request(&argv);\nassert!(matches!(actual, super::SelectionRequest::Unresolved(_)));\n}",
        "verify();",
        true,
    )
}
#[test]
fn shadowed_and_skipped_assertions_are_refused() -> Result<(), String> {
    check(
        "macro_rules! assert { ($cond:expr) => { let _ = $cond; }; }",
        "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));",
        false,
    )?;
    check(
        "",
        "if false { assert!(matches!(actual, super::SelectionRequest::Unresolved(_))); }",
        false,
    )?;
    check(
        "",
        "let _unused = || { assert!(matches!(actual, super::SelectionRequest::Unresolved(_))); };",
        false,
    )?;
    check("", "let _ = actual;", false)
}
#[test]
fn shadowed_and_deferred_helper_assertions_are_refused() -> Result<(), String> {
    let helper = "fn verify(actual: super::SelectionRequest) { assert!(matches!(actual, super::SelectionRequest::Unresolved(_))); }";
    check(helper, "if false { verify(actual); }", false)?;
    check(helper, "let _unused = || { verify(actual); };", false)?;
    check(
        &format!("macro_rules! assert {{ ($cond:expr) => {{ let _ = $cond; }}; }}\n{helper}"),
        "verify(actual);",
        false,
    )
}
#[test]
fn plain_import_binding_the_std_root_keeps_the_glob_foreign() -> Result<(), String> {
    let body = "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));";
    check("use other::std;\nuse std::prelude::v1::*;", body, false)
}
#[test]
fn self_import_binding_the_std_root_keeps_the_glob_foreign() -> Result<(), String> {
    let body = "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));";
    check(
        "use other::std::{self};\nuse std::prelude::v1::*;",
        body,
        false,
    )
}
#[test]
fn unrelated_assertion_remains_unconfirmed() -> Result<(), String> {
    let finding = analyze("", "assert!(true);")?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_ne!(
        format!("{:?}", finding.ripr.reveal.discriminate.state),
        "Yes",
        "{finding:?}"
    );
    Ok(())
}
#[test]
fn unestablished_plain_assertion_names_its_macro_in_human_and_json() -> Result<(), String> {
    let output = analyze_output(
        "use rayon::prelude::*;",
        "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));",
    )?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.family == ProbeFamily::MatchArm)
        .ok_or("MatchArm must be present")?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    for format in [
        OutputFormat::Human,
        OutputFormat::HumanFull,
        OutputFormat::Json,
    ] {
        let text = render_check(&output, &format)?;
        assert!(text.contains("`assert!`"), "{text}");
        assert!(!text.contains("`assert_eq!`"), "{text}");
    }
    Ok(())
}

#[test]
fn shadowed_reachable_unrevealed_assertion_names_its_macro_in_all_formats() -> Result<(), String> {
    let output = analyze_output(
        "macro_rules! assert { ($cond:expr) => { let _ = $cond; }; }",
        "assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));",
    )?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.family == ProbeFamily::MatchArm)
        .ok_or("MatchArm must be present")?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{finding:?}"
    );
    for format in [
        OutputFormat::Human,
        OutputFormat::HumanFull,
        OutputFormat::Json,
    ] {
        let text = render_check(&output, &format)?;
        if format == OutputFormat::Human {
            assert!(text.contains("standard `assert!`"), "{text}");
            assert!(text.contains("Not credited: `assert!`"), "{text}");
        }
        // Full human and JSON retain their existing refusal representation.
        assert!(
            text.contains("rust_assertion_context_unestablished"),
            "{text}"
        );
        assert!(!text.contains("`assert_eq!`"), "{text}");
    }
    Ok(())
}
