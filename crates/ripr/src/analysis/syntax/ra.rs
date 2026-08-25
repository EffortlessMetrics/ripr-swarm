use crate::domain::{OracleKind, OracleStrength, SymbolId};
use ra_ap_syntax::{
    AstNode, Edition, SourceFile, TextSize,
    ast::{self, HasAttrs, HasName},
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::facts::FileFacts;
use super::{RaRustSyntaxAdapter, RustSyntaxAdapter, SyntaxNodeFact, TextRange};
use crate::analysis::rust_index::{
    FunctionFact, OracleFact, PROBE_SHAPE_CALL_DELETION, PROBE_SHAPE_ERROR_PATH,
    PROBE_SHAPE_FIELD_CONSTRUCTION, PROBE_SHAPE_MATCH_ARM, PROBE_SHAPE_PREDICATE,
    PROBE_SHAPE_RETURN_VALUE, PROBE_SHAPE_SIDE_EFFECT, ProbeShapeFact, TestFact,
    classify_assertion, err_return_guard_oracles, extract_call_facts, extract_identifier_tokens,
    extract_line_scanned_oracles, extract_literal_facts, extract_return_facts,
    is_unwrap_err_bound_error_assertion, unwrap_err_bound_variables,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RustIncludeDirective {
    pub(crate) line: usize,
    pub(crate) expression: String,
    pub(crate) literal_path: Option<PathBuf>,
    pub(crate) is_file_level: bool,
}

/// Extract actual `include!` macro nodes from parser-backed Rust syntax.
/// Comments, string contents, and similarly named macros never enter this
/// producer. Non-literal token trees remain explicit unsupported directives.
pub(crate) fn rust_include_directives(
    _path: &Path,
    text: &str,
    max_directives: usize,
) -> Result<Vec<RustIncludeDirective>, String> {
    let parse = SourceFile::parse(text, Edition::CURRENT);
    if !parse.errors().is_empty() {
        return Err("rust_include_parent_parse_unavailable".to_string());
    }
    let line_index = LineIndex::new(text);
    let mut directives = Vec::new();
    for macro_call in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|macro_call| {
            macro_call
                .path()
                .is_some_and(|path| path.syntax().text().to_string().replace(' ', "") == "include")
        })
        .take(max_directives.saturating_add(1))
    {
        let range = macro_call.syntax().text_range();
        let expression = slice_text(text, range.start(), range.end());
        directives.push(RustIncludeDirective {
            line: line_index.line(range.start()),
            literal_path: include_literal_path(&expression),
            is_file_level: !macro_call
                .syntax()
                .ancestors()
                .skip(1)
                .any(|node| ast::Module::can_cast(node.kind())),
            expression,
        });
    }
    directives.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.expression.cmp(&right.expression))
    });
    Ok(directives)
}

fn include_literal_path(expression: &str) -> Option<PathBuf> {
    let (_, arguments) = expression.split_once('!')?;
    let arguments = arguments.trim();
    let inner = match (arguments.chars().next()?, arguments.chars().last()?) {
        ('(', ')') | ('{', '}') | ('[', ']') => arguments.get(1..arguments.len() - 1)?.trim(),
        _ => return None,
    };
    parse_rust_string_literal(inner).map(PathBuf::from)
}

fn parse_rust_string_literal(literal: &str) -> Option<String> {
    if let Some(body) = literal
        .strip_prefix('"')
        .and_then(|body| body.strip_suffix('"'))
    {
        let mut decoded = String::new();
        let mut chars = body.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                decoded.push(ch);
                continue;
            }
            match chars.next()? {
                '\\' => decoded.push('\\'),
                '"' => decoded.push('"'),
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                '0' => decoded.push('\0'),
                _ => return None,
            }
        }
        return Some(decoded);
    }

    let hash_count = literal
        .strip_prefix('r')?
        .chars()
        .take_while(|ch| *ch == '#')
        .count();
    let prefix_len = 1 + hash_count;
    let suffix = format!("\"{}", "#".repeat(hash_count));
    let body = literal.get(prefix_len..)?.strip_prefix('"')?;
    body.strip_suffix(&suffix).map(ToString::to_string)
}

impl RustSyntaxAdapter for RaRustSyntaxAdapter {
    fn summarize_file(&self, path: &Path, text: &str) -> Result<FileFacts, String> {
        summarize_file_with_parser(path, text)
    }

    fn changed_nodes(&self, facts: &FileFacts, ranges: &[TextRange]) -> Vec<SyntaxNodeFact> {
        owner_changed_nodes(facts, ranges)
    }
}

