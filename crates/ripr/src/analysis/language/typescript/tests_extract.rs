//! Test extraction for the TypeScript preview adapter.

use super::*;

#[derive(Clone, Copy)]
enum TestDeclarationRoot {
    Test,
    Describe,
}

impl TestDeclarationRoot {
    fn matches_identifier(self, name: &str) -> bool {
        match self {
            Self::Test => matches!(name, "test" | "it"),
            Self::Describe => name == "describe",
        }
    }
}

pub(crate) fn extract_tests(file: &Path, source: &str) -> Vec<TypeScriptTest> {
    // Same guard as owner extraction (#4101): a file under the nesting
    // budget can still overflow the caller stack, so the second parse must
    // not run on the main thread.
    let Ok(tests) = parse_on_worker(file, source, |file, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() {
            return Vec::new();
        }
        let imports = extract_imports_from_statements(&ret.program.body);
        let mocks = extract_mocks_from_statements(&ret.program.body);
        let mut tests = Vec::new();
        collect_tests_from_statements(
            &ret.program.body,
            file,
            source,
            &mocks,
            &imports,
            &mut TestScope::default(),
            &mut tests,
        );
        tests
    }) else {
        return Vec::new();
    };
    tests
}

/// Walk a list of statements and collect every syntactic
/// `vi.mock("path")` / `jest.mock("path")` (and the non-hoisted `doMock`
/// variants) argument we see, at ANY statement depth the runners hoist
/// through — including `describe(...)` callback bodies, where both Jest and
/// Vitest legally allow `mock`/`doMock` calls. The list is deduplicated and
/// used by the classifier to surface the `mocked_module` static-limit per
/// RIPR-SPEC-0026.
///
/// This is purely syntactic — the adapter does not resolve the mocked
/// module identifier through the project's import graph, so the limit
/// surfaces exactly when the test file contains the mock call shape.
pub(crate) fn extract_mocks_from_statements(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    collect_mock_paths(statements, &mut out);
    out
}

