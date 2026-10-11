//! RIPR-SPEC-0247: literal mock-call payloads, coercion-safe loose equality
//! and literal inline snapshots read exact; every neighbouring shape keeps
//! its RIPR-SPEC-0243 / RIPR-SPEC-0027 reading.

use super::*;

fn called_with(target: &str, expected: &str) -> TypeScriptMockPayload {
    TypeScriptMockPayload {
        target: target.to_string(),
        expected: expected.to_string(),
        kind: TypeScriptMockPayloadKind::CalledWith,
    }
}

const NOTIFY_OWNER: &str = "export function announce(id: number, send: (m: string) => void) {\n  send(\"order \" + id);\n}";

const NOTIFY_LINE: &str = "  send(\"order \" + id);";

fn params(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn notify_facts(owner_body: Option<&str>) -> MockCallFacts {
    MockCallFacts::read(NOTIFY_LINE, owner_body, &params(&["id", "send"]))
}

#[test]
fn mock_payload_on_a_test_local_mock_called_once_is_credited() {
    let facts = notify_facts(Some(NOTIFY_OWNER));
    let payload = called_with("send", "\"order 7\"");
    assert!(facts.credits(&payload, "const send = jest.fn(); announce(7, send);"));
    assert!(facts.credits(&payload, "let send = vi.fn<(m: string) => void>();"));
    let property = called_with("db.write", "2");
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db, v) {\n  db.write(v + 2);\n}"),
        &params(&["db", "v"]),
    );
    assert!(facts.credits(&property, "const db = { write: jest.fn() }; save(db, 1);"));
}

#[test]
fn mock_payload_without_proof_of_the_only_call_is_not_credited() {
    let payload = called_with("send", "\"order 7\"");
    let local = "const send = jest.fn();";
    // No readable owner body: nothing shows the changed call is the only one.
    assert!(!notify_facts(None).credits(&payload, local));
    // A second call to the same mock could still match after a mutant.
    let twice = "function announce(id, send) {\n  send(\"order \" + id);\n  send(\"order 7\");\n}";
    assert!(!notify_facts(Some(twice)).credits(&payload, local));
    // `send` is not a parameter: the changed call may reach an import, not
    // the mock the test passes in.
    let imported = MockCallFacts::read(NOTIFY_LINE, Some(NOTIFY_OWNER), &params(&["id", "cb"]));
    assert!(!imported.credits(&payload, local));
    // The owner shadows the parameter.
    let shadowed =
        "function announce(id, send) {\n  const send = realSend;\n  send(\"order \" + id);\n}";
    assert!(!notify_facts(Some(shadowed)).credits(&payload, local));
    // The test calls the mock itself, which can satisfy the match.
    assert!(
        !notify_facts(Some(NOTIFY_OWNER))
            .credits(&payload, "const send = jest.fn(); send(\"order 7\");")
    );
    // The changed line does not call the asserted mock.
    let other = MockCallFacts::read(
        "  log(\"order \" + id);",
        Some("function a(log) {\n  log(\"order \" + id);\n}"),
        &params(&["log"]),
    );
    assert!(!other.credits(&payload, local));
    // A keyword is not a call.
    let keyword = MockCallFacts::read(
        "  if (send) {",
        Some("function a(send) {\n  if (send) {\n}"),
        &params(&["if", "send"]),
    );
    assert!(!keyword.credits(&called_with("if", "1"), "const if = jest.fn();"));
}

#[test]
fn mock_payload_on_a_mock_not_made_in_the_test_is_not_credited() {
    let facts = notify_facts(Some(NOTIFY_OWNER));
    let payload = called_with("send", "\"order 7\"");
    for body in [
        "jest.mock('./mailer'); announce(7, send);",
        "const send = jest.spyOn(mailer, 'send');",
        "const send = makeSend(); announce(7, send);",
        "const resend = jest.fn();",
        "const db = { write: { inner: jest.fn() } };",
    ] {
        assert!(!facts.credits(&payload, body), "{body}");
    }
    let times = TypeScriptMockPayload {
        kind: TypeScriptMockPayloadKind::CalledTimes,
        ..payload
    };
    assert!(!facts.credits(&times, "const send = jest.fn();"));
}

fn assertion_grades(source: &str) -> Vec<(String, OracleKind, OracleStrength)> {
    let tests = extract_tests(Path::new("test/a.test.ts"), source);
    assert_eq!(tests.len(), 1, "fixture must register one test: {tests:?}");
    tests[0]
        .assertions
        .iter()
        .map(|assertion| {
            (
                assertion
                    .rendered_call
                    .clone()
                    .unwrap_or_else(|| assertion.matcher.clone()),
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone(),
            )
        })
        .collect()
}

#[test]
fn loose_equality_with_a_coercion_safe_literal_reads_exact() {
    let grades = assertion_grades(
        r#"
import assert from 'node:assert'
import { expect } from 'vitest'

test('tiers', () => {
  assert.equal(tier(999), "bronze")
  assert.equal(fee(10), 20)
  assert.equal(delta(), -3)
  assert.equal(label(), `plain`)
  assert.deepEqual(line(3), { count: 3, label: "items: 3" })
  assert.deepEqual(pair(), ["a", 2.5])
  assert.equal(flag(), 1)
  assert.equal(flag(), 0)
  assert.equal(name(), "")
  assert.equal(name(), " ")
  assert.equal(code(), "20")
  assert.equal(code(), "0x1f")
  assert.equal(on(), true)
  assert.equal(none(), null)
  assert.equal(none(), undefined)
  assert.equal(obj(), { a: "x" })
  assert.equal(text(), `n=${n}`)
  assert.deepEqual(line(3), { count: 0, label: "x" })
  assert.deepEqual(line(3), { ...base, label: "x" })
  assert.deepEqual(line(3), { [key]: "x" })
  assert.deepEqual(line(3), want)
})
"#,
    );
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let relational = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let got: Vec<(OracleKind, OracleStrength)> = grades
        .iter()
        .map(|(_, kind, strength)| (kind.clone(), strength.clone()))
        .collect();
    let mut expected = vec![exact; 6];
    expected.extend(vec![relational; 15]);
    assert_eq!(got, expected, "{grades:?}");
}

#[test]
fn inline_snapshot_with_literal_text_reads_strong() {
    let grades = assertion_grades(
        r#"
test('summary', () => {
  expect(summary(2)).toMatchInlineSnapshot(`"2 items"`);
  expect(receipt(7)).toMatchInlineSnapshot(`
    {
      "currency": "USD",
    }
  `);
  expect(summary(2)).toMatchInlineSnapshot();
  expect(summary(2)).toMatchInlineSnapshot(`""`.length ? `` : ``);
  expect(summary(n)).toMatchInlineSnapshot(`"${n} items"`);
  expect(receipt(7)).toMatchInlineSnapshot({ id: expect.any(Number) }, `{}`);
  expect(summary(2)).toMatchSnapshot();
});
"#,
    );
    let got: Vec<OracleStrength> = grades
        .iter()
        .map(|(_, kind, strength)| {
            assert_eq!(*kind, OracleKind::Snapshot, "{grades:?}");
            strength.clone()
        })
        .collect();
    let mut expected = vec![OracleStrength::Strong; 2];
    expected.extend(vec![OracleStrength::Medium; 5]);
    assert_eq!(got, expected, "{grades:?}");
}
