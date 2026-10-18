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

/// An element naming a top-level primitive `const` declared before the call
/// reads as that literal; any other binding of the name collects nothing.
#[test]
fn foreach_element_bound_to_a_literal_const_reads_as_the_literal() {
    let wrap = |body: &str| {
        format!(
            "import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n{body}\n}});\n"
        )
    };
    let source =
        wrap("  const n = 200;\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });");
    assert_eq!(
        observed("tests/price.test.ts", &source),
        vec![some("price(150)", "140", 4), some("price(200)", "140", 4)]
    );
    let source = wrap(
        "  const label = \"vip\";\n  [label].forEach((v) => { expect(price(v)).toBe(140); });",
    );
    assert_eq!(
        observed("tests/price.test.ts", &source),
        vec![some("price(\"vip\")", "140", 4)]
    );
    for body in [
        "  let n = 200;\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  var n = 200;\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  [150, n].forEach((v) => { expect(price(v)).toBe(140); });\n  const n = 200;",
        "  const n = 200;\n  {\n    const n = 300;\n    [150, n].forEach((v) => { expect(price(v)).toBe(140); });\n  }",
        "  const n = { amount: 200 };\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = load();\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = `2\n00`;\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = 200;\n  function n() {}\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        // A template is no table cell, and a line continuation spans lines.
        "  const n = `vip`;\n  [n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = `v${1}`;\n  [n].forEach((v) => { expect(price(v)).toBe(140); });",
        // RIPR-SPEC-0247 rule 2 does not pin a boolean or `0`.
        "  const n = true;\n  [n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = 0;\n  [150, n].forEach((v) => { expect(price(v)).toBe(140); });",
        "  const n = \"v\\\nip\";\n  [n].forEach((v) => { expect(price(v)).toBe(140); });",
    ] {
        assert_eq!(
            observed("tests/price.test.ts", &wrap(body)),
            vec![],
            "{body}"
        );
    }
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
    settings_finding_in(label, SETTINGS, SETTINGS_LINE, test, removed)
}

/// `settings_finding` with another owner source and changed line.
fn settings_finding_in(
    label: &str,
    source: &str,
    line: (usize, &str),
    test: &str,
    removed: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/settings.ts"), source)?;
    ts_write_file(&root.join("tests/settings.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/settings.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: line.0,
            text: line.1.to_string(),
            new_side_line: line.0,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: line.0,
            text: removed.to_string(),
            new_side_line: line.0,
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
        .find(|finding| finding.probe.location.line == line.0)
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

    // A change that is not copy-only is outside rule 1. A dropped override
    // the tested input cannot see is rule 3's (example 3); with an input
    // that lacks the key or holds another value the ladder verdict stands.
    let finding = settings_finding(
        "identity-copy-other-change",
        STRUCTURAL_TEST,
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("only sets or drops `ok: true` after spreading `cfg`")),
        "{:?}",
        finding.missing
    );
    let other_value = STRUCTURAL_TEST.replace("{ ok: true }", "{ ok: false }");
    let finding = settings_finding(
        "identity-copy-other-change-false",
        &other_value,
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);

    // #8712 capped the negated oracle at weak credit; a negated identity
    // matcher (`not.toBe`) asserts NON-identity, so it cannot pin the
    // returned object's identity for a copy-for-identity change either. Only
    // the unconditional structural assertion grants credit here, and rule 1
    // demotes the copy-for-identity change.
    let negated = "import { ready } from \"../src/settings\";\ntest(\"ready\", () => {\n  const other = { ok: true };\n  expect(ready(cfg)).not.toBe(other);\n  expect(ready(cfg)).toEqual(cfg);\n});\n";
    let finding = settings_finding("identity-copy-negated-pin", negated, "  return cfg;")?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "a negated identity matcher must not pin identity: {:?}",
        finding.missing
    );
    Ok(())
}

