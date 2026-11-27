//! End-to-end delegation pins for TypeScript error-variant repair packets
//! (#12953): a weakly-exposed thrown-variant change whose only test uses a
//! bare `toThrow()` must yield a validator-complete packet pinning the new
//! variant, while a test that already pins it offers no packet.

use crate::output::gap_decision_ledger::{
    GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
    render_gap_decision_ledger_json,
};
use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"parse-lib\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"jest\": \"^29.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"jest\"\n  }\n}\n";

const PARSE_TS: &str = "export class EmptyError extends Error {}\nexport class TooLongError extends Error {}\nexport function parse(s: string): number {\n  if (s.length === 0) {\n    throw new EmptyError();\n  }\n  if (s.length > 10) {\n    throw new TooLongError();\n  }\n  return s.length;\n}\n";

const PARSE_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -6,7 +6,7 @@ export function parse(s: string): number {\n   if (s.length > 10) {\n-    throw new EmptyError();\n+    throw new TooLongError();\n   }\n   return s.length;\n }\n";

const BARE_TEST: &str = "import { parse } from '../src/index';\n\ntest('long input is error', () => {\n  expect(() => parse('abcdefghijklmnop')).toThrow();\n});\n";

const PINNED_TEST: &str = "import { parse } from '../src/index';\n\ntest('long input is error', () => {\n  expect(() => parse('abcdefghijklmnop')).toThrow(TooLongError);\n});\n";

const DIVIDE_TS: &str = "export function divide(a: number, b: number): number {\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,3 +1,6 @@\n export function divide(a: number, b: number): number {\n+  if (b === 0) {\n+    throw new Error('division by zero');\n+  }\n   return Math.floor(a / b);\n }\n";

const DIVIDE_BARE_TEST: &str = "import { divide } from '../src/index';\n\ntest('divide by zero throws', () => {\n  expect(() => divide(1, 0)).toThrow();\n});\n";

const DIVIDE_LOOKUP_TS: &str = "export function divide(a: number, b: number): number {\n  if (b === 0) {\n    throw new Error(t('division by zero'));\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_LOOKUP_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,3 +1,6 @@\n export function divide(a: number, b: number): number {\n+  if (b === 0) {\n+    throw new Error(t('division by zero'));\n+  }\n   return Math.floor(a / b);\n }\n";

const DIVIDE_SHORTENED_TS: &str = "export function divide(a: number, b: number): number {\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_SHORTENED_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,6 +1,6 @@\n export function divide(a: number, b: number): number {\n   if (b === 0) {\n-    throw new Error('division by zero occurred');\n+    throw new Error('division by zero');\n   }\n   return Math.floor(a / b);\n }\n";

const DIVIDE_MOVED_TS: &str = "export function divide(a: number, b: number): number {\n  const q = Math.floor(a / b);\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return q;\n}\n";

const DIVIDE_MOVED_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,5 +1,7 @@\n export function divide(a: number, b: number): number {\n-  if (b === 0) throw new Error('division by zero occurred');\n   const q = Math.floor(a / b);\n+  if (b === 0) {\n+    throw new Error('division by zero');\n+  }\n   return q;\n }\n";

const DIVIDE_INLINED_TS: &str = "function checkDivisor(b: number): void {\n  if (b === 0) throw new Error('division by zero occurred');\n}\n\nexport function divide(a: number, b: number): number {\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_INLINED_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,8 +1,10 @@\n function checkDivisor(b: number): void {\n   if (b === 0) throw new Error('division by zero occurred');\n }\n \n export function divide(a: number, b: number): number {\n-  checkDivisor(b);\n+  if (b === 0) {\n+    throw new Error('division by zero');\n+  }\n   return Math.floor(a / b);\n }\n";

const DIVIDE_EARLIER_THROW_TS: &str = "export function divide(a: number, b: number): number {\n  const q = a / b;\n  if (q === Infinity) {\n    throw new Error('division by zero');\n  }\n  return q;\n}\n";