/// Recursively collect mock paths from statements. Statement containers
/// (blocks, if/else, loops, try/catch, switch) and function-bodied call
/// arguments (describe/it/beforeAll callbacks) are walked; everything else
/// is ignored. Deduplication preserves first-seen order.
fn collect_mock_paths(statements: &[Statement<'_>], out: &mut Vec<String>) {
    for stmt in statements {
        match stmt {
            Statement::BlockStatement(block) => collect_mock_paths(&block.body, out),
            Statement::ExpressionStatement(expr_stmt) => {
                collect_mock_path_from_expression(&expr_stmt.expression, out);
            }
            Statement::IfStatement(if_stmt) => {
                collect_mock_paths(std::slice::from_ref(&if_stmt.consequent), out);
                if let Some(alternate) = &if_stmt.alternate {
                    collect_mock_paths(std::slice::from_ref(alternate), out);
                }
            }
            Statement::DoWhileStatement(do_while) => {
                collect_mock_paths(std::slice::from_ref(&do_while.body), out)
            }
            Statement::WhileStatement(while_stmt) => {
                collect_mock_paths(std::slice::from_ref(&while_stmt.body), out)
            }
            Statement::ForStatement(for_stmt) => {
                collect_mock_paths(std::slice::from_ref(&for_stmt.body), out)
            }
            Statement::ForInStatement(for_in) => {
                collect_mock_paths(std::slice::from_ref(&for_in.body), out)
            }
            Statement::ForOfStatement(for_of) => {
                collect_mock_paths(std::slice::from_ref(&for_of.body), out)
            }
            Statement::LabeledStatement(labeled) => {
                collect_mock_paths(std::slice::from_ref(&labeled.body), out)
            }
            Statement::TryStatement(try_stmt) => {
                collect_mock_paths(&try_stmt.block.body, out);
                if let Some(handler) = &try_stmt.handler {
                    collect_mock_paths(&handler.body.body, out);
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    collect_mock_paths(&finalizer.body, out);
                }
            }
            Statement::SwitchStatement(switch_stmt) => {
                for case in &switch_stmt.cases {
                    collect_mock_paths(&case.consequent, out);
                }
            }
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init {
                        collect_mock_path_from_expression(init, out);
                    }
                }
            }
            Statement::FunctionDeclaration(func) => {
                if let Some(body) = &func.body {
                    collect_mock_paths(&body.statements, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect a mock path from an expression: a direct `vi.mock("path")` /
/// `jest.doMock("path")` call, or a call whose function arguments are
/// callbacks to recurse into (so a describe-scoped mock is found). A mock
/// call chained after another call (`jest.mock("a").mock("b")`) is found by
/// descending into a `StaticMemberExpression` callee's object, so chained
/// registrations stay under the owner-module mock guard.
fn collect_mock_path_from_expression(expression: &Expression<'_>, out: &mut Vec<String>) {
    let Expression::CallExpression(call) = expression else {
        return;
    };
    if let Some(path) = mock_path_from_call(call)
        && !out.iter().any(|existing| existing == &path)
    {
        out.push(path);
    }
    if let Expression::StaticMemberExpression(member) = &call.callee {
        collect_mock_path_from_expression(&member.object, out);
    }
    for argument in &call.arguments {
        match argument {
            oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
                collect_mock_paths(&arrow.body.statements, out);
            }
            oxc_ast::ast::Argument::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    collect_mock_paths(&body.statements, out);
                }
            }
            _ => {}
        }
    }
}

/// Extract the mocked module path from a `vi.mock("path")` /
/// `jest.mock("path")` / `vi.doMock("path")` / `jest.doMock("path")` call.
/// `doMock` is the non-hoisted variant both runners expose (it affects only
/// modules loaded after the call); the adapter cannot prove the observed
/// call reaches the real module, so the owner-module mock guard treats it
/// exactly like `mock` (#4103 shape 2).
fn mock_path_from_call(call: &oxc_ast::ast::CallExpression<'_>) -> Option<String> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(object_ident) = &member.object else {
        return None;
    };
    let object_name = object_ident.name.as_str();
    if object_name != "vi" && object_name != "jest" {
        return None;
    }
    if !matches!(member.property.name.as_str(), "mock" | "doMock") {
        return None;
    }
    let first_arg = call.arguments.first()?;
    let oxc_ast::ast::Argument::StringLiteral(literal) = first_arg else {
        return None;
    };
    Some(literal.value.to_string())
}

pub(crate) fn collect_tests_from_statements(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    file: &Path,
    source: &str,
    mocks: &[String],
    imports: &[TypeScriptImport],
    scope: &mut TestScope,
    tests: &mut Vec<TypeScriptTest>,
) {
    // Bindings that hold for every test in this scope: variable, function and
    // class declarations and `beforeEach`/`beforeAll` assignments at this
    // level. Collected before any test so a hook written after a test still
    // counts, as it does at runtime.
    let mut level = Vec::new();
    for stmt in statements {
        collect_scope_bindings(stmt, source, &mut level);
    }
    scope.levels.push(level);
    for stmt in statements {
        if let Some((describe_name, body)) = describe_body_from_statement(stmt) {
            // `describe.each(...)('x', (cart) => ...)` binds its parameters
            // for every test inside.
            scope.levels.push(
                statement_callback_parameter_names(stmt, 1)
                    .into_iter()
                    .map(|name| (name, ScopeValue::Other, Phase::Declaration))
                    .collect(),
            );
            scope.describe_names.push(describe_name);
            collect_tests_from_statements(body, file, source, mocks, imports, scope, tests);
            scope.describe_names.pop();
            scope.levels.pop();
            continue;
        }
        if let Some(mut test) = test_from_statement(stmt, file, source, &scope.describe_names) {
            test.mocks_in_file = mocks.to_vec();
            test.imports_in_file = imports.to_vec();
            // Test callback parameters (`it.each` rows, Vitest fixtures)
            // shadow every enclosing binding of the same name.
            let parameters = statement_callback_parameter_names(stmt, 1)
                .into_iter()
                .map(|name| (name, ScopeValue::Other, Phase::Declaration))
                .collect();
            test.scope_bindings = scope.resolve_with(parameters);
            tests.push(test);
        }
    }
    scope.levels.pop();
}

/// The enclosing scopes of the statements being walked.
#[derive(Default)]
pub(crate) struct TestScope {
    /// Describe names, outermost first.
    describe_names: Vec<String>,
    /// Name bindings of the file and each enclosing describe, outermost first.
    levels: Vec<Vec<ScopeEntry>>,
}

/// One binding a scope-level statement makes, and when it runs.
type ScopeEntry = (String, ScopeValue, Phase);

/// When a scope-level write runs relative to a test in that scope, in order:
/// declarations while the file or `describe` callback runs, then `beforeAll`
/// hooks, then `beforeEach` hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Declaration,
    BeforeAll,
    BeforeEach,
}

/// What one setup statement binds a name to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScopeValue {
    /// `let cart: Cart;`: declared here, assigned elsewhere or never.
    Declared,
    /// `new Cart()` / `new shop.Cart()`, by constructor text.
    Constructed(String),
    /// Anything else, including a shadowing function, class or parameter.
    Other,
}

