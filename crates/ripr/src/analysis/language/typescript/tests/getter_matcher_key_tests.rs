//! #13592 cause B4: a class getter observed only as a literal key of a
//! property matcher on the constructed receiver itself
//! (`expect(url).toMatchObject({ origin: "..." })`,
//! `expect(url).toHaveProperty("origin", "...")`) ran the getter body but was
//! reported `no_static_path` because reach was keyed on a `url.origin` read.
//! The key credits reach only through an allowlist of shapes and the oracle
//! stays weak, so the verdict is `weakly_exposed` and never `exposed`.

use super::*;

const URL_CLASS: &str = "export class Url {\n  protocol: string;\n  constructor(input = \"\") {\n    this.protocol = input.split(\":\")[0];\n  }\n  get hasProtocol() {\n    return this.protocol.length;\n  }\n  get origin(): string {\n    return (this.protocol ? this.protocol + \"//\" : \"\") + \"host\";\n  }\n}\n";

const CHANGED: &str = "    return (this.protocol ? this.protocol + \"//\" : \"\") + \"host\";";

/// A test file whose `test` body is `body`, importing `Url` directly.
fn test_file(body: &str) -> String {
    format!(
        "import {{ describe, expect, test }} from 'vitest';\nimport {{ Url }} from '../src/u';\n\ndescribe('Url', () => {{\n  test('getters', () => {{\n{body}\n  }});\n}});\n"
    )
}

fn origin_finding(label: &str, test_source: &str) -> Result<Finding, String> {
    let line = URL_CLASS
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/u.ts"), URL_CLASS)?;
    ts_write_file(&root.join("tests/u.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/u.ts", &[(line, CHANGED)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::origin"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for the `origin` getter"))
}

fn assert_credited(label: &str, body: &str) -> Result<(), String> {
    let finding = origin_finding(label, &test_file(body))?;
    assert!(
        !finding.related_tests.is_empty(),
        "{label}: the matcher key reads the getter"
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    // The matcher is a relational check: the verdict is capped below
    // `exposed` and never reaches a repair start.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    Ok(())
}

fn assert_uncredited(label: &str, test_source: &str) -> Result<(), String> {
    let finding = origin_finding(label, test_source)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: no reach is established, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn assert_body_uncredited(label: &str, body: &str) -> Result<(), String> {
    assert_uncredited(label, &test_file(body))
}

#[test]
fn literal_key_of_to_match_object_on_the_receiver_credits_the_getter() -> Result<(), String> {
    for (label, matcher) in [
        (
            "getter-match-plain",
            "expect(url).toMatchObject({ origin: 'https://host', hasProtocol: 5 });",
        ),
        (
            "getter-match-quoted",
            "expect(url).toMatchObject({ \"origin\": 'https://host' });",
        ),
        (
            "getter-has-property",
            "expect(url).toHaveProperty('origin', 'https://host');",
        ),
    ] {
        assert_credited(
            label,
            &format!("    const url = new Url('https://x');\n    {matcher}"),
        )?;
    }
    Ok(())
}

#[test]
fn a_getter_read_through_the_receiver_stays_exposed_beside_the_matcher_key() -> Result<(), String> {
    // Control: the existing `url.origin` read keeps its exact oracle.
    let finding = origin_finding(
        "getter-read-control",
        &test_file(
            "    const url = new Url('https://x');\n    expect(url.origin).toBe('https://host');",
        ),
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn keys_that_do_not_name_the_getter_literally_earn_no_credit() -> Result<(), String> {
    let url = "    const url = new Url('https://x');\n";
    for (label, assertion) in [
        (
            "computed key",
            "const key = 'origin';\n    expect(url).toMatchObject({ [key]: 'https://host' });",
        ),
        (
            "computed literal key",
            "expect(url).toMatchObject({ ['origin']: 'https://host' });",
        ),
        (
            "spread",
            "const rest = { origin: 'https://host' };\n    expect(url).toMatchObject({ ...rest });",
        ),
        (
            "nested key",
            "expect(url).toMatchObject({ meta: { origin: 'https://host' } });",
        ),
        (
            "dotted property path",
            "expect(url).toHaveProperty('meta.origin', 'https://host');",
        ),
        (
            "nested path array",
            "expect(url).toHaveProperty(['origin'], 'https://host');",
        ),
        (
            "non literal value",
            "const expected = 'https://host';\n    expect(url).toMatchObject({ origin: expected });",
        ),
        (
            "asymmetric matcher",
            "expect(url).toMatchObject({ origin: expect.any(String) });",
        ),
        (
            "non-object argument",
            "expect(url).toMatchObject(expectedUrl);",
        ),
        (
            "a sibling getter key only",
            "expect(url).toMatchObject({ hasProtocol: 5 });",
        ),
        (
            "negated matcher",
            "expect(url).not.toMatchObject({ origin: 'https://other' });",
        ),
        (
            "property existence without a value",
            "expect(url).toHaveProperty('origin');",
        ),
    ] {
        assert_body_uncredited(label, &format!("{url}    {assertion}"))?;
    }
    Ok(())
}

#[test]
fn a_key_of_a_different_object_earns_no_credit() -> Result<(), String> {
    let url = "    const url = new Url('https://x');\n";
    for (label, assertion) in [
        (
            "plain object subject",
            "const other = { origin: 'https://host' };\n    expect(other).toMatchObject({ origin: 'https://host' });",
        ),
        (
            "member of the receiver",
            "expect(url.protocol).toMatchObject({ origin: 'https://host' });",
        ),
        (
            "call on the receiver",
            "expect(url.toString()).toMatchObject({ origin: 'https://host' });",
        ),
        (
            "wrapped receiver",
            "expect({ url }).toMatchObject({ origin: 'https://host' });",
        ),
    ] {
        assert_body_uncredited(label, &format!("{url}    {assertion}"))?;
    }
    Ok(())
}

#[test]
fn a_shadowed_or_rebound_receiver_earns_no_credit() -> Result<(), String> {
    // The `describe` scope constructs `url`, but the test body declares its own
    // plain-object `url`, so the matcher subject is not the constructed one.
    assert_uncredited(
        "getter-shadowed-receiver",
        "import { describe, expect, test } from 'vitest';\nimport { Url } from '../src/u';\n\ndescribe('Url', () => {\n  const url = new Url('https://x');\n  test('getters', () => {\n    const url = { origin: 'https://host' };\n    expect(url).toMatchObject({ origin: 'https://host' });\n  });\n});\n",
    )?;
    // The body rebinds the constructed receiver before the assertion.
    assert_body_uncredited(
        "getter-rebound-receiver",
        "    let url = new Url('https://x');\n    url = { origin: 'https://host' };\n    expect(url).toMatchObject({ origin: 'https://host' });",
    )?;
    // The getter is replaced in the file, so the matcher may not run the body.
    assert_uncredited(
        "getter-replaced-in-file",
        "import { describe, expect, test, vi } from 'vitest';\nimport { Url } from '../src/u';\n\ndescribe('Url', () => {\n  test('getters', () => {\n    const url = new Url('https://x');\n    vi.spyOn(url, 'origin', 'get').mockReturnValue('https://host');\n    expect(url).toMatchObject({ origin: 'https://host' });\n  });\n});\n",
    )?;
    Ok(())
}
