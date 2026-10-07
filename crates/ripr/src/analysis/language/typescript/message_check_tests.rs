//! Literal message checks in chai and `node:assert` (RIPR-SPEC-0243 rule 10)
//! and the message-only guard they share with rule 4.

use super::*;

fn assertions_in(file: &str, source: &str) -> Vec<TypeScriptAssertion> {
    let tests = extract_tests(Path::new(file), source);
    assert_eq!(tests.len(), 1, "fixture must register one test: {tests:?}");
    tests.into_iter().flat_map(|test| test.assertions).collect()
}

fn oracle_rows(assertions: &[TypeScriptAssertion]) -> Vec<(OracleKind, String)> {
    assertions
        .iter()
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion_oracle_text(assertion),
            )
        })
        .collect()
}

#[test]
fn chai_throw_with_one_string_literal_pins_the_message() {
    let assertions = assertions_in(
        "test/parse.test.ts",
        r#"
import { expect } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  expect(() => parse("")).to.throw("blank");
  expect(() => parse("")).to.throw(TypeError);
  expect(() => parse("")).to.throw();
  expect(() => parse("")).to.throw(/blank/);
  expect(() => parse("")).to.throw(TypeError, "blank");
  expect(() => parse("x")).not.to.throw("blank");
});
"#,
    );
    assert_eq!(
        oracle_rows(&assertions),
        vec![
            (
                OracleKind::ExactErrorVariant,
                "expect(...).to.throw(\"blank\")".to_string()
            ),
            (
                OracleKind::BroadError,
                "expect(...).to.throw(...)".to_string()
            ),
            (
                OracleKind::BroadError,
                "expect(...).to.throw(...)".to_string()
            ),
            (
                OracleKind::BroadError,
                "expect(...).to.throw(...)".to_string()
            ),
            (
                OracleKind::BroadError,
                "expect(...).to.throw(...)".to_string()
            ),
            (
                OracleKind::BroadError,
                "expect(...).not.to.throw(...)".to_string()
            ),
        ]
    );
    assert_eq!(assertions[0].oracle_strength, OracleStrength::Strong);
    assert_eq!(assertions[0].oracle_confidence, OracleConfidence::High);
    assert_eq!(
        assertions[0]
            .error_payload
            .as_ref()
            .and_then(|payload| payload.message_check.as_deref()),
        Some("blank")
    );
}

#[test]
fn node_assert_anchored_regex_or_message_object_pins_the_message() {
    let assertions = assertions_in(
        "test/charge.test.ts",
        r#"
import assert from "node:assert";
import { test } from "node:test";
import { parse, charge } from "../src/lib";

test("errors", async () => {
  assert.throws(() => parse(""), /^Error: blank$/);
  assert.throws(() => parse(""), /blank/);
  assert.throws(() => parse(""), /^Error: (empty|blank)$/);
  assert.throws(() => parse(""), "blank");
  assert.throws(() => parse(""), TypeError);
  await assert.rejects(charge(-1), { message: "charge must be positive" });
  await assert.rejects(charge(-1), { name: "Error" });
  assert.doesNotThrow(() => parse("x"), /^Error: blank$/);
});
"#,
    );
    let kinds: Vec<OracleKind> = assertions
        .iter()
        .map(|assertion| assertion.oracle_kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            OracleKind::ExactErrorVariant,
            OracleKind::BroadError,
            OracleKind::BroadError,
            OracleKind::BroadError,
            OracleKind::BroadError,
            OracleKind::ExactErrorVariant,
            OracleKind::BroadError,
            OracleKind::BroadError,
        ],
        "{assertions:?}"
    );
    assert_eq!(
        assertion_oracle_text(&assertions[0]),
        "assert.throws(..., /^Error: blank$/)"
    );
    assert_eq!(
        assertion_oracle_text(&assertions[5]),
        "assert.rejects(..., { message: \"charge must be positive\" })"
    );
    assert_eq!(
        assertions[5]
            .error_payload
            .as_ref()
            .and_then(|payload| payload.message_check.as_deref()),
        Some("charge must be positive")
    );
}

