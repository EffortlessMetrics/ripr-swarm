//! RIPR-SPEC-0246 rule 1 (a copy-for-identity change needs an identity
//! matcher) and rule 2 (literal `forEach`, returned or awaited `.then` and
//! awaited `t.test` callbacks are read as the test's own assertions). Each
//! test pins the parsed assertions before the class.

use super::*;

fn observed(path: &str, source: &str) -> Vec<(Option<String>, Option<String>, usize)> {
    extract_tests(Path::new(path), source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| {
            (
                assertion.observed_expression.clone(),
                assertion.expected_value_or_variant.clone(),
                assertion.line,
            )
        })
        .collect()
}

fn some(observed: &str, expected: &str, line: usize) -> (Option<String>, Option<String>, usize) {
    (Some(observed.to_string()), Some(expected.to_string()), line)
}

#[test]
fn literal_foreach_reads_one_case_per_element() {
    let source = "import { price } from \"../src/price\";\ntest(\"price\", () => {\n  [150, 200].forEach((v) => { expect(price(v)).toBe(140); });\n});\n";
    assert_eq!(
        observed("tests/price.test.ts", source),
        vec![some("price(150)", "140", 3), some("price(200)", "140", 3)]
    );
}

/// A non-literal array, a callback with two parameters or a parameter read
/// outside parentheses collects nothing (fail closed).
#[test]
fn other_foreach_forms_collect_nothing() {
    for body in [
        "  cases.forEach((v) => { expect(price(v)).toBe(140); });",
        "  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  [150].forEach((v, i) => { expect(price(v)).toBe(i); });",
        "  [].forEach((v) => { expect(price(v)).toBe(140); });",
        "  [150].map((v) => { expect(price(v)).toBe(140); });",
        "  [150].forEach(async (v) => { await tick(); expect(price(v)).toBe(140); });",
        "  [150].forEach((v) => { return load(v).then((r) => { expect(r).toBe(2); }); });",
        "  [150].forEach(function* (v) { expect(price(v)).toBe(140); });",
        "  [-\n150].forEach((v) => { expect(price(v)).toBe(140); });",
    ] {
        let source = format!(
            "import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n{body}\n}});\n"
        );
        assert_eq!(observed("tests/price.test.ts", &source), vec![], "{body}");
    }
    let many = vec!["150"; 65].join(", ");
    let source = format!(
        "import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n  [{many}].forEach((v) => {{ expect(price(v)).toBe(140); }});\n}});\n"
    );
    assert_eq!(observed("tests/price.test.ts", &source), vec![]);
}

#[test]
fn returned_or_awaited_then_reads_the_resolved_receiver() {
    for (head, line) in [
        ("  return total([10, 20]).then((sum) => {", 3),
        ("  await total([10, 20]).then((sum) => {", 3),
    ] {
        let source = format!(
            "import {{ total }} from \"../src/order\";\ntest(\"total\", async () => {{\n{head}\n    expect(sum).toBe(30);\n  }});\n}});\n"
        );
        assert_eq!(
            observed("tests/order.test.ts", &source),
            vec![some("(total([10, 20]))", "30", line + 1)],
            "{head}"
        );
    }
    let no_parameter = "test(\"t\", () => {\n  return ready().then(() => {\n    expect(state()).toBe(1);\n  });\n});\n";
    assert_eq!(
        observed("tests/state.test.ts", no_parameter),
        vec![some("state()", "1", 3)]
    );
}

/// A `.then` callback the runner does not wait for may run after the test
/// settles, a rejection handler can swallow the change, an earlier `.then`
/// step can replace the owner's value, and a body that re-declares a name
/// the receiver reads would change the substituted text: none is collected.
#[test]
fn floating_or_unsafe_then_collects_nothing() {
    for body in [
        "  total([10, 20]).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20]).then((sum) => {\n    expect(sum).toBe(30);\n  }, () => {});",
        "  return total([10, 20]).then(() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total(items).then((sum) => {\n    const items = [];\n    expect(sum).toBe(30);\n  });",
        "  return total(\n    [10, 20],\n  ).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20]).catch(() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20]).then (() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20])[\"then\"](() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20])[\"th\" + \"en\"](() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
        "  return total([10, 20])?.[step](() => 30).then((sum) => {\n    expect(sum).toBe(30);\n  });",
    ] {
        let source = format!("test(\"total\", () => {{\n{body}\n}});\n");
        assert_eq!(observed("tests/order.test.ts", &source), vec![], "{body}");
    }
}

