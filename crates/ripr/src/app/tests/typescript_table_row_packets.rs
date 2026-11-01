//! End-to-end pins for object-row table expected values (#11196): each case
//! runs the production `check` path on an on-disk workspace, so the
//! dynamic-assertion limitation and the repair-packet decision come from the
//! real producer. A table that is not provably a never-touched all-literal
//! `const` stays dynamic, and a resolved table that never reaches the changed
//! boundary must not expose the change.

use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"flags\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"vitest\": \"^1.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"vitest run\"\n  }\n}\n";

const TIER_TS: &str = "export function tier(points: number): string {
  if (points > 1000) {
    return \"gold\";
  }
  return \"silver\";
}
";

const TIER_DIFF: &str = "diff --git a/src/tier.ts b/src/tier.ts
index 0000000..1111111 100644
--- a/src/tier.ts
+++ b/src/tier.ts
@@ -1,5 +1,5 @@
 export function tier(points: number): string {
-  if (points > 1000) {
+  if (points >= 1000) {
     return \"gold\";
   }
   return \"silver\";
 }
";

fn test_source(body: &str) -> String {
    format!(
        "import {{ describe, expect, test }} from \"vitest\";
import {{ tier }} from \"../src/tier\";

{body}
"
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
        "ripr-typescript-table-row-{label}-{}-{stamp}",
        std::process::id()
    ));
    let test = test_source(body);
    let output = (|| -> Result<String, String> {
        for (path, contents) in [
            ("package.json", PACKAGE_JSON),
            ("ripr.toml", TYPESCRIPT_CONFIG),
            ("src/tier.ts", TIER_TS),
            ("tests/tier.test.ts", test.as_str()),
            ("diff.patch", TIER_DIFF),
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

const GOLD_ROW: &str = "{ input: 1000, out: \"gold\" }";

fn loop_source(table_extra: &str, loop_head: &str, cases: &str) -> String {
    format!(
        "describe(\"tier\", () => {{\n  const cases = [\n    {{ input: 999, out: \"silver\" }},\n    {cases},\n  ];\n{table_extra}  {loop_head} {{\n    test(\"tier by points\", () => {{\n      expect(tier(t.input)).toBe(t.out);\n    }});\n  }}\n}});"
    )
}

/// The dynamic-assertion refusal is gone: the expected value is the row set's
/// literal, so the existing classifier decides the finding.
#[test]
fn const_object_row_table_resolves_the_expected_value() -> Result<(), String> {
    let check = check_typescript("loop", &loop_source("", "for (const t of cases)", GOLD_ROW))?;
    if check.dynamic_limit() || !check.has_expected() {
        return Err(format!(
            "object-row table stayed dynamic: {}",
            check.finding
        ));
    }
    Ok(())
}

#[test]
fn inline_each_object_rows_resolve_the_expected_value() -> Result<(), String> {
    let check = check_typescript(
        "each",
        "describe(\"tier\", () => {\n  test.each([\n    { input: 999, out: \"silver\" },\n    { input: 1000, out: \"gold\" },\n  ])(\"tier by points\", (t) => {\n    expect(tier(t.input)).toBe(t.out);\n  });\n});",
    )?;
    if check.dynamic_limit() || !check.has_expected() {
        return Err(format!("inline rows stayed dynamic: {}", check.finding));
    }
    Ok(())
}

/// Discriminating negatives: each keeps today's refusal, and none may expose
/// or ready a repair start.
#[test]
fn touched_partial_or_derived_tables_keep_the_dynamic_refusal() -> Result<(), String> {
    for (label, source) in [
        (
            "pushed",
            loop_source(
                "  cases.push({ input: 1000, out: \"silver\" });\n",
                "for (const t of cases)",
                GOLD_ROW,
            ),
        ),
        (
            "missing-key",
            loop_source("", "for (const t of cases)", "{ input: 1000 }"),
        ),
        (
            "identifier-cell",
            loop_source("", "for (const t of cases)", "{ input: 1000, out: gold }"),
        ),
        (
            "spread-row",
            loop_source("", "for (const t of cases)", "{ ...base, input: 1000 }"),
        ),
        (
            "derived",
            loop_source("", "for (const t of cases.map((row) => row))", GOLD_ROW),
        ),
        (
            "mutated-container",
            "describe(\"tier\", () => {\n  const cases = [\n    { input: { points: 999 }, out: \"silver\" },\n    { input: { points: 1000 }, out: \"silver\" },\n  ];\n  for (const t of cases) {\n    test(\"tier by points\", () => {\n      Object.assign(t.input, { points: 5 });\n      expect(tier(t.input)).toBe(t.out);\n    });\n  }\n});"
                .to_string(),
        ),
        (
            "each-arguments-alias",
            "describe(\"tier\", () => {\n  test.each([\n    { input: 999, out: \"silver\" },\n    { input: 1000, out: \"gold\" },\n  ])(\"tier\", function (t) {\n    expect(tier(t.input, arguments[0].out = tier(0))).toBe(t.out);\n  });\n});"
                .to_string(),
        ),
        (
            "array-loop-eval-rewrite",
            "describe(\"tier\", () => {\n  test(\"tiers\", () => {\n    const rows = [[999, \"silver\"], [1000, \"gold\"]];\n    eval(\"ro\" + \"ws[1][1] = tier(0)\");\n    for (const [points, want] of rows) {\n      expect(tier(points)).toBe(want);\n    }\n  });\n});"
                .to_string(),
        ),
        (
            "each-regex-hidden-eval",
            "describe(\"tier\", () => {\n  test.each([\n    { input: 999, out: \"silver\" },\n    { input: 1000, out: \"gold\" },\n  ])(typeof /'a/, (t) => {\n    expect(tier(t.input, eval(\"t.out = tier(0)\"), \"it's\")).toBe(t.out);\n  }, typeof /\"a/);\n});"
                .to_string(),
        ),
        (
            "each-eval-write",
            "describe(\"tier\", () => {\n  test.each([\n    { input: 999, out: \"silver\" },\n    { input: 1000, out: \"gold\" },\n  ])(\"tier\", (t) => {\n    expect(tier(t.input, eval(\"t.out = tier(0)\"))).toBe(t.out);\n  });\n});"
                .to_string(),
        ),
        (
            "deleted-scalar",
            "describe(\"tier\", () => {\n  const cases = [\n    { input: 999, out: \"silver\" },\n    { input: 1000, out: \"gold\" },\n  ];\n  for (const t of cases) {\n    test(\"tier by points\", () => {\n      expect((delete t.out, tier(t.input))).toBe(t.out);\n    });\n  }\n});"
                .to_string(),
        ),
        (
            "mutating-title",
            "describe(\"tier\", () => {\n  const cases = [\n    { input: { points: 999 }, out: \"silver\" },\n    { input: { points: 1000 }, out: \"silver\" },\n  ];\n  for (const t of cases) {\n    test(String(Object.assign(t.input, { points: 5 })), () => {\n      expect(tier(t.input)).toBe(t.out);\n    });\n  }\n});"
                .to_string(),
        ),
        (
            "aliased-keys",
            "describe(\"tier\", () => {\n  test.each([\n    { input: [1000], out: \"gold\" },\n    { input: [5], out: \"silver\" },\n  ])(\"tier\", ({ input: a, input: b, out }) => {\n    expect(tier(a)).toBe(out);\n  });\n});"
                .to_string(),
        ),
        (
            "aliased",
            loop_source(
                "  const same = cases;\n",
                "for (const t of cases)",
                GOLD_ROW,
            ),
        ),
    ] {
        let check = check_typescript(label, &source)?;
        if check.has_expected()
            || check.class() == Some("exposed")
            || check.repair_packet_ready() == Some(true)
        {
            return Err(format!("{label} was credited: {}", check.finding));
        }
    }
    Ok(())
}

/// A clean all-literal table whose rows never reach the changed boundary
/// resolves its literals but must not turn the finding into an exposed one.
#[test]
fn resolved_table_off_the_boundary_does_not_expose_the_change() -> Result<(), String> {
    let check = check_typescript(
        "off-boundary",
        "describe(\"tier\", () => {\n  const cases = [\n    { input: 100, out: \"silver\" },\n    { input: 5000, out: \"gold\" },\n  ];\n  for (const t of cases) {\n    test(\"tier by points\", () => {\n      expect(tier(t.input)).toBe(t.out);\n    });\n  }\n});",
    )?;
    if check.class() == Some("exposed") || check.repair_packet_ready() == Some(true) {
        return Err(format!(
            "an off-boundary table exposed the change: {}",
            check.finding
        ));
    }
    Ok(())
}
