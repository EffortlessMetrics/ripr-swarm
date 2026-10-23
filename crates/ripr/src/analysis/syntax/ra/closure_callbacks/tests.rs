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
            "mut parameter",
            "fn h(mut f: impl FnMut() -> bool) { let g = f(); assert!(g); }",
            vec![(0, "f")],
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
            "reassigned before the call",
            "fn h(mut f: fn() -> bool) { f = other; let _ = f(); }",
            vec![],
        ),
        (
            "compound or later reassignment",
            "fn h(mut f: fn() -> bool) { let _ = f(); f = other; }",
            vec![],
        ),
        (
            "mutably borrowed",
            "fn h(mut f: fn() -> bool) { swap(&mut f); let _ = f(); }",
            vec![],
        ),
        (
            "reassigned inside a macro argument",
            "fn h(mut f: fn() -> bool) { m!(f = other); let _ = f(); }",
            vec![],
        ),
        (
            "mutable but never reassigned",
            "fn h(mut f: impl FnMut() -> bool) { let _ = f(); }",
            vec![(0, "f")],
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