pub fn summarize_file_with_parser(path: &Path, text: &str) -> Result<FileFacts, String> {
    let parse = SourceFile::parse(text, Edition::CURRENT);
    let errors = parse.errors();
    if !errors.is_empty() {
        return Err(format!("parser reported {} syntax errors", errors.len()));
    }

    let source = parse.tree();
    let line_index = LineIndex::new(text);
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    let mut file_calls = Vec::new();
    let mut file_returns = Vec::new();
    let mut file_literals = Vec::new();
    let mut file_probe_shapes = Vec::new();
    let path_buf = path.to_path_buf();

    for function in source.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(name) = function.name().map(|name| name.text().to_string()) else {
            continue;
        };
        let fn_start = function
            .fn_token()
            .map(|token| token.text_range().start())
            .unwrap_or_else(|| function.syntax().text_range().start());
        let fn_end = function.syntax().text_range().end();
        let start_line = line_index.line(fn_start);
        let end_line = line_index.line_for_range_end(fn_end);
        let body = slice_text(text, fn_start, fn_end);
        let calls = extract_call_facts(&body, start_line);
        let returns = extract_return_facts(&body, start_line);
        let literals = extract_literal_facts(&body, start_line);
        let probe_shapes = extract_parser_probe_shapes(&function, text, &line_index);
        // A plain helper inside an inline `#[cfg(test)]` module is test
        // infrastructure even when it has no `#[test]` attribute. Classify
        // that role at the producer boundary so diff probes, seam inventory,
        // evidence relation, and every downstream renderer consume the same
        // fact instead of re-inferring it independently.
        let has_test_attribute = has_test_attribute(&function);
        let is_test = has_test_attribute || is_cfg_test_module_member(&function);
        let attrs = collect_attr_syntax(&function);

        file_calls.extend(calls.clone());
        file_returns.extend(returns.clone());
        file_literals.extend(literals.clone());
        file_probe_shapes.extend(probe_shapes);

        let function_fact = FunctionFact {
            id: parser_symbol_id(path, &function, &name),
            name: name.clone(),
            file: path_buf.clone(),
            start_line,
            end_line,
            body: body.clone(),
            calls: calls.clone(),
            returns: returns.clone(),
            literals: literals.clone(),
            is_test,
            attrs: attrs.clone(),
        };

        if has_test_attribute {
            tests.push(TestFact {
                name,
                file: path_buf.clone(),
                start_line,
                end_line,
                body,
                calls,
                assertions: extract_parser_oracles(&function, text, &line_index),
                literals,
                attrs,
            });
        }

        functions.push(function_fact);
    }

    disambiguate_duplicate_symbol_ids(&mut functions);

    file_calls.sort_by(|a, b| a.line.cmp(&b.line).then(a.name.cmp(&b.name)));
    file_calls.dedup_by(|a, b| a.line == b.line && a.name == b.name && a.text == b.text);
    file_returns.sort_by(|a, b| a.line.cmp(&b.line).then(a.text.cmp(&b.text)));
    file_returns.dedup_by(|a, b| a.line == b.line && a.text == b.text);
    file_literals.sort_by(|a, b| a.line.cmp(&b.line).then(a.value.cmp(&b.value)));
    file_literals.dedup_by(|a, b| a.line == b.line && a.value == b.value);
    file_probe_shapes.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
            .then(a.kind.cmp(&b.kind))
            .then(a.text.cmp(&b.text))
    });
    file_probe_shapes.dedup_by(|a, b| {
        a.start_line == b.start_line
            && a.end_line == b.end_line
            && a.kind == b.kind
            && a.text == b.text
    });

    Ok(FileFacts {
        path: path_buf,
        functions,
        tests,
        calls: file_calls,
        returns: file_returns,
        literals: file_literals,
        probe_shapes: file_probe_shapes,
        used_lexical_fallback: false,
        source: text.to_string(),
    })
}

fn parser_symbol_id(path: &Path, function: &ast::Fn, name: &str) -> SymbolId {
    let mut segments = vec![path.display().to_string()];

    let mut modules = function
        .syntax()
        .ancestors()
        .skip(1)
        .filter_map(ast::Module::cast)
        .filter_map(|module| {
            module
                .name()
                .map(|module_name| module_name.text().to_string())
        })
        .collect::<Vec<_>>();
    modules.reverse();
    segments.extend(modules);

    if let Some(impl_block) = function
        .syntax()
        .ancestors()
        .skip(1)
        .find_map(ast::Impl::cast)
    {
        segments.push(impl_owner_segment(&impl_block));
    }

    segments.push(name.to_string());
    SymbolId(segments.join("::"))
}

