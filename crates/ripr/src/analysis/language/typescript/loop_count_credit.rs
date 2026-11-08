//! A deliberately bounded witness for a retry loop observed by node:test.
//!
//! This reads only an exact retry body and an unconditional, direct test with
//! an always-rejecting mock and a literal count. Other shapes retain rule 8's
//! abstention; this is not a general loop interpreter.
use super::*;
use oxc_ast::ast::{CallExpression, VariableDeclarationKind};

const MAX_WITNESS_SOURCE_BYTES: u64 = 65_536;
const MAX_RELATED_CANDIDATES: usize = 16;

/// Read from the analyzed snapshot, under the adapter's bounded-read policy.
fn read_small_source(root: &Path, path: &Path) -> Option<String> {
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    crate::analysis::input_boundary::observe_module_read(root, &root.join(path), false);
    let mut read = super::bounded_read::read_workspace_sources_capped(
        root,
        &[path.to_path_buf()],
        MAX_WITNESS_SOURCE_BYTES,
        MAX_WITNESS_SOURCE_BYTES,
    );
    read.sources.remove(path)
}

fn ident<'s>(expr: &'s Expression<'_>) -> Option<&'s str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn member<'s>(expr: &'s Expression<'_>, property: &str) -> Option<&'s Expression<'s>> {
    match expr {
        Expression::StaticMemberExpression(m) if !m.optional && m.property.name == property => {
            Some(&m.object)
        }
        _ => None,
    }
}

fn call<'s>(expr: &'s Expression<'s>) -> Option<&'s CallExpression<'s>> {
    match expr {
        Expression::CallExpression(c) if !c.optional && c.type_arguments.is_none() => Some(c),
        _ => None,
    }
}

fn stmt_call<'s>(stmt: &'s Statement<'s>) -> Option<&'s CallExpression<'s>> {
    match stmt {
        Statement::ExpressionStatement(s) => call(&s.expression),
        _ => None,
    }
}

fn unsigned(arg: &Argument<'_>) -> Option<u32> {
    let Argument::NumericLiteral(n) = arg else {
        return None;
    };
    let text = n.raw.as_ref()?.as_str();
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let value = text.parse::<u32>().ok()?;
    (1..=10_000).contains(&value).then_some(value)
}

fn simple_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'$')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

/// Evaluate a zero-based, increment-by-one counter and a parameter offset.
fn bound(header: &str, parameter: &str, value: u32) -> Option<(String, u32)> {
    let inner = header.trim().strip_prefix("for (")?.strip_suffix(") {")?;
    if inner.contains(['(', ')']) {
        return None;
    }
    let clauses = super::loop_count_guard::for_header_clauses(header)?;
    let counter = clauses[0]
        .strip_prefix("let ")?
        .strip_suffix(" = 0")?
        .trim();
    if !simple_identifier(counter) || counter == parameter || clauses[2] != format!("{counter}++") {
        return None;
    }
    let after_counter = clauses[1].trim().strip_prefix(counter)?.trim_start();
    let expr = after_counter.strip_prefix('<')?.trim_start();
    let rest = expr.strip_prefix(parameter)?.trim_start();
    let result = if rest.is_empty() {
        i64::from(value)
    } else {
        let op = rest.get(..1)?;
        let digits = rest.get(1..)?.trim();
        if digits.is_empty()
            || (digits.len() > 1 && digits.starts_with('0'))
            || !digits.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let offset: i64 = digits.parse().ok()?;
        if offset > 10_000 {
            return None;
        }
        match op {
            "+" => i64::from(value) + offset,
            "-" => i64::from(value) - offset,
            _ => return None,
        }
    };
    if !(1..=10_000).contains(&result) {
        return None;
    }
    Some((counter.to_owned(), u32::try_from(result).ok()?))
}

fn retry_body(owner: &TypeScriptOwner, new: &str, root: Option<&Path>) -> Option<(String, String)> {
    if owner.arity != Some(2) || owner.params.len() != 2 {
        return None;
    }
    let callback = &owner.params[0];
    let attempts = &owner.params[1];
    if callback == attempts {
        return None;
    }
    let text = owner.source_text.as_deref()?;
    if text.len() > MAX_WITNESS_SOURCE_BYTES as usize {
        return None;
    }
    let file = read_small_source(root?, &owner.file)?;
    let file = file.trim();
    let body = text.trim().strip_prefix("export ").unwrap_or(text.trim());
    if file.strip_prefix("export ") != Some(body) {
        return None;
    }
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    if lines.len() != 11 {
        return None;
    }
    let last = lines[1].strip_prefix("let ")?.strip_suffix(": unknown;")?;
    let error = lines[5].strip_prefix("} catch (")?.strip_suffix(") {")?;
    if !simple_identifier(last)
        || !simple_identifier(error)
        || last == error
        || owner.params.iter().any(|p| p == last || p == error)
    {
        return None;
    }
    let expected_start = format!(
        "async function {}<T>({}: () => Promise<T>, {}: number): Promise<T> {{",
        owner.name, callback, attempts
    );
    let expected_return = format!("return await {callback}();");
    if lines[0].strip_prefix("export ").unwrap_or(lines[0]) != expected_start
        || lines[2] != new.trim()
        || lines[3] != "try {"
        || lines[4] != expected_return
        || lines[6] != format!("{last} = {error};")
        || lines[7] != "}"
        || lines[8] != "}"
        || lines[9] != format!("throw {last};")
        || lines[10] != "}"
    {
        return None;
    }
    Some((last.to_owned(), error.to_owned()))
}