const DIVIDE_EARLIER_THROW_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,8 +1,7 @@\n export function divide(a: number, b: number): number {\n-  if (b === 0) throw new Error('division by zero occurred');\n   const q = a / b;\n   if (q === Infinity) {\n-    throw new Error('overflow');\n+    throw new Error('division by zero');\n   }\n   return q;\n }\n";

const DIVIDE_USER_CLASS_TS: &str = "class DivisionError extends Error {\n  constructor(what: string) {\n    super(what + ' by zero');\n  }\n}\n\nexport function divide(a: number, b: number): number {\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_USER_CLASS_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -6,7 +6,7 @@ class DivisionError extends Error {\n \n export function divide(a: number, b: number): number {\n   if (b === 0) {\n-    throw new DivisionError('division');\n+    throw new Error('division by zero');\n   }\n   return Math.floor(a / b);\n }\n";

const PARSE_AGE_TS: &str = "function checkAge(n: number): number {\n  if (n < 0) {\n    throw new Error('invalid age');\n  }\n  return n;\n}\n\nexport function parseAge(s: string): number {\n  if (s === '') {\n    throw new Error('invalid age: empty');\n  }\n  return checkAge(Number(s));\n}\n";

const PARSE_AGE_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,3 +1,6 @@\n function checkAge(n: number): number {\n+  if (n < 0) {\n+    throw new Error('invalid age');\n+  }\n   return n;\n }\n \n";

const PARSE_AGE_TEST: &str = "import { parseAge } from '../src/index';\n\ntest('empty age throws', () => {\n  expect(() => parseAge('')).toThrow();\n});\n";

const DIVIDE_TWO_THROWS_TS: &str = "export function divide(a: number, b: number): number {\n  if (a < 0) {\n    throw new Error('negative dividend');\n  }\n  if (b === 0) {\n    throw new Error('division by zero');\n  }\n  return Math.floor(a / b);\n}\n";

const DIVIDE_TWO_THROWS_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -1,6 +1,9 @@\n export function divide(a: number, b: number): number {\n   if (a < 0) {\n     throw new Error('negative dividend');\n   }\n+  if (b === 0) {\n+    throw new Error('division by zero');\n+  }\n   return Math.floor(a / b);\n }\n";

struct TypeScriptCheck {
    finding: Value,
    ledger: Value,
}

impl TypeScriptCheck {
    fn class(&self) -> Option<&str> {
        self.finding.get("classification").and_then(Value::as_str)
    }

    fn repair_packet_ready(&self) -> Option<bool> {
        self.finding
            .get("preview_actionability")
            .and_then(|actionability| actionability.get("repair_packet_ready"))
            .and_then(Value::as_bool)
    }

    fn assertion_shape(&self) -> Option<&str> {
        self.finding
            .get("typescript_repair_packet")
            .and_then(|packet| packet.get("assertion_shape"))
            .and_then(Value::as_str)
    }

    fn any_agent_packet_eligible(&self) -> bool {
        self.ledger
            .get("records")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|record| {
                record
                    .get("projection_eligibility")
                    .and_then(|projection| projection.get("agent_packet"))
                    .and_then(|packet| packet.get("eligible"))
                    .and_then(Value::as_bool)
                    == Some(true)
            })
    }
}

fn check_typescript(label: &str, test: &str) -> Result<TypeScriptCheck, String> {
    check_typescript_with(label, PARSE_TS, PARSE_DIFF, test)
}