impl TestScope {
    /// Resolve every bound name to its innermost scope. Within that scope the
    /// last hook write decides, since hooks run after the declarations and
    /// before every test. Without a hook write the name is constructed only
    /// when every declaration of it constructs the same class; a declaration
    /// without an initializer is neutral.
    fn resolve_with(&self, innermost: Vec<ScopeEntry>) -> Vec<TypeScriptScopeBinding> {
        let mut resolved: Vec<TypeScriptScopeBinding> = Vec::new();
        for level in std::iter::once(&innermost).chain(self.levels.iter().rev()) {
            let mut names: Vec<&str> = level.iter().map(|(name, _, _)| name.as_str()).collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                if resolved.iter().any(|binding| binding.name == name) {
                    continue;
                }
                let mut writes: Vec<(&ScopeValue, Phase)> = level
                    .iter()
                    .filter(|(bound, _, _)| bound == name)
                    .map(|(_, value, phase)| (value, *phase))
                    .collect();
                // Stable: source order is kept within a phase.
                writes.sort_by_key(|(_, phase)| *phase);
                let constructed_by = match writes.last() {
                    Some((value, phase)) if *phase != Phase::Declaration => match value {
                        ScopeValue::Constructed(constructor) => Some(constructor.clone()),
                        ScopeValue::Declared | ScopeValue::Other => None,
                    },
                    _ => {
                        let mut constructors = writes.iter().filter_map(|(value, _)| match value {
                            ScopeValue::Declared => None,
                            ScopeValue::Constructed(constructor) => Some(Some(constructor)),
                            ScopeValue::Other => Some(None),
                        });
                        let first = constructors.next().flatten();
                        first
                            .filter(|first| constructors.all(|next| next == Some(*first)))
                            .cloned()
                    }
                };
                resolved.push(TypeScriptScopeBinding {
                    name: name.to_string(),
                    constructed_by,
                });
            }
        }
        resolved
    }
}

/// Record the names one scope-level statement binds for every test in the
/// scope: declarations, and assignments made by a `beforeEach`/`beforeAll`
/// hook. Test bodies, `describe` blocks, `afterEach` hooks and skipped tests
/// bind nothing here, and a hook's own local declarations stay local.
fn collect_scope_bindings(stmt: &Statement<'_>, source: &str, out: &mut Vec<ScopeEntry>) {
    match stmt {
        Statement::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
                    let value = match &declarator.init {
                        None => ScopeValue::Declared,
                        Some(init) => constructed_value(init),
                    };
                    out.push((identifier.name.to_string(), value, Phase::Declaration));
                } else {
                    for identifier in declarator.id.get_binding_identifiers() {
                        out.push((
                            identifier.name.to_string(),
                            ScopeValue::Other,
                            Phase::Declaration,
                        ));
                    }
                }
            }
        }
        Statement::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                out.push((
                    identifier.name.to_string(),
                    ScopeValue::Other,
                    Phase::Declaration,
                ));
            }
        }
        Statement::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                out.push((
                    identifier.name.to_string(),
                    ScopeValue::Other,
                    Phase::Declaration,
                ));
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                return;
            };
            let Expression::Identifier(callee) = &call.callee else {
                return;
            };
            let phase = match callee.name.as_str() {
                "beforeAll" => Phase::BeforeAll,
                "beforeEach" => Phase::BeforeEach,
                _ => return,
            };
            let Some(body) = call
                .arguments
                .first()
                .and_then(function_body_statements_from_argument)
            else {
                return;
            };
            // Writes to the hook's own declarations stay in the hook.
            let mut hook_locals = Vec::new();
            for hook_stmt in body {
                collect_scope_bindings(hook_stmt, source, &mut hook_locals);
            }
            let mut writes = Vec::new();
            for hook_stmt in body {
                collect_hook_assignment(hook_stmt, source, &mut writes);
            }
            out.extend(
                writes
                    .into_iter()
                    .filter(|(name, _)| !hook_locals.iter().any(|(local, _, _)| local == name))
                    .map(|(name, value)| (name, value, phase)),
            );
        }
        _ => {}
    }
}

/// A hook statement `name = <expr>;` binds `name`. Any other statement that
/// may assign a name (a nested block, a loop, a compound or destructuring
/// assignment) marks every name it assigns as ambiguous, read from its text
/// because the adapter has no nested-statement walk.
fn collect_hook_assignment(
    stmt: &Statement<'_>,
    source: &str,
    out: &mut Vec<(String, ScopeValue)>,
) {
    match stmt {
        // A hook's own declarations are local to the hook, but an
        // initializer may still write an outer name (`const r = (cart = x)`
        // or a closure that assigns it).
        Statement::VariableDeclaration(declaration) => {
            for init in declaration
                .declarations
                .iter()
                .filter_map(|declarator| declarator.init.as_ref())
            {
                let span = init.span();
                let text = source
                    .get(span.start as usize..span.end as usize)
                    .unwrap_or_default();
                for name in assigned_identifier_names(text) {
                    out.push((name, ScopeValue::Other));
                }
            }
            return;
        }
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::AssignmentExpression(assignment) =
                expr_stmt.expression.without_parentheses()
                && assignment.operator == oxc_ast::ast::AssignmentOperator::Assign
                && let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) =
                    &assignment.left
            {
                out.push((
                    target.name.to_string(),
                    constructed_value(&assignment.right),
                ));
                return;
            }
        }
        _ => {}
    }
    let span = stmt.span();
    let text = source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default();
    for name in assigned_identifier_names(text) {
        out.push((name, ScopeValue::Other));
    }
}