/// chai's `assert.throws(fn, ErrorLike, /regex/)` has another signature, so
/// rule 10 does not read its second argument.
#[test]
fn chai_assert_throws_keeps_the_broad_reading() {
    let assertions = assertions_in(
        "test/parse.test.ts",
        r#"
import { assert } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  assert.throws(() => parse(""), /^Error: blank$/);
});
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    assert_eq!(assertions[0].oracle_kind, OracleKind::BroadError);
    assert!(assertions[0].error_payload.is_none());
}

fn messages(old: Option<&str>, new: Option<&str>) -> MessageOnlyChange {
    MessageOnlyChange {
        old_message: old.map(str::to_string),
        new_message: new.map(str::to_string),
    }
}

#[test]
fn message_only_change_reads_both_messages() {
    let change = |old: &str, new: &str| message_only_change(old, new);
    assert_eq!(
        change(
            "    throw new Error(\"empty\");",
            "    throw new Error(\"blank\");"
        ),
        Some(messages(Some("empty"), Some("blank")))
    );
    assert_eq!(
        change(
            "  return Promise.reject(new Error('a' + \"b\"));",
            "  return Promise.reject(new Error('a' + \"c\"));"
        ),
        Some(messages(Some("ab"), Some("ac")))
    );
    assert_eq!(
        change("throw \"bad\";", "throw \"worse\";"),
        Some(messages(Some("bad"), Some("worse")))
    );
    // An interpolated operand leaves the messages unknown.
    assert_eq!(
        change(
            "throw new Error(\"left \" + count);",
            "throw new Error(\"remain \" + count);"
        ),
        Some(messages(None, None))
    );
    assert_eq!(
        change(
            "throw new Error(`${count} left`);",
            "throw new Error(`${count} remain`);"
        ),
        Some(messages(None, None))
    );
}

#[test]
fn a_change_outside_the_message_literal_is_not_message_only() {
    for (old, new) in [
        // The `+` chain is new code, not a changed literal.
        (
            "throw new Error(\"unsupported currency\");",
            "throw new Error(\"unsupported \" + \"currency\");",
        ),
        // A literal in the condition on the same line does not count.
        (
            "if (s === \"a\") throw new Error(\"x\");",
            "if (s === \"b\") throw new Error(\"x\");",
        ),
        // A class swap.
        (
            "throw new TypeError(\"x\");",
            "throw new RangeError(\"x\");",
        ),
        // A changed interpolation.
        (
            "throw new Error(`${count} left`);",
            "throw new Error(`${total} left`);",
        ),
        // Not a throw or reject line.
        ("return \"a\";", "return \"b\";"),
        // No change at all.
        ("throw new Error(\"x\");", "throw new Error(\"x\");"),
    ] {
        assert_eq!(message_only_change(old, new), None, "{old} -> {new}");
    }
}

fn payload(kind: TypeScriptErrorPayloadKind, check: &str) -> TypeScriptErrorPayload {
    TypeScriptErrorPayload {
        expected: String::new(),
        kind,
        message_check: Some(check.to_string()),
    }
}