fn impl_owner_segment(impl_block: &ast::Impl) -> String {
    let self_ty = match impl_block.self_ty() {
        Some(ty) => compact_syntax_text(ty.syntax().text().to_string()),
        None => "unknown".to_string(),
    };
    match impl_block.trait_() {
        Some(trait_ty) => format!(
            "impl {} for {self_ty}",
            compact_syntax_text(trait_ty.syntax().text().to_string())
        ),
        None => format!("impl {self_ty}"),
    }
}

fn compact_syntax_text(text: String) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn disambiguate_duplicate_symbol_ids(functions: &mut [FunctionFact]) {
    let mut totals = BTreeMap::new();
    for function in functions.iter() {
        let entry = totals.entry(function.id.0.clone()).or_insert(0usize);
        *entry += 1;
    }

    for function in functions.iter_mut() {
        let total = match totals.get(&function.id.0) {
            Some(total) => *total,
            None => 0,
        };
        if total > 1 {
            function.id.0 = format!("{}#L{}", function.id.0, function.start_line);
        }
    }
}

fn has_test_attribute(function: &ast::Fn) -> bool {
    function.attrs().any(|attr| {
        let compact = attr
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        compact == "#[test]"
            || compact.starts_with("#[tokio::test")
            || compact.starts_with("#[async_std::test")
            || compact == "#[rstest]"
            || compact.starts_with("#[rstest(")
    })
}

fn is_cfg_test_module_member(function: &ast::Fn) -> bool {
    function
        .syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .any(|module| {
            module.attrs().any(|attr| {
                let compact = attr
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect::<String>();
                // #3284: `#[cfg(all(test, ...))]` gates the module on test
                // plus other predicates, which still means harness-only in
                // every non-test build that satisfies the other conjuncts
                // is NOT guaranteed — but the module is compiled under
                // test, so its helpers are evidence role. `any(test, ..)`
                // and `not(test)` stay excluded: they compile outside
                // test builds.
                compact == "#[cfg(test)]" || compact.starts_with("#[cfg(all(test,")
            })
        })
}

fn collect_attr_syntax(function: &ast::Fn) -> Vec<String> {
    function
        .attrs()
        .map(|attr| attr.syntax().text().to_string())
        .collect()
}