fn imported(test: &TypeScriptTest, source: &str, imported: Option<&str>, local: &str) -> bool {
    test.imports_in_file.iter().any(|i| {
        i.source == source && i.imported.as_deref() == imported && i.local == local && !i.namespace
    })
}

fn test_count(test: &TypeScriptTest, owner: &str) -> Option<(u32, u32, usize)> {
    if test.conditional_registration.is_some()
        || test.factory_call_line.is_some()
        || !test.mocks_in_file.is_empty()
        || !imported(test, "node:test", Some("it"), "it")
        || !imported(test, "node:test", Some("mock"), "mock")
        || !imported(test, "node:assert/strict", Some("default"), "assert")
    {
        return None;
    }
    // The whole-file registration grammar below permits no writes or escapes
    // of these bindings. The general replacement scanner cannot distinguish
    // those from the genuine `mock.fn` spelling and flags it as unknown.
    if ["it", "mock", "assert", "Error", owner].iter().any(|name| {
        test.written_locals_in_file
            .iter()
            .any(|written| written == name)
            || test
                .scope_bindings
                .iter()
                .any(|binding| binding.name == *name && (!binding.file_level || *name == "Error"))
    }) {
        return None;
    }
    let owner = owner.to_owned();
    super::parse::parse_on_worker(&test.file, &test.body_text, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() || parsed.program.body.len() != 1 { return None; }
        let registration = stmt_call(&parsed.program.body[0])?;
        if ident(&registration.callee) != Some("it") || registration.arguments.len() != 2 || !matches!(registration.arguments.first(), Some(Argument::StringLiteral(_))) { return None; }
        let Some(Argument::ArrowFunctionExpression(body)) = registration.arguments.get(1) else { return None; };
        if !body.r#async || body.expression || !body.params.items.is_empty() || body.params.rest.is_some() || !body.body.directives.is_empty() { return None; }
        let [Statement::VariableDeclaration(decl), Statement::ExpressionStatement(rejects), count_stmt] = body.body.statements.as_slice() else { return None; };
        if decl.kind != VariableDeclarationKind::Const || decl.declarations.len() != 1 { return None; }
        let decl = &decl.declarations[0];
        let local = super::owners::binding_identifier_name(&decl.id)?;
        if ["it", "mock", "assert", "Error", owner.as_str()].contains(&local) { return None; }
        let factory = call(decl.init.as_ref()?)?;
        if ident(member(&factory.callee, "fn")?) != Some("mock") || factory.arguments.len() != 1 { return None; }
        let Some(Argument::ArrowFunctionExpression(callback)) = factory.arguments.first() else { return None; };
        if !callback.r#async || callback.expression || !callback.params.items.is_empty() || callback.params.rest.is_some() || !callback.body.directives.is_empty() { return None; }
        let [Statement::ThrowStatement(thrown)] = callback.body.statements.as_slice() else { return None; };
        let Expression::NewExpression(error) = &thrown.argument else { return None; };
        if ident(&error.callee) != Some("Error") || error.arguments.len() != 1 { return None; }
        let Some(Argument::StringLiteral(message)) = error.arguments.first() else { return None; };
        if message.value.is_empty() || !message.value.bytes().all(|b| b.is_ascii_alphanumeric()) { return None; }
        let Expression::AwaitExpression(awaited) = &rejects.expression else { return None; };
        let rejection = call(&awaited.argument)?;
        if ident(member(&rejection.callee, "rejects")?) != Some("assert") || rejection.arguments.len() != 2 { return None; }
        let invocation = call(rejection.arguments.first()?.as_expression()?)?;
        if ident(&invocation.callee) != Some(owner.as_str()) || invocation.arguments.len() != 2 { return None; }
        if !matches!(invocation.arguments.first(), Some(Argument::Identifier(id)) if id.name == local) { return None; }
        let input = unsigned(invocation.arguments.get(1)?)?;
        let Some(Argument::RegExpLiteral(regex)) = rejection.arguments.get(1) else { return None; };
        let span = regex.span();
        if source.get(span.start as usize..span.end as usize)? != format!("/{}/", message.value) { return None; }
        let assertion = stmt_call(count_stmt)?;
        if ident(member(&assertion.callee, "equal")?) != Some("assert") || assertion.arguments.len() != 2 { return None; }
        let observed = call(assertion.arguments.first()?.as_expression()?)?;
        if !observed.arguments.is_empty() { return None; }
        let root = member(member(&observed.callee, "callCount")?, "mock")?;
        if ident(root) != Some(local) { return None; }
        let offset = source.get(..count_stmt.span().start as usize)?.bytes().filter(|b| *b == b'\n').count();
        Some((input, unsigned(assertion.arguments.get(1)?)?, offset))
    }).ok().flatten()
}

