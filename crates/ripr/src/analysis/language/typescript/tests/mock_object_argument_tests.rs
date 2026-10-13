//! RIPR-SPEC-0255: a literal mock-call payload credits a changed
//! `p.m(...)` call when the test passes its mock as the `m` entry of an
//! inline object literal, and when the owner's nested functions sit away
//! from the changed line.

use super::*;

fn called_with(target: &str, expected: &str) -> TypeScriptMockPayload {
    TypeScriptMockPayload {
        target: target.to_string(),
        expected: expected.to_string(),
        kind: TypeScriptMockPayloadKind::CalledWith,
    }
}

const CHECKOUT_LINE: &str = "  notifier.send(owner, `charged ${total} cents`);";

fn checkout_owner(before: &str, after: &str) -> String {
    format!(
        "export function checkout(owner: string, lines: Line[], notifier: Notifier): number {{\n{before}\n{CHECKOUT_LINE}\n{after}\n}}"
    )
}

fn checkout_facts(owner: &str) -> MockCallFacts {
    MockCallFacts::read(
        CHECKOUT_LINE,
        Some(owner),
        "checkout",
        &["owner", "lines", "notifier"]
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>(),
    )
}

const REDUCE: &str = "  const total = lines.reduce((sum, line) => sum + lineTotal(line), 0);";

const RETURN: &str = "  return total;";

fn charged_payload() -> TypeScriptMockPayload {
    called_with("send", "\"ann\", \"charged 360\"")
}

const CHECKOUT_TEST: &str = "it(\"tells the owner\", () => {\n    const send = vi.fn();\n    checkout(\"ann\", [apple], { send });\n    expect(send).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  });";

/// Rule 1 and rule 2 together: the corpus shape, a renamed entry, and an
/// owner without nested functions.
#[test]
fn mock_in_an_inline_object_is_credited() {
    let facts = checkout_facts(&checkout_owner(REDUCE, RETURN));
    assert!(facts.credits(&charged_payload(), CHECKOUT_TEST));
    assert!(facts.credits(
        &called_with("notify", "\"ann\", \"charged 360\""),
        "it(\"t\", () => {\n    const notify = jest.fn();\n    checkout(\"ann\", [apple], { send: notify, flush: jest.fn() });\n    expect(notify).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  });"
    ));
    let plain = checkout_facts(&checkout_owner("  const total = sum(lines);", RETURN));
    assert!(plain.credits(&charged_payload(), CHECKOUT_TEST));
    // Control blocks, object literals and `try` are not nested functions:
    // a changed line inside an `if` block still credits without rule 1.
    let blocks = checkout_facts(&format!(
        "export function checkout(owner: string, lines: Line[], notifier: Notifier): number {{\n  const meta = {{ a: 1, b: [2] }};\n  let total = 0;\n  try {{\n    log(total);\n  }} catch (e) {{\n    log(e);\n  }} finally {{\n    log(meta);\n  }}\n  if (meta.a) {{\n{CHECKOUT_LINE}\n  }} else {{\n    total = 1;\n  }}\n  return total;\n}}"
    ));
    assert!(blocks.credits(&charged_payload(), CHECKOUT_TEST));
    // After a block statement or as the body's first statement.
    for (before, after) in [
        (
            "  const total = sum(lines);\n  if (total > 0) {\n    log(total);\n  }",
            RETURN,
        ),
        ("", "  const total = lines.map((l) => l.qty);"),
    ] {
        let facts = checkout_facts(&checkout_owner(before, after));
        assert!(
            facts.credits(&charged_payload(), CHECKOUT_TEST),
            "{before} / {after}"
        );
    }
}

