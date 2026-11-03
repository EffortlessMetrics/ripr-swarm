//! End-to-end pins for the single-token clause rule (#11442): a touched
//! comparison is named only when exactly one token was substituted in place in
//! a site the old line already had. Each case runs the production `check` path
//! on an on-disk workspace, so the discriminator and the repair-packet decision
//! come from the real producer. Two tokens in one clause and an added
//! comparison clause must not start a repair.

use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"gate\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"vitest\": \"^1.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"vitest run\"\n  }\n}\n";

const TEST_TS: &str = "import { expect, test } from \"vitest\";\nimport { gate } from \"../src/gate\";\n\ntest(\"gates\", () => {\n  expect(gate(0, 0, true)).toBeDefined();\n});\n";

fn source(condition: &str) -> String {
    format!(
        "export function gate(a: number, b: number, ok: boolean): boolean {{\n  if ({condition}) {{\n    return true;\n  }}\n  return false;\n}}\n"
    )
}

fn diff(old: &str, new: &str) -> String {
    format!(
        "diff --git a/src/gate.ts b/src/gate.ts\nindex 0000000..1111111 100644\n--- a/src/gate.ts\n+++ b/src/gate.ts\n@@ -1,6 +1,6 @@\n export function gate(a: number, b: number, ok: boolean): boolean {{\n-  if ({old}) {{\n+  if ({new}) {{\n     return true;\n   }}\n   return false;\n }}\n"
    )
}

struct Check {
    finding: Value,
}

impl Check {
    fn missing(&self) -> Vec<&str> {
        self.finding
            .get("activation")
            .and_then(|activation| activation.get("missing_discriminators"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| row.get("value").and_then(Value::as_str))
            .collect()
    }

    fn packet_ready(&self) -> bool {
        self.finding
            .get("preview_actionability")
            .and_then(|actionability| actionability.get("repair_packet_ready"))
            .and_then(Value::as_bool)
            == Some(true)
    }

    fn class(&self) -> Option<&str> {
        self.finding.get("classification").and_then(Value::as_str)
    }

    fn names_discriminator(&self, value: &str) -> bool {
        self.missing().iter().any(|missing| missing.contains(value))
    }

    fn names_any_comparison(&self) -> bool {
        self.missing()
            .iter()
            .any(|missing| missing.contains(" == "))
    }
}

fn check_typescript(label: &str, old: &str, new: &str) -> Result<Check, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-typescript-clause-{label}-{}-{stamp}",
        std::process::id()
    ));
    let src = source(new);
    let patch = diff(old, new);
    let output = (|| -> Result<String, String> {
        for (path, contents) in [
            ("package.json", PACKAGE_JSON),
            ("ripr.toml", TYPESCRIPT_CONFIG),
            ("src/gate.ts", src.as_str()),
            ("tests/gate.test.ts", TEST_TS),
            ("diff.patch", patch.as_str()),
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

/// Positive control: a single operator token substituted in place names that
/// clause, so the harness really reaches a discriminator.
///
/// `repair_packet_ready` is not asserted here: the TypeScript preview is
/// `preview_advisory_only` and never emits a ready packet, so the negatives'
/// `packet_ready()` check is a regression tripwire, and the discriminating
/// signal is that a named `==` discriminator appears (positives) or not
/// (negatives).
#[test]
fn single_operator_substitution_names_its_clause() -> Result<(), String> {
    let check = check_typescript("operator", "a > 1 && ok", "a >= 1 && ok")?;
    if !check.names_discriminator("a == 1") {
        return Err(format!("expected `a == 1`: {}", check.finding));
    }
    Ok(())
}

/// Positive control: a single literal token substituted in place.
#[test]
fn single_literal_substitution_names_its_clause() -> Result<(), String> {
    let check = check_typescript("literal", "a > 1 && ok", "a > 2 && ok")?;
    if !check.names_discriminator("a == 2") {
        return Err(format!("expected `a == 2`: {}", check.finding));
    }
    Ok(())
}

/// #11442 case 1: operator and literal changed in one clause. No boundary is
/// named, no repair packet starts, and nothing is exposed.
#[test]
fn two_tokens_in_one_clause_start_no_repair() -> Result<(), String> {
    let check = check_typescript("two-tokens", "a > 1 && ok", "a >= 2 && ok")?;
    if check.names_any_comparison() || check.packet_ready() || check.class() == Some("exposed") {
        return Err(format!("two-token edit named a clause: {}", check.finding));
    }
    Ok(())
}

/// #11442 case 2: an added comparison clause. The suggested call could not
/// reach it past the untouched `a > 1`, so no boundary is named.
#[test]
fn added_comparison_clause_starts_no_repair() -> Result<(), String> {
    let check = check_typescript("added", "a > 1", "a > 1 && b < 2")?;
    if check.names_any_comparison() || check.packet_ready() || check.class() == Some("exposed") {
        return Err(format!("added clause named a boundary: {}", check.finding));
    }
    Ok(())
}

/// Positive control: a pure operand swap with the mirrored operator is the same
/// boundary written the other way round, so it keeps naming its clause.
#[test]
fn pure_operand_swap_names_its_clause() -> Result<(), String> {
    let check = check_typescript("swap", "total >= 100 && ok", "100 <= total && ok")?;
    if !check.names_discriminator("100 == total") {
        return Err(format!("expected `100 == total`: {}", check.finding));
    }
    Ok(())
}

/// Discriminating negative: exchanging operands without mirroring the operator
/// flips the predicate and must not name a boundary.
#[test]
fn unmirrored_operand_exchange_starts_no_repair() -> Result<(), String> {
    let check = check_typescript("flip", "total >= 100 && ok", "100 >= total && ok")?;
    if check.names_any_comparison() || check.packet_ready() || check.class() == Some("exposed") {
        return Err(format!("flipped swap named a clause: {}", check.finding));
    }
    Ok(())
}

/// Positive control: the swap is the whole edit, so an unchanged neighbour
/// clause does not stop it naming its clause.
#[test]
fn pure_operand_swap_beside_an_unchanged_clause_names_its_clause() -> Result<(), String> {
    let check = check_typescript("swap-beside", "a > 1 && b < 2", "1 < a && b < 2")?;
    if !check.names_discriminator("1 == a") {
        return Err(format!("expected `1 == a`: {}", check.finding));
    }
    Ok(())
}

/// Discriminating negative: a swap plus an edit to another clause is not the
/// whole edit, so it names no boundary.
#[test]
fn swap_plus_neighbour_clause_edit_starts_no_repair() -> Result<(), String> {
    let check = check_typescript("swap-plus", "a > 1 && b < 2", "1 < a && b < 3")?;
    if check.names_any_comparison() || check.packet_ready() || check.class() == Some("exposed") {
        return Err(format!("swap plus edit named a clause: {}", check.finding));
    }
    Ok(())
}
