//! Frozen nine-walk oracle from main 14afcde1c; do not optimize this oracle.
use super::*;

#[test]
fn arm_geometry_refuses_macro_arrows_and_invalid_source() {
    assert!(
        match_arm_source_ranges("macro_rules! m { ($x:expr) => { $x }; } pub fn f() { m!(1); }")
            .is_empty()
    );
    assert!(match_arm_source_ranges("fn f() { match x { 0 =>").is_empty());
}

pub(super) fn reference_probe_shapes(
    function: &ast::Fn,
    text: &str,
    source: &Arc<str>,
    line_index: &LineIndex,
) -> Vec<ProbeShapeFact> {
    let mut shapes = Vec::new();
    push_unsafe_boundary_probe_shapes(&mut shapes, function, source, line_index);

    for if_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::IfExpr::cast)
    {
        if let Some(condition) = if_expr.condition() {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::Predicate,
                condition.syntax().text_range().start(),
                condition.syntax().text_range().end(),
            );
        }
    }

    for while_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::WhileExpr::cast)
    {
        if let Some(condition) = while_expr.condition() {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::Predicate,
                condition.syntax().text_range().start(),
                condition.syntax().text_range().end(),
            );
        }
    }

    for bin_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
    {
        if bin_expr
            .op_token()
            .is_some_and(|token| is_predicate_operator(token.text()))
        {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::Predicate,
                bin_expr.syntax().text_range().start(),
                bin_expr.syntax().text_range().end(),
            );
        }
        // RIPR-SPEC-0228: a field assignment (`self.f = ..`, `self.f op= ..`)
        // is a field write, probed as `field_construction` like a
        // constructed field instead of falling to `static_unknown`.
        if field_assignment_path(&bin_expr).is_some() {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::FieldConstruction,
                bin_expr.syntax().text_range().start(),
                bin_expr.syntax().text_range().end(),
            );
        }
    }

    let body = function.body();
    for return_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::ReturnExpr::cast)
    {
        let range = return_expr.syntax().text_range();
        push_probe_shape(
            &mut shapes,
            line_index,
            text,
            source,
            ProbeShapeKind::ReturnValue,
            range.start(),
            range.end(),
        );
        let return_text = slice_text(text, range.start(), range.end());
        // #6904: a `return` inside a closure, `async` block or nested `fn`
        // leaves that body, not the owner. Share the `direct_calls` filter.
        if has_error_path_text(&return_text)
            && !body
                .as_ref()
                .is_some_and(|body| enclosed_by_nested_fn_like(return_expr.syntax(), body))
        {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ErrorPath,
                range.start(),
                range.end(),
            );
        }
    }

    if let Some(tail_expr) = body.as_ref().and_then(|body| body.tail_expr()) {
        let range = tail_expr.syntax().text_range();
        let tail_text = slice_text(text, range.start(), range.end());
        if is_tail_return_value_text(&tail_text) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ReturnValue,
                range.start(),
                range.end(),
            );
            if has_error_path_text(&tail_text) {
                push_probe_shape(
                    &mut shapes,
                    line_index,
                    text,
                    source,
                    ProbeShapeKind::ErrorPath,
                    range.start(),
                    range.end(),
                );
            }
        }
    }

    for call_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
    {
        let range = call_expr.syntax().text_range();
        let call_text = slice_text(text, range.start(), range.end());
        push_call_deletion_probe_shape(&mut shapes, line_index, text, source, call_expr.syntax());
        if has_return_value_text(&call_text) && !call_is_argument(&call_expr) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ReturnValue,
                range.start(),
                range.end(),
            );
        }
        if has_error_path_text(&call_text) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ErrorPath,
                range.start(),
                range.end(),
            );
        }
    }

    for method_call in function
        .syntax()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
    {
        let range = method_call.syntax().text_range();
        let method_text = slice_text(text, range.start(), range.end());
        push_call_deletion_probe_shape(&mut shapes, line_index, text, source, method_call.syntax());
        if method_call
            .name_ref()
            .is_some_and(|name| is_effect_call_name(&name.syntax().text().to_string()))
            || has_effect_text(&method_text)
        {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::SideEffect,
                range.start(),
                range.end(),
            );
        }
    }

    for field in function
        .syntax()
        .descendants()
        .filter_map(ast::RecordExprField::cast)
    {
        let range = field.syntax().text_range();
        push_probe_shape(
            &mut shapes,
            line_index,
            text,
            source,
            ProbeShapeKind::FieldConstruction,
            range.start(),
            range.end(),
        );
    }

    for match_expr in function
        .syntax()
        .descendants()
        .filter_map(ast::MatchExpr::cast)
    {
        if let Some(token) = match_expr.match_token() {
            let snippet = match_expr_probe_text(
                text,
                match_expr.expr().map(|expr| expr.syntax().text_range()),
                match_expr.syntax().text_range(),
            );
            push_probe_shape_with_text(
                &mut shapes,
                line_index,
                ProbeShapeKind::MatchArm,
                token.text_range().start(),
                token.text_range().end(),
                link_shape_text(source, token.text_range().start(), snippet),
                Some((
                    line_index.line(token.text_range().start()),
                    line_index.line_for_range_end(
                        match_expr
                            .expr()
                            .map(|expr| expr.syntax().text_range().end())
                            .unwrap_or(token.text_range().end()),
                    ),
                )),
            );
        }
    }

    for arm in function
        .syntax()
        .descendants()
        .filter_map(ast::MatchArm::cast)
    {
        if let Some(token) = arm.fat_arrow_token() {
            let snippet = match_arm_probe_text(
                text,
                arm.syntax().text_range().start(),
                token.text_range().start(),
            );
            push_probe_shape_with_text(
                &mut shapes,
                line_index,
                ProbeShapeKind::MatchArm,
                token.text_range().start(),
                token.text_range().end(),
                link_shape_text(source, token.text_range().start(), snippet),
                Some((
                    line_index.line(
                        arm.pat()
                            .map(|pat| pat.syntax().text_range().start())
                            .unwrap_or(token.text_range().start()),
                    ),
                    line_index.line_for_range_end(token.text_range().end()),
                )),
            );
        }
    }

    shapes.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
            .then(a.kind.as_str().cmp(b.kind.as_str()))
            .then(a.text.cmp(&b.text))
    });
    shapes.dedup_by(|a, b| {
        a.start_line == b.start_line
            && a.end_line == b.end_line
            && a.kind == b.kind
            && a.text == b.text
    });
    shapes
}

