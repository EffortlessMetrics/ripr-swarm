use super::*;
use std::error::Error;

fn positions(test: &str, helper: &str) -> Result<BTreeSet<usize>, String> {
    closure_argument_positions(test, helper).ok_or_else(|| format!("premise: parses: {test}"))
}

fn invoked(helper: &str) -> Result<Vec<(usize, String)>, String> {
    Ok(invoked_callback_params(helper)
        .ok_or_else(|| format!("premise: parses and is a free fn: {helper}"))?
        .into_iter()
        .map(|param| (param.position, param.name))
        .collect())
}

#[test]
fn closure_literal_positions_require_a_call_that_runs_with_the_closure()
-> Result<(), Box<dyn Error>> {
    for (shape, body, want) in [
        ("expression closure", "run(true, || gate(1));", vec![1]),
        ("block closure", "run(|| { gate(1) }, true);", vec![0]),
        ("method call", "run(true, || x.gate());", vec![1]),
        ("two closures", "run(|| gate(1), || gate(2));", vec![0, 1]),
        ("no call", "run(true, || true);", vec![]),
        ("plain value", "let g = || gate(1); run(true, g);", vec![]),
        ("boxed closure", "run(true, Box::new(|| gate(1)));", vec![]),
        ("reference to closure", "run(true, &|| gate(1));", vec![]),
        (
            "async block in closure",
            "run(true, || async { gate(1) });",
            vec![],
        ),
        (
            "closure returning closure",
            "run(true, || || gate(1));",
            vec![],
        ),
        (
            "nested fn in closure",
            "run(true, || { fn inner() { gate(1) } 1 });",
            vec![],
        ),
        ("async closure", "run(true, async || gate(1));", vec![]),
        (
            "exit in an earlier argument, statement",
            "run({ return; true }, || gate(1));",
            vec![],
        ),
        (
            "exit in a later argument, statement",
            "run(|| gate(1), { return; true });",
            vec![],
        ),
        (
            "exit in an argument, let initializer",
            "let _ = run({ return; true }, || gate(1));",
            vec![],
        ),
        (
            "exit in a later argument, let initializer",
            "let _ = run(|| gate(1), { return; true });",
            vec![],
        ),
        (
            "exit in an argument, tail",
            "run({ return; true }, || gate(1))",
            vec![],
        ),
        (
            "exit in a later argument, tail",
            "run(|| gate(1), { return; true })",
            vec![],
        ),
        (
            "question mark in an argument",
            "run(x?, || gate(1));",
            vec![],
        ),
        (
            "opaque macro in an argument",
            "run(m!(), || gate(1));",
            vec![],
        ),
        (
            "transparent macro in an argument",
            "run(format!(\"x\"), || gate(1));",
            vec![1],
        ),
        (
            "a closure-owned return in an argument is refused too (shared escape authority)",
            "run((|| { return 1; })(), || gate(1));",
            vec![],
        ),
        (
            "return written in the callback literal itself",
            "run(true, || { return gate(1); });",
            vec![],
        ),
        (
            "question mark written in the callback literal itself",
            "run(true, || { gate(1)?; true });",
            vec![],
        ),
        (
            "hidden return in a transparent macro argument",
            "run(&format!(\"{}\", { return; \"x\" }), || gate(1));",
            vec![],
        ),
        (
            "hidden return in a preceding macro statement",
            "let _ = format!(\"{}\", { return; \"x\" }); run(true, || gate(1));",
            vec![],
        ),
        (
            "hidden question mark in a tail macro argument",
            "run(format!(\"{}\", x?), || gate(1))",
            vec![],
        ),
        (
            "benign transparent macro arguments",
            "run(&format!(\"hi {}\", \"bob\"), || gate(1));",
            vec![1],
        ),
        (
            "return after the executed helper call",
            "run(true, || gate(1)); return;",
            vec![1],
        ),
        ("let initializer", "let _ = run(true, || gate(1));", vec![1]),
        ("tail call", "run(true, || gate(1))", vec![1]),
        (
            "under if false",
            "if false { run(true, || gate(1)); }",
            vec![],
        ),
        (
            "after a possible early return",
            "if x { return; } run(true, || gate(1));",
            vec![],
        ),
        (
            "after an opaque macro",
            "m!(); run(true, || gate(1));",
            vec![],
        ),
        (
            "after an assertion macro",
            "assert!(x); run(true, || gate(1));",
            vec![1],
        ),
        (
            "after an unrelated nested fn exit",
            "fn o() { return; } run(true, || gate(1));",
            vec![1],
        ),
        ("path call", "other::run(true, || gate(1));", vec![]),
        ("method call of helper", "x.run(true, || gate(1));", vec![]),
        (
            "helper call under test closure",
            "let l = || run(true, || gate(1)); l();",
            vec![],
        ),
        ("different helper", "other(true, || gate(1));", vec![]),
    ] {
        let text = format!("fn t() {{ {body} }}");
        let got = positions(&text, "run")?.into_iter().collect::<Vec<_>>();
        assert_eq!(got, want, "{shape}");
    }
    assert_eq!(closure_argument_positions("fn t( {", "run"), None);
    Ok(())
}

