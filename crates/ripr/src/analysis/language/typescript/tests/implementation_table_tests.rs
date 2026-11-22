//! One suite run over several implementations of a module (#13592, unjs/ohash
//! `test/crypto.test.ts`): a loop destructures the export from each entry of
//! a file-level table of namespace imports and calls it by its bare name.
//!
//! Each iteration runs the namespace member call (`cryptoJS.digest(...)`), so
//! the test relates to the export as an imported owner call, and to what the
//! export reaches through the existing module-entry reach. Every negative
//! asserts its positive twin first, so the refusal is the single changed
//! element and not a parse accident.

use super::*;

type Files<'a> = Vec<(&'a str, String)>;

const JS: &str = "function pad(s: string): string {\n  return s + \"=\";\n}\n\nexport function digest(message: string): string {\n  return pad(message);\n}\n";
const NODE: &str =
    "export function digest(message: string): string {\n  return message + \"=\";\n}\n";
const PAD_LINE: &str = "  return s + \"=\";";
const DIGEST_LINE: &str = "  return pad(message);";

const HEADER: &str = "import { describe, expect, it } from \"vitest\";\nimport * as cryptoJS from \"../src/crypto/js\";\nimport * as cryptoNode from \"../src/crypto/node\";\n\n";

/// ohash's shape: an object table, `Object.entries`, `[name, { digest }]`.
fn entries_test(table: &str, body: &str) -> String {
    format!(
        "{HEADER}{table}\ndescribe(\"crypto:digest\", () => {{\n  for (const [name, {{ digest }}] of Object.entries(impls)) {{\n    describe(name, () => {{\n      it(\"digest\", () => {{\n{body}      }});\n    }});\n  }}\n}});\n"
    )
}

const TABLE: &str = "const impls = { js: cryptoJS, node: cryptoNode };\n";
const BODY: &str = "        expect(digest(\"a\")).toBe(\"a=\");\n";