/// Rule 1 negatives: with a nested function in the owner, the changed line
/// must be a whole top-level statement with no function on it; loops and
/// recursion still refuse.
#[test]
fn changed_call_a_nested_function_may_run_is_not_credited() {
    for owner in [
        // Inside a callback block.
        "function checkout(owner, lines, notifier) {\n  lines.forEach((line) => {\n  notifier.send(owner, `charged ${total} cents`);\n  });\n}",
        // Inside a callback block, after another statement.
        "function checkout(owner, lines, notifier) {\n  lines.forEach((line) => { log(line);\n  notifier.send(owner, `charged ${total} cents`);\n  });\n}",
        // The body of an expression-bodied arrow on the line before.
        "function checkout(owner, lines, notifier) {\n  const later = () =>\n  notifier.send(owner, `charged ${total} cents`);\n  return later;\n}",
        // A braceless `if` or `else`.
        "function checkout(owner, lines, notifier) {\n  const ok = lines.some((l) => l.qty);\n  if (ok)\n  notifier.send(owner, `charged ${total} cents`);\n}",
        "function checkout(owner, lines, notifier) {\n  const ok = lines.some((l) => l.qty);\n  if (!ok) {} else\n  notifier.send(owner, `charged ${total} cents`);\n}",
        // A loop or recursion elsewhere in the body.
        "function checkout(owner, lines, notifier) {\n  const total = lines.reduce((s, l) => s + l, 0);\n  for (const l of lines) log(l);\n  notifier.send(owner, `charged ${total} cents`);\n}",
        "function checkout(owner, lines, notifier) {\n  const total = lines.reduce((s, l) => s + l, 0);\n  if (!total) checkout(owner, [], log);\n  notifier.send(owner, `charged ${total} cents`);\n}",
        // The changed line appears twice (also a third mention of `notifier`).
        "function checkout(owner, lines, notifier) {\n  const total = lines.reduce((s, l) => s + l, 0);\n  notifier.send(owner, `charged ${total} cents`);\n  notifier.send(owner, `charged ${total} cents`);\n}",
        // The changed line inside a method shorthand or class member that
        // runs it twice, with no arrow or `function` in the body.
        "function checkout(owner, lines, notifier) {\n  const h = { run(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { run(total): void {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
        "function checkout(owner, lines, notifier) {\n  class H { get t() { return 1; } run(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } }\n  new H().run(1); new H().run(2);\n}",
    ] {
        assert!(
            !checkout_facts(owner).credits(&charged_payload(), CHECKOUT_TEST),
            "{owner}"
        );
    }
    // A function on the changed line itself: an arrow, a `function`, or a
    // one-line method whose statement sits at the top level.
    for line in [
        "  notifier.send(owner, lines.map((l) => l.sku).join());",
        "  notifier.send(owner, lines.map(function (l) { return l.sku; }).join());",
        "  const h = { run(total) { notifier.send(owner, `charged ${total} cents`); } }; h.run(1); h.run(2);",
    ] {
        let owner = format!("function checkout(owner, lines, notifier) {{\n{line}\n}}");
        assert!(
            !line_facts(line, &owner).credits(&charged_payload(), CHECKOUT_TEST),
            "{line}"
        );
    }
    // A `{` after a `}` or a string literal opens a typed method body
    // (`run(): { n: number } {`, `run(): "a" {`), so it counts as a nested
    // function even with no arrow or `function` in the owner.
    for owner in [
        "function checkout(owner, lines, notifier) {\n  const h = { run(total): { n: number } {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { run(total): \"a\" {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
    ] {
        assert!(
            !checkout_facts(owner).credits(&charged_payload(), CHECKOUT_TEST),
            "{owner}"
        );
    }
}

fn line_facts(line: &str, owner: &str) -> MockCallFacts {
    MockCallFacts::read(
        line,
        Some(owner),
        "checkout",
        &["owner", "lines", "notifier"]
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>(),
    )
}

/// Rule 2 negatives: the object must be an inline flat literal at the
/// parameter's position whose only `send` entry is the test-local mock,
/// mentioned nowhere else.
#[test]
fn mock_the_object_may_not_carry_is_not_credited() {
    let facts = checkout_facts(&checkout_owner(REDUCE, RETURN));
    for body in [
        // A spread, a second key, a quoted key or a method can replace it.
        "const send = vi.fn(); checkout(\"ann\", [apple], { ...base, send });",
        "const send = vi.fn(); checkout(\"ann\", [apple], { send, send: other });",
        "const send = vi.fn(); checkout(\"ann\", [apple], { \"send\": send });",
        "const send = vi.fn(); checkout(\"ann\", [apple], { send() {} });",
        // Another key, or the mock as another entry's value.
        "const send = vi.fn(); checkout(\"ann\", [apple], { post: send });",
        // Not inline, at another position, or not the whole argument.
        "const send = vi.fn(); const notifier = { send }; checkout(\"ann\", [apple], notifier);",
        "const send = vi.fn(); checkout(\"ann\", { send }, base);",
        "const send = vi.fn(); checkout(\"ann\", [apple], { send }.self);",
        "const send = vi.fn(); checkout(\"ann\", ...[[apple], { send }]);",
        // Not a test-local mock, or reached elsewhere.
        "const send = makeSend(); checkout(\"ann\", [apple], { send });",
        "const send = vi.fn(); checkout(\"ann\", [apple], { send }); send(\"ann\", \"charged 360\");",
        "const send = vi.fn(); checkout(\"ann\", [apple], { send }); checkout(\"bo\", [], { send });",
        "if (ready) { const send = vi.fn(); } checkout(\"ann\", [apple], { send });",
        // A spread before the position shifts which parameter gets it.
        "const send = vi.fn(); checkout(...pre, [apple], { send });",
    ] {
        let test = format!(
            "it(\"t\", () => {{\n    {body}\n    expect(send).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  }});"
        );
        assert!(!facts.credits(&charged_payload(), &test), "{test}");
    }
    // A duplicate `send` key, whichever side names the mock, may replace it.
    for body in [
        "const notify = vi.fn(); checkout(\"ann\", [apple], { send: notify, send: other });",
        "const notify = vi.fn(); checkout(\"ann\", [apple], { send: other, send: notify });",
    ] {
        let test = format!(
            "it(\"t\", () => {{\n    {body}\n    expect(notify).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  }});"
        );
        assert!(
            !facts.credits(&called_with("notify", "\"ann\", \"charged 360\""), &test),
            "{test}"
        );
    }
    // The same harness credits the plain inline shape, so each refusal
    // above is the shape's own.
    let accepted = "const send = vi.fn(); checkout(\"ann\", [apple], { send });";
    let test = format!(
        "it(\"t\", () => {{\n    {accepted}\n    expect(send).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  }});"
    );
    assert!(facts.credits(&charged_payload(), &test), "{test}");
    // An object payload or a call count is never credited.
    assert!(!facts.credits(&called_with("send", "{ to: \"ann\" }"), CHECKOUT_TEST));
    let times = TypeScriptMockPayload {
        kind: TypeScriptMockPayloadKind::CalledTimes,
        ..charged_payload()
    };
    assert!(!facts.credits(&times, CHECKOUT_TEST));
}