fn equivalent_file_facts(text: &str) -> Result<FileFacts, String> {
    let path = Path::new("src/probe_shape_equivalence.rs");
    let actual = summarize_file_with_parser(path, text)?;
    let reference = summarize_file_with_probe_shapes(path, text, reference_probe_shapes)?;
    assert_eq!(actual, reference, "complete FileFacts changed");
    for (left, right) in actual.probe_shapes.iter().zip(&reference.probe_shapes) {
        assert_eq!(
            left.text.shared_source().is_some(),
            right.text.shared_source().is_some()
        );
        if let Some(source) = left.text.shared_source() {
            assert!(Arc::ptr_eq(source, &actual.source));
        }
        if let Some(source) = right.text.shared_source() {
            assert!(Arc::ptr_eq(source, &reference.source));
        }
    }
    Ok(actual)
}

const MIXED: &str = r#"
struct State { value: i32 }
unsafe fn raw(ptr: *const i32) -> i32 { unsafe { *ptr } }
fn mixed(mut x: i32, mut state: State) -> Result<i32, String> {
    if x > 0 { notify(); }
    while x < 3 { x += 1; }
    state.value = x;
    let _record = State { value: x };
    state.send();
    let y = match get(x) {
        Ok(v) if v > 2 => v,
        Ok(v) => v + 1,
        Err(e) => return Err(e),
    };
    Ok(y)
}
#[test]
fn observes() { assert_eq!(mixed(1, State { value: 0 }).unwrap(), 4); }
"#;

#[test]
fn probe_walk_full_facts_all_shape_categories() -> Result<(), String> {
    let facts = equivalent_file_facts(MIXED)?;
    assert_eq!(facts.functions.len(), 3);
    assert_eq!(facts.tests.len(), 1);
    for kind in [
        ProbeShapeKind::Predicate,
        ProbeShapeKind::ReturnValue,
        ProbeShapeKind::ErrorPath,
        ProbeShapeKind::CallDeletion,
        ProbeShapeKind::FieldConstruction,
        ProbeShapeKind::SideEffect,
        ProbeShapeKind::MatchArm,
        ProbeShapeKind::UnsafeBoundary,
    ] {
        assert!(
            facts.probe_shapes.iter().any(|shape| shape.kind == kind),
            "missing {kind:?}"
        );
    }
    assert!(
        facts
            .probe_shapes
            .iter()
            .any(|shape| shape.end_byte > shape.start_byte)
    );
    Ok(())
}

#[test]
fn probe_walk_full_facts_nested_return_targets_and_duplicate_shapes() -> Result<(), String> {
    let text = r#"
fn outer(x: i32) -> Result<i32, String> {
    fn inner() -> Result<i32, String> { return Err("inner".into()); }
    let deferred = || { return Err::<i32, _>("closure"); };
    let future = async { return Err::<i32, _>("async"); };
    let _ = (deferred, future);
    if x > 0 { return Err("outer".into()); }
    Ok(x)
}
"#;
    let facts = equivalent_file_facts(text)?;
    assert_eq!(facts.functions.len(), 2);
    let outer = facts
        .functions
        .iter()
        .find(|f| f.name == "outer")
        .ok_or("missing outer")?;
    let inner = facts
        .functions
        .iter()
        .find(|f| f.name == "inner")
        .ok_or("missing inner")?;
    assert!(outer.start_line < inner.start_line && outer.end_line > inner.end_line);
    // Nested function predicates/returns are still visited; only the existing
    // return-target exclusion controls ErrorPath ownership. File-level dedup
    // must remain identical when outer and inner contribute the same shape.
    assert!(
        facts
            .probe_shapes
            .iter()
            .any(|s| s.kind == ProbeShapeKind::ReturnValue && s.text.contains("inner"))
    );
    Ok(())
}