fn extract_parser_probe_shapes(
    function: &ast::Fn,
    text: &str,
    line_index: &LineIndex,
) -> Vec<ProbeShapeFact> {
    let mut shapes = Vec::new();
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
                PROBE_SHAPE_PREDICATE,
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
                PROBE_SHAPE_PREDICATE,
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
                PROBE_SHAPE_PREDICATE,
                bin_expr.syntax().text_range().start(),
                bin_expr.syntax().text_range().end(),
            );
        }
    }

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
            PROBE_SHAPE_RETURN_VALUE,
            range.start(),
            range.end(),
        );
        let return_text = slice_text(text, range.start(), range.end());
        if has_error_path_text(&return_text) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                PROBE_SHAPE_ERROR_PATH,
                range.start(),
                range.end(),
            );
        }
    }

    if let Some(tail_expr) = function.body().and_then(|body| body.tail_expr()) {
        let range = tail_expr.syntax().text_range();
        let tail_text = slice_text(text, range.start(), range.end());
        if is_tail_return_value_text(&tail_text) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                PROBE_SHAPE_RETURN_VALUE,
                range.start(),
                range.end(),
            );
            if has_error_path_text(&tail_text) {
                push_probe_shape(
                    &mut shapes,
                    line_index,
                    text,
                    PROBE_SHAPE_ERROR_PATH,
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
        push_probe_shape(
            &mut shapes,
            line_index,
            text,
            PROBE_SHAPE_CALL_DELETION,
            range.start(),
            range.end(),
        );
        if has_return_value_text(&call_text) && !call_is_argument(&call_expr) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                PROBE_SHAPE_RETURN_VALUE,
                range.start(),
                range.end(),
            );
        }
        if has_error_path_text(&call_text) {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                PROBE_SHAPE_ERROR_PATH,
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
        push_probe_shape(
            &mut shapes,
            line_index,
            text,
            PROBE_SHAPE_CALL_DELETION,
            range.start(),
            range.end(),
        );
        if method_call
            .name_ref()
            .is_some_and(|name| is_effect_call_name(&name.syntax().text().to_string()))
            || has_effect_text(&method_text)
        {
            push_probe_shape(
                &mut shapes,
                line_index,
                text,
                PROBE_SHAPE_SIDE_EFFECT,
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
            PROBE_SHAPE_FIELD_CONSTRUCTION,
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
            push_probe_shape_with_text(
                &mut shapes,
                line_index,
                PROBE_SHAPE_MATCH_ARM,
                token.text_range().start(),
                token.text_range().end(),
                match_expr_probe_text(
                    text,
                    match_expr.expr().map(|expr| expr.syntax().text_range()),
                    match_expr.syntax().text_range(),
                ),
            );
        }
    }

    for arm in function
        .syntax()
        .descendants()
        .filter_map(ast::MatchArm::cast)
    {
        if let Some(token) = arm.fat_arrow_token() {
            push_probe_shape_with_text(
                &mut shapes,
                line_index,
                PROBE_SHAPE_MATCH_ARM,
                token.text_range().start(),
                token.text_range().end(),
                match_arm_probe_text(
                    text,
                    arm.syntax().text_range().start(),
                    token.text_range().start(),
                ),
            );
        }
    }

    shapes.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(a.end_line.cmp(&b.end_line))
            .then(a.kind.cmp(&b.kind))
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

fn push_probe_shape(
    shapes: &mut Vec<ProbeShapeFact>,
    line_index: &LineIndex,
    text: &str,
    kind: &str,
    start: TextSize,
    end: TextSize,
) {
    let snippet = slice_text(text, start, end)
        .trim()
        .trim_end_matches(';')
        .to_string();
    if snippet.is_empty() {
        return;
    }
    push_probe_shape_with_text(shapes, line_index, kind, start, end, snippet);
}

fn push_probe_shape_with_text(
    shapes: &mut Vec<ProbeShapeFact>,
    line_index: &LineIndex,
    kind: &str,
    start: TextSize,
    end: TextSize,
    snippet: String,
) {
    if snippet.is_empty() {
        return;
    }
    shapes.push(ProbeShapeFact {
        start_line: line_index.line(start),
        end_line: line_index.line_for_range_end(end),
        start_byte: u32::from(start) as usize,
        kind: kind.to_string(),
        text: snippet,
    });
}

fn match_expr_probe_text(
    text: &str,
    scrutinee_range: Option<ra_ap_syntax::TextRange>,
    fallback_range: ra_ap_syntax::TextRange,
) -> String {
    if let Some(range) = scrutinee_range {
        let scrutinee = normalize_probe_shape_text(&slice_text(text, range.start(), range.end()));
        if !scrutinee.is_empty() {
            return format!("match {scrutinee}");
        }
    }

    let raw = slice_text(text, fallback_range.start(), fallback_range.end());
    let snippet = raw.trim();
    let head = snippet
        .split_once('{')
        .map(|(head, _)| head)
        .unwrap_or(snippet);
    normalize_probe_shape_text(head)
}

fn match_arm_probe_text(text: &str, start: TextSize, arrow_start: TextSize) -> String {
    let raw = slice_text(text, start, arrow_start);
    let pattern = raw.trim();
    let head = format!("{pattern} =>");
    normalize_probe_shape_text(&head)
}

fn normalize_probe_shape_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_predicate_operator(operator: &str) -> bool {
    matches!(
        operator,
        "==" | "!=" | "<=" | ">=" | "<" | ">" | "&&" | "||"
    )
}

fn has_return_value_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("Ok(")
        || trimmed.starts_with("Some(")
        || trimmed.contains(" Ok(")
        || trimmed.contains(" Some(")
        || trimmed.contains("None")
}

fn call_is_argument(call: &ast::CallExpr) -> bool {
    call.syntax()
        .parent()
        .and_then(ast::ArgList::cast)
        .is_some()
}

fn is_tail_return_value_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    !trimmed.is_empty()
        && !trimmed.starts_with("if ")
        && !trimmed.starts_with("match ")
        && !trimmed.starts_with("while ")
        && !trimmed.starts_with("for ")
        && !trimmed.starts_with("loop ")
}

fn has_error_path_text(text: &str) -> bool {
    text.contains("Err(")
        || text.contains("Error::")
        || text.contains("map_err")
        || text.contains("bail!")
        || text.contains("anyhow!")
}

fn has_effect_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        ".save(",
        ".publish(",
        ".send(",
        ".write(",
        ".insert(",
        ".push(",
        ".remove(",
        ".delete(",
        ".emit(",
        ".increment(",
        "metrics.",
        "log::",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_effect_call_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "save"
            | "publish"
            | "send"
            | "write"
            | "insert"
            | "push"
            | "remove"
            | "delete"
            | "emit"
            | "increment"
    )
}

