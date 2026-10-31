//! End-to-end pins for all-literal object and array expected values
//! (#11255): each case runs the production `check` path on an on-disk
//! citty-shaped workspace, so the dynamic-assertion limitation and the
//! repair-packet decision come from the real producer. Literal containers that
//! do not pin the changed behavior must still fail closed.

use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"flags\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"vitest\": \"^1.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"vitest run\"\n  }\n}\n";

const PARSER_TS: &str = "export function parseFlags(args: string[], limit: number): { _: string[]; name: string; admin: boolean } {\n  const result = { _: [] as string[], name: \"John\", admin: false };\n  if (args.length >= limit) {\n    result.admin = true;\n  }\n  return result;\n}\n";

const PARSER_DIFF: &str = "diff --git a/src/parser.ts b/src/parser.ts\nindex 0000000..1111111 100644\n--- a/src/parser.ts\n+++ b/src/parser.ts\n@@ -1,7 +1,7 @@\n export function parseFlags(args: string[], limit: number): { _: string[]; name: string; admin: boolean } {\n   const result = { _: [] as string[], name: \"John\", admin: false };\n-  if (args.length > limit) {\n+  if (args.length >= limit) {\n     result.admin = true;\n   }\n   return result;\n }\n";

fn test_source(body: &str) -> String {
    format!(
        "import {{ expect, test }} from \"vitest\";\nimport {{ parseFlags }} from \"../src/parser\";\n\ntest(\"parses flags\", () => {{\n{body}\n}});\n"
    )
}

struct Check {
    finding: Value,
}

impl Check {
    fn class(&self) -> Option<&str> {
        self.finding.get("classification").and_then(Value::as_str)
    }

    fn evidence(&self) -> Vec<&str> {
        self.finding
            .get("evidence")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    }

    fn dynamic_limit(&self) -> bool {
        self.evidence()
            .contains(&"typescript_limitation: typescript_dynamic_assertion_unresolved")
    }

    fn has_expected(&self) -> bool {
        self.evidence()
            .iter()
            .any(|line| line.starts_with("typescript_oracle_expected:"))
    }

    fn repair_packet_ready(&self) -> Option<bool> {
        self.finding
            .get("preview_actionability")
            .and_then(|actionability| actionability.get("repair_packet_ready"))
            .and_then(Value::as_bool)
    }
}

fn check_typescript(label: &str, body: &str) -> Result<Check, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-typescript-container-{label}-{}-{stamp}",
        std::process::id()
    ));
    let test = test_source(body);
    let output = (|| -> Result<String, String> {
        for (path, contents) in [
            ("package.json", PACKAGE_JSON),
            ("ripr.toml", TYPESCRIPT_CONFIG),
            ("src/parser.ts", PARSER_TS),
            ("tests/parser.test.ts", test.as_str()),
            ("diff.patch", PARSER_DIFF),
        ] {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
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
    let rendered = output?;
    cleanup?;
    let value: Value =
        serde_json::from_str(&rendered).map_err(|error| format!("parse check output: {error}"))?;
    match value.get("findings").and_then(Value::as_array) {
        Some(findings) if findings.len() == 1 => Ok(Check {
            finding: findings[0].clone(),
        }),
        other => Err(format!("expected one finding, got {other:?}")),
    }
}

/// The citty `parser.test.ts:11` shape: an object whose `_: []` property used
/// to flip the whole expectation to "a variable or computed value". It is a
/// literal now: no dynamic limit, an expected value is recorded, and the
/// finding is decided by the classifier.
#[test]
fn citty_shaped_literal_object_is_a_concrete_expected_value() -> Result<(), String> {
    let check = check_typescript(
        "citty",
        "  const result = parseFlags([\"a\", \"b\"], 2);\n  expect(result).toEqual({ _: [], name: \"John\", admin: true });",
    )?;
    if check.dynamic_limit() || !check.has_expected() {
        return Err(format!("literal object stayed dynamic: {}", check.finding));
    }
    Ok(())
}

/// Control for the cases above and below: the same expectation held in a
/// variable is still dynamic, so the positive case really moved.
#[test]
fn identifier_in_the_expected_object_stays_dynamic() -> Result<(), String> {
    let check = check_typescript(
        "identifier",
        "  const admin = true;\n  const result = parseFlags([\"a\", \"b\"], 2);\n  expect(result).toEqual({ _: [], name: \"John\", admin });",
    )?;
    if !check.dynamic_limit() || check.has_expected() || check.repair_packet_ready() == Some(true) {
        return Err(format!(
            "shorthand identifier was credited: {}",
            check.finding
        ));
    }
    Ok(())
}

/// Discriminating negative: a literal container that does not pin the changed
/// behavior earns no exposure. The call below never reaches the changed
/// boundary (`args.length === limit`), so a concrete literal must not turn the
/// finding into an exposed one.
#[test]
fn literal_container_off_the_boundary_does_not_expose_the_change() -> Result<(), String> {
    let check = check_typescript(
        "off-boundary",
        "  const result = parseFlags([\"a\"], 5);\n  expect(result).toEqual({ _: [], name: \"John\", admin: false });",
    )?;
    if check.class() == Some("exposed") {
        return Err(format!(
            "an off-boundary literal object exposed the change: {}",
            check.finding
        ));
    }
    Ok(())
}
