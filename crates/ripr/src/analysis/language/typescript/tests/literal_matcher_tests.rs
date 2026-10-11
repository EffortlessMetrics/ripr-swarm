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

const NOTIFY_TEST: &str =
    "const send = jest.fn(); announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\");";

fn params(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn notify_facts(owner_body: Option<&str>) -> MockCallFacts {
    MockCallFacts::read(
        NOTIFY_LINE,
        owner_body,
        "announce",
        &params(&["id", "send"]),
    )
}

fn order_payload() -> TypeScriptMockPayload {
    called_with("send", "\"order 7\"")
}

#[test]
fn mock_payload_on_the_parameters_own_mock_is_credited() {
    let facts = notify_facts(Some(NOTIFY_OWNER));
    assert!(facts.credits(&order_payload(), NOTIFY_TEST));
    assert!(facts.credits(
        &order_payload(),
        "let send = vi.fn<(m: string) => void>(); announce(7, send);"
    ));
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db.write(v + 2);\n}"),
        "save",
        &params(&["db", "v"]),
    );
    assert!(facts.credits(
        &called_with("db.write", "2"),
        "const db = { write: jest.fn() }; save(db, 1); expect(db.write).toHaveBeenCalledWith(2);"
    ));
}

#[test]
fn mock_payload_another_call_could_satisfy_is_not_credited() {
    let payload = order_payload();
    // No readable owner body.
    assert!(!notify_facts(None).credits(&payload, NOTIFY_TEST));
    // The owner reaches the parameter again: a second call, `call`, an
    // alias, passing it on, or an inner parameter of the same name.
    for owner in [
        "function announce(id, send) {\n  send(\"order \" + id);\n  send(\"order 7\");\n}",
        "function announce(id, send) {\n  send(\"order \" + id);\n  send.call(null, \"order 7\");\n}",
        "function announce(id, send) {\n  const s = send;\n  send(\"order \" + id);\n}",
        "function announce(id, send) {\n  send(\"order \" + id);\n  retry(send);\n}",
        "function announce(id, send) {\n  send(\"order \" + id);\n  [1].forEach((send) => send(1));\n}",
        "function announce(id, send) {\n  send(\"order \" + id);\n  arguments[1](\"order 7\");\n}",
    ] {
        assert!(
            !notify_facts(Some(owner)).credits(&payload, NOTIFY_TEST),
            "{owner}"
        );
    }
    // The test reaches the mock other than through the one owner call.
    for test in [
        "const send = jest.fn(); announce(7, send); send(\"order 7\");",
        "const send = jest.fn(); announce(7, send); remind(send, 7);",
        "const send = jest.fn(); announce(7, send); announce(8, send);",
        "const send = jest.fn(); send.call(null, \"order 7\"); announce(7, send);",
        "const send = jest.fn(); const invoke = send; announce(7, send); invoke(\"order 7\");",
        "const send = jest.fn(); announce(7, send); expect(send(\"order 7\")).toBeUndefined();",
        "const send = jest.fn(); const reexpect = (f) => f(1); announce(7, send); reexpect(send);",
        "const send = jest.fn(); announce(7, jest.fn()); fake.announce(7, send);",
        "const send = jest.fn(); announce(...[7], send);",
        // Passed at another parameter's position.
        "const send = jest.fn(); announce(send, 7);",
    ] {
        assert!(
            !notify_facts(Some(NOTIFY_OWNER)).credits(&payload, test),
            "{test}"
        );
    }
}

#[test]
fn mock_payload_on_another_target_is_not_credited() {
    let payload = order_payload();
    // The changed call goes through a member of another parameter, and the
    // asserted mock is passed elsewhere (`ship(notifier, cb)`).
    let ship = MockCallFacts::read(
        "  notifier.send(\"shipped\");",
        Some("function ship(notifier, cb) {\n  notifier.send(\"shipped\");\n  cb(\"shipped\");\n}"),
        "ship",
        &params(&["notifier", "cb"]),
    );
    assert!(!ship.credits(
        &called_with("send", "\"shipped\""),
        "const send = jest.fn(); ship({ send: jest.fn() }, send);"
    ));
    // `send` is not a parameter: the changed call may reach an import.
    let imported = MockCallFacts::read(
        NOTIFY_LINE,
        Some(NOTIFY_OWNER),
        "announce",
        &params(&["id", "cb"]),
    );
    assert!(!imported.credits(&payload, NOTIFY_TEST));
    // The mock is not made in the test.
    let facts = notify_facts(Some(NOTIFY_OWNER));
    for test in [
        "jest.mock('./mailer'); announce(7, send);",
        "const send = jest.spyOn(mailer, 'send'); announce(7, send);",
        "const send = makeSend(); announce(7, send);",
    ] {
        assert!(!facts.credits(&payload, test), "{test}");
    }
    // An object payload: Jest equality ignores a property set to undefined.
    assert!(!facts.credits(&called_with("send", "{ id: 7 }"), NOTIFY_TEST));
    let times = TypeScriptMockPayload {
        kind: TypeScriptMockPayloadKind::CalledTimes,
        ..payload
    };
    assert!(!facts.credits(&times, NOTIFY_TEST));
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
  assert.equal(code(), "1e3")
  assert.equal(code(), "\uFEFF20")
  assert.equal(code(), "Infinity")
  assert.equal(ratio(), 1.0)
  assert.equal(ratio(), -0)
  assert.equal(on(), true)
  assert.equal(none(), null)
  assert.equal(none(), undefined)
  assert.equal(obj(), { a: "x" })
  assert.equal(text(), `n=${n}`)
  assert.deepEqual(line(3), { count: 0, label: "x" })
  assert.deepEqual(line(3), { ...base, label: "x" })
  assert.deepEqual(line(3), { [key]: "x" })
  assert.deepEqual(line(3), want)
  assert.deepEqual(line(3), { __proto__: { a: "x" } })
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
    expected.extend(vec![relational; 21]);
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

#[test]
fn chai_loose_equal_reads_like_legacy_node_assert() {
    let grades = assertion_grades(
        r#"
const { assert } = require('chai')

it('tiers', function () {
  assert.equal(tier(1000), "silver")
  assert.equal(flag(), 1)
})
"#,
    );
    let got: Vec<(OracleKind, OracleStrength)> = grades
        .iter()
        .map(|(_, kind, strength)| (kind.clone(), strength.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (OracleKind::ExactValue, OracleStrength::Strong),
            (OracleKind::RelationalCheck, OracleStrength::Weak),
        ],
        "{grades:?}"
    );
}
