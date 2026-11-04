//! A count of array elements does not necessarily observe a changed element.
use super::*;

fn push_finding(
    label: &str,
    old: &str,
    new: &str,
    body: &str,
) -> Result<crate::domain::Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let owner =
        format!("export function logLine(buf: string[], msg: string): void {{\n  {new}\n}}\n");
    let test = format!(
        "import {{ test, expect }} from 'vitest';\nimport {{ logLine }} from '../src/log';\ntest('log', () => {{\n{body}\n}});\n"
    );
    ts_write_file(&root.join("src/log.ts"), &owner)?;
    ts_write_file(&root.join("tests/log.test.ts"), &test)?;
    let mut changed = changed_with_lines("src/log.ts", &[(2, &format!("  {new}"))]);
    changed.removed_lines = vec![crate::analysis::diff::ChangedLine {
        line: 2,
        text: format!("  {old}"),
        new_side_line: 2,
    }];
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|f| f.probe.location.line == 2)
        .ok_or_else(|| format!("{label}: missing finding"))
}

fn push_class(label: &str, old: &str, new: &str, body: &str) -> Result<ExposureClass, String> {
    Ok(push_finding(label, old, new, body)?.class)
}

#[test]
fn length_does_not_pin_the_changed_push_argument() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);";
    assert_eq!(
        push_class(
            "push-length",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            body
        )?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn indexed_value_still_pins_the_changed_argument() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[0]).toBe('HI');";
    assert_eq!(
        push_class(
            "push-value",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            body
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn mixed_length_and_wrong_index_do_not_restore_credit() -> Result<(), String> {
    for (label, body) in [
        (
            "length-and-other",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);\nexpect(1).toBe(1);",
        ),
        (
            "wrong-index",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[1]).toBe('old');",
        ),
    ] {
        assert_eq!(
            push_class(
                label,
                "buf.push(msg);",
                "buf.push(msg.toUpperCase());",
                body
            )?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn mixed_length_and_changed_index_keep_credit() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);\nexpect(b[0]).toBe('HI');";
    assert_eq!(
        push_class(
            "length-and-value",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            body
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn changed_cardinality_is_not_a_value_only_replacement() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(2);";
    assert_eq!(
        push_class("push-arity", "buf.push(msg);", "buf.push(msg, msg);", body)?,
        ExposureClass::Exposed
    );
    Ok(())
}

fn witness_applies(label: &str, old: &str, new: &str, body: &str) -> Result<bool, String> {
    let source =
        format!("export function logLine(buf: string[], msg: string): void {{\n  {new}\n}}\n");
    let test_source = format!(
        "import {{ test, expect }} from 'vitest';\nimport {{ logLine }} from '../src/log';\ntest('log', () => {{\n{body}\n}});\n"
    );
    let owners = extract_owners(Path::new("src/log.ts"), &source);
    let tests = extract_tests(Path::new("tests/log.test.ts"), &test_source);
    let owner = owners
        .first()
        .ok_or_else(|| format!("{label}: owner missing"))?;
    let test = tests
        .first()
        .ok_or_else(|| format!("{label}: test missing"))?;
    let candidate = TypeScriptRelatedCandidate::with_owner_path(
        test,
        TypeScriptRelationKind::DirectOwnerCall,
        TypeScriptOwnerPathDisposition::TrustedOwnerPath,
    );
    let facts =
        super::super::push_argument_guard::PushArgumentFacts::read(Some(old), owner, &[candidate]);
    let assertion = test
        .assertions
        .first()
        .ok_or_else(|| format!("{label}: assertion missing"))?;
    Ok(facts.withhold(test, assertion))
}

#[test]
fn refuses_ambiguous_push_argument_witnesses() -> Result<(), String> {
    let length = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);";
    assert!(witness_applies(
        "baseline-witness",
        "buf.push(msg);",
        "buf.push(msg.toUpperCase());",
        length
    )?);
    for (label, old, new, body) in [
        (
            "cardinality",
            "buf.push(msg);",
            "buf.push(msg, msg);",
            length,
        ),
        (
            "side-effect",
            "buf.push(msg);",
            "buf.push((buf.pop(), msg.toUpperCase()));",
            length,
        ),
        (
            "alias",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            "const b: string[] = [];\nconst alias = b;\nlogLine(alias, 'hi');\nexpect(b.length).toBe(1);",
        ),
        (
            "extra-argument",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            "const b: string[] = [];\nlogLine(b, 'hi', b.push('old'));\nexpect(b[0]).toBe('old');",
        ),
        (
            "getter",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[evil()]).toBe('old');\nexpect(b[0]).toBe('HI');",
        ),
        (
            "custom-push",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            "const b: string[] = [];\nb.push = custom;\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);",
        ),
        (
            "shadowed-owner",
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            "const b: string[] = [];\nconst logLine = fake;\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);",
        ),
    ] {
        assert!(!witness_applies(label, old, new, body)?, "{label}");
    }
    Ok(())
}

#[test]
fn count_only_report_names_the_missing_element() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);";
    let finding = push_finding(
        "push-report",
        "buf.push(msg);",
        "buf.push(msg.toUpperCase());",
        body,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .confidence_basis
            .contains(&crate::domain::ConfidenceCap::ObservationNotConfirmed)
    );
    assert!(
        !finding
            .confidence_basis
            .contains(&crate::domain::ConfidenceCap::OracleBelowStrong)
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|text| text.contains("appended element"))
    );
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .is_some_and(|text| text.contains("appended element"))
    );
    assert!(
        finding
            .ripr
            .reveal
            .observe
            .summary
            .contains("does not establish")
    );
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("appended")
    );
    Ok(())
}