#[test]
fn probe_walk_full_facts_multiline_match_metadata_and_unicode() -> Result<(), String> {
    for ending in ["\n", "\r\n"] {
        let text = "// café 🦀\nfn choose(x: i32) -> i32 {\n    match\n        x + 1\n    {\n        1\n        | 2 if x > 0 => 4,\n        _ => 5,\n    }\n}\n".replace('\n', ending);
        let facts = equivalent_file_facts(&text)?;
        let arms: Vec<_> = facts
            .probe_shapes
            .iter()
            .filter(|s| s.kind == ProbeShapeKind::MatchArm)
            .collect();
        assert_eq!(arms.len(), 3);
        assert!(arms.iter().any(|s| s.start_line == 3));
        assert!(arms.iter().any(|s| s.start_line == 7));
    }
    Ok(())
}

#[test]
fn probe_walk_full_facts_macro_opacity_comments_and_same_line_nodes() -> Result<(), String> {
    let text = r##"
macro_rules! hidden { () => { if fake() { send(); } }; }
fn compact(x: i32) -> i32 { let s = r#"match fake() { _ => send() }"#; /* while fake() {} */ hidden!(); if x > 0 { get(x) } else { get(0) } }
fn sibling(x: i32) -> i32 { match x { 0 => 1, _ => 2 } }
"##;
    let facts = equivalent_file_facts(text)?;
    assert_eq!(facts.functions.len(), 2);
    assert!(!facts.probe_shapes.iter().any(|s| s.text.contains("fake")));
    assert!(
        facts
            .probe_shapes
            .iter()
            .any(|s| s.kind == ProbeShapeKind::MatchArm)
    );
    Ok(())
}

#[test]
fn probe_walk_full_facts_real_sources() -> Result<(), String> {
    for (path, text) in [
        (
            "analysis/classify/related_tests.rs",
            include_str!("../../classify/related_tests.rs"),
        ),
        (
            "analysis/workspace/module_graph.rs",
            include_str!("../../workspace/module_graph.rs"),
        ),
    ] {
        let actual = summarize_file_with_parser(Path::new(path), text)?;
        let reference =
            summarize_file_with_probe_shapes(Path::new(path), text, reference_probe_shapes)?;
        assert!(!actual.functions.is_empty());
        assert!(!actual.probe_shapes.is_empty());
        assert_eq!(actual, reference, "complete FileFacts changed for {path}");
    }
    Ok(())
}

#[test]
fn probe_walk_full_facts_parser_refusal_parity() {
    for text in ["fn broken( {", "fn valid() {}\nfn broken() {"] {
        let actual = summarize_file_with_parser(Path::new("bad.rs"), text);
        let reference =
            summarize_file_with_probe_shapes(Path::new("bad.rs"), text, reference_probe_shapes);
        assert!(actual.is_err());
        assert_eq!(actual, reference);
    }
}

#[test]
fn probe_walk_full_facts_bodyless_methods_and_empty_bodies() -> Result<(), String> {
    let text = r#"
trait Service {
    fn required(&self, value: i32) -> Result<i32, String>;
    fn provided(&self, value: i32) -> i32 { if value > 0 { value } else { 0 } }
}
impl Service for State {
    fn required(&self, value: i32) -> Result<i32, String> { Ok(value) }
}
fn empty() {}
"#;
    let facts = equivalent_file_facts(text)?;
    assert_eq!(facts.functions.len(), 4);
    assert!(
        facts
            .probe_shapes
            .iter()
            .any(|s| s.kind == ProbeShapeKind::Predicate)
    );
    Ok(())
}

#[test]
fn probe_walk_full_facts_equal_sort_keys_keep_first_span() -> Result<(), String> {
    let text = "fn repeated() { notify(); notify(); }";
    let facts = equivalent_file_facts(text)?;
    let calls: Vec<_> = facts
        .probe_shapes
        .iter()
        .filter(|s| s.kind == ProbeShapeKind::CallDeletion && s.text == "notify()")
        .collect();
    assert_eq!(calls.len(), 1);
    let start = text.find("notify()").ok_or("missing literal call")?;
    assert_eq!(calls[0].start_byte, start);
    assert_eq!(calls[0].end_byte, start + "notify()".len());
    assert!(!calls[0].value_consumed);
    Ok(())
}