#[test]
fn awaited_subtest_on_the_receiver_is_collected() {
    let source = "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { memberPrice } from \"../src/member\";\ntest(\"member\", async (t) => {\n  await t.test(\"five off\", (t2) => {\n    assert.strictEqual(memberPrice(30), 25);\n    t2.diagnostic(\"checked\");\n  });\n});\n";
    assert_eq!(
        observed("tests/member.test.ts", source),
        vec![some("memberPrice(30)", "25", 6)]
    );
    let with_options = source.replace(
        "t.test(\"five off\", ",
        "t.test(\"five off\", { timeout: 50 }, ",
    );
    assert_eq!(
        observed("tests/member.test.ts", &with_options),
        vec![some("memberPrice(30)", "25", 6)]
    );
    for body in [
        "  t.test(\"five off\", () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test(\"five off\", { skip: true }, () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test(\"five off\", { todo: true }, () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await other.test(\"five off\", () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test(() => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test(name, () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test({ timeout: 50 }, () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
        "  await t.test(\"five off\", { ...opts }, () => {\n    assert.strictEqual(memberPrice(30), 25);\n  });",
    ] {
        let source = format!(
            "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ memberPrice }} from \"../src/member\";\ntest(\"member\", async (t) => {{\n{body}\n}});\n"
        );
        assert_eq!(observed("tests/member.test.ts", &source), vec![], "{body}");
    }
}

const SETTINGS: &str = "export function ready(cfg: { ok: boolean }) {\n  return { ...cfg };\n}\n";
const SETTINGS_LINE: (usize, &str) = (2, "  return { ...cfg };");

