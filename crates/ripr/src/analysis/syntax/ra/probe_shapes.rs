use super::*;

// Keep typed descendants only for the duration of this function's shape extraction.
// A single parser walk replaces nine repeated descendant traversals.
// Separate buckets retain the original category order, including equal sort keys.
#[derive(Default)]
struct ProbeShapeNodes {
    if_exprs: Vec<ast::IfExpr>,
    while_exprs: Vec<ast::WhileExpr>,
    bin_exprs: Vec<ast::BinExpr>,
    return_exprs: Vec<ast::ReturnExpr>,
    call_exprs: Vec<ast::CallExpr>,
    method_calls: Vec<ast::MethodCallExpr>,
    fields: Vec<ast::RecordExprField>,
    match_exprs: Vec<ast::MatchExpr>,
    arms: Vec<ast::MatchArm>,
}

impl ProbeShapeNodes {
    fn collect(function: &ast::Fn) -> Self {
        let mut nodes = Self::default();
        for node in function.syntax().descendants() {
            match node.kind() {
                ra_ap_syntax::SyntaxKind::IF_EXPR => {
                    if let Some(value) = ast::IfExpr::cast(node) {
                        nodes.if_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::WHILE_EXPR => {
                    if let Some(value) = ast::WhileExpr::cast(node) {
                        nodes.while_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::BIN_EXPR => {
                    if let Some(value) = ast::BinExpr::cast(node) {
                        nodes.bin_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::RETURN_EXPR => {
                    if let Some(value) = ast::ReturnExpr::cast(node) {
                        nodes.return_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::CALL_EXPR => {
                    if let Some(value) = ast::CallExpr::cast(node) {
                        nodes.call_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::METHOD_CALL_EXPR => {
                    if let Some(value) = ast::MethodCallExpr::cast(node) {
                        nodes.method_calls.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::RECORD_EXPR_FIELD => {
                    if let Some(value) = ast::RecordExprField::cast(node) {
                        nodes.fields.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::MATCH_EXPR => {
                    if let Some(value) = ast::MatchExpr::cast(node) {
                        nodes.match_exprs.push(value);
                    }
                }
                ra_ap_syntax::SyntaxKind::MATCH_ARM => {
                    if let Some(value) = ast::MatchArm::cast(node) {
                        nodes.arms.push(value);
                    }
                }
                _ => {}
            }
        }
        nodes
    }
}

pub(super) fn extract_parser_probe_shapes(
    function: &ast::Fn,
    text: &str,
    source: &Arc<str>,
    line_index: &LineIndex,
) -> Vec<ProbeShapeFact> {
    let nodes = ProbeShapeNodes::collect(function);
    let mut shapes = Vec::new();
    push_unsafe_boundary_probe_shapes(&mut shapes, function, source, line_index);

    for if_expr in nodes.if_exprs {
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

    for while_expr in nodes.while_exprs {
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

    for bin_expr in nodes.bin_exprs {
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
    for return_expr in nodes.return_exprs {
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

    for call_expr in nodes.call_exprs {
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

    for method_call in nodes.method_calls {
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

    for field in nodes.fields {
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

    for match_expr in nodes.match_exprs {
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

    for arm in nodes.arms {
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