fn finding(
    label: &str,
    test_source: &str,
    changed_line: &str,
    owner: &str,
) -> Result<Finding, String> {
    let files: Files<'_> = vec![
        ("src/crypto/js.ts", JS.to_string()),
        ("src/crypto/node.ts", NODE.to_string()),
        ("test/crypto.test.ts", test_source.to_string()),
    ];
    let root = ts_unique_tempdir(label)?;
    for (path, source) in &files {
        ts_write_file(&root.join(path), source)?;
    }
    let line = JS
        .lines()
        .position(|line| line == changed_line)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing"))?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "src/crypto/js.ts",
            &[(line, changed_line)],
        )],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let suffix = format!("::{owner}");
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|candidate| candidate.0.ends_with(&suffix))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner}`"))
}

fn pad_finding(label: &str, test_source: &str) -> Result<Finding, String> {
    finding(label, test_source, PAD_LINE, "pad")
}

fn relation(finding: &Finding, reason: crate::domain::RelationReason) -> bool {
    finding
        .related_tests
        .iter()
        .any(|test| test.relation_reason == Some(reason))
}

/// The helper the export bare-calls is reached through the module entry,
/// as the existing same-module reach reads it: related, never `exposed`.
fn assert_reached(label: &str, finding: &Finding) {
    assert!(
        relation(finding, crate::domain::RelationReason::HelperOwnerCall),
        "{label}: the loop's call of `digest` reaches `pad`, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
}

fn assert_unreached(label: &str, finding: &Finding) {
    assert!(
        finding.related_tests.is_empty(),
        "{label}: related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
}

#[test]
fn the_export_a_table_loop_calls_is_an_imported_owner_call() -> Result<(), String> {
    let finding = finding(
        "table-digest",
        &entries_test(TABLE, BODY),
        DIGEST_LINE,
        "digest",
    )?;
    assert!(
        finding.related_tests.iter().any(|test| {
            test.relation_reason == Some(crate::domain::RelationReason::ImportPathAffinity)
                && test.oracle_strength == OracleStrength::Strong
        }),
        "related: {:?}",
        finding.related_tests
    );
    Ok(())
}

#[test]
fn a_helper_the_looped_export_calls_is_reached_through_the_entry() -> Result<(), String> {
    assert_reached(
        "table-entries",
        &pad_finding("table-entries", &entries_test(TABLE, BODY))?,
    );
    // `Object.values` with `{ digest }`, and an array table iterated directly.
    let values = format!(
        "{HEADER}{TABLE}\nfor (const {{ digest }} of Object.values(impls)) {{\n  it(\"digest\", () => {{\n{BODY}  }});\n}}\n"
    );
    assert_reached("table-values", &pad_finding("table-values", &values)?);
    let array = format!(
        "{HEADER}const impls = [cryptoJS, cryptoNode];\n\nfor (const {{ digest }} of impls) {{\n  it(\"digest\", () => {{\n{BODY}  }});\n}}\n"
    );
    assert_reached("table-array", &pad_finding("table-array", &array)?);
    // Shorthand table keys name the namespaces themselves.
    let shorthand = entries_test("const impls = { cryptoJS, cryptoNode };\n", BODY);
    assert_reached(
        "table-shorthand",
        &pad_finding("table-shorthand", &shorthand)?,
    );
    Ok(())
}

#[test]
fn a_table_written_anywhere_else_lends_no_reach() -> Result<(), String> {
    assert_reached(
        "write-twin",
        &pad_finding("write-twin", &entries_test(TABLE, BODY))?,
    );
    // An entry swapped before the loop runs: the loop may call the fake.
    let swapped = entries_test(
        &format!("{TABLE}impls.js = {{ digest: (s: string) => s + \"=\" }} as never;\n"),
        BODY,
    );
    assert_unreached("write-swapped", &pad_finding("write-swapped", &swapped)?);
    // Passed elsewhere first: the callee may mutate it.
    let passed = entries_test(&format!("{TABLE}Object.freeze(impls);\n"), BODY);
    assert_unreached("write-passed", &pad_finding("write-passed", &passed)?);
    Ok(())
}

#[test]
fn a_table_without_the_owner_module_lends_no_reach() -> Result<(), String> {
    assert_reached(
        "module-twin",
        &pad_finding("module-twin", &entries_test(TABLE, BODY))?,
    );
    let other = entries_test("const impls = { node: cryptoNode };\n", BODY);
    assert_unreached("module-other", &pad_finding("module-other", &other)?);
    // A value that is not a namespace import: `{ digest }` reads a member of
    // an object the test built, which the table cannot vouch for.
    let built = entries_test(
        "const fake = { digest: (s: string) => s + \"=\" };\nconst impls = { js: fake };\n",
        BODY,
    );
    assert_unreached("module-built", &pad_finding("module-built", &built)?);
    Ok(())
}

#[test]
fn a_renamed_or_shadowed_binding_lends_no_reach() -> Result<(), String> {
    assert_reached(
        "bind-twin",
        &pad_finding("bind-twin", &entries_test(TABLE, BODY))?,
    );
    // `{ digest: run }` reads `digest` into `run`; the member is not the
    // bound name, so the call is not matched.
    let renamed = format!(
        "{HEADER}{TABLE}\nfor (const [name, {{ digest: run }}] of Object.entries(impls)) {{\n  it(name, () => {{\n    expect(run(\"a\")).toBe(\"a=\");\n  }});\n}}\n"
    );
    assert_unreached("bind-renamed", &pad_finding("bind-renamed", &renamed)?);
    // A body-local `digest` is the one the call runs.
    let shadowed = entries_test(
        TABLE,
        "        const digest = (s: string) => s + \"=\";\n        expect(digest(\"a\")).toBe(\"a=\");\n",
    );
    assert_unreached("bind-shadowed", &pad_finding("bind-shadowed", &shadowed)?);
    // A describe-level `impls` hides the file table from the loop.
    let hidden = format!(
        "{HEADER}{TABLE}\ndescribe(\"inner\", () => {{\n  const impls = {{ js: {{ digest: (s: string) => s }} }};\n  for (const [name, {{ digest }}] of Object.entries(impls)) {{\n    it(name, () => {{\n{BODY}    }});\n  }}\n}});\n"
    );
    assert_unreached("bind-hidden", &pad_finding("bind-hidden", &hidden)?);
    Ok(())
}