fn constructed_value(expression: &Expression<'_>) -> ScopeValue {
    let Expression::NewExpression(new_expression) = expression.without_parentheses() else {
        return ScopeValue::Other;
    };
    match &new_expression.callee {
        Expression::Identifier(identifier) => ScopeValue::Constructed(identifier.name.to_string()),
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(object) => {
                ScopeValue::Constructed(format!("{}.{}", object.name, member.property.name))
            }
            _ => ScopeValue::Other,
        },
        _ => ScopeValue::Other,
    }
}

/// Identifiers written as an assignment target anywhere in `text`
/// (`name =`, `name +=`, `name ??=`), excluding comparisons and arrows, and
/// member targets (`this.name =`). Over-reports on purpose: a false entry only
/// withholds a relation.
fn assigned_identifier_names(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if !(ch == '_' || ch == '$' || ch.is_ascii_alphabetic()) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && {
            let ch = bytes[index] as char;
            ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
        } {
            index += 1;
        }
        let preceded_by_member = text[..start].trim_end().ends_with('.');
        let rest = text[index..].trim_start();
        let operator_end = rest
            .find(|ch: char| {
                !matches!(
                    ch,
                    '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '?' | '<' | '>'
                )
            })
            .unwrap_or(rest.len());
        let after_operator = &rest[operator_end..];
        let assigns = after_operator.starts_with('=')
            && !after_operator[1..].starts_with(['=', '>'])
            && !matches!(&rest[..operator_end], "<" | ">" | "!");
        if assigns && !preceded_by_member && !names.iter().any(|name| name == &text[start..index]) {
            names.push(text[start..index].to_string());
        }
    }
    names
}

/// Parameter names of the callback at `index` in a statement's call
/// (`describe.each(...)('x', (row) => ...)`, `it('x', ({ fixture }) => ...)`).
fn statement_callback_parameter_names(stmt: &Statement<'_>, index: usize) -> Vec<String> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return Vec::new();
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return Vec::new();
    };
    let params = match call.arguments.get(index) {
        Some(oxc_ast::ast::Argument::ArrowFunctionExpression(arrow)) => &arrow.params,
        Some(oxc_ast::ast::Argument::FunctionExpression(function)) => &function.params,
        _ => return Vec::new(),
    };
    params
        .items
        .iter()
        .flat_map(|param| param.pattern.get_binding_identifiers())
        .map(|identifier| identifier.name.to_string())
        .collect()
}

pub(crate) fn describe_body_from_statement<'a>(
    stmt: &'a Statement<'a>,
) -> Option<(String, &'a oxc_allocator::Vec<'a, Statement<'a>>)> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return None;
    };
    if !call_callee_is_active_declaration(call, TestDeclarationRoot::Describe)
        && !call_callee_is_active_each_declaration(call, TestDeclarationRoot::Describe)
    {
        return None;
    }
    let name = string_argument(call.arguments.first()?)?;
    let body = function_body_statements_from_argument(call.arguments.get(1)?)?;
    Some((name, body))
}

pub(crate) fn test_from_statement(
    stmt: &Statement<'_>,
    file: &Path,
    source: &str,
    describe_stack: &[String],
) -> Option<TypeScriptTest> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return None;
    };
    let (name, assertions) = test_name_and_assertions_from_call(call, source)?;
    Some(TypeScriptTest {
        name: qualified_test_name(describe_stack, &name),
        local_name: name,
        describe_names: describe_stack.to_vec(),
        file: file.to_path_buf(),
        line: line_for_offset(source, call.span.start as usize),
        body_text: source[call.span.start as usize..call.span.end as usize].to_string(),
        assertions,
        // Populated by `extract_tests` (the only public extractor) once
        // per file before the test is returned to the caller.
        mocks_in_file: Vec::new(),
        imports_in_file: Vec::new(),
        scope_bindings: Vec::new(),
    })
}

