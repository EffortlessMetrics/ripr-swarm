//! Narrow source-side witness for a literal return in a switch arm.
//!
//! This is only one half of infection evidence: a caller must also establish
//! that a test actually invokes the owner with the returned selector.
use super::*;

pub(crate) fn selected_label(
    owner: &TypeScriptOwner,
    line: usize,
    line_text: &str,
    old_line: Option<&str>,
) -> Option<(String, u64, u64)> {
    let source = owner.source_text.as_deref()?.trim();
    let source = source.strip_prefix("export ").unwrap_or(source);
    let old = old_line?
        .trim()
        .strip_prefix("return ")?
        .trim_end_matches(';')
        .trim();
    if old.is_empty()
        || !old.bytes().all(|c| c.is_ascii_digit())
        || (old.len() > 1 && old.starts_with('0'))
    {
        return None;
    }
    let old: u64 = old.parse().ok()?;
    if old > 9_007_199_254_740_991 {
        return None;
    }
    let relative = line.checked_sub(owner.start_line)?;
    if source.lines().nth(relative)?.trim() != line_text.trim() {
        return None;
    }
    let param = match owner.params.as_slice() {
        [param] => param.clone(),
        _ => return None,
    };
    super::parse::parse_on_worker(&owner.file, source, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() {
            return None;
        }
        let [Statement::FunctionDeclaration(function)] = parsed.program.body.as_slice() else {
            return None;
        };
        if function.r#async
            || function.generator
            || function.params.rest.is_some()
            || function.params.items.len() != 1
            || function.params.items[0].initializer.is_some()
        {
            return None;
        }
        let body = function.body.as_ref()?;
        let [Statement::SwitchStatement(switch)] = body.statements.as_slice() else {
            return None;
        };
        if !matches!(&switch.discriminant, Expression::Identifier(id) if id.name == param.as_str())
        {
            return None;
        }
        let mut labels = Vec::new();
        let mut target = None;
        let mut default_seen = false;
        for case in &switch.cases {
            let label = match &case.test {
                Some(Expression::StringLiteral(value)) => {
                    let value = value.value.to_string();
                    if labels.contains(&value) {
                        return None;
                    }
                    labels.push(value.clone());
                    Some(value)
                }
                None if !default_seen => {
                    default_seen = true;
                    None
                }
                _ => return None,
            };
            let [Statement::ReturnStatement(ret)] = case.consequent.as_slice() else {
                return None;
            };
            let Some(Expression::NumericLiteral(value)) = &ret.argument else {
                return None;
            };
            let span = ret.span();
            let offset = span.start as usize;
            let start_line = source
                .get(..offset)?
                .bytes()
                .filter(|c| *c == b'\n')
                .count();
            if start_line == relative {
                let text = source.get(value.span.start as usize..value.span.end as usize)?;
                if text.is_empty()
                    || !text.bytes().all(|c| c.is_ascii_digit())
                    || (text.len() > 1 && text.starts_with('0'))
                {
                    return None;
                }
                let new: u64 = text.parse().ok()?;
                if new > 9_007_199_254_740_991 || new == old {
                    return None;
                }
                target = label.map(|label| (label, old, new));
            }
        }
        target
    })
    .ok()
    .flatten()
}

/// A test module containing only imports and one direct test registration,
/// whose first statement directly initializes a local from the owner call.
/// The caller must separately verify the import binds the actual owner and is
/// not shadowed or replaced.
pub(crate) fn direct_test_call(
    test: &TypeScriptTest,
    root: &Path,
    name: &str,
    label: &str,
    owner_import: &str,
    old: u64,
    new: u64,
) -> bool {
    let Some(source) = super::bounded_read::read_workspace_sources_capped(
        root,
        &[test.file.clone()],
        super::bounded_read::ts_file_read_limit(),
        super::bounded_read::ts_workspace_read_budget(),
    )
    .sources
    .remove(&test.file) else {
        return false;
    };
    let body = test.body_text.clone();
    let name = name.to_string();
    let label = label.to_string();
    let owner_import = owner_import.to_string();
    super::parse::parse_on_worker(&test.file, &source, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() { return false; }
        let mut registration = None;
        let mut imported = 0;
        for statement in &parsed.program.body {
            if let Statement::ImportDeclaration(import) = statement {
                if import.source.value != "vitest" && import.source.value != owner_import.as_str() { return false; }
                if import.import_kind != ImportOrExportKind::Value || import.with_clause.is_some() { return false; }
                let Some(specifiers) = &import.specifiers else { return false; };
                if specifiers.is_empty() || specifiers.iter().any(|specifier| !matches!(specifier,
                    ImportDeclarationSpecifier::ImportSpecifier(spec) if spec.import_kind == ImportOrExportKind::Value)) { return false; }
                imported += specifiers.len();
                continue;
            }
            let Statement::ExpressionStatement(statement) = statement else { return false; };
            let Expression::CallExpression(call) = &statement.expression else { return false; };
            if registration.is_some() || source.get(call.span.start as usize..call.span.end as usize) != Some(body.as_str()) { return false; }
            registration = Some(call);
        }
        if imported != 3 { return false; }
        let Some(call) = registration else { return false; };
        if !matches!(&call.callee, Expression::Identifier(id) if id.name == "test")
            || call.arguments.len() != 2 || !matches!(call.arguments.first(), Some(Argument::StringLiteral(_))) { return false; }
        let Some(Argument::ArrowFunctionExpression(callback)) = call.arguments.get(1) else { return false; };
        if callback.r#async || callback.expression || !callback.params.items.is_empty() || callback.params.rest.is_some() { return false; }
        let Some(Statement::VariableDeclaration(declaration)) = callback.body.statements.first() else { return false; };
        if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const || declaration.declarations.len() != 1 { return false; }
        let declaration = &declaration.declarations[0];
        let Some(local) = super::owners::binding_identifier_name(&declaration.id) else { return false; };
        if local == name || local == "test" || local == "expect" { return false; }
        let Some(Expression::CallExpression(call)) = &declaration.init else { return false; };
        if call.optional || !matches!(&call.callee, Expression::Identifier(id) if id.name == name.as_str())
            || !matches!(call.arguments.as_slice(), [Argument::StringLiteral(value)] if value.value == label.as_str()) { return false; }
        let [_, Statement::ExpressionStatement(assertion)] = callback.body.statements.as_slice() else { return false; };
        let Expression::CallExpression(matcher) = &assertion.expression else { return false; };
        let Expression::StaticMemberExpression(member) = &matcher.callee else { return false; };
        if matcher.optional || member.optional || member.property.name != "toBeLessThan" { return false; }
        let [Argument::NumericLiteral(threshold)] = matcher.arguments.as_slice() else { return false; };
        if !threshold.value.is_finite() || (old as f64) >= threshold.value || (new as f64) >= threshold.value { return false; }
        let Expression::CallExpression(expect) = &member.object else { return false; };
        !expect.optional && matches!(&expect.callee, Expression::Identifier(id) if id.name == "expect")
            && matches!(expect.arguments.as_slice(), [Argument::Identifier(id)] if id.name == local)
    }).unwrap_or(false)
}
