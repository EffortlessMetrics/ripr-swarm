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
    assert!(facts.credits(
        &called_with("db.write", "2"),
        "const db = { write: jest.fn((a, b) => a), size: 1 }; save(db, 1);"
    ));
    // Arrow and method owners, with a block or an expression body.
    for owner in [
        "const announce = (id: number, send: (m: string) => void) => {\n  send(\"order \" + id);\n}",
        "const announce = (id, send) => send(\"order \" + id)",
        "announce(id: number, send: Send): void {\n  send(\"order \" + id);\n}",
    ] {
        assert!(
            notify_facts(Some(owner)).credits(&order_payload(), NOTIFY_TEST),
            "{owner}"
        );
    }
    // Inside a real test callback, with or without a destructured parameter.
    assert!(notify_facts(Some(NOTIFY_OWNER)).credits(
        &order_payload(),
        "test(\"announces {\", async ({ task }) => { const send = vi.fn(); announce(7, send); })"
    ));
    assert!(notify_facts(Some(NOTIFY_OWNER)).credits(
        &order_payload(),
        "it(\"announces\", () => { const send = jest.fn(); announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\"); })"
    ));
}

#[test]
fn mock_payload_on_a_binding_the_owner_may_not_receive_is_not_credited() {
    let payload = order_payload();
    // A nested block declares the mock, so `send` at the owner call is an
    // outer binding (an imported mock that may already hold the payload).
    for test in [
        "it(\"t\", () => { if (ready) { const send = jest.fn(); } announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\"); })",
        "it(\"t\", function () { { let send = vi.fn(); } announce(7, send); })",
        // A nested arrow block before the callback's own statements.
        "it(\"t\", function () { [1].forEach(() => { const send = jest.fn(); }); announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\"); })",
        // Braces and quotes in the test name or a comment.
        "it('function {\"', function () { if (ready) { // \"\n const send = jest.fn(); } announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\"); })",
    ] {
        assert!(
            !notify_facts(Some(NOTIFY_OWNER)).credits(&payload, test),
            "{test}"
        );
    }
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db.write(v + 2);\n}"),
        "save",
        &params(&["db", "v"]),
    );
    // The changed call goes through a computed receiver, not `db` itself.
    for line in ["  getTable(db).write(v + 2);", "  tables[db].write(v + 2);"] {
        let owner = format!("function save(db: Db, v: number) {{\n{line}\n}}");
        let computed = MockCallFacts::read(line, Some(&owner), "save", &params(&["db", "v"]));
        assert!(
            !computed.credits(
                &called_with("db.write", "2"),
                "const db = { write: jest.fn() }; save(db, 1);"
            ),
            "{line}"
        );
    }
    let optional = MockCallFacts::read(
        "  db?.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db?.write(v + 2);\n}"),
        "save",
        &params(&["db", "v"]),
    );
    assert!(optional.credits(
        &called_with("db.write", "2"),
        "const db = { write: jest.fn() }; save(db, 1);"
    ));
    // A spread, a repeated key or a quoted key can replace the factory.
    for test in [
        "const db = { write: jest.fn(), ...defaults }; save(db, 1);",
        "const db = { write: jest.fn(), write: defaults.write }; save(db, 1);",
        "const db = { write: jest.fn(), \"write\": defaults.write }; save(db, 1);",
        "const db = { write: jest.fn(), write }; save(db, 1);",
        "it(\"t\", () => { if (ready) { const db = { write: jest.fn() }; } save(db, 1); })",
    ] {
        assert!(
            !facts.credits(&called_with("db.write", "2"), test),
            "{test}"
        );
    }
}

