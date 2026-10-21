//! RIPR-SPEC-0256: a literal mock-call payload credits a changed
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
    // The owner call as an `if` condition runs once.
    assert!(facts.credits(
        &charged_payload(),
        "it(\"t\", () => {\n    const send = vi.fn();\n    if (checkout(\"ann\", [apple], { send })) {\n      log(1);\n    }\n    expect(send).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  });"
    ));
    // The owner call as an argument of an assertion, after a mock factory
    // with an implementation.
    assert!(facts.credits(
        &charged_payload(),
        "it(\"t\", () => {\n    const send = vi.fn((m: string) => m);\n    expect(checkout(\"ann\", [apple], { send })).toBe(360);\n    expect(send).toHaveBeenCalledWith(\"ann\", \"charged 360\");\n  });"
    ));
    let plain = checkout_facts(&checkout_owner("  const total = sum(lines);", RETURN));
    assert!(plain.credits(&charged_payload(), CHECKOUT_TEST));
    // Control blocks, object literals and `try` are not nested functions:
    // a changed line inside an `if` block still credits without rule 1.
    let blocks = checkout_facts(&format!(
        "export function checkout(owner: string, lines: Line[], notifier: Notifier): number {{\n  const meta = {{ a: 1, b: [2] }};\n  let total = 0;\n  try {{\n    log(total);\n  }} catch (e) {{\n    log(e);\n  }} finally {{\n    log(meta);\n  }}\n  if (meta.a) {{\n{CHECKOUT_LINE}\n  }} else {{\n    total = 1;\n  }}\n  return total;\n}}"
    ));
    assert!(blocks.credits(&charged_payload(), CHECKOUT_TEST));
    // Nested control blocks: an `if` inside an `if` block or a bare block.
    for owner in [
        format!(
            "function checkout(owner, lines, notifier) {{\n  let total = 0;\n  if (lines) {{ if (owner) {{\n{CHECKOUT_LINE}\n  }} }}\n  return total;\n}}"
        ),
        format!(
            "function checkout(owner, lines, notifier) {{\n  let total = 0;\n  {{ if (total < 0) {{ total += 1; }} }}\n  try {{ log(1); }} catch (e) {{ if (e) {{\n{CHECKOUT_LINE}\n  }} }}\n  return total;\n}}"
        ),
    ] {
        assert!(
            checkout_facts(&owner).credits(&charged_payload(), CHECKOUT_TEST),
            "{owner}"
        );
    }
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
        // A method named like a control keyword: `catch` not after a `}`,
        // or `if`/`switch` after an object literal's `{` or `,`.
        "function checkout(owner, lines, notifier) {\n  const h = { catch(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.catch(1); h.catch(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { if(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.if(1); h.if(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { a: 1, switch(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.switch(1); h.switch(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { a: { b: 1 }, if(total) {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.if(1); h.if(2);\n}",
        // A block word that ends a property access in a return type
        // (`typeof cfg.try`) does not open a block.
        "function checkout(owner, lines, notifier) {\n  const h = { run(total): typeof cfg.try {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
        "function checkout(owner, lines, notifier) {\n  const h = { run(total): typeof cfg.return {\n  notifier.send(owner, `charged ${total} cents`);\n  } };\n  h.run(1); h.run(2);\n}",
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
    // A line that continues the expression before it: the `}` there closes
    // an object literal inside an arrow, which runs the line once per item.
    for (before, line) in [
        (
            "  const each = (id) => id < 0 ? {}",
            "    : notifier.send(owner, `charged ${total} cents`);",
        ),
        (
            "  const each = (id) => id < 0 && {}",
            "    || notifier.send(owner, `charged ${total} cents`);",
        ),
    ] {
        let owner = format!(
            "function checkout(owner, lines, notifier) {{\n{before}\n{line}\n  lines.forEach(each);\n}}"
        );
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

/// RIPR-SPEC-0256 rule 1 (2026-10-15): the changed line as the branch of a
/// braceless `if (...)` or `else` that sits directly in the owner body runs
/// at most once, so it credits; every other head before the line refuses.
#[test]
fn changed_call_under_a_top_level_braceless_branch() {
    const SOME: &str = "  const ok = lines.some((l) => l.qty);";
    for head in [
        // The corpus shape: a braceless `if` after a callback statement.
        "  if (ok)",
        // A condition that spans lines or holds its own arrow runs once,
        // before the branch.
        "  if (ok &&\n    lines.length > 0)",
        "  if (lines.some((l) => l.qty))",
        // An `if` as the first statement or after a block.
        "  if (ok) { log(1); }\n  if (ok)",
        // A braceless `else` after a block or after a braceless branch.
        "  if (!ok) {} else",
        "  if (!ok) log(1); else",
    ] {
        let owner = format!(
            "function checkout(owner, lines, notifier) {{\n{SOME}\n{head}\n{CHECKOUT_LINE}\n}}"
        );
        assert!(
            checkout_facts(&owner).credits(&charged_payload(), CHECKOUT_TEST),
            "{owner}"
        );
    }
    let first = format!(
        "function checkout(owner, lines, notifier) {{\n  if (owner)\n{CHECKOUT_LINE}\n  lines.map((l) => l.qty);\n}}"
    );
    assert!(
        checkout_facts(&first).credits(&charged_payload(), CHECKOUT_TEST),
        "{first}"
    );
    for head in [
        // `if` must start a statement: nested, `else if`, labelled.
        "  if (owner) if (ok)",
        "  if (!ok) log(1);\n  else if (ok)",
        "  done: if (ok)",
        // A property named `if` or `else` is not a branch head.
        "  cfg.if (ok)",
        "  cfg.else",
        // Another word before `(...)`, or no word at all.
        "  log(ok)",
        "  (ok)",
        // `else` must follow a `;` or a `}`.
        "  if (!ok) log(1)\n  else",
        // Another word after a `;` is not a branch head: `return` ends the
        // statement, so the line after it never runs.
        "  log(1);\n  return",
        // A loop heading the branch may run it more than once.
        "  for (const l of lines)\n  if (ok)",
        // Inside a callback the branch may run once per item.
        "  lines.forEach((l) => { if (ok)",
    ] {
        let tail = if head.contains("forEach") {
            "  });"
        } else {
            ""
        };
        let owner = format!(
            "function checkout(owner, lines, notifier) {{\n{SOME}\n{head}\n{CHECKOUT_LINE}\n{tail}\n}}"
        );
        assert!(
            !checkout_facts(&owner).credits(&charged_payload(), CHECKOUT_TEST),
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
        // One call site run more than once: any recorded call may match.
        "const send = vi.fn(); for (const who of [\"ann\", \"bo\"]) checkout(who, [apple], { send });",
        "const send = vi.fn(); [\"ann\", \"bo\"].forEach((who) => checkout(who, [apple], { send }));",
        "const send = vi.fn(); const h = { run(who) { checkout(who, [apple], { send }); } }; h.run(\"ann\"); h.run(\"bo\");",
        "const send = vi.fn(); class H { run(w) { checkout(w, [apple], { send }); } } new H().run(\"ann\"); new H().run(\"bo\");",
        "const send = vi.fn(); [{ who: \"ann\" }, { who: \"bo\" }].forEach(function ({ who }) { checkout(who, [apple], { send }); });",
        "const send = vi.fn(); [\"ann\", \"bo\"].forEach((who) => wrap<string, number>(checkout(who, [apple], { send })));",
        "const send = vi.fn(); const h = { run(r = checkout(\"ann\", [apple], { send })) { return r; } }; h.run(); h.run();",
        "const send = vi.fn(); const run = (r = checkout(\"ann\", [apple], { send })): number => r; run(); run();",
        "const send = vi.fn(); const n = vi.fn<9; const h = { run() { if (n > 1) checkout(\"ann\", [apple], { send }); } }; h.run(); h.run();",
        "const send = vi.fn(); const odd = vi.fn<3; [\"ann\", \"bo\"].forEach(function (w) { if (w.length>(0)) checkout(w, [apple], { send }); });",
        "const send = vi.fn(); const odd = vi.fn<3 && [\"ann\", \"bo\"].forEach((w) => w.length>(0) && checkout(w, [apple], { send }));",
        "const send = vi.fn(); const odd = vi.fn<3; const h = { run() { odd>(1) && checkout(\"ann\", [apple], { send }); } }; h.run(); h.run();",
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

/// A test that reaches the owner only through a module entry
/// (`ModuleEntryCall`) does not bind a mock payload to the owner's call: the
/// one textual `helpers._announce(...)` call may reach another function.
#[test]
fn mock_payload_through_a_module_entry_is_not_credited() -> Result<(), String> {
    let mut owner = super::test_owner("_announce", "src/lib.ts");
    owner.params = vec!["id".to_string(), "send".to_string()];
    owner.source_text =
        Some("function _announce(id, send) {\n  send(\"order \" + id);\n}".to_string());
    owner.module_entries = vec![TypeScriptModuleEntry {
        name: "announce".to_string(),
        exported_as_default: false,
        class_member: None,
    }];
    let tests = extract_tests(
        Path::new("tests/lib.test.ts"),
        r#"import { announce } from "../src/lib";
test("announces", () => {
    const send = vi.fn();
    announce(1);
    helpers._announce(7, send);
    expect(send).toHaveBeenCalledWith("order 7");
});
"#,
    );
    let classify = |tests: &[TypeScriptTest]| {
        classify_change(
            Path::new("src/lib.ts"),
            2,
            "  send(\"order \" + id);",
            std::slice::from_ref(&owner),
            tests,
            None,
            &ReExportIndex::empty(),
            None,
        )
        .ok_or_else(|| "expected TypeScript preview finding".to_string())
    };
    let finding = classify(&tests)?;
    assert!(
        matches!(finding.class, ExposureClass::WeaklyExposed),
        "{:?}",
        finding.class
    );
    // Control: the same payload on a direct owner call credits.
    let direct = extract_tests(
        Path::new("tests/lib.test.ts"),
        r#"import { _announce } from "../src/lib";
test("announces", () => {
    const send = vi.fn();
    _announce(7, send);
    expect(send).toHaveBeenCalledWith("order 7");
});
"#,
    );
    let finding = classify(&direct)?;
    assert!(
        matches!(finding.class, ExposureClass::Exposed),
        "{:?}",
        finding.class
    );
    Ok(())
}