#[test]
fn message_check_credits_only_when_it_passes_on_exactly_one_side() {
    let change = |old: &str, new: &str| messages(Some(old), Some(new));
    let chai = payload(TypeScriptErrorPayloadKind::ChaiThrowLiteral, "blank");
    assert!(message_check_tells_change_apart(
        &chai,
        &change("empty", "blank")
    ));
    // chai matches a substring: "blank" is inside "not blank" too.
    assert!(!message_check_tells_change_apart(
        &chai,
        &change("not blank", "blank")
    ));
    // A test still pinned to the old message fails on the new one.
    let old_pin = payload(TypeScriptErrorPayloadKind::ChaiThrowLiteral, "empty");
    assert!(message_check_tells_change_apart(
        &old_pin,
        &change("empty", "blank")
    ));
    // Matching neither side passes or fails alike.
    let neither = payload(TypeScriptErrorPayloadKind::ChaiThrowLiteral, "zero");
    assert!(!message_check_tells_change_apart(
        &neither,
        &change("empty", "blank")
    ));

    let object = payload(TypeScriptErrorPayloadKind::AssertRejectsObject, "positive");
    assert!(message_check_tells_change_apart(
        &object,
        &change("nonnegative", "positive")
    ));
    assert!(!message_check_tells_change_apart(
        &object,
        &change("must be positive", "must be > 0")
    ));

    let regex = payload(
        TypeScriptErrorPayloadKind::AssertThrowsRegex,
        "^Error: blank$",
    );
    assert!(message_check_tells_change_apart(
        &regex,
        &change("empty", "blank")
    ));
    assert!(!message_check_tells_change_apart(
        &regex,
        &change("empty", "void")
    ));
    let escaped = payload(
        TypeScriptErrorPayloadKind::AssertThrowsRegex,
        "^Error: 1\\.5$",
    );
    assert!(message_check_tells_change_apart(
        &escaped,
        &change("2.5", "1.5")
    ));
    // A class or wildcard inside the anchors is not a plain literal.
    for pattern in ["^Error: \\w+$", "^Error: .*$"] {
        let regex = payload(TypeScriptErrorPayloadKind::AssertThrowsRegex, pattern);
        assert!(
            !message_check_tells_change_apart(&regex, &change("empty", "blank")),
            "{pattern}"
        );
    }

    // An unknown message credits no message check.
    assert!(!message_check_tells_change_apart(
        &chai,
        &messages(None, Some("blank"))
    ));
}

fn classify_parse_throw(
    old_line: Option<&str>,
    new_line: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let tests = extract_tests(Path::new("test/parse.test.ts"), test_source);
    let owner = TypeScriptOwner {
        name: "parse".to_string(),
        file: PathBuf::from("src/parse.ts"),
        start_line: 1,
        end_line: 6,
        owner_kind: OwnerKind::Function,
        class_name: None,
        decorated: false,
        exported_as_default: false,
        class_default_export: false,
        module_entries: Vec::new(),
        arity: None,
        params: Vec::new(),
        source_text: None,
        imports: Vec::new(),
        method_kind: TypeScriptMethodKind::Ordinary,
    };
    classify_change_with_alias_state(
        Path::new("src/parse.ts"),
        3,
        new_line,
        old_line.map_or(ReplacedLine::Inserted, ReplacedLine::Paired),
        &[owner],
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| format!("expected a finding for {new_line}"))
}

const CHAI_BLANK: &str = r#"
import { expect } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  expect(() => parse("")).to.throw("blank");
});
"#;