/// RIPR-SPEC-0247 rule 1, amended 2026-10-15 (RIPR-SPEC-0243 example 30):
/// `const db = makeDb();` reads the flat literal a test-file factory
/// returns, so the payload pins the changed call as it does for an inline
/// `{ write: jest.fn() }`; every other factory or binding shape refuses.
#[test]
fn mock_payload_on_a_test_file_factory_mock_is_credited() {
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db.write(v + 2);\n}"),
        "save",
        &params(&["db", "v"]),
    );
    let pin = "  const db = makeDb();\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);";
    let file = |factory: &str, body: &str| {
        format!(
            "import {{ save }} from \"../src/save\";\n{factory}\ntest(\"save\", () => {{\n{body}\n}});\n"
        )
    };
    let credited = |source: &str| -> Result<bool, String> {
        let tests = extract_tests(Path::new("test/save.test.ts"), source);
        let [test] = tests.as_slice() else {
            return Err(format!("expected one test in {source}"));
        };
        Ok(facts.credits_with_factories(
            &called_with("db.write", "2"),
            &test.body_text,
            &test.mock_factories_in_file,
        ))
    };
    for factory in [
        "function makeDb(): Db & { write: jest.Mock } {\n  return { write: jest.fn() };\n}",
        "const makeDb = () => ({ write: jest.fn(), read: jest.fn() });",
        "const makeDb = () => {\n  return { write: vi.fn() };\n};",
        // A space before `()` is the same call (#8138).
        "function makeDb() {\n  return { write: jest.fn () };\n}",
    ] {
        let source = file(factory, pin);
        let tests = extract_tests(Path::new("test/save.test.ts"), &source);
        assert_eq!(tests.len(), 1, "{factory}");
        assert_eq!(
            tests[0]
                .mock_factories_in_file
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["makeDb"],
            "{factory}"
        );
        assert_eq!(credited(&source), Ok(true), "{factory}");
    }
    // Without the factory reading the same test stays medium.
    let plain = file(
        "function makeDb() {\n  return { write: jest.fn() };\n}",
        pin,
    );
    let tests = extract_tests(Path::new("test/save.test.ts"), &plain);
    assert_eq!(tests.len(), 1);
    assert!(!facts.credits(&called_with("db.write", "2"), &tests[0].body_text));

    let factory = "function makeDb() {\n  return { write: jest.fn() };\n}";
    for (label, source) in [
        // The factory itself could return something else.
        (
            "spread",
            file(
                "function makeDb() {\n  return { write: jest.fn(), ...defaults };\n}",
                pin,
            ),
        ),
        (
            "shared-value",
            file("function makeDb() {\n  return { write: shared };\n}", pin),
        ),
        (
            "parameter",
            file(
                "function makeDb(seed: number) {\n  return { write: jest.fn() };\n}",
                pin,
            ),
        ),
        (
            "arrow-parameter",
            file(
                "const makeDb = (seed: number) => ({ write: jest.fn() });",
                pin,
            ),
        ),
        (
            "async-arrow",
            file("const makeDb = async () => ({ write: jest.fn() });", pin),
        ),
        (
            "generator",
            file(
                "function* makeDb() {\n  return { write: jest.fn() };\n}",
                pin,
            ),
        ),
        // The mock value could record or return another mock's calls.
        (
            "implementation",
            file(
                "function recordTwo(this: Db) {\n  this.write(2);\n}\nfunction makeDb() {\n  return { write: jest.fn(recordTwo) };\n}",
                pin,
            ),
        ),
        (
            "trailing-operator",
            file(
                "function makeDb() {\n  return { write: jest.fn() && pre.write };\n}",
                pin,
            ),
        ),
        (
            "two-statements",
            file(
                "function makeDb() {\n  log();\n  return { write: jest.fn() };\n}",
                pin,
            ),
        ),
        (
            "exported",
            file(
                "export function makeDb() {\n  return { write: jest.fn() };\n}",
                pin,
            ),
        ),
        (
            "async",
            file(
                "async function makeDb() {\n  return { write: jest.fn() };\n}",
                pin,
            ),
        ),
        (
            "let-arrow",
            file("let makeDb = () => ({ write: jest.fn() });", pin),
        ),
        // The name can be rebound or reached without a direct call.
        (
            "reassigned",
            file(
                factory,
                &format!("  makeDb = () => ({{ write: shared }});\n{pin}"),
            ),
        ),
        (
            "passed-on",
            file(factory, &format!("  register(makeDb);\n{pin}")),
        ),
        (
            "body-shadow",
            file(
                factory,
                &format!("  function makeDb() {{\n    return {{ write: shared }};\n  }}\n{pin}"),
            ),
        ),
        ("eval", file(factory, &format!("  eval(\"x\");\n{pin}"))),
        // The binding is not exactly the factory's fresh result.
        (
            "member-of-result",
            file(
                factory,
                "  const db = makeDb().inner;\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        // A line break does not end the statement before `.` or `&&`.
        (
            "member-next-line",
            file(
                factory,
                "  const db = makeDb()\n    .inner;\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        (
            "operator-next-line",
            file(
                factory,
                "  const db = makeDb()\n    && pre;\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        (
            "argument",
            file(
                factory,
                "  const db = makeDb(1);\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        (
            "other-factory",
            file(
                &format!("{factory}\nfunction otherDb() {{\n  return {{ write: shared }};\n}}"),
                "  const db = otherDb();\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        (
            "nested-binding",
            file(
                factory,
                "  if (ready) {\n    const db = makeDb();\n  }\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
        ("second-use", file(factory, &format!("{pin}\n  other(db);"))),
        // The test's own call on the mock could satisfy the pin (#8140).
        (
            "own-member-call",
            file(
                factory,
                "  const db = makeDb();\n  save(db, 1);\n  db.write(2);\n  expect(db.write).toHaveBeenCalledWith(2);",
            ),
        ),
    ] {
        assert_eq!(credited(&source), Ok(false), "{label}");
    }
}

/// #8138: the mock binding must be exactly a fresh mock. A trailing
/// operator or member can bind another object, a named or `function`
/// implementation can record the asserted call through `this`, and a line
/// break does not end a statement whose next line continues it.
#[test]
fn mock_payload_binding_must_be_exactly_a_fresh_mock() {
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db.write(v + 2);\n}"),
        "save",
        &params(&["db", "v"]),
    );
    let db_payload = called_with("db.write", "2");
    for test in [
        "const db = { write: jest.fn() }; save(db, 1);",
        "const db = { write: jest.fn((v) => v) }; save(db, 1);",
        "const db = { write: jest.fn(() => 2) }; save(db, 1);",
        "const db = { write: jest.fn(async (v) => v) }; save(db, 1);",
        "const db = { write: jest.fn(v => v) }; save(db, 1);",
        "const db = { write: vi.fn<(v: number) => void>() };\nsave(db, 1);",
    ] {
        assert!(facts.credits(&db_payload, test), "{test}");
    }
    for test in [
        "const db = { write: jest.fn() && pre.write }; save(db, 1);",
        "const db = { write: jest.fn(recordTwo) }; save(db, 1);",
        "const db = { write: jest.fn(function (v) { this.write(2); }) }; save(db, 1);",
        // A call or a conditional wrapping an arrow can evaluate to a named
        // implementation (#8148 review); the arrow token inside it is not
        // the implementation. A comma expression or a helper call that
        // receives an arrow and returns a named function behaves the same.
        "const db = { write: jest.fn((() => recordTwo)()) }; save(db, 1);",
        "const db = { write: jest.fn(ready ? (v) => v : recordTwo) }; save(db, 1);",
        "const db = { write: jest.fn((() => 0, recordTwo)) }; save(db, 1);",
        "const db = { write: jest.fn(choose(() => 0)) }; save(db, 1);",
        "const db = { write: jest.fn(), inner: pre }.inner; save(db, 1);",
        "const db = { write: jest.fn() }\n  && pre;\nsave(db, 1);",
        "const db = { write: jest.fn().mockName(\"w\") }; save(db, 1);",
    ] {
        assert!(!facts.credits(&db_payload, test), "{test}");
    }

    let send = |binding: &str| {
        format!("{binding}\nannounce(7, send);\nexpect(send).toHaveBeenCalledWith(\"order 7\");")
    };
    for binding in [
        "const send = jest.fn();",
        "const send = jest.fn((m: string) => m);",
        "const send = jest.fn(async (m: string) => m);",
        "const send = vi.fn<(m: string) => void>();",
        // A line break before a new statement ends it.
        "const send = jest.fn()",
    ] {
        assert!(
            notify_facts(Some(NOTIFY_OWNER)).credits(&order_payload(), &send(binding)),
            "{binding}"
        );
    }
    for binding in [
        "const send = jest.fn() && pre;",
        "const send = jest.fn()\n  && pre;",
        "const send = jest.fn()\n  .bind(pre);",
        "const send = jest.fn()\n  instanceof Function;",
        "const send = jest.fn()\n  (pre);",
        "const send = jest.fn(record);",
        "const send = jest.fn(function () {});",
        // A wrapping call or conditional can hand back a named
        // implementation (#8148 review).
        "const send = jest.fn((() => recordTwo)());",
        "const send = jest.fn(ready ? (m) => m : recordTwo);",
    ] {
        assert!(
            !notify_facts(Some(NOTIFY_OWNER)).credits(&order_payload(), &send(binding)),
            "{binding}"
        );
    }
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
        // The changed call may run more than once, or never.
        "function announce(ids, send) {\n  for (const id of ids) send(\"order \" + id);\n}",
        "function announce(ids, send) {\n  while (ids.length) send(\"order \" + ids.pop());\n}",
        "function announce(id, send) {\n  const later = () => send(\"order \" + id);\n}",
        "function announce(id, send) {\n  return function () { send(\"order \" + id); };\n}",
        "function announce(id, send) {\n  if (id > 0) announce(id - 1, s);\n  send(\"order \" + id);\n}",
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
        // One call site run more than once (RIPR-SPEC-0256): in a loop or a
        // callback, any recorded call may match the payload.
        "const send = jest.fn(); for (const id of [7, 8]) announce(id, send);",
        "const send = jest.fn(); let id = 6; while (id++ < 8) announce(id, send);",
        "const send = jest.fn(); [7, 8].forEach((id) => announce(id, send));",
        "const send = jest.fn(); [7, 8].forEach(function (id) { announce(id, send); });",
        // A method, getter or class may run the call twice.
        "const send = jest.fn(); const h = { run(id) { announce(id, send); } }; h.run(7); h.run(8);",
        "const send = jest.fn(); const h = { get go() { return announce(7, send); } }; h.go; h.go;",
        "const send = jest.fn(); class H { x = announce(7, send); } new H(); new H();",
        // A `{` in a function's parameters, or a generic's comma inside an
        // arrow's expression body, does not end the function early.
        "const send = jest.fn(); [{ id: 7 }, { id: 8 }].forEach(function ({ id }) { announce(id, send); });",
        "const send = jest.fn(); [7, 8].forEach((id) => wrap<number, number>(announce(id, send)));",
        // A default parameter value runs on every call that omits it.
        "const send = jest.fn(); const run = (r = announce(7, send)) => r; run(); run();",
        "const send = jest.fn(); function run(r = announce(7, send)) { return r; } run(); run();",
        // A `.fn<` that is a comparison, not a mock factory's type
        // arguments, does not hide the callback after it.
        "const send = jest.fn(); const small = cfg.fn<3; [7, 8].forEach((id) => id > 0 && announce(id, send));",
        "const send = jest.fn(); const small = cfg.fn<3; [7, 8].forEach((id) => { if (id >(0)) announce(id, send); });",
        // A local definition with the owner's name reads as a second call
        // and fails safe.
        "function announce(a, b) {} const send = jest.fn(); announce(7, send);",
    ] {
        assert!(
            !notify_facts(Some(NOTIFY_OWNER)).credits(&payload, test),
            "{test}"
        );
        // Inside a test callback, so the scope is the test's own block.
        let wrapped = format!("it(\"t\", () => {{\n    {test}\n  }});");
        assert!(
            !notify_facts(Some(NOTIFY_OWNER)).credits(&payload, &wrapped),
            "{wrapped}"
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