fn check_typescript_with(
    label: &str,
    source: &str,
    diff: &str,
    test: &str,
) -> Result<TypeScriptCheck, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-typescript-error-variant-{label}-{}-{stamp}",
        std::process::id()
    ));
    let files = [
        ("package.json", PACKAGE_JSON),
        ("ripr.toml", TYPESCRIPT_CONFIG),
        ("src/index.ts", source),
        ("tests/parse.test.ts", test),
    ];
    let output = (|| -> Result<String, String> {
        for (path, contents) in &files {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff fixture: {error}"))?;
        let config = crate::config::tests_only_parse(TYPESCRIPT_CONFIG)?;
        let output = crate::app::check_workspace_with_config(
            crate::CheckInput {
                root: root.clone(),
                base: None,
                diff_file: Some(root.join("diff.patch")),
                mode: crate::Mode::Draft,
                format: crate::OutputFormat::Json,
                include_unchanged_tests: true,
                perl_facts_path: None,
                suppression_policy: None,
                git_timeout: None,
                git_candidate: None,
                scope: Default::default(),
            },
            &config,
        )?;
        crate::render_check(&output, &crate::OutputFormat::Json)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let check_output = output?;
    cleanup?;
    let value: Value = serde_json::from_str(&check_output)
        .map_err(|error| format!("parse check output: {error}"))?;
    // The error-path finding is the one a thrown-error packet is about; an
    // added guard also yields a predicate finding for its condition.
    let finding = match value.get("findings").and_then(Value::as_array) {
        Some(findings) if findings.len() == 1 => findings[0].clone(),
        Some(findings) => findings
            .iter()
            .find(|finding| {
                finding.pointer("/probe/family").and_then(Value::as_str) == Some("error_path")
            })
            .cloned()
            .ok_or_else(|| format!("expected an error_path finding, got {findings:?}"))?,
        None => return Err("check output has no findings array".to_string()),
    };
    let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: format!("source-fixture/{label}"),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: format!("{label}.json"),
        records_json: Ok(check_output),
    });
    let ledger = render_gap_decision_ledger_json(&report)?;
    let ledger: Value =
        serde_json::from_str(&ledger).map_err(|error| format!("parse ledger: {error}"))?;
    Ok(TypeScriptCheck { finding, ledger })
}

/// #12953 (a): the bare `toThrow()` finding is weakly exposed, packet-ready
/// through the shared validator, and its edit target pins the new variant.
#[test]
fn bare_tothrow_error_variant_gets_a_complete_packet() -> Result<(), String> {
    let check = check_typescript("bare", BARE_TEST)?;
    if check.class() != Some("weakly_exposed")
        || check.repair_packet_ready() != Some(true)
        || !check.any_agent_packet_eligible()
    {
        return Err(format!("bare toThrow lost its packet: {}", check.finding));
    }
    let shape = check.assertion_shape().unwrap_or_default();
    if shape != "expect(() => parse('abcdefghijklmnop')).toThrow(TooLongError)" {
        return Err(format!("packet must pin the thrown variant: {shape}"));
    }
    Ok(())
}

/// #12953 negative control: a test that already pins the thrown variant
/// reads `exposed` and offers no packet or agent start.
#[test]
fn pinned_tothrow_error_variant_offers_no_packet() -> Result<(), String> {
    let check = check_typescript("pinned", PINNED_TEST)?;
    if check.class() != Some("exposed")
        || check.repair_packet_ready() == Some(true)
        || check.assertion_shape().is_some()
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "pinned toThrow must offer no packet: {}",
            check.finding
        ));
    }
    Ok(())
}

/// #13827: an added `throw new Error('<message>')` under a bare `toThrow()`
/// test gets a packet that pins the literal message (`toThrow(Error)` could
/// not discriminate). Mirrors the repair-start set's `typescript/panic-guard`.
#[test]
fn bare_tothrow_generic_error_literal_message_gets_a_message_packet() -> Result<(), String> {
    let check = check_typescript_with("divide", DIVIDE_TS, DIVIDE_DIFF, DIVIDE_BARE_TEST)?;
    if check.class() != Some("weakly_exposed") || !check.any_agent_packet_eligible() {
        return Err(format!(
            "added Error guard lost its packet: {}",
            check.finding
        ));
    }
    let shape = check.assertion_shape().unwrap_or_default();
    if shape != "expect(() => divide(1, 0)).toThrow('division by zero')" {
        return Err(format!("packet must pin the literal message: {shape}"));
    }
    Ok(())
}

