//! #13035: unread Insta configuration withholds a gap without inventing a
//! field observation. The direct owner/result and changed-field guards remain.

use super::*;

const SETTINGS: &str = r#"let mut settings = insta::Settings::clone_current();
        settings.add_redaction(".id", "[id]");
        let _guard = settings.bind_to_scope();"#;

const SNAPSHOT: &str = "insta::assert_json_snapshot!(record);";
const INLINE: &str = "insta::assert_json_snapshot!(record, @\"Record\");";

fn changed_fields() -> [(String, usize); 2] {
    [
        (
            diff(
                8,
                "        id: format!(\"{}-0\", prefix),",
                "        id: format!(\"{}-1\", prefix),",
            ),
            8,
        ),
        (insta_edit(), 9),
    ]
}

fn unread_configuration_withheld(assertion: &str, edit: &str, line: usize) -> Result<(), String> {
    let findings = findings_on(&insta_source(assertion), edit, line)?;
    let [finding] = findings.as_slice() else {
        return Err(format!(
            "expected exactly one changed-field finding on {line}, got {}",
            findings.len()
        ));
    };
    if finding.probe.family != ripr::ProbeFamily::FieldConstruction
        || finding.class != ExposureClass::StaticUnknown
        || !limited(finding)
        || finding.related_tests.is_empty()
        || finding.canonical_gap.is_some()
    {
        return Err(format!(
            "unread configuration must withhold the nonempty owner-field subject: {:?}",
            (
                finding.probe.family.as_str(),
                finding.class.as_str(),
                finding.static_limit_kind
            )
        ));
    }
    let advice = finding.recommended_next_step.as_deref().unwrap_or("");
    if !advice.contains("settings or redactions that ripr does not read")
        || advice.contains("no redaction names")
    {
        return Err(format!(
            "unread configuration advice is inaccurate: {advice}"
        ));
    }
    Ok(())
}

#[test]
fn unread_settings_withhold_both_fields() -> Result<(), String> {
    for (edit, line) in changed_fields() {
        unread_configuration_withheld(&format!("{SETTINGS}\n        {SNAPSHOT}"), &edit, line)?;
    }
    Ok(())
}

#[test]
fn with_settings_withholds_both_fields() -> Result<(), String> {
    let assertion = r#"insta::with_settings!({ redactions => vec![(".id", "[id]")] }, {
            insta::assert_json_snapshot!(record);
        });"#;
    for (edit, line) in changed_fields() {
        unread_configuration_withheld(assertion, &edit, line)?;
    }
    Ok(())
}

#[test]
fn inline_snapshot_with_unread_configuration_is_limited() -> Result<(), String> {
    for (edit, line) in changed_fields() {
        unread_configuration_withheld(&format!("{SETTINGS}\n        {INLINE}"), &edit, line)?;
        keeps_gap(
            &insta_source(INLINE),
            &edit,
            line,
            "ordinary inline control",
        )?;
    }
    Ok(())
}

#[test]
fn constant_selectors_remain_withheld() -> Result<(), String> {
    let assertion = r#"const SELECTOR: &str = ".id";
        insta::assert_json_snapshot!(record, { SELECTOR => "[id]" });"#;
    for (edit, line) in changed_fields() {
        withheld(&insta_source(assertion), &edit, line, "constant selector")?;
    }
    Ok(())
}

#[test]
fn unread_settings_still_require_the_owner_result_snapshot() -> Result<(), String> {
    for assertion in [
        format!("{SETTINGS}\n        let other = 5; insta::assert_json_snapshot!(other);"),
        format!("{SETTINGS}\n        let record = 5; {SNAPSHOT}"),
        format!("{SETTINGS}\n        assert!(record.items > 0);"),
    ] {
        keeps_gap(&insta_source(&assertion), &insta_edit(), 9, &assertion)?;
    }
    Ok(())
}

#[test]
fn noncode_near_names_and_later_settings_do_not_change_literal_redactions() -> Result<(), String> {
    let redacted = "insta::assert_json_snapshot!(record, { \".items\" => \"[n]\" });";
    for assertion in [
        format!("// Settings with_settings add_redaction\n        {redacted}"),
        format!("let note = \"Settings with_settings add_redaction\";\n        {redacted}"),
        format!("let settings_note = OtherSettings;\n        {redacted}"),
        format!("let with_settings_note = 0;\n        {redacted}"),
        format!("let add_redaction_note = 0;\n        {redacted}"),
        format!("{redacted}\n        {SETTINGS}"),
    ] {
        keeps_gap(&insta_source(&assertion), &insta_edit(), 9, &assertion)?;
    }
    Ok(())
}

#[test]
fn an_exact_field_assertion_keeps_its_credit_with_unread_settings() -> Result<(), String> {
    let source = insta_source(&format!("{SETTINGS}\n        assert_eq!(record.items, 2);"));
    let findings = findings_on(&source, &insta_edit(), 9)?;
    let [finding] = findings.as_slice() else {
        return Err(format!(
            "expected one field control, got {}",
            findings.len()
        ));
    };
    if finding.class != ExposureClass::Exposed || finding.static_limit_kind.is_some() {
        return Err(format!(
            "independent exact field assertion must keep its credit, got {:?}",
            (finding.class.as_str(), finding.static_limit_kind)
        ));
    }
    Ok(())
}

#[test]
fn test_only_settings_edits_reclassify_a_warm_input() -> Result<(), String> {
    let literal = insta_source("insta::assert_json_snapshot!(record, { \".id\" => \"[id]\" });");
    let settings = insta_source(&format!("{SETTINGS}\n        {SNAPSHOT}"));
    let (edit, line) = &changed_fields()[0];
    let repo = TempRepo::create(&literal, edit)?;
    for (source, expected) in [
        (&literal, ExposureClass::WeaklyExposed),
        (&literal, ExposureClass::WeaklyExposed),
        (&settings, ExposureClass::StaticUnknown),
        (&settings, ExposureClass::StaticUnknown),
        (&literal, ExposureClass::WeaklyExposed),
    ] {
        std::fs::write(repo.root.join("src/lib.rs"), source)
            .map_err(|error| format!("replace only the test configuration: {error}"))?;
        let findings = repo.findings()?;
        let [finding] = findings.as_slice() else {
            return Err(format!(
                "expected one warm field finding, got {}",
                findings.len()
            ));
        };
        if finding.probe.location.line != *line
            || finding.class != expected
            || limited(finding) != (expected == ExposureClass::StaticUnknown)
        {
            return Err(format!(
                "warm input must reclassify after the test-only edit: expected {}, got {:?}",
                expected.as_str(),
                (finding.class.as_str(), finding.static_limit_kind)
            ));
        }
        if expected == ExposureClass::StaticUnknown && finding.canonical_gap.is_some() {
            return Err("warm limited input retained a canonical gap".to_string());
        }
    }
    Ok(())
}