fn settings_finding(label: &str, test: &str, removed: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/settings.ts"), SETTINGS)?;
    ts_write_file(&root.join("tests/settings.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/settings.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: SETTINGS_LINE.0,
            text: SETTINGS_LINE.1.to_string(),
            new_side_line: SETTINGS_LINE.0,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: SETTINGS_LINE.0,
            text: removed.to_string(),
            new_side_line: SETTINGS_LINE.0,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == SETTINGS_LINE.0)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

const STRUCTURAL_TEST: &str = "import { ready } from \"../src/settings\";\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toEqual(cfg);\n});\n";

#[test]
fn copy_for_identity_change_needs_an_identity_matcher() -> Result<(), String> {
    assert_eq!(
        observed("tests/settings.test.ts", STRUCTURAL_TEST),
        vec![(Some("ready(cfg)".to_string()), None, 4)]
    );
    let finding = settings_finding("identity-copy-structural", STRUCTURAL_TEST, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    let identity = "import { ready } from \"../src/settings\";\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBe(cfg);\n});\n";
    let finding = settings_finding("identity-copy-pinned", identity, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::Exposed);

    // An identity matcher on a member of the result does not pin identity.
    let member = "import { ready } from \"../src/settings\";\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg).ok).toBe(true);\n  expect(ready(cfg)).toEqual(cfg);\n});\n";
    let finding = settings_finding("identity-copy-member", member, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // Nor does a member or call read the observed text hides behind
    // brackets or optional chaining, or a pin on a literal (a branch).
    for (label, pin) in [
        (
            "identity-copy-index",
            "expect(ready(cfg)[\"ok\"]).toBe(true);",
        ),
        (
            "identity-copy-optional",
            "expect(ready(cfg)?.ok).toBe(true);",
        ),
        ("identity-copy-literal", "expect(ready(cfg)).toBe(null);"),
        (
            "identity-copy-compound",
            "expect(ready(cfg) && ready(cfg)).toBe(cfg);",
        ),
        (
            "identity-copy-string-paren",
            "expect(ready(\"(\") + f(\")\")).toBe(cfg);",
        ),
    ] {
        let test = format!(
            "import {{ ready }} from \"../src/settings\";\ntest(\"ready\", () => {{\n  const cfg = {{ ok: true }};\n  {pin}\n  expect(ready(cfg)).toEqual(cfg);\n}});\n"
        );
        let finding = settings_finding(label, &test, "  return cfg;")?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{pin}");
    }

    // A change that is not copy-only keeps the ladder verdict.
    let finding = settings_finding(
        "identity-copy-other-change",
        STRUCTURAL_TEST,
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

const STRICT_CUSTOM_IDENTITY: &str = "import { ready } from \"../src/settings\";\nexpect.extend({ toBeSame(r, e) { return { pass: r === e }; } });\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBeSame(cfg);\n});\n";

/// RIPR-SPEC-0246 rule 1 / #7924: a credited strict custom matcher compares
/// objects by identity, so a copy-for-identity change stays `exposed`. The
/// gate reads the assertion's identity property, not `exact_value` (which
/// `toEqual` shares) and not a widened matcher-name list.
#[test]
fn copy_for_identity_change_credits_strict_custom_matcher() -> Result<(), String> {
    let tests = extract_tests(Path::new("tests/settings.test.ts"), STRICT_CUSTOM_IDENTITY);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "toBeSame")
        .ok_or_else(|| "expected a toBeSame assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::ExactValue);
    assert_eq!(assertion.oracle_strength, OracleStrength::Strong);
    assert!(
        assertion.compares_reference_identity,
        "credited toBeSame must carry reference identity, got {assertion:?}"
    );
    assert!(assertion.has_dynamic_matcher_arg);

    let finding = settings_finding(
        "identity-copy-strict-custom",
        STRICT_CUSTOM_IDENTITY,
        "  return cfg;",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);

    // `toEqual` is also `exact_value` / strong and must not pin identity.
    let structural = extract_tests(Path::new("tests/settings.test.ts"), STRUCTURAL_TEST);
    let to_equal = structural
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "toEqual")
        .ok_or_else(|| "expected a toEqual assertion".to_string())?;
    assert_eq!(to_equal.matcher, "toEqual");
    assert_eq!(to_equal.oracle_kind, OracleKind::ExactValue);
    assert!(
        !to_equal.compares_reference_identity,
        "toEqual shares exact_value and must not carry identity, got {to_equal:?}"
    );

    // A refused custom matcher (`==`) stays unknown and does not pin identity.
    let loose = "import { ready } from \"../src/settings\";\nexpect.extend({ toBeSame(r, e) { return { pass: r == e }; } });\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBeSame(cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), loose);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "toBeSame")
        .ok_or_else(|| "expected a refused toBeSame assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::Unknown);
    assert!(
        !assertion.compares_reference_identity,
        "a refused custom matcher must not carry identity, got {assertion:?}"
    );
    let finding = settings_finding("identity-copy-loose-custom", loose, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // A literal expected value still pins a branch, not the returned object.
    let literal = "import { ready } from \"../src/settings\";\nexpect.extend({ toBeSame(r, e) { return { pass: r === e }; } });\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBeSame(null);\n  expect(ready(cfg)).toEqual(cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), literal);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "toBeSame")
        .ok_or_else(|| "expected a toBeSame(null) assertion".to_string())?;
    assert!(assertion.compares_reference_identity);
    assert!(
        !assertion.has_dynamic_matcher_arg,
        "toBeSame(null) must keep the literal exclusion, got {assertion:?}"
    );
    let finding = settings_finding("identity-copy-custom-literal", literal, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // A refused custom matcher named `is` must not inherit AVA identity.
    let colliding = "import { ready } from \"../src/settings\";\nexpect.extend({ is(r, e) { return { pass: r == e }; } });\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).is(cfg);\n  expect(ready(cfg)).toEqual(cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), colliding);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "is")
        .ok_or_else(|| "expected a custom is assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::Unknown);
    assert!(
        !assertion.compares_reference_identity,
        "a refused Jest custom matcher named is must not inherit AVA identity, got {assertion:?}"
    );
    let finding = settings_finding("identity-copy-colliding-is", colliding, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // Tape / node-style `t.strictEqual` is identity on the AVA receiver path;
    // `t.deepEqual` is structural `exact_value` and must not pin.
    let ava_strict = "import { ready } from \"../src/settings\";\ntest(\"ready\", (t) => {\n  const cfg = { ok: true };\n  t.strictEqual(ready(cfg), cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), ava_strict);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "strictEqual")
        .ok_or_else(|| "expected a t.strictEqual assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::ExactValue);
    assert!(
        assertion.compares_reference_identity,
        "t.strictEqual must pin identity, got {assertion:?}"
    );
    let finding = settings_finding("identity-copy-t-strictEqual", ava_strict, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::Exposed);

    let ava_deep = "import { ready } from \"../src/settings\";\ntest(\"ready\", (t) => {\n  const cfg = { ok: true };\n  t.deepEqual(ready(cfg), cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), ava_deep);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "deepEqual")
        .ok_or_else(|| "expected a t.deepEqual assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::ExactValue);
    assert!(
        !assertion.compares_reference_identity,
        "t.deepEqual shares exact_value and must not pin identity, got {assertion:?}"
    );
    let finding = settings_finding("identity-copy-t-deepEqual", ava_deep, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // #7916 remains refused: a comment between `expect` and `.extend` is not
    // credited, so this PR must not loosen that guard to pass identity.
    let commented = "import { ready } from \"../src/settings\";\nexpect /* token */ .extend({ toBeSame(r, e) { return { pass: r === e }; } });\ntest(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBeSame(cfg);\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), commented);
    let assertion = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .find(|assertion| assertion.matcher == "toBeSame")
        .ok_or_else(|| "expected a commented-extend toBeSame assertion".to_string())?;
    assert_eq!(assertion.oracle_kind, OracleKind::Unknown);
    assert!(!assertion.compares_reference_identity);
    let finding = settings_finding("identity-copy-comment-extend", commented, "  return cfg;")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn identity_pin_counts_only_oracle_relations() -> Result<(), String> {
    let source = "test(\"ready\", () => {\n  const cfg = { ok: true };\n  expect(ready(cfg)).toBe(cfg);\n});\n";
    assert_eq!(
        observed("tests/ready.test.ts", source),
        vec![(Some("ready(cfg)".to_string()), None, 3)]
    );
    let tests = extract_tests(Path::new("tests/ready.test.ts"), source);
    let [test] = tests.as_slice() else {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    };
    let direct = TypeScriptRelatedCandidate::with_owner_path(
        test,
        TypeScriptRelationKind::DirectOwnerCall,
        TypeScriptOwnerPathDisposition::TrustedOwnerPath,
    );
    assert!(ts_candidates_pin_identity(&[direct], "ready"));
    // The same `toBe` reached only by test-name similarity cannot see the
    // owner's result.
    let name_linked = TypeScriptRelatedCandidate::with_owner_path(
        test,
        TypeScriptRelationKind::TestName,
        TypeScriptOwnerPathDisposition::OwnerNameCallUnanchored,
    );
    assert!(!ts_candidates_pin_identity(&[name_linked], "ready"));
    Ok(())
}

#[test]
fn identity_copy_subject_reads_both_directions() {
    for (old, new, subject) in [
        ("return cfg;", "return { ...cfg };", Some("cfg")),
        ("return { ...cfg };", "return cfg;", Some("cfg")),
        ("return items", "return [...items]", Some("items")),
        ("return cfg;", "return Object.assign({}, cfg);", Some("cfg")),
        ("return cfg;", "return structuredClone(cfg);", Some("cfg")),
        ("return cfg;", "return { ...cfg, ok: true };", None),
        ("return cfg;", "return { ...other };", None),
        ("return cfg;", "return cfg2;", None),
        ("returned = cfg;", "returned = { ...cfg };", None),
    ] {
        assert_eq!(
            ts_identity_copy_subject(old, new).as_deref(),
            subject,
            "{old} -> {new}"
        );
    }
}