/// #13827 negative control: the first literal on the line is a lookup key,
/// not the thrown message, so no message packet is offered.
#[test]
fn bare_tothrow_generic_error_lookup_message_offers_no_packet() -> Result<(), String> {
    let check = check_typescript_with(
        "divide-lookup",
        DIVIDE_LOOKUP_TS,
        DIVIDE_LOOKUP_DIFF,
        DIVIDE_BARE_TEST,
    )?;
    // Precondition: the lookup-key line still yields the weak error-path
    // finding whose discriminator names the key, so the absence below is the
    // message guard refusing it, not a different classification.
    let discriminator = check
        .finding
        .pointer("/activation/missing_discriminators/0/value")
        .and_then(Value::as_str);
    if check.class() != Some("weakly_exposed")
        || discriminator != Some("throws Error matching 'division by zero'")
    {
        return Err(format!(
            "lookup-key fixture lost its weak discriminator: {}",
            check.finding
        ));
    }
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
    {
        return Err(format!("lookup key must not be pinned: {}", check.finding));
    }
    if check.any_agent_packet_eligible() {
        return Err(format!(
            "lookup-key message must offer no agent start: {}",
            check.finding
        ));
    }
    Ok(())
}

/// #13827 negative control: a shortened message (`'division by zero
/// occurred'` to `'division by zero'`) satisfies `toThrow('division by zero')`
/// on both versions, so no message packet is offered.
#[test]
fn bare_tothrow_shortened_message_offers_no_message_packet() -> Result<(), String> {
    let check = check_typescript_with(
        "divide-shortened",
        DIVIDE_SHORTENED_TS,
        DIVIDE_SHORTENED_DIFF,
        DIVIDE_BARE_TEST,
    )?;
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "a shortened message must not be pinned: {}",
            check.finding
        ));
    }
    Ok(())
}

/// #13827 negative control: with another generic `Error` throw in the owner,
/// the bare `toThrow()` test may observe that one, so the changed message is
/// not pinned.
#[test]
fn bare_tothrow_message_at_one_of_two_throws_offers_no_message_packet() -> Result<(), String> {
    let check = check_typescript_with(
        "divide-two-throws",
        DIVIDE_TWO_THROWS_TS,
        DIVIDE_TWO_THROWS_DIFF,
        DIVIDE_BARE_TEST,
    )?;
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "a non-sole throw must not be pinned: {}",
            check.finding
        ));
    }
    Ok(())
}

/// #13827 negative control: the old throw is deleted in one place and a
/// shorter message is thrown from another, so the added throw has no paired
/// old line. It is not a pure addition: the deleted message already contains
/// the new one, so `toThrow('division by zero')` would pass on both versions.
#[test]
fn bare_tothrow_moved_throw_with_contained_message_offers_no_message_packet() -> Result<(), String>
{
    let changed = crate::analysis::parse_unified_diff(DIVIDE_MOVED_DIFF);
    let [file] = changed.as_slice() else {
        return Err(format!("fixture: one changed file, got {}", changed.len()));
    };
    if file.aligned_counterparts().contains_key(&4) {
        return Err("fixture: the moved throw must have no paired old line".to_string());
    }
    let check = check_typescript_with(
        "divide-moved",
        DIVIDE_MOVED_TS,
        DIVIDE_MOVED_DIFF,
        DIVIDE_BARE_TEST,
    )?;
    let discriminator = check
        .finding
        .pointer("/activation/missing_discriminators/0/value")
        .and_then(Value::as_str);
    if check.class() != Some("weakly_exposed")
        || discriminator != Some("throws Error matching 'division by zero'")
    {
        return Err(format!(
            "fixture: the throw must still read a missing message discriminator: {}",
            check.finding
        ));
    }
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "a moved throw whose old message contains the new one must not be pinned: {}",
            check.finding
        ));
    }
    Ok(())
}