pub(crate) fn test_name_and_assertions_from_call(
    call: &oxc_ast::ast::CallExpression<'_>,
    source: &str,
) -> Option<(String, Vec<TypeScriptAssertion>)> {
    if !call_callee_is_active_declaration(call, TestDeclarationRoot::Test)
        && !call_callee_is_active_each_declaration(call, TestDeclarationRoot::Test)
    {
        return None;
    }

    let name = string_argument(call.arguments.first()?)?;
    let callback = call.arguments.get(1)?;
    let receiver = test_callback_receiver_name(callback);
    let assertions = function_body_statements_from_argument(callback)
        .map(|statements| {
            collect_expect_assertions_in_statements(statements, source, receiver.as_deref())
        })
        .unwrap_or_default();
    Some((name, assertions))
}

/// Extract the name bound to the test callback's first parameter.
///
/// AVA / node:test / tape pass an execution context (conventionally `t`) as the
/// first argument of the test callback, and assertions are made on it
/// (`t.is(...)`, `t.deepEqual(...)`). Jest/Vitest callbacks take no such
/// receiver, so this returns `None` for a zero-parameter callback and the AVA
/// assertion matcher is never attempted (fail-closed: no receiver, no AVA
/// assertions credited).
fn test_callback_receiver_name(arg: &oxc_ast::ast::Argument<'_>) -> Option<String> {
    let params = match arg {
        oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => &arrow.params,
        oxc_ast::ast::Argument::FunctionExpression(func) => &func.params,
        _ => return None,
    };
    let first = params.items.first()?;
    super::owners::binding_identifier_name(&first.pattern).map(|name| name.to_string())
}

fn call_callee_is_active_declaration(
    call: &oxc_ast::ast::CallExpression<'_>,
    root: TestDeclarationRoot,
) -> bool {
    expression_is_active_declaration(&call.callee, root)
}

fn call_callee_is_active_each_declaration(
    call: &oxc_ast::ast::CallExpression<'_>,
    root: TestDeclarationRoot,
) -> bool {
    let Expression::CallExpression(each_call) = &call.callee else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &each_call.callee else {
        return false;
    };
    member.property.name.as_str() == "each"
        && expression_is_active_declaration(&member.object, root)
}

fn expression_is_active_declaration(
    expression: &Expression<'_>,
    root: TestDeclarationRoot,
) -> bool {
    match expression {
        Expression::Identifier(ident) => root.matches_identifier(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            is_active_declaration_modifier(member.property.name.as_str())
                && expression_is_active_declaration(&member.object, root)
        }
        _ => false,
    }
}

fn is_active_declaration_modifier(name: &str) -> bool {
    matches!(name, "only" | "concurrent" | "sequential")
}

pub(crate) fn string_argument(arg: &oxc_ast::ast::Argument<'_>) -> Option<String> {
    match arg {
        oxc_ast::ast::Argument::StringLiteral(literal) => Some(literal.value.to_string()),
        _ => None,
    }
}

pub(crate) fn function_body_statements_from_argument<'a>(
    arg: &'a oxc_ast::ast::Argument<'a>,
) -> Option<&'a oxc_allocator::Vec<'a, Statement<'a>>> {
    match arg {
        oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => Some(&arrow.body.statements),
        oxc_ast::ast::Argument::FunctionExpression(func) => {
            func.body.as_ref().map(|body| &body.statements)
        }
        _ => None,
    }
}

pub(crate) fn qualified_test_name(describe_stack: &[String], name: &str) -> String {
    if describe_stack.is_empty() {
        return name.to_string();
    }
    let mut parts = describe_stack.to_vec();
    parts.push(name.to_string());
    parts.join(" ")
}

/// Detect test registrations that the syntax-first extractor silently drops
/// from a recognized test file that parses cleanly.
///
/// Real producer for the `typescript_test_extraction_partial` named limitation
/// (classification-neutral additive disclosure): the test index still contains
/// exactly what `extract_tests` extracted; this function only reports that the
/// index is PARTIAL, so consumers know a confident `no_static_path` can be a
/// false negative for owners whose only tests use unsupported shapes.
///
/// Detected shapes (bounded preview slice — extracting these shapes is a
/// separate backlog item; this lane only discloses them):
///
/// - `` it(`title ${x}`, fn) `` / `` test(`title`, fn) `` — template-literal
///   titles (`string_argument` accepts `StringLiteral` only).
/// - `` test.each`table`('name', fn) `` / `` it.each`table`('name', fn) `` —
///   tagged-template `.each` (the tagged template sits in callee position, so
///   the extractor's identifier/member callee check never recognizes it).
/// - `it(...)` / `test(...)` calls nested inside loop, callback, or other
///   non-`describe` bodies — `collect_tests_from_statements` recurses only into
///   `describe(...)` bodies.
///
/// Returns `None` for a fully extracted file (the negative control contract):
/// every test-shaped call at a position the extractor visits carries a string
/// literal title and therefore already appears in `extracted`.
pub(crate) fn detect_partial_test_extraction(
    file: &Path,
    source: &str,
    extracted: &[TypeScriptTest],
) -> Option<TypeScriptTestExtractionGap> {
    // Span starts are pure string search. Compute them before the worker:
    // the parse closure is `'static` and cannot borrow `extracted`.
    let extracted_starts = extracted_span_starts(source, extracted);
    let Ok(gap) = parse_on_worker(file, source, move |file, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() {
            // Parse-error disclosure owns this case; do not double-report.
            return None;
        }
        let mut finder = UnextractedTestFinder {
            source,
            extracted_starts: &extracted_starts,
            gap: None,
        };
        finder.visit_statements(&ret.program.body);
        finder.gap.map(
            |(sample_line, shape, snippet)| TypeScriptTestExtractionGap {
                file: file.to_path_buf(),
                sample_line,
                shape,
                snippet,
            },
        )
    }) else {
        return None;
    };
    gap
}