pub(crate) struct LoopCountWitness {
    pub(crate) test_name: String,
    pub(crate) test_file: PathBuf,
    pub(crate) assertion_line: usize,
    pub(crate) input: u32,
    pub(crate) before: u32,
    pub(crate) after: u32,
}

pub(crate) fn discriminates(
    old: Option<&str>,
    new: &str,
    line: usize,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    root: Option<&Path>,
) -> Option<LoopCountWitness> {
    let old = old?;
    if old.len() > 4_096
        || new.len() > 4_096
        || candidates.len() > MAX_RELATED_CANDIDATES
        || line.checked_sub(owner.start_line) != Some(2)
    {
        return None;
    }
    let (last, error) = retry_body(owner, new, root)?;
    candidates.iter().find_map(|candidate| {
        if candidate.relation != TypeScriptRelationKind::DirectOwnerCall
            || !registration_is_live(candidate.test, &owner.name, root)
        {
            return None;
        }
        let (input, expected, assertion_offset) = test_count(candidate.test, &owner.name)?;
        let before = bound(old, &owner.params[1], input)?;
        let after = bound(new, &owner.params[1], input)?;
        if before.0 != after.0
            || before.0 == owner.params[0]
            || before.0 == last
            || before.0 == error
            || before.1 != expected
            || before.1 == after.1
        {
            return None;
        }
        Some(LoopCountWitness {
            test_name: candidate.test.name.clone(),
            test_file: candidate.test.file.clone(),
            assertion_line: candidate.test.line.checked_add(assertion_offset)?,
            input,
            before: before.1,
            after: after.1,
        })
    })
}

/// Do not credit a registration hidden in a helper, condition, skipped suite,
/// or behind arbitrary file-level code. The only accepted sibling is a simple
/// success control using its own node:test context mock.
fn registration_is_live(test: &TypeScriptTest, owner: &str, root: Option<&Path>) -> bool {
    let Some(root) = root else {
        return false;
    };
    let Some(source) = read_small_source(root, &test.file) else {
        return false;
    };
    let test_body = test.body_text.clone();
    let owner = owner.to_string();
    let describe_imported = imported(test, "node:test", Some("describe"), "describe")
        && !test
            .scope_bindings
            .iter()
            .any(|binding| binding.name == "describe" && !binding.file_level)
        && !test
            .written_locals_in_file
            .iter()
            .any(|name| name == "describe");
    let owner_sources: Vec<_> = test
        .imports_in_file
        .iter()
        .filter(|import| {
            import.local == owner
                && import.imported.as_deref() == Some(owner.as_str())
                && !import.namespace
        })
        .map(|import| import.source.clone())
        .collect();
    let [owner_source] = owner_sources.as_slice() else {
        return false;
    };
    if !owner_source.starts_with('.') {
        return false;
    }
    let owner_source = owner_source.clone();
    super::parse::parse_on_worker(&test.file, &source, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() {
            return false;
        }
        let mut found = 0usize;
        for stmt in &parsed.program.body {
            if let Statement::ImportDeclaration(import) = stmt {
                if !safe_import(import, &owner_source, &owner) {
                    return false;
                }
                continue;
            }
            let Some(registration) = stmt_call(stmt) else {
                return false;
            };
            if ident(&registration.callee) == Some("it") {
                if !registration_or_control(stmt, source, &test_body, &owner, &mut found) {
                    return false;
                }
            } else if ident(&registration.callee) == Some("describe") && describe_imported {
                if registration.arguments.len() != 2
                    || !matches!(
                        registration.arguments.first(),
                        Some(Argument::StringLiteral(_))
                    )
                {
                    return false;
                }
                let Some(Argument::ArrowFunctionExpression(suite)) = registration.arguments.get(1)
                else {
                    return false;
                };
                if suite.r#async
                    || suite.expression
                    || !suite.params.items.is_empty()
                    || suite.params.rest.is_some()
                    || !suite.body.directives.is_empty()
                {
                    return false;
                }
                for inner in &suite.body.statements {
                    if !registration_or_control(inner, source, &test_body, &owner, &mut found) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }
        found == 1
    })
    .unwrap_or(false)
}

fn safe_import(
    import: &oxc_ast::ast::ImportDeclaration<'_>,
    owner_source: &str,
    owner: &str,
) -> bool {
    if import.with_clause.is_some()
        || import.phase.is_some()
        || import.import_kind != ImportOrExportKind::Value
    {
        return false;
    }
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    if specifiers.is_empty() {
        return false;
    }
    specifiers.iter().all(|specifier| match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(named)
            if named.import_kind == ImportOrExportKind::Value =>
        {
            let ModuleExportName::IdentifierName(imported) = &named.imported else {
                return false;
            };
            let name = imported.name.as_str();
            named.local.name == name
                && if import.source.value == "node:test" {
                    ["it", "mock", "describe"].contains(&name)
                } else {
                    import.source.value == owner_source && name == owner
                }
        }
        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
            import.source.value == "node:assert/strict" && default.local.name == "assert"
        }
        _ => false,
    })
}

