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
    // A describe-level `impls` hides the file table from the loop. The file
    // table's text scan already refuses it (the second declaration is a
    // mention that is not an enumeration); the scope guard is the backstop.
    let hidden = format!(
        "{HEADER}{TABLE}\ndescribe(\"inner\", () => {{\n  const impls = {{ js: {{ digest: (s: string) => s }} }};\n  for (const [name, {{ digest }}] of Object.entries(impls)) {{\n    it(name, () => {{\n{BODY}    }});\n  }}\n}});\n"
    );
    assert_unreached("bind-hidden", &pad_finding("bind-hidden", &hidden)?);
    Ok(())
}

/// A loop body the same iteration by iteration, as `const` per iteration.
fn loop_test(header: &str, body: &str) -> String {
    format!("{HEADER}{TABLE}\n{header} {{\n{body}}}\n")
}

const ENTRIES: &str = "for (const [name, { digest }] of Object.entries(impls))";
const PLAIN: &str = "  it(name, () => {\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n";

#[test]
fn only_a_const_binding_each_iteration_runs_lends_reach() -> Result<(), String> {
    assert_reached(
        "iter-twin",
        &pad_finding("iter-twin", &loop_test(ENTRIES, PLAIN))?,
    );
    // `var`: one binding, read by every test after the loop ends, so every
    // test runs the last entry's `digest`.
    let shared = loop_test(
        "for (var [name, { digest }] of Object.entries(impls))",
        PLAIN,
    );
    assert_unreached("iter-var", &pad_finding("iter-var", &shared)?);
    // `let` reassigned in the body: no test runs either export.
    let reassigned = loop_test(
        "for (let [name, { digest }] of Object.entries(impls))",
        &format!("  digest = (s: string) => s + \"=\";\n{PLAIN}"),
    );
    assert_unreached("iter-let", &pad_finding("iter-let", &reassigned)?);
    Ok(())
}

#[test]
fn a_loop_that_may_skip_an_iteration_lends_no_reach() -> Result<(), String> {
    assert_reached(
        "skip-twin",
        &pad_finding("skip-twin", &loop_test(ENTRIES, PLAIN))?,
    );
    let skipped = loop_test(
        ENTRIES,
        &format!("  if (name === \"js\") continue;\n{PLAIN}"),
    );
    assert_unreached("skip-continue", &pad_finding("skip-continue", &skipped)?);
    let guarded = loop_test(
        ENTRIES,
        "  if (name !== \"js\") {\n    it(name, () => {\n      expect(digest(\"a\")).toBe(\"a=\");\n    });\n  }\n",
    );
    assert_unreached("skip-if", &pad_finding("skip-if", &guarded)?);
    let short = loop_test(
        ENTRIES,
        "  name !== \"js\" && it(name, () => {\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n",
    );
    assert_unreached("skip-and", &pad_finding("skip-and", &short)?);
    // Inside a suite callback the same holds.
    let nested = loop_test(
        ENTRIES,
        "  describe(name, () => {\n    if (name === \"js\") return;\n    it(\"digest\", () => {\n      expect(digest(\"a\")).toBe(\"a=\");\n    });\n  });\n",
    );
    assert_unreached("skip-return", &pad_finding("skip-return", &nested)?);
    // A test body that reads the key can skip its own call.
    let body = loop_test(
        ENTRIES,
        "  it(\"digest\", () => {\n    if (name === \"js\") return;\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n",
    );
    assert_unreached("skip-body", &pad_finding("skip-body", &body)?);
    // A member callee can skip by its own arguments.
    let member = loop_test(
        ENTRIES,
        "  describe.skipIf(name === \"js\")(name, () => {\n    it(\"digest\", () => {\n      expect(digest(\"a\")).toBe(\"a=\");\n    });\n  });\n",
    );
    assert_unreached("skip-member", &pad_finding("skip-member", &member)?);
    // The key titling a suite or a test is not a skip.
    let titled = loop_test(
        ENTRIES,
        "  describe(`impl ${name}`, () => {\n    it(name, () => {\n      expect(digest(\"a\")).toBe(\"a=\");\n    });\n  });\n",
    );
    assert_reached("skip-titled", &pad_finding("skip-titled", &titled)?);
    // Vitest's options argument can skip by the key.
    let options = loop_test(
        ENTRIES,
        "  it(name, { skip: name === \"js\" }, () => {\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n",
    );
    assert_unreached("skip-options", &pad_finding("skip-options", &options)?);
    let suite_options = loop_test(
        ENTRIES,
        "  describe(name, { skip: name === \"js\" }, () => {\n    it(\"digest\", () => {\n      expect(digest(\"a\")).toBe(\"a=\");\n    });\n  });\n",
    );
    assert_unreached(
        "skip-suite-options",
        &pad_finding("skip-suite-options", &suite_options)?,
    );
    Ok(())
}

#[test]
fn a_test_that_reads_another_loop_member_lends_no_reach() -> Result<(), String> {
    let guard = "  it(\"digest\", () => {\n    if (!supported) return;\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n";
    let unread = "  it(\"digest\", () => {\n    expect(digest(\"a\")).toBe(\"a=\");\n  });\n";
    for (label, header) in [
        (
            "entries",
            "for (const [name, { digest, supported }] of Object.entries(impls))",
        ),
        (
            "values",
            "for (const { digest, supported } of Object.values(impls))",
        ),
    ] {
        // A member the tests never read takes nothing away.
        assert_reached(
            &format!("member-twin-{label}"),
            &pad_finding(&format!("member-twin-{label}"), &loop_test(header, unread))?,
        );
        assert_unreached(
            &format!("member-guard-{label}"),
            &pad_finding(&format!("member-guard-{label}"), &loop_test(header, guard))?,
        );
    }
    Ok(())
}

#[test]
fn a_type_only_wrapper_keeps_the_table() -> Result<(), String> {
    for (label, table) in [
        (
            "as-const",
            "const impls = { js: cryptoJS, node: cryptoNode } as const;\n",
        ),
        (
            "satisfies",
            "const impls = { js: cryptoJS, node: cryptoNode } satisfies Record<string, unknown>;\n",
        ),
    ] {
        assert_reached(label, &pad_finding(label, &entries_test(table, BODY))?);
    }
    Ok(())
}

#[test]
fn an_exported_table_or_a_local_object_lends_no_reach() -> Result<(), String> {
    assert_reached(
        "bound-twin",
        &pad_finding("bound-twin", &loop_test(ENTRIES, PLAIN))?,
    );
    // Another file can import an exported table and swap an entry.
    let exported = format!("{HEADER}export {TABLE}\n{ENTRIES} {{\n{PLAIN}}}\n");
    assert_unreached("bound-exported", &pad_finding("bound-exported", &exported)?);
    // `Object.entries` must be the global.
    let local = format!(
        "{HEADER}const Object = {{ entries: (o: object) => [[\"js\", {{ digest: (s: string) => s + \"=\" }}]] }};\n{TABLE}\n{ENTRIES} {{\n{PLAIN}}}\n"
    );
    assert_unreached("bound-object", &pad_finding("bound-object", &local)?);
    Ok(())
}