/// Shared body for the #13827 diff-shape negatives: the fixture's changed
/// throw at new-side `line` has the expected paired old line (or none) and
/// still reads the weak message discriminator, yet no message packet is
/// offered.
fn assert_no_message_packet(
    label: &str,
    source: &str,
    diff: &str,
    line: usize,
    counterpart: Option<&str>,
) -> Result<(), String> {
    let changed = crate::analysis::parse_unified_diff(diff);
    let [file] = changed.as_slice() else {
        return Err(format!(
            "{label} fixture: one changed file, got {}",
            changed.len()
        ));
    };
    let paired = file
        .aligned_counterparts()
        .get(&line)
        .map(|removed| removed.text.trim().to_string());
    if paired.as_deref() != counterpart {
        return Err(format!(
            "{label} fixture: line {line} pairs with {paired:?}, expected {counterpart:?}"
        ));
    }
    let check = check_typescript_with(label, source, diff, DIVIDE_BARE_TEST)?;
    let discriminator = check
        .finding
        .pointer("/activation/missing_discriminators/0/value")
        .and_then(Value::as_str);
    if check.class() != Some("weakly_exposed")
        || discriminator != Some("throws Error matching 'division by zero'")
    {
        return Err(format!(
            "{label} fixture: the throw must still read a missing message discriminator: {}",
            check.finding
        ));
    }
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "{label}: the old side may satisfy the pin, so no message packet: {}",
            check.finding
        ));
    }
    Ok(())
}

/// #13827 negative control: a removed helper call (`checkDivisor(b)`) is
/// inlined as a shorter throw. The helper threw a message containing the new
/// one, so the pin would pass on both versions although no removed line
/// says `throw`.
#[test]
fn bare_tothrow_inlined_helper_throw_offers_no_message_packet() -> Result<(), String> {
    assert_no_message_packet(
        "divide-inlined",
        DIVIDE_INLINED_TS,
        DIVIDE_INLINED_DIFF,
        7,
        None,
    )
}

/// #13827 negative control: the changed throw pairs with an old throw whose
/// message (`'overflow'`) does not contain the new one, but a separately
/// removed earlier throw did, and it fires first on the same input.
#[test]
fn bare_tothrow_removed_earlier_throw_offers_no_message_packet() -> Result<(), String> {
    assert_no_message_packet(
        "divide-earlier-throw",
        DIVIDE_EARLIER_THROW_TS,
        DIVIDE_EARLIER_THROW_DIFF,
        4,
        Some("throw new Error('overflow');"),
    )
}

/// #13827 negative control: the old line threw a user error class whose
/// constructor builds a longer runtime message (`'division' + ' by zero'`),
/// so its literal argument is not the old message and the pin is refused.
#[test]
fn bare_tothrow_replaced_user_error_class_offers_no_message_packet() -> Result<(), String> {
    assert_no_message_packet(
        "divide-user-class",
        DIVIDE_USER_CLASS_TS,
        DIVIDE_USER_CLASS_DIFF,
        9,
        Some("throw new DivisionError('division');"),
    )
}

/// #13827 negative control: the test reaches the changed private helper only
/// through an exported wrapper whose own unchanged throw fires first with a
/// message containing the new one, so no message packet is offered.
#[test]
fn bare_tothrow_through_an_exported_wrapper_offers_no_message_packet() -> Result<(), String> {
    let check = check_typescript_with("parse-age", PARSE_AGE_TS, PARSE_AGE_DIFF, PARSE_AGE_TEST)?;
    if check
        .assertion_shape()
        .is_some_and(|shape| shape.contains("toThrow('"))
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "a wrapper's own throw may satisfy the pin, so no message packet: {}",
            check.finding
        ));
    }
    Ok(())
}