fn extract_parser_oracles(
    function: &ast::Fn,
    text: &str,
    line_index: &LineIndex,
) -> Vec<OracleFact> {
    // RIPR-SPEC-0106 (Part A): pre-scan the function body for `unwrap_err`/
    // `expect_err` variable bindings so assertions on those variables can be
    // upgraded to ExactErrorVariant.
    let function_text = function.syntax().text().to_string();
    let bound_error_vars = unwrap_err_bound_variables(&function_text);

    let mut assertions = Vec::new();
    for macro_call in function
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
    {
        let Some(path) = macro_call.path() else {
            continue;
        };
        let macro_name = path.syntax().text().to_string().replace(' ', "");
        if !is_assertion_macro(&macro_name) {
            continue;
        }
        let range = macro_call.syntax().text_range();
        let assertion_text = slice_macro_call_text(text, range.start(), range.end());
        let mut classification = classify_assertion(&assertion_text);
        // Upgrade exact assertions on unwrap_err-bound variables to
        // ExactErrorVariant when the assertion pins a specific error result
        // (RIPR-SPEC-0106, Part A). Constructor-payload equality reaches this
        // point as WholeObjectEquality.
        if matches!(
            classification.kind,
            OracleKind::ExactValue | OracleKind::WholeObjectEquality
        ) && is_unwrap_err_bound_error_assertion(&assertion_text, &bound_error_vars)
        {
            classification.kind = OracleKind::ExactErrorVariant;
            classification.strength = OracleStrength::Strong;
        }
        assertions.push(OracleFact {
            line: line_index.line(range.start()),
            kind: classification.kind,
            strength: classification.strength,
            observed_tokens: extract_identifier_tokens(&assertion_text),
            text: assertion_text,
        });
    }

    for method_call in function
        .syntax()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
    {
        let Some(name) = method_call
            .name_ref()
            .map(|name| name.syntax().text().to_string())
        else {
            continue;
        };
        if name != "unwrap" && name != "expect" {
            continue;
        }
        let range = method_call.syntax().text_range();
        let text = slice_text(text, range.start(), range.end())
            .trim()
            .trim_end_matches(';')
            .to_string();
        assertions.push(OracleFact {
            line: line_index.line(range.start()),
            kind: OracleKind::SmokeOnly,
            strength: OracleStrength::Smoke,
            observed_tokens: extract_identifier_tokens(&text),
            text,
        });
    }

    let function_start = line_index.line(function.syntax().text_range().start());
    // #3284: terminal Err-return guards credit as their assertion twins
    // on the parser path too, so both adapters agree.
    for oracle in err_return_guard_oracles(&function_text, function_start) {
        assertions.push(oracle);
    }
    for oracle in
        extract_line_scanned_oracles(&function.syntax().text().to_string(), function_start)
    {
        assertions.push(oracle);
    }

    assertions.sort_by(|a, b| a.line.cmp(&b.line).then(a.text.cmp(&b.text)));
    assertions.dedup_by(|a, b| a.line == b.line && a.text == b.text);
    assertions
}

fn is_assertion_macro(macro_name: &str) -> bool {
    matches!(
        macro_name,
        "assert" | "assert_eq" | "assert_ne" | "assert_matches" | "matches"
    ) || macro_name.starts_with("insta::assert")
        || macro_name.contains("snapshot")
}

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { starts }
    }

    fn line(&self, offset: TextSize) -> usize {
        self.line_from_offset(text_size_to_usize(offset))
    }

    fn line_for_range_end(&self, offset: TextSize) -> usize {
        self.line_from_offset(text_size_to_usize(offset).saturating_sub(1))
    }

    fn line_from_offset(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index.max(1),
        }
    }
}

fn text_size_to_usize(offset: TextSize) -> usize {
    let value: u32 = offset.into();
    value as usize
}

fn slice_text(text: &str, start: TextSize, end: TextSize) -> String {
    let start = text_size_to_usize(start);
    let end = text_size_to_usize(end);
    text.get(start..end).unwrap_or("").to_string()
}

fn slice_macro_call_text(text: &str, start: TextSize, end: TextSize) -> String {
    let start = text_size_to_usize(start);
    let mut end = text_size_to_usize(end);
    let bytes = text.as_bytes();
    let mut cursor = end;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'\n' {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b';') {
        end = cursor + 1;
    }
    text.get(start..end).unwrap_or("").trim().to_string()
}