#[test]
fn unchanged_case_and_wrong_index_do_not_claim_length_is_the_problem() -> Result<(), String> {
    for (label, body) in [
        (
            "same-case",
            "const b: string[] = [];\nlogLine(b, 'HI');\nexpect(b[0]).toBe('HI');",
        ),
        (
            "report-wrong-index",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[1]).toBe('old');",
        ),
    ] {
        let finding = push_finding(
            label,
            "buf.push(msg);",
            "buf.push(msg.toUpperCase());",
            body,
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        let next = finding
            .recommended_next_step
            .as_deref()
            .ok_or_else(|| format!("{label}: no recommendation"))?;
        assert!(
            next.contains("input changed by the edit"),
            "{label}: {next}"
        );
        assert!(!next.contains("array length"), "{label}: {next}");
        assert!(
            finding
                .missing
                .iter()
                .any(|text| text.contains("does not establish")),
            "{label}"
        );
        assert!(
            finding
                .ripr
                .reveal
                .observe
                .summary
                .contains("does not establish"),
            "{label}"
        );
        assert!(
            finding
                .ripr
                .reveal
                .discriminate
                .summary
                .contains("does not establish"),
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn array_types_do_not_masquerade_as_index_writes() {
    for source in ["const b: string[] = [];", "const b: string[][] = [];"] {
        assert!(
            !super::super::tests_extract::member_replacement_names(source)
                .iter()
                .any(|name| name == "*"),
            "{source}"
        );
    }
    for source in ["target[key] = v;", "target[''] = v;", "target[0] = v;"] {
        assert!(
            super::super::tests_extract::member_replacement_names(source)
                .iter()
                .any(|name| name == "*"),
            "{source}"
        );
    }
}

#[test]
fn confidence_does_not_borrow_a_rejected_owner_path() -> Result<(), String> {
    let source = "export function logLine(buf: string[], msg: string): void {\n  buf.push(msg.toUpperCase());\n}\n";
    let test_source = "import { test, expect } from 'vitest';\nimport { logLine } from '../src/log';\ntest('log', () => {\nconst b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);\n});\n";
    let weak_source = "import { test, expect } from 'vitest';\nimport { logLine } from '../src/log';\ntest('weak', () => { logLine([], 'hi'); expect(true).toBeTruthy(); });\n";
    let owners = extract_owners(Path::new("src/log.ts"), source);
    let tests = extract_tests(Path::new("tests/log.test.ts"), test_source);
    let weak_tests = extract_tests(Path::new("tests/weak.test.ts"), weak_source);
    let owner = owners.first().ok_or("missing owner")?;
    let test = tests.first().ok_or("missing witness test")?;
    let weak = weak_tests.first().ok_or("missing weak test")?;
    let trusted = TypeScriptRelatedCandidate::with_owner_path(
        test,
        TypeScriptRelationKind::DirectOwnerCall,
        TypeScriptOwnerPathDisposition::TrustedOwnerPath,
    );
    let rejected = TypeScriptRelatedCandidate::with_owner_path(
        test,
        TypeScriptRelationKind::DirectOwnerCall,
        TypeScriptOwnerPathDisposition::RejectedOwnerModuleMock,
    );
    let eligible_weak = TypeScriptRelatedCandidate::with_owner_path(
        weak,
        TypeScriptRelationKind::DirectOwnerCall,
        TypeScriptOwnerPathDisposition::TrustedOwnerPath,
    );
    let facts = super::super::push_argument_guard::PushArgumentFacts::read(
        Some("buf.push(msg);"),
        owner,
        &[trusted],
    );
    assert!(ts_push_raw_eligible(
        &[trusted],
        &facts,
        &ProbeFamily::SideEffect
    ));
    assert!(!ts_push_raw_eligible(
        &[rejected, eligible_weak],
        &facts,
        &ProbeFamily::SideEffect
    ));
    Ok(())
}

#[test]
fn escaped_string_values_use_decoded_comparison() -> Result<(), String> {
    for (label, body, expected) in [
        (
            "escaped-differentiates",
            r"const b: string[] = []; logLine(b, 'a\n'); expect(b[0]).toBe('A\n');",
            ExposureClass::Exposed,
        ),
        (
            "escaped-does-not-differentiate",
            r"const b: string[] = []; logLine(b, '\\nhi'); expect(b[0]).toBe('\nhi');",
            ExposureClass::WeaklyExposed,
        ),
        (
            "hex-differentiates",
            r"const b: string[] = []; logLine(b, '\x61'); expect(b[0]).toBe('\x41');",
            ExposureClass::Exposed,
        ),
        (
            "unicode-mixed-quotes",
            r#"const b: string[] = []; logLine(b, '\u0061'); expect(b[0]).toBe("A");"#,
            ExposureClass::Exposed,
        ),
        (
            "escaped-mixed-assertions",
            r"const b: string[] = []; logLine(b, 'a\n'); expect(b.length).toBe(1); expect(b[0]).toBe('A\n');",
            ExposureClass::Exposed,
        ),
    ] {
        assert_eq!(
            push_class(
                label,
                "buf.push(msg);",
                "buf.push(msg.toUpperCase());",
                body
            )?,
            expected,
            "{label}"
        );
    }
    Ok(())
}
