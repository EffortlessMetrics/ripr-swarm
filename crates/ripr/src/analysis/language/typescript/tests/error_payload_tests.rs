//! RIPR-SPEC-0243 rule 4 through `analyze_diff`: the classifier reads the
//! changed line's removed side, and an error payload that cannot tell the
//! two versions apart reads `broad_error` / weak.
//!
//! The subject is examples 12 and 13: `throw new Error("empty")` becomes
//! `throw new Error("blank")`. Each test first pins the parsed assertion's
//! payload, then the class.

use super::*;

const OWNER: &str = "export function parse(s: string): string {\n  if (s === \"\") {\n    throw new Error(\"blank\");\n  }\n  return s;\n}\n";
const ADDED: (usize, &str) = (3, "    throw new Error(\"blank\");");
const REMOVED: &str = "    throw new Error(\"empty\");";

fn test_source(matcher: &str) -> String {
    format!(
        "import {{ parse }} from \"../src/lib\";\n\ntest(\"parse\", () => {{\n  expect(() => parse(\"\")).{matcher};\n}});\n"
    )
}

/// The finding on the changed line, with the removed side paired when
/// `with_old_line` is set.
fn finding_for(label: &str, matcher: &str, with_old_line: bool) -> Result<Finding, String> {
    let test = test_source(matcher);
    let tests = extract_tests(Path::new("tests/parse.test.ts"), &test);
    let [parsed] = tests.as_slice() else {
        return Err(format!("{label}: expected one test, got {tests:?}"));
    };
    let [assertion] = parsed.assertions.as_slice() else {
        return Err(format!(
            "{label}: expected one assertion, got {:?}",
            parsed.assertions
        ));
    };
    if assertion.oracle_kind != OracleKind::ExactErrorVariant || assertion.error_payload.is_none() {
        return Err(format!(
            "{label}: the extractor must read an exact error payload, got {assertion:?}"
        ));
    }
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/lib.ts"), OWNER)?;
    ts_write_file(&root.join("tests/parse.test.ts"), &test)?;
    let mut changed = ChangedFile {
        path: PathBuf::from("src/lib.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: ADDED.0,
            text: ADDED.1.to_string(),
            new_side_line: ADDED.0,
        }],
        removed_lines: Vec::new(),
    };
    if with_old_line {
        changed
            .removed_lines
            .push(crate::analysis::diff::ChangedLine {
                line: ADDED.0,
                text: REMOVED.to_string(),
                new_side_line: ADDED.0,
            });
    }
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == ADDED.0)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn assert_observed_kind(finding: &Finding, kind: &str) {
    let summary = &finding.ripr.reveal.observe.summary;
    assert!(
        summary.contains(&format!("oracle kind: `{kind}`")),
        "expected `{kind}` in the observe summary: {summary}"
    );
}

/// Example 13: a class payload passes on both versions of a message-only
/// change, and `toThrow(Error)` matches every thrown error.
#[test]
fn rule4_ex13_class_payload_reads_broad_on_a_message_only_change() -> Result<(), String> {
    let finding = finding_for("rule4-ex13", "toThrow(Error)", true)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_observed_kind(&finding, "broad_error");
    Ok(())
}

/// Example 12 and its counterparts: a string payload credits when it
/// matches exactly one of the old and new messages.
#[test]
fn rule4_string_payload_credits_only_when_it_separates_the_messages() -> Result<(), String> {
    let new_message = finding_for("rule4-new-message", "toThrow(\"blank\")", true)?;
    assert_eq!(new_message.class, ExposureClass::Exposed);
    assert_observed_kind(&new_message, "exact_error_variant");

    let old_message = finding_for("rule4-old-message", "toThrow(\"empty\")", true)?;
    assert_eq!(old_message.class, ExposureClass::Exposed);

    // `"x"` is a substring of neither message: it passes on neither side.
    let neither = finding_for("rule4-neither", "toThrow(\"x\")", true)?;
    assert_eq!(neither.class, ExposureClass::WeaklyExposed);
    assert_observed_kind(&neither, "broad_error");

    // Without the removed side nothing is message-only, so the same test
    // keeps RIPR-SPEC-0097's exact reading: the guard reads the old line.
    let unpaired = finding_for("rule4-neither-unpaired", "toThrow(\"x\")", false)?;
    assert_eq!(unpaired.class, ExposureClass::Exposed);
    assert_observed_kind(&unpaired, "exact_error_variant");
    Ok(())
}