fn owner_changed_nodes(
    facts: &crate::analysis::facts::FileFacts,
    ranges: &[TextRange],
) -> Vec<SyntaxNodeFact> {
    let mut nodes = Vec::new();
    for range in ranges {
        let mut owners = facts
            .functions
            .iter()
            .filter(|function| {
                ranges_overlap(
                    range.start_line,
                    range.end_line,
                    function.start_line,
                    function.end_line,
                )
            })
            .collect::<Vec<_>>();
        owners.sort_by(|left, right| {
            function_span(left)
                .cmp(&function_span(right))
                .then(right.start_line.cmp(&left.start_line))
                .then(left.id.0.cmp(&right.id.0))
        });
        if let Some(function) = owners.first() {
            nodes.push(SyntaxNodeFact {
                file: function.file.clone(),
                kind: if function.is_test {
                    "test_function".to_string()
                } else {
                    "function".to_string()
                },
                start_line: function.start_line,
                end_line: function.end_line,
                text: function.body.clone(),
                owner: Some(function.id.clone()),
            });
        }
    }
    nodes.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.start_line.cmp(&right.start_line))
            .then(left.end_line.cmp(&right.end_line))
            .then(left.kind.cmp(&right.kind))
            .then(left.owner.cmp(&right.owner))
    });
    nodes.dedup_by(|left, right| {
        left.file == right.file
            && left.start_line == right.start_line
            && left.end_line == right.end_line
            && left.kind == right.kind
            && left.owner == right.owner
    });
    nodes
}

fn ranges_overlap(start1: usize, end1: usize, start2: usize, end2: usize) -> bool {
    start1 <= end2 && start2 <= end1
}

