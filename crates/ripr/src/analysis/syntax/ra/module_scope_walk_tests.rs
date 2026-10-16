use super::*;

// Frozen from the pre-optimization producer. Keep this independent of the
// candidate's collector: equality includes every scope map and full FileFacts.
fn original_module_item_scopes_in(
    source: &ast::SourceFile,
    line_index: &LineIndex,
) -> ModuleItemScopes {
    let mut scopes = ModuleItemScopes::default();
    for function in source.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(name), Some(fn_token)) = (function.name(), function.fn_token()) else {
            continue;
        };
        let key = (
            line_index.line(fn_token.text_range().start()),
            name.text().to_string(),
        );
        if function
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attribute| {
                let compact: String = attribute
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                compact.starts_with("#[cfg") || compact.starts_with("#![cfg")
            })
        {
            scopes.fns_with_cfg.insert(key.clone());
        }
        let bound = function
            .syntax()
            .descendants()
            .filter(|node| node != function.syntax())
            .filter_map(|node| {
                ast::AnyHasName::cast(node)
                    .and_then(|named| named.name())
                    .map(|name| name.text().to_string())
            })
            .collect();
        scopes.bound_names.insert(key.clone(), bound);
        if let Some(body) = function.body() {
            if body
                .syntax()
                .descendants()
                .any(|node| ast::Use::can_cast(node.kind()))
            {
                scopes.fns_with_local_use.insert(key.clone());
            }
            if function.async_token().is_some()
                || body.syntax().descendants().any(|node| {
                    ast::ClosureExpr::can_cast(node.kind())
                        || ast::BlockExpr::cast(node)
                            .is_some_and(|block| block.async_token().is_some())
                })
            {
                scopes.fns_with_deferred_code.insert(key.clone());
            }
            let called = body
                .syntax()
                .descendants()
                .filter_map(ast::CallExpr::cast)
                // A call inside a closure, `async` block or nested `fn` may
                // never run.
                .filter(|call| !enclosed_by_nested_fn_like(call.syntax(), &body))
                .filter_map(|call| match call.expr()? {
                    ast::Expr::PathExpr(path) => {
                        let path = path.path()?;
                        if path.qualifier().is_some() {
                            return None;
                        }
                        Some(path.segment()?.name_ref()?.text().to_string())
                    }
                    _ => None,
                })
                .collect();
            scopes.direct_calls.insert(key.clone(), called);
        }
        let Some(parent) = function.syntax().parent() else {
            continue;
        };
        let module = match parent.kind() {
            ra_ap_syntax::SyntaxKind::SOURCE_FILE => None,
            ra_ap_syntax::SyntaxKind::ITEM_LIST => {
                match parent.parent().and_then(ast::Module::cast) {
                    Some(module) => {
                        let range = module.syntax().text_range();
                        Some((
                            line_index.line(range.start()),
                            line_index.line_for_range_end(range.end()),
                        ))
                    }
                    None => continue,
                }
            }
            _ => continue,
        };
        scopes.item_fns.insert(key, module);
    }
    scopes
}

fn compare(text: &str) -> Result<ModuleItemScopes, String> {
    let parse = parse_clean_source_file(text).ok_or("fixture must parse cleanly")?;
    let lines = LineIndex::new(text);
    let expected = original_module_item_scopes_in(&parse.tree(), &lines);
    let actual = module_item_scopes_in(&parse.tree(), &lines);
    assert!(!actual.bound_names.is_empty(), "nonempty scope subjects");
    let path = Path::new("src/fixture.rs");
    let current = summarize_file_with_parser(path, text);
    let reference = summarize_file_with_item_scopes(path, text, original_module_item_scopes_in);
    assert!(
        current.is_ok(),
        "fixture must produce FileFacts: {current:?}"
    );
    assert_eq!(current, reference, "complete FileFacts changed");
    assert_eq!(
        actual, expected,
        "all module scope fields before name filtering"
    );
    Ok(actual)
}