/// Recover the byte-offset span start for every extracted test from its
/// recorded `body_text` (the exact source slice of the call), so the
/// finder can tell extracted registrations apart from dropped ones without
/// threading span state through the public extractor signature. Extraction
/// visits registrations in source order, so a forward watermark disambiguates
/// identical bodies on the same line.
fn extracted_span_starts(source: &str, extracted: &[TypeScriptTest]) -> Vec<usize> {
    let mut watermark = 0usize;
    let mut starts = Vec::new();
    for test in extracted {
        if let Some(pos) = source
            .get(watermark..)
            .and_then(|rest| rest.find(test.body_text.as_str()))
        {
            let start = watermark + pos;
            starts.push(start);
            // The next extracted test starts strictly after this one.
            watermark = start + 1;
        }
    }
    starts
}

/// Bounded AST walk that flags the first test-shaped registration the
/// extractor did not index. Intentionally a separate, shallower walk than the
/// extractor: it only needs enough recursion to reach realistic dropped shapes
/// (statement containers, call arguments, function bodies). Array-element and
/// object-property subtrees are out of scope for this disclosure slice.
struct UnextractedTestFinder<'a> {
    source: &'a str,
    extracted_starts: &'a [usize],
    gap: Option<(usize, &'static str, String)>,
}