/// RIPR-SPEC-0246 rule 3: a dropped or added literal field after a spread
/// keeps `exposed` only when an observed call's input is shown to lack the
/// field or hold another value there.
#[test]
fn spread_override_needs_an_input_that_lacks_the_field() -> Result<(), String> {
    let test = |body: &str| {
        format!(
            "import {{ ready }} from \"../src/settings\";\ntest(\"ready\", () => {{\n  {body}\n}});\n"
        )
    };
    for (label, body, expected) in [
        (
            "spread-inline-carries",
            "expect(ready({ ok: true })).toEqual({ ok: true });",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-inline-lacks",
            "expect(ready({ name: \"bo\" })).toEqual({ name: \"bo\", ok: true });",
            ExposureClass::Exposed,
        ),
        (
            "spread-local-lacks",
            "const cfg = { name: \"bo\" };\n  expect(ready(cfg)).toEqual({ ...cfg, ok: true });",
            ExposureClass::Exposed,
        ),
        // One input that differs is enough.
        (
            "spread-mixed",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ ok: false })).toEqual({ ok: true });",
            ExposureClass::Exposed,
        ),
        // An unreadable input is not shown to lack the field.
        (
            "spread-unknown-input",
            "const cfg = load();\n  expect(ready(cfg)).toEqual(cfg);",
            ExposureClass::WeaklyExposed,
        ),
        // With no observed whole-result call the ladder stands (decision 6),
        // and so it does when the only whole-result assertion is weak.
        (
            "spread-member-only",
            "expect(ready({ ok: true }).ok).toBe(true);",
            ExposureClass::Exposed,
        ),
        (
            "spread-weak-matcher",
            "expect(ready({ ok: true })).toBeTruthy();\n  expect(ready({ ok: true }).ok).toBe(true);",
            ExposureClass::Exposed,
        ),
        (
            "spread-input-spread",
            "const cfg = { ...base };\n  expect(ready(cfg)).toEqual(cfg);",
            ExposureClass::WeaklyExposed,
        ),
        // A typed binding and a `//` inside a string still read.
        (
            "spread-typed-local-lacks",
            "const cfg: Settings = { name: \"bo\" };\n  expect(ready(cfg)).toEqual({ name: \"bo\", ok: true });",
            ExposureClass::Exposed,
        ),
        (
            "spread-typed-local-carries",
            "const cfg: Record<string, boolean> = { ok: true };\n  expect(ready(cfg)).toEqual(cfg);",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-url-string",
            "expect(ready({ url: \"http://x\" })).toEqual({ url: \"http://x\", ok: true });",
            ExposureClass::Exposed,
        ),
        // An unreadable input does not cancel one that lacks the field.
        (
            "spread-unknown-and-lacks",
            "const a = load();\n  expect(ready(a)).toEqual(a);\n  expect(ready({ name: \"bo\" })).toEqual({ name: \"bo\", ok: true });",
            ExposureClass::Exposed,
        ),
        (
            "spread-strict-carries",
            "expect(ready({ ok: true })).toStrictEqual({ ok: true });",
            ExposureClass::WeaklyExposed,
        ),
        // A strong pin on the key itself sees an input that lacks the field;
        // a pin on another member, or on an input that carries it, does not.
        (
            "spread-key-pin-lacks",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" }).ok).toBe(true);",
            ExposureClass::Exposed,
        ),
        (
            "spread-key-bracket-pin-lacks",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" })[\"ok\"]).toBe(true);",
            ExposureClass::Exposed,
        ),
        (
            "spread-other-member-pin",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" }).name).toBe(\"bo\");",
            ExposureClass::WeaklyExposed,
        ),
        // A method that may write the input (`save`, `apply`) fails the read.
        (
            "spread-local-method-call",
            "const cfg = { name: \"bo\" };\n  new Defaults().apply(cfg);\n  expect(ready(cfg)).toEqual(cfg);",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-local-strict-matcher",
            "const cfg = { name: \"bo\" };\n  expect(ready(cfg)).toStrictEqual({ ...cfg, ok: true });",
            ExposureClass::Exposed,
        ),
        // Anything after the key can pass on both versions.
        (
            "spread-key-pin-nullish",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" }).ok ?? true).toBe(true);",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-key-pin-or",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" }).ok || true).toBe(true);",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-key-pin-deeper",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"bo\" }).ok?.foo).toBe(undefined);",
            ExposureClass::WeaklyExposed,
        ),
        // Non-ASCII text in the call reads without a panic.
        (
            "spread-key-pin-non-ascii",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ name: \"éé\" }).ok).toBe(true);",
            ExposureClass::Exposed,
        ),
        (
            "spread-key-pin-non-ascii-other",
            "const ñ = 1;\n  expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready(\"éé\", ñ).ok).toBe(true);",
            ExposureClass::WeaklyExposed,
        ),
        (
            "spread-key-pin-carries",
            "expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ready({ ok: true }).ok).toBe(true);",
            ExposureClass::WeaklyExposed,
        ),
    ] {
        let finding = settings_finding(label, &test(body), "  return { ...cfg, ok: true };")?;
        assert_eq!(finding.class, expected, "{label}");
    }
    // A body-local `ns` that lacks the field is not an owner call. The
    // real owner pin carries `ok`, so dropping the override stays weak.
    let shadowed = "import * as ns from \"../src/settings\";\nimport { ready } from \"../src/settings\";\ntest(\"ready\", () => {\n  const ns = fakeReady();\n  expect(ready({ ok: true })).toEqual({ ok: true });\n  expect(ns.ready({ name: \"bo\" })).toEqual({ name: \"bo\", ok: true });\n});\n";
    let tests = extract_tests(Path::new("tests/settings.test.ts"), shadowed);
    assert_eq!(
        tests
            .iter()
            .flat_map(|test| test.assertions.iter())
            .map(|assertion| assertion.observed_expression.clone())
            .collect::<Vec<_>>(),
        vec![
            Some("ready({ ok: true })".to_string()),
            Some("ns.ready({ name: \"bo\" })".to_string())
        ]
    );
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "ns") {
        return Err("body-local `ns` must read as a shadow".to_string());
    }
    let finding = settings_finding(
        "spread-shadowed-namespace",
        shadowed,
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // A member written after the binding makes the input unreadable, so the
    // finding is not described as carrying the field.
    let finding = settings_finding(
        "spread-local-mutated",
        &test(
            "const cfg = { ok: true };\n  cfg.ok = false;\n  expect(ready(cfg)).toEqual({ ok: true });",
        ),
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("no tested input is shown to lack it")
                && !line.contains("same value")),
        "{:?}",
        finding.missing
    );
    // Only a known-carrying input claims both versions return one value.
    let finding = settings_finding(
        "spread-carries-message",
        &test("expect(ready({ ok: true })).toEqual({ ok: true });"),
        "  return { ...cfg, ok: true };",
    )?;
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("already carries it, so both versions return the same value")),
        "{:?}",
        finding.missing
    );
    // A destructuring write before the call changes the input.
    let finding = settings_finding(
        "spread-local-destructured",
        &test(
            "const cfg = { ok: true };\n  [cfg.ok] = [false];\n  expect(ready(cfg)).toEqual({ ok: true });",
        ),
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("no tested input is shown to lack it")
                && !line.contains("same value")),
        "{:?}",
        finding.missing
    );
    // A destructuring loop target writes the member, so an input that starts
    // without the value is not read as lacking it.
    let finding = settings_finding(
        "spread-local-loop-destructured",
        &test(
            "const cfg = { ok: false };\n  for ([cfg.ok] of [[true]]) {}\n  expect(ready(cfg)).toEqual({ ok: true });",
        ),
        "  return { ...cfg, ok: true };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // The added direction reads the same way.
    let added =
        "export function ready(cfg: { ok: boolean }) {\n  return { ...cfg, ok: true };\n}\n";
    let finding = settings_finding_in(
        "spread-added-field",
        added,
        (2, "  return { ...cfg, ok: true };"),
        &test("expect(ready({ ok: true })).toEqual({ ok: true });"),
        "  return { ...cfg };",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // An owner that writes, passes on or rebinds `cfg` keeps the ladder
    // verdict: the input it spreads is not the input the test wrote.
    for (label, source) in [
        (
            "spread-owner-writes",
            "export function ready(cfg: { ok: boolean }) {\n  cfg.ok = false;\n  return { ...cfg };\n}\n",
        ),
        (
            "spread-owner-passes",
            "export function ready(cfg: { ok: boolean }) {\n  normalize(cfg);\n  return { ...cfg };\n}\n",
        ),
        (
            "spread-owner-rebinds",
            "export function ready(cfg: { ok: boolean }) {\n  cfg = { ok: false };\n  return { ...cfg };\n}\n",
        ),
    ] {
        let finding = settings_finding_in(
            label,
            source,
            (3, "  return { ...cfg };"),
            &test("expect(ready({ ok: true })).toEqual({ ok: true });"),
            "  return { ...cfg, ok: true };",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    // Dropping two fields is not rule 3: the ladder verdict stands.
    let finding = settings_finding(
        "spread-two-fields",
        &test("expect(ready({ ok: true })).toEqual({ ok: true });"),
        "  return { ...cfg, ok: true, extra: 1 };",
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