/// RIPR-SPEC-0243 example 14.
#[test]
fn chai_message_check_exposes_a_message_change_it_tells_apart() -> Result<(), String> {
    let finding = classify_parse_throw(
        Some("    throw new Error(\"empty\");"),
        "    throw new Error(\"blank\");",
        CHAI_BLANK,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    let finding = classify_parse_throw(
        Some("    throw new Error(\"not blank\");"),
        "    throw new Error(\"blank\");",
        CHAI_BLANK,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .related_tests
            .iter()
            .all(|test| test.oracle_kind == OracleKind::BroadError),
        "{finding:?}"
    );
    Ok(())
}

/// The #6686 `mocha-spec14-chai-throw-string` shape: the test still pins
/// the old message, so the new message fails it.
#[test]
fn chai_message_check_pinned_to_the_old_message_exposes_the_change() -> Result<(), String> {
    let finding = classify_parse_throw(
        Some("    throw new Error(\"empty\");"),
        "    throw new Error(\"blank\");",
        &CHAI_BLANK.replace("\"blank\"", "\"empty\""),
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// RIPR-SPEC-0243 example 34.
#[test]
fn node_assert_anchored_regex_exposes_only_a_plain_literal_pin() -> Result<(), String> {
    let source = |pattern: &str| {
        format!(
            r#"
import assert from "node:assert";
import {{ test }} from "node:test";
import {{ parse }} from "../src/parse";

test("parse", () => {{
  assert.throws(() => parse(""), {pattern});
}});
"#
        )
    };
    let old = Some("    throw new Error(\"empty\");");
    let new = "    throw new Error(\"blank\");";
    for (pattern, class) in [
        ("/^Error: blank$/", ExposureClass::Exposed),
        ("/blank/", ExposureClass::WeaklyExposed),
        ("/^Error: (empty|blank)$/", ExposureClass::WeaklyExposed),
        ("/^Error: \\w+$/", ExposureClass::WeaklyExposed),
    ] {
        let finding = classify_parse_throw(old, new, &source(pattern))?;
        assert_eq!(finding.class, class, "{pattern}: {finding:?}");
    }
    Ok(())
}

/// Runs the adapter over a real unified diff of `src/parse.ts` with a chai
/// test that pins `"blank"`, and returns the finding on `line`.
fn adapter_finding(
    label: &str,
    new_source: &str,
    diff: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = super::tests::ts_unique_tempdir(label)?;
    let write =
        |path: &str, contents: &str| super::tests::ts_write_file(&root.join(path), contents);
    write(
        "package.json",
        r#"{"name":"pkg","scripts":{"test":"mocha"},"devDependencies":{"mocha":"^10.0.0","chai":"^4.0.0"}}"#,
    )?;
    write("src/parse.ts", new_source)?;
    write("test/parse.test.ts", CHAI_BLANK)?;
    let changed_files = crate::analysis::diff::parse_unified_diff(diff);
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    result
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("no finding on line {line}"))
}

/// The production path pairs each added line with the removed line it
/// replaced by position in the block. A two-line replacement used to pair
/// only its first line, so the `"blank"` → `"not blank"` line skipped the
/// guard and read strong `exposed` from a chai check that passes on both.
#[test]
fn adapter_guards_every_line_of_a_replaced_block() -> Result<(), String> {
    let new_source = "export function parse(s: string): string {\n  if (s === \"\") {\n    // empty input\n    throw new Error(\"not blank\");\n  }\n  return s;\n}\n";
    let diff = "diff --git a/src/parse.ts b/src/parse.ts\n--- a/src/parse.ts\n+++ b/src/parse.ts\n@@ -1,7 +1,7 @@\n export function parse(s: string): string {\n   if (s === \"\") {\n-    // empty\n-    throw new Error(\"blank\");\n+    // empty input\n+    throw new Error(\"not blank\");\n   }\n   return s;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .replaced_line_counterpart(4)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"blank\");"),
        "fixture: the second added line pairs with the second removed line"
    );
    let finding = adapter_finding("message-guard-block", new_source, diff, 4)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(
        finding
            .related_tests
            .iter()
            .map(|test| test.oracle_kind.clone())
            .collect::<Vec<_>>(),
        vec![OracleKind::BroadError],
        "the chai check is related and reads broad under the guard"
    );
    Ok(())
}

