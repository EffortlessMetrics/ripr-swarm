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

/// #14385: when the owner's tail is an `if`/`else`, each branch block's tail
/// value is what the owner returns, so it is probed like the tail itself.
/// `else if` chains and branch tails that are nested `if`s recurse; branch
/// tails that are `match`/loops follow `is_tail_return_value_text` and get no
/// shape. A value on a line of its `if`'s condition gets none: an inline branch
/// already flows to the return through that line's predicate (#7626), and a
/// second shape there would add a row the predicate already judges. An edit
/// on that line that misses the condition is the value's, not the
/// predicate's; diff probing retargets it through
/// [`inline_branch_edit`].
pub(super) fn push_tail_if_branch_return_values(
    shapes: &mut Vec<ProbeShapeFact>,
    line_index: &LineIndex,
    text: &str,
    source: &Arc<str>,
    tail: &ast::Expr,
) {
    let ast::Expr::IfExpr(if_expr) = tail else {
        return;
    };
    let condition_lines = if_expr.condition().map(|condition| {
        let range = condition.syntax().text_range();
        line_index.line(range.start())..=line_index.line_for_range_end(range.end())
    });
    let mut branch_tails = Vec::new();
    if let Some(value) = if_expr.then_branch().and_then(|block| block.tail_expr()) {
        branch_tails.push(value);
    }
    match if_expr.else_branch() {
        Some(ast::ElseBranch::Block(block)) => {
            if let Some(value) = block.tail_expr() {
                branch_tails.push(value);
            }
        }
        Some(ast::ElseBranch::IfExpr(nested)) => branch_tails.push(ast::Expr::IfExpr(nested)),
        None => return,
    }
    for value in branch_tails {
        let range = value.syntax().text_range();
        let value_text = slice_text(text, range.start(), range.end());
        let value_line = line_index.line(range.start());
        let own_line = !condition_lines
            .as_ref()
            .is_some_and(|lines| lines.contains(&value_line));
        if own_line && returnable_branch_value(&value, &value_text) {
            push_probe_shape(
                shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ReturnValue,
                range.start(),
                range.end(),
            );
            if has_error_path_text(&value_text) {
                push_probe_shape(
                    shapes,
                    line_index,
                    text,
                    source,
                    ProbeShapeKind::ErrorPath,
                    range.start(),
                    range.end(),
                );
            }
        }
        push_tail_if_branch_return_values(shapes, line_index, text, source, &value);
    }
}

/// Whether a branch tail of an owner-tail `if` reads as a returned value:
/// not a nested control-flow expression (decided by AST kind, not text, so
/// `if\n` or `match\n` formatting cannot admit the whole expression) and
/// not a comparison or boolean value. A comparison branch value is already a
/// predicate probe; a second, weaker return-value row on the same expression
/// would be a false actionable gap when the predicate is discriminated
/// (verdict corpus `spec-b-0159-recursive-helper`). Parentheses and `!` wrap
/// the same predicate, so they are peeled first.
fn returnable_branch_value(value: &ast::Expr, value_text: &str) -> bool {
    let control_flow = matches!(
        value,
        ast::Expr::IfExpr(_)
            | ast::Expr::MatchExpr(_)
            | ast::Expr::WhileExpr(_)
            | ast::Expr::ForExpr(_)
            | ast::Expr::LoopExpr(_)
    );
    let mut core = value.clone();
    loop {
        let inner = match &core {
            ast::Expr::ParenExpr(paren) => paren.expr(),
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Not) => {
                prefix.expr()
            }
            _ => None,
        };
        match inner {
            Some(inner) => core = inner,
            None => break,
        }
    }
    let predicate_value = ast::BinExpr::cast(core.syntax().clone()).is_some_and(|inner| {
        inner
            .op_kind()
            .is_some_and(|op| matches!(op, ast::BinaryOp::CmpOp(_) | ast::BinaryOp::LogicOp(_)))
    });
    !control_flow && !predicate_value && is_tail_return_value_text(value_text)
}

/// Where an edit on an `if`'s condition line falls when it misses the
/// condition and lies wholly inside one branch block (F1:
/// `if price > 100 { 10 } else { 0 }` to `{ 15 }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InlineBranchEdit {
    /// The branch block's tail value when it holds the edit and shares the
    /// condition's line, with whether the owner returns it.
    pub(crate) value: Option<(std::ops::Range<usize>, bool)>,
}