#[test]
fn module_scope_walk_nested_visibility_and_shadowing() -> Result<(), String> {
    let facts = compare(
        r#"
fn leaf() {}
fn outer(arg: i32) {
    let leaf = 1;
    fn nested() { nested_leaf(); }
    leaf();
    { fn block_item() {} }
    let deferred = || { closure_leaf(); };
    let future = async { async_leaf(); };
    let moved = async move { moved_leaf(); };
    mod inside { pub fn inner() { inner_leaf(); } }
}
mod sibling { pub(crate) fn sibling() { sibling_leaf(); } }
impl Thing { pub fn method() { method_leaf(); } }
trait Interface { fn declared(); fn defaulted() { default_leaf(); } }
"#,
    )?;
    let outer = facts
        .bound_names
        .iter()
        .find(|((_, n), _)| n == "outer")
        .ok_or("outer bindings")?;
    assert!(outer.1.contains("leaf") && outer.1.contains("nested") && outer.1.contains("arg"));
    assert!(
        !facts
            .item_fns
            .keys()
            .any(|(_, n)| n == "nested" || n == "method")
    );
    let calls = facts.direct_calls.get(outer.0).ok_or("outer calls")?;
    assert!(calls.contains("leaf"));
    assert!(!calls.contains("inner_leaf"));
    assert!(
        !calls.contains("nested_leaf")
            && !calls.contains("closure_leaf")
            && !calls.contains("async_leaf")
    );
    Ok(())
}

#[test]
fn module_scope_walk_cfg_at_all_depths() -> Result<(), String> {
    let facts = compare(
        r#"
#[cfg_attr(feature = "x", inline)]
fn outer() {
    #![cfg(any())]
    #[ cfg ( all() ) ] let x = 1;
    fn nested() { #[cfg(test)] let y = 2; }
}
fn only_nested() { fn hidden() { #[cfg(any())] let x = 0; } }
fn macro_only() { opaque! { #[cfg(any())] fn hidden() {} } }
"#,
    )?;
    assert!(facts.fns_with_cfg.iter().any(|(_, n)| n == "only_nested"));
    assert!(!facts.fns_with_cfg.iter().any(|(_, n)| n == "macro_only"));
    Ok(())
}

#[test]
fn module_scope_walk_raw_identifiers_and_signature_bindings() -> Result<(), String> {
    compare(
        r#"
fn r#type<T>(r#match: i32, (a, b): (i32, i32)) {
    let r#loop = r#match;
    let pair = |r#move: i32| r#move;
    match r#loop { value => { let rebound = value; } }
    for element in [1] { let local = element; }
    if let Some(binding) = Some(1) { leaf(binding); }
    r#check(); qualified::check(); receiver.check();
}
fn r#check() {}
fn signature<const N: usize>() -> [u8; { signature_call() }] { [0; 0] }
"#,
    )?;
    Ok(())
}

#[test]
fn module_scope_walk_uses_deferred_and_bodyless() -> Result<(), String> {
    let facts = compare(
        r#"
async fn future() { leaf(); }
fn plain() { use crate::leaf as check; check(); }
fn nested_use() { let f = || { use crate::leaf; leaf(); }; }
fn nested_async() { fn local() { let f = async { leaf(); }; } }
trait Thing { async fn declared(); fn other(); fn defaulted() { leaf(); } }
"#,
    )?;
    assert!(
        facts
            .fns_with_local_use
            .iter()
            .any(|(_, n)| n == "nested_use")
    );
    assert!(
        !facts
            .fns_with_deferred_code
            .iter()
            .any(|(_, n)| n == "declared")
    );
    assert!(!facts.direct_calls.keys().any(|(_, n)| n == "declared"));
    Ok(())
}

#[test]
fn module_scope_walk_macro_strings_and_external_modules() -> Result<(), String> {
    compare(
        r#"
mod external;
macro_rules! opaque { () => { fn hidden() { use x; hidden_call(); } }; }
fn outer() {
    opaque!();
    let text = "fn fake() { use x; fake(); }";
    // #[cfg(test)] fn comment() { comment(); }
    leaf();
}
mod a { mod b { pub(super) fn visible() { leaf(); } } }
"#,
    )?;
    Ok(())
}

#[test]
fn module_scope_walk_same_line_keys_and_crlf_unicode() -> Result<(), String> {
    compare(
        "fn r#type() { let café = 1; } mod café { fn r#type() {} }\r\nfn outer() { fn outer() { leaf(); } }\r\n",
    )?;
    Ok(())
}

#[test]
fn module_scope_walk_real_sources() -> Result<(), String> {
    for text in [
        include_str!("../ra.rs"),
        include_str!("../../facts/test_helpers.rs"),
    ] {
        compare(text)?;
    }
    Ok(())
}

#[test]
fn module_scope_walk_refusal_parity() {
    for text in ["fn broken( {", "fn open() {"] {
        assert_eq!(
            summarize_file_with_parser(Path::new("src/lib.rs"), text),
            summarize_file_with_item_scopes(
                Path::new("src/lib.rs"),
                text,
                original_module_item_scopes_in
            )
        );
        assert!(module_item_scopes(text).is_none());
    }
}