impl UnextractedTestFinder<'_> {
    fn visit_statements(&mut self, statements: &[Statement<'_>]) {
        for stmt in statements {
            if self.gap.is_some() {
                return;
            }
            self.visit_statement(stmt);
        }
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>) {
        match stmt {
            Statement::BlockStatement(block) => self.visit_statements(&block.body),
            Statement::ExpressionStatement(expr_stmt) => {
                self.visit_expression(&expr_stmt.expression, true);
            }
            Statement::ReturnStatement(return_stmt) => {
                if let Some(argument) = &return_stmt.argument {
                    self.visit_expression(argument, false);
                }
            }
            Statement::ThrowStatement(throw_stmt) => {
                self.visit_expression(&throw_stmt.argument, false);
            }
            Statement::IfStatement(if_stmt) => {
                self.visit_statement(&if_stmt.consequent);
                if let Some(alternate) = &if_stmt.alternate {
                    self.visit_statement(alternate);
                }
            }
            Statement::DoWhileStatement(do_while) => self.visit_statement(&do_while.body),
            Statement::WhileStatement(while_stmt) => self.visit_statement(&while_stmt.body),
            Statement::ForStatement(for_stmt) => self.visit_statement(&for_stmt.body),
            Statement::ForInStatement(for_in) => self.visit_statement(&for_in.body),
            Statement::ForOfStatement(for_of) => self.visit_statement(&for_of.body),
            Statement::LabeledStatement(labeled) => self.visit_statement(&labeled.body),
            Statement::SwitchStatement(switch_stmt) => {
                for case in &switch_stmt.cases {
                    self.visit_statements(&case.consequent);
                    if self.gap.is_some() {
                        return;
                    }
                }
            }
            Statement::TryStatement(try_stmt) => {
                self.visit_statements(&try_stmt.block.body);
                if self.gap.is_none()
                    && let Some(handler) = &try_stmt.handler
                {
                    self.visit_statements(&handler.body.body);
                }
                if self.gap.is_none()
                    && let Some(finalizer) = &try_stmt.finalizer
                {
                    self.visit_statements(&finalizer.body);
                }
            }
            Statement::WithStatement(with_stmt) => self.visit_statement(&with_stmt.body),
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init {
                        self.visit_expression(init, false);
                    }
                    if self.gap.is_some() {
                        return;
                    }
                }
            }
            Statement::FunctionDeclaration(func) => self.visit_function(func),
            Statement::ClassDeclaration(class) => self.visit_class(class),
            Statement::ExportDefaultDeclaration(export_default) => {
                match &export_default.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                        self.visit_function(func);
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        self.visit_class(class);
                    }
                    _ => {}
                }
            }
            Statement::ExportNamedDeclaration(export_named) => {
                match export_named.declaration.as_ref() {
                    Some(Declaration::FunctionDeclaration(func)) => self.visit_function(func),
                    Some(Declaration::ClassDeclaration(class)) => self.visit_class(class),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn visit_function(&mut self, func: &Function<'_>) {
        if let Some(body) = &func.body {
            self.visit_statements(&body.statements);
        }
    }

    fn visit_class(&mut self, class: &Class<'_>) {
        for element in &class.body.body {
            match element {
                ClassElement::StaticBlock(block) => self.visit_statements(&block.body),
                ClassElement::MethodDefinition(method) => self.visit_function(&method.value),
                _ => {}
            }
            if self.gap.is_some() {
                return;
            }
        }
    }

    fn visit_expression(&mut self, expr: &Expression<'_>, statement_position: bool) {
        match expr {
            Expression::CallExpression(call) => self.visit_call(call, statement_position),
            Expression::NewExpression(new_expr) => self.visit_arguments(&new_expr.arguments),
            Expression::TaggedTemplateExpression(tagged) => {
                self.visit_tagged_template(tagged, statement_position);
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.visit_statements(&arrow.body.statements);
            }
            Expression::FunctionExpression(func) => self.visit_function(func),
            Expression::ClassExpression(class) => self.visit_class(class),
            Expression::AwaitExpression(await_expr) => {
                self.visit_expression(&await_expr.argument, false);
            }
            Expression::UnaryExpression(unary) => {
                self.visit_expression(&unary.argument, false);
            }
            // `UpdateExpression.argument` is an assignment target and
            // `ChainExpression.expression` is a `ChainElement`; neither can
            // contain a statement-position test registration, so both are
            // intentionally not recursed into.
            Expression::YieldExpression(yield_expr) => {
                if let Some(argument) = &yield_expr.argument {
                    self.visit_expression(argument, false);
                }
            }
            Expression::BinaryExpression(binary) => {
                self.visit_expression(&binary.left, false);
                self.visit_expression(&binary.right, false);
            }
            Expression::LogicalExpression(logical) => {
                self.visit_expression(&logical.left, false);
                self.visit_expression(&logical.right, false);
            }
            Expression::ConditionalExpression(conditional) => {
                self.visit_expression(&conditional.consequent, false);
                self.visit_expression(&conditional.alternate, false);
            }
            Expression::AssignmentExpression(assignment) => {
                self.visit_expression(&assignment.right, false);
            }
            Expression::SequenceExpression(sequence) => {
                for item in &sequence.expressions {
                    self.visit_expression(item, false);
                }
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                self.visit_expression(&parenthesized.expression, false);
            }
            Expression::StaticMemberExpression(member) => {
                self.visit_expression(&member.object, false);
            }
            Expression::ComputedMemberExpression(member) => {
                self.visit_expression(&member.object, false);
                self.visit_expression(&member.expression, false);
            }
            Expression::PrivateFieldExpression(member) => {
                self.visit_expression(&member.object, false);
            }
            Expression::ImportExpression(import) => {
                self.visit_expression(&import.source, false);
                if let Some(options) = &import.options {
                    self.visit_expression(options, false);
                }
            }
            Expression::TemplateLiteral(template) => {
                for item in &template.expressions {
                    self.visit_expression(item, false);
                }
            }
            _ => {}
        }
    }

    /// A `test(...)` / `it(...)` call — including `.each(...)` and modifier
    /// chains — that the extractor did not index. Only checked at statement
    /// position: nested calls (assertions, helpers) are never registrations.
    fn visit_call(&mut self, call: &oxc_ast::ast::CallExpression<'_>, statement_position: bool) {
        if statement_position && self.gap.is_none() {
            // Tagged-template `.each`: `` test.each`table`('name', fn) `` — the
            // tagged template sits in CALLEE position, so the call's callee is
            // not an identifier/member the extractor recognizes at all; the
            // registration is dropped regardless of the title shape.
            if let Expression::TaggedTemplateExpression(tagged) = &call.callee
                && !self.extracted_starts.contains(&(call.span.start as usize))
                && tagged_template_tag_is_test_each(&tagged.tag)
            {
                self.gap = Some((
                    line_for_offset(self.source, call.span.start as usize),
                    "tagged-template .each",
                    snippet_for_span(
                        self.source,
                        call.span.start as usize,
                        call.span.end as usize,
                    ),
                ));
                return;
            }
            if !self.extracted_starts.contains(&(call.span.start as usize))
                && (call_callee_is_active_declaration(call, TestDeclarationRoot::Test)
                    || call_callee_is_active_each_declaration(call, TestDeclarationRoot::Test))
            {
                let shape = match call.arguments.first() {
                    Some(Argument::TemplateLiteral(_)) => "template-literal title",
                    _ => "test/it call in loop/callback/nested body",
                };
                self.gap = Some((
                    line_for_offset(self.source, call.span.start as usize),
                    shape,
                    snippet_for_span(
                        self.source,
                        call.span.start as usize,
                        call.span.end as usize,
                    ),
                ));
                return;
            }
        }
        self.visit_arguments(&call.arguments);
    }

    fn visit_arguments(&mut self, arguments: &[Argument<'_>]) {
        for argument in arguments {
            if self.gap.is_some() {
                return;
            }
            self.visit_argument(argument);
        }
    }

    /// Recurse into a call argument. `Argument` inherits every `Expression`
    /// variant but is a distinct type with no borrowed conversion, so dispatch
    /// the container shapes the walker recurses into explicitly.
    fn visit_argument(&mut self, argument: &Argument<'_>) {
        match argument {
            Argument::SpreadElement(spread) => self.visit_expression(&spread.argument, false),
            Argument::CallExpression(call) => self.visit_call(call, false),
            Argument::NewExpression(new_expr) => self.visit_arguments(&new_expr.arguments),
            Argument::TaggedTemplateExpression(tagged) => {
                self.visit_tagged_template(tagged, false);
            }
            Argument::ArrowFunctionExpression(arrow) => {
                self.visit_statements(&arrow.body.statements);
            }
            Argument::FunctionExpression(func) => self.visit_function(func),
            Argument::ClassExpression(class) => self.visit_class(class),
            Argument::AwaitExpression(await_expr) => {
                self.visit_expression(&await_expr.argument, false);
            }
            Argument::ParenthesizedExpression(parenthesized) => {
                self.visit_expression(&parenthesized.expression, false);
            }
            Argument::SequenceExpression(sequence) => {
                for item in &sequence.expressions {
                    self.visit_expression(item, false);
                }
            }
            Argument::ConditionalExpression(conditional) => {
                self.visit_expression(&conditional.consequent, false);
                self.visit_expression(&conditional.alternate, false);
            }
            Argument::LogicalExpression(logical) => {
                self.visit_expression(&logical.left, false);
                self.visit_expression(&logical.right, false);
            }
            Argument::BinaryExpression(binary) => {
                self.visit_expression(&binary.left, false);
                self.visit_expression(&binary.right, false);
            }
            Argument::StaticMemberExpression(member) => {
                self.visit_expression(&member.object, false);
            }
            Argument::ComputedMemberExpression(member) => {
                self.visit_expression(&member.object, false);
                self.visit_expression(&member.expression, false);
            }
            Argument::PrivateFieldExpression(member) => {
                self.visit_expression(&member.object, false);
            }
            Argument::TemplateLiteral(template) => {
                for item in &template.expressions {
                    self.visit_expression(item, false);
                }
            }
            _ => {}
        }
    }

    fn visit_tagged_template(
        &mut self,
        tagged: &oxc_ast::ast::TaggedTemplateExpression<'_>,
        statement_position: bool,
    ) {
        if statement_position && self.gap.is_none() && tagged_template_tag_is_test_each(&tagged.tag)
        {
            self.gap = Some((
                line_for_offset(self.source, tagged.span.start as usize),
                "tagged-template .each",
                snippet_for_span(
                    self.source,
                    tagged.span.start as usize,
                    tagged.span.end as usize,
                ),
            ));
            return;
        }
        self.visit_expression(&tagged.tag, false);
        for quasi_expr in &tagged.quasi.expressions {
            self.visit_expression(quasi_expr, false);
        }
    }
}

/// Whether a tagged template's tag is the `test.each` / `it.each` tagged-template
/// form (`` test.each`table` ``), as opposed to the call form
/// `test.each([...])(...)` handled by `call_callee_is_active_each_declaration`.
fn tagged_template_tag_is_test_each(tag: &Expression<'_>) -> bool {
    let Expression::StaticMemberExpression(member) = tag else {
        return false;
    };
    member.property.name.as_str() == "each"
        && expression_is_active_declaration(&member.object, TestDeclarationRoot::Test)
}

/// Single-line, length-bounded source snippet for limitation details.
pub(crate) fn snippet_for_span(source: &str, start: usize, end: usize) -> String {
    let snippet: String = source
        .get(start..end)
        .unwrap_or_default()
        .chars()
        .take(80)
        .collect();
    let collapsed = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 77 {
        format!("{}...", collapsed.chars().take(77).collect::<String>())
    } else {
        collapsed
    }
}