fn function_span(function: &FunctionFact) -> usize {
    function.end_line - function.start_line
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-{name}-{stamp}"));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn write_manifest(root: &Path) -> Result<(), Box<dyn Error>> {
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='test'\nversion='0.1.0'\nedition='2024'\n",
        )?;
        Ok(())
    }

    #[test]
    fn ra_adapter_parses_valid_rust_source() -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_parser")?;
        fs::create_dir_all(root.join("src"))?;
        write_manifest(&root)?;
        fs::write(
            root.join("src/lib.rs"),
            r#"
pub fn calculate(x: i32, y: i32) -> i32 {
    if x > y { x } else { y }
}

#[test]
fn test_calculate() {
    assert_eq!(calculate(5, 3), 5);
}
"#,
        )?;

        let adapter = RaRustSyntaxAdapter;
        let text = fs::read_to_string(root.join("src/lib.rs"))?;
        let facts = adapter.summarize_file(&root.join("src/lib.rs"), &text)?;

        assert!(!facts.functions.is_empty());
        assert!(!facts.tests.is_empty());
        assert!(!facts.probe_shapes.is_empty());
        Ok(())
    }

    #[test]
    fn ra_adapter_ignores_unannotated_helpers_in_test_files_and_keeps_rstest()
    -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_test_helpers")?;
        fs::create_dir_all(root.join("tests"))?;
        write_manifest(&root)?;
        fs::write(
            root.join("tests/pipeline.rs"),
            r#"
fn helper() {
    run_pipeline();
}

#[rstest]
#[case("alpha")]
fn parameterized_case(input: &str) {
    helper();
    assert_eq!(input, "alpha");
}

#[test]
fn integration_smoke() {
    helper();
}
"#,
        )?;

        let adapter = RaRustSyntaxAdapter;
        let text = fs::read_to_string(root.join("tests/pipeline.rs"))?;
        let facts = adapter.summarize_file(&root.join("tests/pipeline.rs"), &text)?;
        let test_names = facts
            .tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(facts.functions.len(), 3);
        assert_eq!(test_names, vec!["parameterized_case", "integration_smoke"]);
        assert!(
            facts
                .tests
                .iter()
                .find(|test| test.name == "parameterized_case")
                .is_some_and(|test| test.attrs.iter().any(|attr| attr.contains("rstest"))),
            "rstest attrs should remain available for value resolution"
        );
        Ok(())
    }

    #[test]
    fn ra_adapter_marks_inline_cfg_test_helpers_as_test_role() -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_cfg_test_helpers")?;
        fs::create_dir_all(root.join("src"))?;
        write_manifest(&root)?;
        fs::write(
            root.join("src/lib.rs"),
            r#"
pub fn production_control(value: i32) -> Result<i32, String> {
    if value < 0 { return Err("negative".to_string()); }
    Ok(value)
}

#[cfg(test)]
mod tests {
    fn helper_returns_result(value: i32) -> Result<(), String> {
        if value < 0 { return Err("negative".to_string()); }
        Ok(())
    }

    #[test]
    fn helper_is_evidence() {
        helper_returns_result(1).expect("fixture should pass");
    }
}
"#,
        )?;

        let adapter = RaRustSyntaxAdapter;
        let text = fs::read_to_string(root.join("src/lib.rs"))?;
        let facts = adapter.summarize_file(&root.join("src/lib.rs"), &text)?;
        let helper = facts
            .functions
            .iter()
            .find(|function| function.name == "helper_returns_result")
            .ok_or("missing cfg(test) helper fact")?;

        assert!(
            helper.is_test,
            "cfg(test) helper must be producer-owned test role"
        );
        assert!(
            facts
                .tests
                .iter()
                .any(|test| test.name == "helper_is_evidence"),
            "actual test function remains available as evidence input"
        );
        assert!(
            facts
                .tests
                .iter()
                .all(|test| test.name != "helper_returns_result"),
            "cfg(test) helper must not be promoted to a test fact"
        );
        assert!(
            facts
                .functions
                .iter()
                .find(|function| function.name == "production_control")
                .is_some_and(|function| !function.is_test),
            "production control must remain production role"
        );
        Ok(())
    }

    #[test]
    fn ra_adapter_extracts_probe_shapes() -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_probe_shapes")?;
        fs::create_dir_all(root.join("src"))?;
        write_manifest(&root)?;
        fs::write(
            root.join("src/lib.rs"),
            r#"
pub fn validate(value: i32) -> Result<i32, String> {
    if value < 0 {
        Err("negative".to_string())
    } else {
        Ok(value * 2)
    }
}
"#,
        )?;

        let adapter = RaRustSyntaxAdapter;
        let text = fs::read_to_string(root.join("src/lib.rs"))?;
        let facts = adapter.summarize_file(&root.join("src/lib.rs"), &text)?;

        assert!(
            facts
                .probe_shapes
                .iter()
                .any(|p| p.kind == PROBE_SHAPE_PREDICATE),
            "Should extract predicate probe shapes"
        );
        assert!(
            facts
                .probe_shapes
                .iter()
                .any(|p| p.kind == PROBE_SHAPE_ERROR_PATH),
            "Should extract error_path probe shapes"
        );
        Ok(())
    }

    #[test]
    fn ra_adapter_does_not_promote_nested_constructor_arguments_to_returns()
    -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_nested_constructor_returns")?;
        fs::create_dir_all(root.join("src"))?;
        write_manifest(&root)?;
        fs::write(
            root.join("src/lib.rs"),
            r#"
fn consume(_: Option<u64>) {}

pub fn wrap(value: u64) -> Result<Option<u64>, ()> {
    consume(Some(value));
    Ok(Some(value))
}
"#,
        )?;

        let adapter = RaRustSyntaxAdapter;
        let text = fs::read_to_string(root.join("src/lib.rs"))?;
        let facts = adapter.summarize_file(&root.join("src/lib.rs"), &text)?;
        let return_shapes = facts
            .probe_shapes
            .iter()
            .filter(|shape| shape.kind == PROBE_SHAPE_RETURN_VALUE)
            .map(|shape| shape.text.as_str())
            .collect::<Vec<_>>();

        if return_shapes != ["Ok(Some(value))"] {
            return Err(format!("unexpected return probe shapes: {return_shapes:?}").into());
        }
        Ok(())
    }

    #[test]
    fn ra_adapter_handles_parser_errors() -> Result<(), Box<dyn Error>> {
        let root = temp_dir("ra_parser_error")?;
        let adapter = RaRustSyntaxAdapter;
        let invalid_rust = "pub fn broken( { invalid rust";
        let result = adapter.summarize_file(&root.join("invalid.rs"), invalid_rust);

        assert!(matches!(result, Err(ref err) if err.contains("syntax errors")));
        Ok(())
    }

    #[test]
    fn ra_adapter_changed_nodes_returns_empty_for_missing_file() {
        let _index = crate::analysis::facts::RustIndex::default();
        let adapter = RaRustSyntaxAdapter;
        let ranges = vec![TextRange {
            start_line: 1,
            start_column: 1,
            end_line: 10,
            end_column: 80,
        }];

        let nodes = adapter.changed_nodes(
            &crate::analysis::facts::FileFacts {
                path: std::path::PathBuf::from("nonexistent.rs"),
                functions: vec![],
                tests: vec![],
                calls: vec![],
                returns: vec![],
                literals: vec![],
                probe_shapes: vec![],
                used_lexical_fallback: false,
                source: String::new(),
            },
            &ranges,
        );

        assert!(nodes.is_empty());
    }
}

