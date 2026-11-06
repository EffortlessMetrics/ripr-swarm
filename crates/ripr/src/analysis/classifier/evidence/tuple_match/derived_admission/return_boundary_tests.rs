//! The projection must be observed before the containing test returns.

use super::{named_function, parsed, top_level_projection_observes};

const RECEIPTS: &str = r#"let receipts = vec![Receipt { id: "receipt-1".to_string() }];"#;
const OWNER: &str = "let terminal = terminalize_proof(&receipts);";
const LENGTH: &str = "assert_eq!(terminal.len(), 1);";
const IDENTITY: &str = r#"assert_eq!(terminal[0].0.id, "receipt-1");"#;
const RELATION: &str = r#"assert_eq!(terminal[0].1, "request_identity_v2");"#;

fn admission(body: &str) -> Result<Option<bool>, String> {
    let source = format!("#[test] fn projection() {{ {body} }}");
    let root = parsed(&source).ok_or("return-boundary fixture must parse")?;
    let function = named_function(&root, "projection")
        .ok_or("return-boundary fixture must have one projection test")?;
    Ok(top_level_projection_observes(
        &function,
        "terminalize_proof",
        "request_identity_v2",
    ))
}

#[test]
fn projection_return_boundary_requires_every_observation_before_return() -> Result<(), String> {
    for assertions in [
        [LENGTH, IDENTITY, RELATION],
        [LENGTH, RELATION, IDENTITY],
        [IDENTITY, LENGTH, RELATION],
        [IDENTITY, RELATION, LENGTH],
        [RELATION, LENGTH, IDENTITY],
        [RELATION, IDENTITY, LENGTH],
    ] {
        let statements = [RECEIPTS, OWNER, assertions[0], assertions[1], assertions[2]];
        assert_eq!(admission(&statements.join("\n"))?, Some(true));
        // Return before the input, owner, or any required observation must
        // reject. A return after all observations must not over-reject.
        for position in 0..=statements.len() {
            let mut with_return = statements.to_vec();
            with_return.insert(position, "return;");
            assert_eq!(
                admission(&with_return.join("\n"))?,
                Some(position == statements.len()),
                "return position {position}; assertions {assertions:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn projection_return_boundary_does_not_search_nested_scopes_or_text() -> Result<(), String> {
    for prefix in [
        "let _not_called = || { return; };",
        "fn not_called() { return; }",
        "// return; is only a comment\n",
        r#"let _note = "return;";"#,
    ] {
        let body = [prefix, RECEIPTS, OWNER, LENGTH, IDENTITY, RELATION].join("\n");
        assert_eq!(admission(&body)?, Some(true), "{prefix}");
    }
    Ok(())
}

#[test]
fn projection_return_boundary_preserves_whole_body_owner_uniqueness() -> Result<(), String> {
    let body = [
        RECEIPTS,
        OWNER,
        LENGTH,
        IDENTITY,
        RELATION,
        "return;",
        "let another = terminalize_proof(&receipts);",
    ]
    .join("\n");
    assert_eq!(admission(&body)?, None);
    Ok(())
}

#[test]
fn projection_conditional_or_diverging_exit_before_observation_stays_unverified()
-> Result<(), String> {
    // Each exit sits between the owner call and the last observation, so the
    // later assertions are not unconditionally reachable. The complete
    // control (no exit) must still promote, and a trailing-only exit after
    // every observation must not over-reject.
    let complete = [RECEIPTS, OWNER, LENGTH, IDENTITY, RELATION];
    assert_eq!(admission(&complete.join("\n"))?, Some(true));
    for exit in [
        "if terminal.is_empty() { return; }",
        "if terminal.is_empty() { return Default::default(); }",
        "if terminal.is_empty() { panic!(\"empty\"); }",
        "let _checked = fallible()?;",
        "match terminal.len() { 0 => return, _ => {} }",
        "loop { break; }",
        "match terminal.len() { 0 => unreachable!(), _ => {} }",
        "if terminal.is_empty() { std::process::exit(1); }",
    ] {
        for position in 2..complete.len() {
            let mut body = complete.to_vec();
            body.insert(position, exit);
            assert_eq!(
                admission(&body.join("\n"))?,
                None,
                "exit {exit:?} at position {position}"
            );
        }
        let mut trailing = complete.to_vec();
        trailing.push(exit);
        // A block-like exit as the last item would parse as a tail expression.
        trailing.push("let _after = 0;");
        assert_eq!(
            admission(&trailing.join("\n"))?,
            Some(true),
            "trailing {exit:?}"
        );
    }
    Ok(())
}

#[test]
fn projection_exit_inside_uninvoked_closure_or_item_does_not_bound() -> Result<(), String> {
    for prefix in [
        "let _f = || { if true { return; } };",
        "fn helper() { if true { return; } }",
        "let _g = |x: u8| -> Option<u8> { Some(fallible(x)?) };",
    ] {
        let body = [RECEIPTS, OWNER, prefix, LENGTH, IDENTITY, RELATION].join("\n");
        assert_eq!(admission(&body)?, Some(true), "{prefix}");
    }
    Ok(())
}
