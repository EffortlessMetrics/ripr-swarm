//! A bare `p.then((v) => assert...)` statement fails the run under
//! `node:test`, which reports an unhandled rejection as a failure, so its
//! assertion supplies a discriminator there. Jest and Vitest drop a rejection
//! that settles after the test finished, so the same shape stays weak
//! (#9686).

use super::*;

const TIER: &str = "export function loyaltyTier(points: number): Promise<string> {\n  if (1000 <= points) {\n    return Promise.resolve(\"gold\");\n  }\n  return Promise.resolve(\"silver\");\n}\n";
const TIER_LINE: (usize, &str) = (2, "  if (1000 <= points) {");

fn node_test(body: &str) -> String {
    format!(
        "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ loyaltyTier }} from \"../src/tier\";\n\ntest(\"tiers\", () => {{\n{body}\n}});\n"
    )
}

fn jest_test(body: &str) -> String {
    format!(
        "import {{ loyaltyTier }} from \"../src/tier\";\n\ntest(\"tiers\", () => {{\n{body}\n}});\n"
    )
}

const FLOATING: &str = "  loyaltyTier(1000).then((tier) => {\n    assert.equal(tier, \"gold\");\n  });\n  loyaltyTier(999).then((tier) => {\n    assert.equal(tier, \"silver\");\n  });";

fn tier_class(label: &str, source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join("tests/tier.test.ts"), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == TIER_LINE.0)
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn assertion_count(source: &str) -> usize {
    extract_tests(Path::new("tests/tier.test.ts"), source)
        .iter()
        .map(|test| test.assertions.len())
        .sum()
}

#[test]
fn a_floating_then_assertion_counts_under_node_test() -> Result<(), String> {
    let source = node_test(FLOATING);
    assert_eq!(assertion_count(&source), 2);
    assert_eq!(
        tier_class("node-floating", &source)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn the_same_statement_collects_nothing_outside_node_test() {
    let source = jest_test(
        &FLOATING
            .replace(
                "assert.equal(tier, \"gold\")",
                "expect(tier).toBe(\"gold\")",
            )
            .replace(
                "assert.equal(tier, \"silver\")",
                "expect(tier).toBe(\"silver\")",
            ),
    );
    assert_eq!(assertion_count(&source), 0);
}

#[test]
fn a_chain_that_can_replace_the_value_stays_refused_under_node_test() {
    for body in [
        "  loyaltyTier(1000).then((x) => x).then((tier) => {\n    assert.equal(tier, \"gold\");\n  });",
        "  loyaltyTier(1000).catch(() => \"gold\").then((tier) => {\n    assert.equal(tier, \"gold\");\n  });",
    ] {
        assert_eq!(assertion_count(&node_test(body)), 0, "{body}");
    }
}