/// The branch an edit on an `if`'s condition line changed, when the edit
/// missed the condition. The `if` is the one whose condition starts at
/// `condition_start`; `edit` is the changed byte range of the head line.
/// `Some` only when the edit starts past the condition's end and lies wholly
/// inside the `then` block or a plain `else` block (an `else if` link is a
/// predicate of its own and is not read here). A tail value is reported only
/// on the condition's line; its flag says whether the owner returns it: the
/// `if` is the owner body's tail, possibly through `else if` links and branch
/// tails, it has an `else`, and the value passes [`returnable_branch_value`],
/// the rule that gives the same value on its own line a `return_value` shape
/// (#14385), minus error values (their `error_path` twin is not modeled
/// here). `source` is the text `root` was parsed from. Anything the parse
/// cannot place answers `None`.
pub(crate) fn inline_branch_edit(
    root: &ast::SourceFile,
    source: &str,
    condition_start: usize,
    edit: std::ops::Range<usize>,
) -> Option<InlineBranchEdit> {
    let condition_offset = u32::try_from(condition_start).ok()?;
    let token = root
        .syntax()
        .token_at_offset(condition_offset.into())
        .right_biased()?;
    let if_expr = token.parent_ancestors().find_map(|node| {
        let if_expr = ast::IfExpr::cast(node)?;
        let condition = if_expr.condition()?;
        (usize::from(condition.syntax().text_range().start()) == condition_start).then_some(if_expr)
    })?;
    let condition_end = usize::from(if_expr.condition()?.syntax().text_range().end());
    // An empty `edit` is a pure deletion at that point.
    if edit.start < condition_end {
        return None;
    }
    let covers = |range: ra_ap_syntax::TextRange| {
        usize::from(range.start()) <= edit.start && edit.end <= usize::from(range.end())
    };
    let mut blocks = Vec::new();
    if let Some(block) = if_expr.then_branch() {
        blocks.push(block);
    }
    if let Some(ast::ElseBranch::Block(block)) = if_expr.else_branch() {
        blocks.push(block);
    }
    let block = blocks
        .into_iter()
        .find(|block| covers(block.syntax().text_range()))?;
    let value = block
        .tail_expr()
        .filter(|value| covers(value.syntax().text_range()))
        .and_then(|value| {
            let range = value.syntax().text_range();
            let range = usize::from(range.start())..usize::from(range.end());
            // A value on its own line already has its own shape.
            if source.get(condition_end..range.start)?.contains('\n') {
                return None;
            }
            let value_text = source.get(range.clone())?;
            let returned = if_expr.else_branch().is_some()
                && returnable_branch_value(&value, value_text)
                && !has_error_path_text(value_text)
                && if_is_owner_tail(&if_expr);
            Some((range, returned))
        });
    Some(InlineBranchEdit { value })
}

/// Whether `if_expr` is what its owner `fn` returns as its body's tail,
/// directly or as the tail of a branch (or `else if`) of such an `if`. The
/// same chain [`push_tail_if_branch_return_values`] walks down.
fn if_is_owner_tail(if_expr: &ast::IfExpr) -> bool {
    let mut node = if_expr.syntax().clone();
    loop {
        let Some(parent) = node.parent() else {
            return false;
        };
        if let Some(outer) = ast::IfExpr::cast(parent.clone()) {
            // `else if`: the nested `if` is the outer one's else branch.
            if matches!(outer.else_branch(), Some(ast::ElseBranch::IfExpr(nested)) if nested.syntax() == &node)
            {
                node = parent;
                continue;
            }
            return false;
        }
        // A block's tail sits in its statement list.
        let is_tail = ast::StmtList::cast(parent.clone())
            .and_then(|list| list.tail_expr())
            .is_some_and(|tail| tail.syntax() == &node);
        if !is_tail {
            return false;
        }
        let Some(block) = parent.parent().and_then(ast::BlockExpr::cast) else {
            return false;
        };
        // A block's syntax parent is the `fn` for the owner body, the `if`
        // for a branch block.
        let Some(holder) = block.syntax().parent() else {
            return false;
        };
        if ast::Fn::can_cast(holder.kind()) {
            return true;
        }
        if ast::IfExpr::can_cast(holder.kind()) {
            node = holder;
            continue;
        }
        return false;
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
        // RIPR-SPEC-0360: the update of a zero-initialized accumulator that
        // the owner returns is the returned value's change, probed as
        // `return_value`.
        if crate::analysis::syntax::loop_accumulator::loop_accumulator(function, &bin_expr)
            .is_some()
        {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                source,
                ProbeShapeKind::ReturnValue,
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
        push_tail_if_branch_return_values(&mut shapes, line_index, text, source, &tail_expr);
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