#[cfg(test)]
mod cfg_all_test_tests {
    use super::RaRustSyntaxAdapter;
    use super::RustSyntaxAdapter;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("ripr-{name}-{stamp}"));
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn write_manifest(root: &std::path::Path) -> Result<(), String> {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]
name='test'
version='0.1.0'
edition='2024'
",
        )
        .map_err(|error| error.to_string())
    }

    #[test]
    fn cfg_all_test_module_members_carry_evidence_role() -> Result<(), String> {
        // #3284: a module gated on `cfg(all(test, feature))` compiles under
        // test; its helpers are harness plumbing and must carry the
        // evidence role, exactly like plain `#[cfg(test)]`. `not(test)`
        // and `any(test, ..)` stay production.
        let root = temp_dir("ra_cfg_all_test")?;
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        write_manifest(&root)?;
        fs::write(
            root.join("src/lib.rs"),
            "pub fn production_control() -> i32 { 1 }\n\n#[cfg(all(test, feature = \"slow\"))]\nmod slow_tests {\n    fn helper_under_all_test() -> i32 { production_control() }\n\n    #[test]\n    fn runs() { let _ = helper_under_all_test(); }\n}\n\n#[cfg(not(test))]\nmod prod_only {\n    pub fn production_shape() -> i32 { 2 }\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let facts = RaRustSyntaxAdapter
            .summarize_file(
                &root.join("src/lib.rs"),
                &fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        let helper = facts
            .functions
            .iter()
            .find(|function| function.name == "helper_under_all_test")
            .ok_or("helper missing from facts")?;
        assert!(
            helper.is_test,
            "cfg(all(test, ..)) members are evidence role"
        );
        let prod = facts
            .functions
            .iter()
            .find(|function| function.name == "production_shape")
            .ok_or("cfg(not(test)) fn missing from facts")?;
        assert!(!prod.is_test, "cfg(not(test)) members stay production");
        Ok(())
    }
}

#[cfg(test)]
mod guard_pipeline_debug_tests {
    use super::RaRustSyntaxAdapter;
    use super::RustSyntaxAdapter;

    #[test]
    fn parser_path_credits_err_guard_in_test_facts() -> Result<(), String> {
        let source = "use parity_err_guard::discounted_total;\n\n#[test]\nfn boundary_matches_expected() -> Result<(), String> {\n    let actual = discounted_total(100, 100);\n    let expected = 90;\n    if actual != expected {\n        return Err(format!(\"actual={actual:?}\"));\n    }\n    Ok(())\n}\n";
        let facts = RaRustSyntaxAdapter
            .summarize_file(std::path::Path::new("tests/pricing.rs"), source)
            .map_err(|error| error.to_string())?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "boundary_matches_expected")
            .ok_or_else(|| format!("test missing: {:?}", facts.tests))?;
        assert!(
            test.assertions
                .iter()
                .any(|oracle| oracle.kind == crate::domain::OracleKind::RelationalCheck),
            "guard must credit through the parser path: {:?}",
            test.assertions
        );
        Ok(())
    }
}

#[cfg(test)]
mod include_directive_tests {
    use super::rust_include_directives;
    use std::path::{Path, PathBuf};

    #[test]
    fn parser_extracts_only_real_include_macros_and_decodes_literals() -> Result<(), String> {
        let source = r###"
// include!("comment.rs");
const SAMPLE: &str = "include!(\"string.rs\")";
include!("plain.rs");
include!(r#"raw.rs"#);
include!("escaped\\path.rs");
include!("quoted\"file.rs");
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
mod nested { include!("nested.rs"); }
"###;

        let directives = rust_include_directives(Path::new("src/lib.rs"), source, 16)?;
        let literals = directives
            .iter()
            .filter_map(|directive| directive.literal_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            literals,
            vec![
                PathBuf::from("plain.rs"),
                PathBuf::from("raw.rs"),
                PathBuf::from("escaped\\path.rs"),
                PathBuf::from("quoted\"file.rs"),
                PathBuf::from("nested.rs")
            ]
        );
        assert_eq!(directives.len(), 6);
        assert!(directives[4].literal_path.is_none());
        assert!(!directives[5].is_file_level);
        Ok(())
    }
}