fn registration_or_control(
    stmt: &Statement<'_>,
    source: &str,
    test_body: &str,
    owner: &str,
    found: &mut usize,
) -> bool {
    let Some(registration) = stmt_call(stmt) else {
        return false;
    };
    if ident(&registration.callee) != Some("it")
        || registration.arguments.len() != 2
        || !matches!(
            registration.arguments.first(),
            Some(Argument::StringLiteral(_))
        )
    {
        return false;
    }
    let span = stmt.span();
    let Some(text) = source.get(span.start as usize..span.end as usize) else {
        return false;
    };
    if text.trim_end_matches(';') == test_body.trim_end_matches(';') {
        *found += 1;
        return true;
    }
    let Some(Argument::ArrowFunctionExpression(callback)) = registration.arguments.get(1) else {
        return false;
    };
    if !callback.r#async
        || callback.expression
        || callback.params.items.len() != 1
        || callback.params.rest.is_some()
        || !callback.body.directives.is_empty()
    {
        return false;
    }
    if callback.params.items[0].initializer.is_some() {
        return false;
    }
    let Some(context) = super::owners::binding_identifier_name(&callback.params.items[0].pattern)
    else {
        return false;
    };
    if ["it", "mock", "assert", "Error", owner].contains(&context) {
        return false;
    }
    let [Statement::VariableDeclaration(decl), second, third] = callback.body.statements.as_slice()
    else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const || decl.declarations.len() != 1 {
        return false;
    }
    let decl = &decl.declarations[0];
    let Some(local) = super::owners::binding_identifier_name(&decl.id) else {
        return false;
    };
    if ["it", "mock", "assert", "Error", owner, context].contains(&local) {
        return false;
    }
    let Some(factory) = decl.init.as_ref().and_then(call) else {
        return false;
    };
    let Some(base) = member(&factory.callee, "fn").and_then(|e| member(e, "mock")) else {
        return false;
    };
    if ident(base) != Some(context) || factory.arguments.len() != 1 {
        return false;
    }
    let Some(Argument::ArrowFunctionExpression(success)) = factory.arguments.first() else {
        return false;
    };
    if !success.r#async
        || !success.expression
        || !success.params.items.is_empty()
        || success.params.rest.is_some()
    {
        return false;
    }
    let [Statement::ExpressionStatement(returned)] = success.body.statements.as_slice() else {
        return false;
    };
    let Expression::StringLiteral(value) = &returned.expression else {
        return false;
    };
    let Some(assertion) = stmt_call(second) else {
        return false;
    };
    if member(&assertion.callee, "equal").and_then(ident) != Some("assert")
        || assertion.arguments.len() != 2
    {
        return false;
    }
    let Some(Argument::AwaitExpression(awaited)) = assertion.arguments.first() else {
        return false;
    };
    let Some(invocation) = call(&awaited.argument) else {
        return false;
    };
    if ident(&invocation.callee) != Some(owner)
        || invocation.arguments.len() != 2
        || !matches!(invocation.arguments.first(), Some(Argument::Identifier(id)) if id.name == local)
        || invocation.arguments.get(1).and_then(unsigned).is_none()
        || !matches!(assertion.arguments.get(1), Some(Argument::StringLiteral(expected)) if expected.value == value.value)
    {
        return false;
    }
    let Some(assertion) = stmt_call(third) else {
        return false;
    };
    if member(&assertion.callee, "equal").and_then(ident) != Some("assert")
        || assertion.arguments.len() != 2
        || assertion.arguments.get(1).and_then(unsigned) != Some(1)
    {
        return false;
    }
    let Some(observed) = assertion
        .arguments
        .first()
        .and_then(Argument::as_expression)
        .and_then(call)
    else {
        return false;
    };
    observed.arguments.is_empty()
        && member(&observed.callee, "callCount")
            .and_then(|e| member(e, "mock"))
            .and_then(ident)
            == Some(local)
}