#[test]
fn invoked_parameters_are_plain_singly_bound_statement_level_calls() -> Result<(), Box<dyn Error>> {
    for (shape, helper, want) in [
        (
            "generic bound",
            "fn h<F: Fn() -> bool>(w: bool, f: F) { let g = f(); assert_eq!(g, w); }",
            vec![(1, "f")],
        ),
        (
            "two parameters",
            "fn h(a: impl Fn() -> bool, b: impl Fn() -> bool) { assert_eq!(a(), b()); let _ = a(); let _ = b(); }",
            vec![(0, "a"), (1, "b")],
        ),
        (
            "only one invoked",
            "fn h(a: impl Fn() -> bool, b: impl Fn() -> bool) { let _ = &a; let _ = b(); }",
            vec![(1, "b")],
        ),
        (
            "not invoked",
            "fn h(f: impl Fn() -> bool) { let _ = &f; }",
            vec![],
        ),
        (
            "invoked only in a closure",
            "fn h(f: impl Fn() -> bool) { let r = || f(); let _ = r(); }",
            vec![],
        ),
        (
            "invoked only in an async block",
            "fn h(f: impl Fn() -> bool) { let _ = async { f() }; }",
            vec![],
        ),
        (
            "invoked only in a macro argument",
            "fn h(f: impl Fn() -> bool) { assert!(f()); }",
            vec![],
        ),
        (
            "invoked only as a path call",
            "fn h(f: impl Fn() -> bool) { let _ = m::f(); let _ = &f; }",
            vec![],
        ),
        (
            "rebound by let",
            "fn h(f: impl Fn() -> bool) { let f = || true; let _ = f(); }",
            vec![],
        ),
        (
            "rebound by match",
            "fn h(f: Option<fn() -> bool>) { match f { Some(f) => { f(); } None => {} } }",
            vec![],
        ),
        (
            "expression statement",
            "fn h(f: impl Fn() -> bool) { f(); }",
            vec![(0, "f")],
        ),
        (
            "tail call",
            "fn h(f: impl Fn() -> bool) -> bool { f() }",
            vec![(0, "f")],
        ),
        (
            "after an unrelated if without an exit",
            "fn h(f: impl Fn() -> bool, w: bool) { if w { let _ = w; } let _ = f(); }",
            vec![(0, "f")],
        ),
        (
            "under if false",
            "fn h(f: impl Fn() -> bool) { if false { let _ = f(); } }",
            vec![],
        ),
        (
            "under if let",
            "fn h(f: impl Fn() -> bool, o: Option<u8>) { if let Some(_) = o { f(); } }",
            vec![],
        ),
        (
            "under match",
            "fn h(f: impl Fn() -> bool, o: u8) { match o { 1 => { f(); } _ => {} } }",
            vec![],
        ),
        (
            "under a loop",
            "fn h(f: impl Fn() -> bool) { loop { f(); break; } }",
            vec![],
        ),
        (
            "under a for",
            "fn h(f: impl Fn() -> bool) { for _ in 0..1 { f(); } }",
            vec![],
        ),
        (
            "under a while",
            "fn h(f: impl Fn() -> bool) { while false { f(); } }",
            vec![],
        ),
        (
            "after an early return",
            "fn h(f: impl Fn() -> bool, w: bool) { if w { return; } let _ = f(); }",
            vec![],
        ),
        (
            "after a question mark",
            "fn h(f: impl Fn() -> bool, r: Result<u8, u8>) -> Result<(), u8> { let _ = r?; let _ = f(); Ok(()) }",
            vec![],
        ),
        (
            "after an opaque macro",
            "fn h(f: impl Fn() -> bool) { early_exit!(); let _ = f(); }",
            vec![],
        ),
        (
            "after an assertion macro",
            "fn h(f: impl Fn() -> bool) { assert!(true); let _ = f(); }",
            vec![(0, "f")],
        ),
        (
            "after a transparent macro hiding an exit",
            "fn h(f: impl Fn() -> bool) { let _ = format!(\"{}\", { return; 1 }); let _ = f(); }",
            vec![],
        ),
        (
            "after a transparent macro with a benign argument",
            "fn h(f: impl Fn() -> bool, x: u8) { let _ = format!(\"{}\", x); let _ = f(); }",
            vec![(0, "f")],
        ),
        (
            "after a loop",
            "fn h(f: impl Fn() -> bool) { loop { break; } let _ = f(); }",
            vec![],
        ),
        (
            "let else",
            "fn h(f: impl Fn() -> Option<u8>) { let Some(_) = f() else { return; }; }",
            vec![],
        ),
        (
            "result used through a call chain",
            "fn h(f: impl Fn() -> bool) { let _ = f().then(|| 1); }",
            vec![],
        ),
        (
            "mut parameter, never reassigned",
            "fn h(mut f: impl FnMut() -> bool) { let _ = f(); }",
            vec![],
        ),
        (
            "reassigned before the call",
            "fn h(mut f: fn() -> bool) { f = other; let _ = f(); }",
            vec![],
        ),
        (
            "parenthesized assignment target",
            "fn h(mut f: fn() -> String) { (f) = always_hi; let _ = f(); }",
            vec![],
        ),
        (
            "destructuring assignment",
            "fn h(mut f: fn() -> String) { (f,) = (always_hi as fn() -> String,); let _ = f(); }",
            vec![],
        ),
        (
            "mem replace through a parenthesized mutable borrow",
            "fn h(mut f: fn() -> String) { std::mem::replace(&mut (f), always_hi); let _ = f(); }",
            vec![],
        ),
        (
            "reassigned inside a macro argument",
            "fn h(mut f: fn() -> bool) { m!(f = other); let _ = f(); }",
            vec![],
        ),
        (
            "mutable borrow of an immutable name",
            "fn h(f: fn() -> bool) { swap(&mut f); let _ = f(); }",
            vec![],
        ),
        (
            "pattern parameter",
            "fn h((f, _g): (fn() -> bool, u8)) { f(); }",
            vec![],
        ),
        (
            "binding with at pattern",
            "fn h(f @ _: fn() -> bool) { f(); }",
            vec![],
        ),
    ] {
        let want = want
            .into_iter()
            .map(|(position, name)| (position, name.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(invoked(helper)?, want, "{shape}");
    }
    Ok(())
}

#[test]
fn invoked_parameter_reports_the_first_call_line() -> Result<(), Box<dyn Error>> {
    let params = invoked_callback_params(
        "fn h(f: impl Fn() -> bool) {\n    let a = 1;\n    let _ = f();\n    let _ = f();\n}",
    )
    .ok_or("premise: parses")?;

    assert_eq!(params.len(), 1);
    assert_eq!((params[0].end_line, params[0].end_column), (3, 16));
    Ok(())
}

#[test]
fn methods_and_unparsable_text_have_no_invoked_parameters() {
    assert_eq!(
        invoked_callback_params("impl S { fn h(&self, f: impl Fn()) { f(); } }"),
        None
    );
    assert_eq!(invoked_callback_params("fn h(f: impl Fn() {"), None);
    assert_eq!(invoked_callback_params("fn h(f: impl Fn());"), None);
}

#[test]
fn invocation_end_orders_statements_sharing_a_line() -> Result<(), Box<dyn Error>> {
    let params = invoked_callback_params("fn h(f: impl Fn() -> bool) { let g = f(); assert!(g); }")
        .ok_or("premise: parses")?;

    assert_eq!(params.len(), 1);
    assert_eq!(params[0].end_line, 1);
    // The invoking statement ends right after its `;`, before the assertion.
    assert_eq!(params[0].end_column, 41);
    Ok(())
}

#[test]
fn macro_sites_come_from_the_parser_not_from_text() -> Result<(), Box<dyn Error>> {
    let text = "fn h() {\n    /* assert_eq!(a, b); */ let g = f(); assert_eq!(a, b);\n    let s = \"assert_eq!(a, b);\"; m!(x);\n}";

    let commented = macro_call_columns(text, 2).ok_or("premise: parses")?;
    assert_eq!(commented.len(), 1, "{commented:?}");
    assert_eq!(commented[0].0, "assert_eq!(a, b)");
    assert_eq!(
        commented[0].1,
        text.lines()
            .nth(1)
            .and_then(|l| l.rfind("assert_eq!"))
            .ok_or("premise")?
    );

    let in_string = macro_call_columns(text, 3).ok_or("premise: parses")?;
    assert_eq!(in_string.len(), 1, "{in_string:?}");
    assert_eq!(in_string[0].0, "m!(x)");
    assert_eq!(macro_call_columns(text, 1), Some(Vec::new()));
    assert_eq!(macro_call_columns("fn h( {", 1), None);
    Ok(())
}

#[test]
fn any_call_making_closure_literal_handed_to_the_helper_is_withheld() {
    for (shape, test, expected) in [
        (
            "call-making closure",
            "fn t() { run(1, || greet()); }",
            true,
        ),
        ("method-call closure", "fn t() { run(1, || x.go()); }", true),
        (
            "closure at a later position",
            "fn t() { run(|| greet(), 1); }",
            true,
        ),
        (
            "call under a branch the test may skip",
            "fn t() { if c { run(1, || greet()); } }",
            true,
        ),
        (
            "async closure with a call",
            "fn t() { run(1, async || greet()); }",
            true,
        ),
        ("unparsable text fails closed", "fn t( {", true),
        (
            "boxed closure",
            "fn t() { run(1, Box::new(|| greet())); }",
            true,
        ),
        (
            "referenced closure",
            "fn t() { run(1, &|| greet()); }",
            true,
        ),
        (
            "parenthesized closure",
            "fn t() { run(1, (|| greet())); }",
            true,
        ),
        (
            "closure only inside a nested fn item",
            "fn t() { run(1, { fn n() { let _ = || greet(); } 1 }); }",
            false,
        ),
        ("closure without a call", "fn t() { run(1, || 5); }", false),
        ("no closure argument", "fn t() { run(1, 2); }", false),
        (
            "closure bound to a name",
            "fn t() { let f = || greet(); run(1, f); }",
            false,
        ),
        (
            "closure handed to a different callee",
            "fn t() { other(|| greet()); run(1, 2); }",
            false,
        ),
        (
            "path call is not the helper",
            "fn t() { m::run(1, || greet()); }",
            false,
        ),
    ] {
        assert_eq!(passes_call_making_closure(test, "run"), expected, "{shape}");
    }
}