/// An uneven replacement leaves the old side of the throw line unknown, so
/// the guard fails closed instead of pairing it with the wrong line.
#[test]
fn adapter_fails_closed_on_an_uneven_replacement() -> Result<(), String> {
    let new_source = "export function parse(s: string): string {\n  if (s === \"\") {\n    throw new Error(\"not blank\");\n  }\n  return s;\n}\n";
    let diff = "diff --git a/src/parse.ts b/src/parse.ts\n--- a/src/parse.ts\n+++ b/src/parse.ts\n@@ -1,7 +1,6 @@\n export function parse(s: string): string {\n   if (s === \"\") {\n-    // note\n-    throw new Error(\"blank\");\n+    throw new Error(\"not blank\");\n   }\n   return s;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert!(changed[0].replaced_line_counterpart(3).is_none());
    assert!(changed[0].replaces_removed_lines(3));
    let finding = adapter_finding("message-guard-uneven", new_source, diff, 3)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// The control for both adapter cases: the same block where the message
/// change is one the chai check tells apart reads `exposed`.
#[test]
fn adapter_credits_a_paired_message_change_the_check_tells_apart() -> Result<(), String> {
    let new_source = "export function parse(s: string): string {\n  if (s === \"\") {\n    // empty input\n    throw new Error(\"blank\");\n  }\n  return s;\n}\n";
    let diff = "diff --git a/src/parse.ts b/src/parse.ts\n--- a/src/parse.ts\n+++ b/src/parse.ts\n@@ -1,7 +1,7 @@\n export function parse(s: string): string {\n   if (s === \"\") {\n-    // empty\n-    throw new Error(\"empty\");\n+    // empty input\n+    throw new Error(\"blank\");\n   }\n   return s;\n }\n";
    let finding = adapter_finding("message-guard-control", new_source, diff, 4)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// A new throw line with nothing removed keeps the rule 10 credit: the old
/// version did not throw that message at all.
#[test]
fn inserted_and_non_message_changes_keep_the_credit() -> Result<(), String> {
    let new = "    throw new Error(\"blank\");";
    let finding = classify_parse_throw(None, new, CHAI_BLANK)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // The error class changed too, so the change is not message-only.
    let finding = classify_parse_throw(
        Some("    throw new TypeError(\"not blank\");"),
        new,
        CHAI_BLANK,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(
        guarded_message_change(ReplacedLine::Unpaired, new),
        Some(MessageOnlyChange {
            old_message: None,
            new_message: Some("blank".to_string()),
        })
    );
    assert_eq!(guarded_message_change(ReplacedLine::Inserted, new), None);
    Ok(())
}

#[test]
fn chai_throws_and_throw_terminals_pin_the_message() {
    let assertions = assertions_in(
        "test/parse.test.ts",
        r#"
import { expect } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  expect(() => parse("")).to.throws("blank");
  expect(() => parse("")).to.Throw("blank");
});
"#,
    );
    assert!(
        assertions
            .iter()
            .all(|assertion| assertion.oracle_kind == OracleKind::ExactErrorVariant),
        "{assertions:?}"
    );
    assert_eq!(assertions.len(), 2);
}

#[test]
fn regex_flags_escaped_dollars_and_class_pipes() {
    let assertions = assertions_in(
        "test/parse.test.ts",
        r#"
import { strict } from "node:assert";
import { test } from "node:test";
import { parse } from "../src/parse";

test("parse", () => {
  strict.throws(() => parse(""), /^Error: blank$/i);
  strict.throws(() => parse(""), /^Error: blank$/m);
  strict.throws(() => parse(""), /^Error: blank\$/);
  strict.throws(() => parse(""), /^Error: blank\\$/);
  strict.throws(() => parse(""), /^Error: blank\\\$/);
  strict.throws(() => parse(""), /^Error: [|]blank$/);
});
"#,
    );
    let kinds: Vec<OracleKind> = assertions
        .iter()
        .map(|assertion| assertion.oracle_kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            OracleKind::BroadError,
            OracleKind::BroadError,
            OracleKind::BroadError,
            OracleKind::ExactErrorVariant,
            OracleKind::BroadError,
            OracleKind::ExactErrorVariant,
        ],
        "{assertions:?}"
    );
    // The rendered row keeps the test's own callee.
    assert_eq!(
        assertion_oracle_text(&assertions[3]),
        "strict.throws(..., /^Error: blank\\\\$/)"
    );
}

/// `node:assert` tests `String(err)`, which always starts with the error's
/// name, so an anchored literal without that prefix matches neither side.
#[test]
fn anchored_regex_without_the_name_prefix_does_not_tell_apart() {
    let change = messages(Some("empty"), Some("blank"));
    let regex = payload(TypeScriptErrorPayloadKind::AssertThrowsRegex, "^blank$");
    assert!(!message_check_tells_change_apart(&regex, &change));
    let regex = payload(
        TypeScriptErrorPayloadKind::AssertThrowsRegex,
        "^TypeError: blank$",
    );
    assert!(message_check_tells_change_apart(&regex, &change));
}

/// RIPR-SPEC-0243 example 35: `rejects` with a `{ message }` object.
#[test]
fn node_assert_rejects_message_object_exposes_the_change() -> Result<(), String> {
    let source = r#"
import assert from "node:assert";
import { test } from "node:test";
import { parse } from "../src/parse";

test("parse", async () => {
  await assert.rejects(parse(""), { message: "blank" });
});
"#;
    let new = "    return Promise.reject(new Error(\"blank\"));";
    let finding = classify_parse_throw(
        Some("    return Promise.reject(new Error(\"empty\"));"),
        new,
        source,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    let finding = classify_parse_throw(
        Some("    return Promise.reject(new Error(\"blank\" + \"\"));"),
        "    return Promise.reject(new Error(\"bla\" + \"nk\"));",
        source,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}
