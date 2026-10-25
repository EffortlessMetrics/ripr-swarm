//! Lexical runner identities for registration disclosure. Unknown bindings
//! never establish that a suite callback is dormant.
use super::*;
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Value {
    Root(tests_extract::TestDeclarationRoot, bool),
    Namespace,
    GlobalObject,
}

#[derive(Clone)]
struct Alias {
    name: String,
    at: u32,
    members: Vec<String>,
}

impl Alias {
    fn of(expression: &Expression<'_>) -> Option<Self> {
        match expression.get_inner_expression() {
            Expression::Identifier(id) => Some(Self {
                name: id.name.to_string(),
                at: id.span.start,
                members: Vec::new(),
            }),
            Expression::StaticMemberExpression(member) if !member.optional => {
                let mut path = Self::of(&member.object)?;
                path.members.push(member.property.name.to_string());
                Some(path)
            }
            _ => None,
        }
    }
}

#[derive(Default)]
struct Binding {
    value: Option<Value>,
    alias: Option<Alias>,
    potential: Vec<Alias>,
    ready: u32,
}

struct Scope {
    span: oxc_span::Span,
    parent: Option<usize>,
    catch_scope: bool,
    bindings: BTreeMap<String, Binding>,
}

pub(super) struct DeclarationBindings {
    scopes: Vec<Scope>,
    stack: Vec<usize>,
    variable_scopes: Vec<usize>,
    lexical_functions: bool,
    opaque: bool,
    global_writes: Vec<String>,
    member_writes: Vec<Alias>,
    global_members: Vec<(Alias, Option<String>)>,
    writes: Vec<Alias>,
    mutated_runner: bool,
    assigned_aliases: Vec<(Alias, Alias)>,
    escaped: Vec<Alias>,
}

impl DeclarationBindings {
    pub(super) fn of_program(
        program: &oxc_ast::ast::Program<'_>,
        admission: &TypeScriptAdmissionContext,
    ) -> Self {
        let mut index = Self {
            scopes: Vec::new(),
            stack: Vec::new(),
            variable_scopes: Vec::new(),
            lexical_functions: admission.block_functions_are_lexical,
            opaque: false,
            global_writes: Vec::new(),
            member_writes: Vec::new(),
            global_members: Vec::new(),
            writes: Vec::new(),
            mutated_runner: false,
            assigned_aliases: Vec::new(),
            escaped: Vec::new(),
        };
        index.visit_program(program);
        // Admission owns validated import provenance (including literal
        // CommonJS imports and a shadowed require). Do not reclassify modules.
        for import in admission.runner_declaration_imports() {
            let value = if import.namespace {
                Some(Value::Namespace)
            } else {
                match import.imported.as_deref() {
                    Some("default") | None => root_value("test"),
                    Some(imported) => root_value(imported),
                }
            };
            let binding = index.scopes[0]
                .bindings
                .entry(import.local.clone())
                .or_default();
            binding.value = value;
            binding.alias = None;
        }
        for (path, property) in &index.global_members {
            if index.binding_scope(&path.name, path.at).is_none() {
                if let Some(property) = property {
                    index.global_writes.push(property.clone());
                } else {
                    index.opaque = true;
                }
            }
        }
        index.global_members.clear();
        for (target, value) in &index.assigned_aliases {
            if let Some(scope) = index.binding_scope(&target.name, target.at)
                && let Some(binding) = index.scopes[scope].bindings.get_mut(&target.name)
            {
                binding.potential.push(value.clone());
            }
        }
        index.assigned_aliases.clear();
        // Assignment targets retain lexical identity, including targets in
        // destructuring, loops and transparent TypeScript wrappers.
        for path in &index.writes {
            if let Some(scope) = index.binding_scope(&path.name, path.at) {
                if let Some(binding) = index.scopes[scope].bindings.get_mut(&path.name) {
                    binding.alias = None;
                    binding.value = None;
                }
            } else {
                index.global_writes.push(path.name.clone());
            }
        }
        index.writes.clear();
        index.opaque |= tests_extract::byte_preserving_code_mask_for_source_type(
            program.source_text,
            program.source_type,
        )
        .is_none_or(|code| tests_extract::file_is_opaque(&code));
        index.mutated_runner = index
            .member_writes
            .iter()
            .chain(index.escaped.iter())
            .any(|path| index.potential_runner(path, 32));
        index.member_writes.clear();
        index.escaped.clear();
        index
    }

    pub(super) fn active(
        &self,
        expression: &Expression<'_>,
        root: tests_extract::TestDeclarationRoot,
    ) -> bool {
        self.value(expression) == Some(Value::Root(root, false))
    }

    pub(super) fn skipped_describe(&self, expression: &Expression<'_>) -> bool {
        self.value(expression)
            == Some(Value::Root(
                tests_extract::TestDeclarationRoot::Describe,
                true,
            ))
    }

    fn value(&self, expression: &Expression<'_>) -> Option<Value> {
        if self.opaque || self.mutated_runner {
            return None;
        }
        self.resolve(&Alias::of(expression)?, 32)
    }

    fn resolve(&self, path: &Alias, remaining: usize) -> Option<Value> {
        if remaining == 0 {
            return None;
        }
        let binding = self
            .binding_scope(&path.name, path.at)
            .and_then(|index| self.scopes[index].bindings.get(&path.name));
        let mut value = match binding {
            Some(binding) if path.at < binding.ready => return None,
            Some(Binding {
                alias: Some(alias), ..
            }) => self.resolve(alias, remaining - 1)?,
            Some(binding) => binding.value?,
            None if self.global_writes.contains(&path.name) => return None,
            None => root_value(&path.name)?,
        };
        for member in &path.members {
            value = match (value, member.as_str()) {
                (Value::Namespace | Value::GlobalObject, member) => root_value(member)?,
                (
                    Value::Root(root, skipped),
                    "only" | "concurrent" | "sequential" | "serial" | "parallel",
                ) => Value::Root(root, skipped),
                (Value::Root(root, _), "skip" | "fixme") => Value::Root(root, true),
                // Playwright's suite root is a member of its test binding.
                (Value::Root(tests_extract::TestDeclarationRoot::Test, skipped), "describe") => {
                    Value::Root(tests_extract::TestDeclarationRoot::Describe, skipped)
                }
                _ => return None,
            };
        }
        Some(value)
    }

    // Mutable/previously written aliases cannot establish dormancy, but can
    // still carry the runner into a mutation or an opaque call.
    fn potential_runner(&self, path: &Alias, remaining: usize) -> bool {
        self.potential_runner_memo(path, remaining, &mut BTreeMap::new())
    }

    fn potential_runner_memo(
        &self,
        path: &Alias,
        remaining: usize,
        memo: &mut BTreeMap<(usize, String), Option<bool>>,
    ) -> bool {
        if remaining == 0 {
            return true;
        }
        if self.resolve(path, remaining).is_some() {
            return true;
        }
        let Some(scope) = self.binding_scope(&path.name, path.at) else {
            return false;
        };
        let key = (scope, path.name.clone());
        if let Some(cached) = memo.get(&key) {
            return cached.unwrap_or(true);
        }
        // None is an in-progress binding; a cycle remains possible mutation.
        memo.insert(key.clone(), None);
        let possible = self.scopes[scope]
            .bindings
            .get(&path.name)
            .is_some_and(|binding| {
                binding
                    .potential
                    .iter()
                    .any(|alias| self.potential_runner_memo(alias, remaining - 1, memo))
            });
        memo.insert(key, Some(possible));
        possible
    }

    fn binding_scope(&self, name: &str, at: u32) -> Option<usize> {
        let mut scope = self
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.span.start <= at && at < scope.span.end)
            .min_by_key(|(index, scope)| (scope.span.size(), std::cmp::Reverse(*index)))
            .map(|(index, _)| index);
        while let Some(index) = scope {
            if self.scopes[index].bindings.contains_key(name) {
                return Some(index);
            }
            scope = self.scopes[index].parent;
        }
        None
    }

    fn push(&mut self, span: oxc_span::Span, statements: &[Statement<'_>]) {
        let mut names = Vec::new();
        tests_extract::collect_registration_scope_names(
            statements,
            &mut names,
            self.lexical_functions,
        );
        let index = self.scopes.len();
        self.scopes.push(Scope {
            span,
            parent: self.stack.last().copied(),
            catch_scope: false,
            bindings: names
                .into_iter()
                .map(|name| (name, Binding::default()))
                .collect(),
        });
        self.stack.push(index);
    }

    fn member_write(&mut self, object: &Expression<'_>, property: Option<&str>) {
        if matches!(object.get_inner_expression(), Expression::ThisExpression(_)) {
            // A dynamic `this` may carry a runner namespace. Retain the notice.
            self.opaque = true;
        } else if let Some(path) = Alias::of(object) {
            if matches!(
                path.name.as_str(),
                "globalThis" | "window" | "self" | "global"
            ) && path.members.is_empty()
            {
                self.global_members
                    .push((path.clone(), property.map(ToString::to_string)));
            }
            self.member_writes.push(path);
        }
    }

    fn write_expression(&mut self, expression: &Expression<'_>) {
        match expression.get_inner_expression() {
            Expression::Identifier(_) => {
                if let Some(path) = Alias::of(expression) {
                    self.writes.push(path);
                }
            }
            Expression::StaticMemberExpression(member) => {
                self.member_write(&member.object, Some(member.property.name.as_str()))
            }
            Expression::ComputedMemberExpression(member) => {
                let name = match member.expression.get_inner_expression() {
                    Expression::StringLiteral(literal) => Some(literal.value.as_str()),
                    _ => None,
                };
                self.member_write(&member.object, name);
            }
            _ => self.opaque = true,
        }
    }

    fn escape_arguments(&mut self, arguments: &[Argument<'_>]) {
        for argument in arguments {
            if let Argument::SpreadElement(spread) = argument {
                self.escape_value(&spread.argument);
            } else if let Some(value) = argument.as_expression() {
                self.escape_value(value);
            }
        }
    }

    fn escape_value(&mut self, value: &Expression<'_>) {
        if let Some(path) = Alias::of(value) {
            self.escaped.push(path);
            return;
        }
        match value.get_inner_expression() {
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    if let oxc_ast::ast::ArrayExpressionElement::SpreadElement(spread) = element {
                        self.escape_value(&spread.argument);
                    } else if let Some(value) = element.as_expression() {
                        self.escape_value(value);
                    }
                }
            }
            Expression::ObjectExpression(object) => {
                for property in &object.properties {
                    match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            self.escape_value(&property.value)
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            self.escape_value(&spread.argument)
                        }
                    }
                }
            }
            Expression::ConditionalExpression(conditional) => {
                self.escape_value(&conditional.consequent);
                self.escape_value(&conditional.alternate);
            }
            Expression::LogicalExpression(logical) => {
                self.escape_value(&logical.left);
                self.escape_value(&logical.right);
            }
            Expression::SequenceExpression(sequence) => {
                for value in &sequence.expressions {
                    self.escape_value(value);
                }
            }
            _ => {}
        }
    }

    fn bind_unknown(&mut self, name: &str) {
        if let Some(index) = self.stack.last().copied() {
            self.scopes[index]
                .bindings
                .insert(name.to_string(), Binding::default());
        }
    }
}

fn root_value(name: &str) -> Option<Value> {
    use tests_extract::TestDeclarationRoot::{Describe, Test};
    let (name, skipped) = match name {
        "xdescribe" | "xcontext" | "xsuite" | "xit" | "xtest" | "xspecify" => (&name[1..], true),
        other => (other, false),
    };
    if matches!(name, "globalThis" | "window" | "self" | "global") {
        Some(Value::GlobalObject)
    } else if Test.matches_identifier(name) {
        Some(Value::Root(Test, skipped))
    } else if Describe.matches_identifier(name) {
        Some(Value::Root(Describe, skipped))
    } else {
        None
    }
}

impl<'a> Visit<'a> for DeclarationBindings {
    fn visit_simple_assignment_target(
        &mut self,
        target: &oxc_ast::ast::SimpleAssignmentTarget<'a>,
    ) {
        use oxc_ast::ast::SimpleAssignmentTarget as Target;
        match target {
            Target::AssignmentTargetIdentifier(id) => self.writes.push(Alias {
                name: id.name.to_string(),
                at: id.span.start,
                members: Vec::new(),
            }),
            Target::StaticMemberExpression(member) => {
                self.member_write(&member.object, Some(member.property.name.as_str()))
            }
            Target::ComputedMemberExpression(member) => {
                let name = match member.expression.get_inner_expression() {
                    Expression::StringLiteral(literal) => Some(literal.value.as_str()),
                    _ => None,
                };
                self.member_write(&member.object, name);
            }
            Target::TSAsExpression(wrapper) => self.write_expression(&wrapper.expression),
            Target::TSSatisfiesExpression(wrapper) => self.write_expression(&wrapper.expression),
            Target::TSNonNullExpression(wrapper) => self.write_expression(&wrapper.expression),
            Target::TSTypeAssertion(wrapper) => self.write_expression(&wrapper.expression),
            _ => self.opaque = true,
        }
        oxc_ast_visit::walk::walk_simple_assignment_target(self, target);
    }

    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::Program(program) => {
                self.push(program.span, &program.body);
                self.variable_scopes.extend(self.stack.last().copied());
            }
            AstKind::Function(function) => {
                self.push(
                    function.span,
                    function
                        .body
                        .as_ref()
                        .map_or(&[], |body| body.statements.as_slice()),
                );
                self.variable_scopes.extend(self.stack.last().copied());
                if let Some(id) = &function.id {
                    self.bind_unknown(id.name.as_str());
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                self.push(arrow.span, &arrow.body.statements);
                self.variable_scopes.extend(self.stack.last().copied());
            }
            AstKind::BlockStatement(block) => self.push(block.span, &block.body),
            AstKind::CatchClause(catch) => {
                self.push(catch.span, &[]);
                if let Some(scope) = self.stack.last().copied() {
                    self.scopes[scope].catch_scope = true;
                }
                if let Some(param) = &catch.param {
                    for id in param.pattern.get_binding_identifiers() {
                        self.bind_unknown(id.name.as_str());
                    }
                }
            }
            AstKind::ForStatement(statement) => self.push(statement.span, &[]),
            AstKind::ForOfStatement(statement) => self.push(statement.span, &[]),
            AstKind::ForInStatement(statement) => self.push(statement.span, &[]),
            AstKind::SwitchStatement(statement) => {
                self.push(statement.span, &[]);
                for case in &statement.cases {
                    let mut names = Vec::new();
                    tests_extract::collect_registration_scope_names(
                        &case.consequent,
                        &mut names,
                        self.lexical_functions,
                    );
                    for name in names {
                        self.bind_unknown(&name);
                    }
                }
            }
            AstKind::Class(class) => {
                self.push(class.span, &[]);
                if let Some(id) = &class.id {
                    self.bind_unknown(id.name.as_str());
                }
            }
            AstKind::StaticBlock(block) => {
                self.push(block.span, &block.body);
                self.variable_scopes.extend(self.stack.last().copied());
            }
            AstKind::TSModuleBlock(block) => {
                self.push(block.span, &block.body);
                self.variable_scopes.extend(self.stack.last().copied());
            }
            AstKind::FormalParameters(params) => {
                for id in params
                    .items
                    .iter()
                    .flat_map(|param| param.pattern.get_binding_identifiers())
                    .chain(
                        params
                            .rest
                            .iter()
                            .flat_map(|rest| rest.rest.argument.get_binding_identifiers()),
                    )
                {
                    self.bind_unknown(id.name.as_str());
                }
            }
            AstKind::ImportDeclaration(import) => {
                if let Some(specifiers) = &import.specifiers {
                    for specifier in specifiers {
                        self.bind_unknown(specifier.local().name.as_str());
                    }
                }
            }
            AstKind::AssignmentTargetPropertyIdentifier(property) => {
                if let Some(init) = &property.init {
                    self.escape_value(init);
                }
                self.writes.push(Alias {
                    name: property.binding.name.to_string(),
                    at: property.binding.span.start,
                    members: Vec::new(),
                });
            }
            AstKind::AssignmentExpression(assignment) => {
                if let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) =
                    &assignment.left
                    && let Some(value) = Alias::of(&assignment.right)
                {
                    self.assigned_aliases.push((
                        Alias {
                            name: id.name.to_string(),
                            at: id.span.start,
                            members: Vec::new(),
                        },
                        value,
                    ));
                } else {
                    self.escape_value(&assignment.right);
                }
            }
            AstKind::AssignmentPattern(pattern) => self.escape_value(&pattern.right),
            AstKind::AssignmentTargetWithDefault(target) => self.escape_value(&target.init),
            AstKind::CallExpression(call) => self.escape_arguments(&call.arguments),
            AstKind::NewExpression(call) => self.escape_arguments(&call.arguments),
            AstKind::ReturnStatement(returned) => {
                if let Some(value) = &returned.argument {
                    self.escape_value(value);
                }
            }
            AstKind::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    for id in declarator.id.get_binding_identifiers() {
                        let Some(current) = self.stack.last().copied() else {
                            continue;
                        };
                        let owner =
                            if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var {
                                self.variable_scopes.last().copied().unwrap_or(current)
                            } else {
                                current
                            };
                        if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var {
                            // One hoisted binding, including repeated `var d;`.
                            // Remove only block copies on this declaration path.
                            for scope in self.stack.iter().rev().copied() {
                                if scope <= owner {
                                    break;
                                }
                                if self.scopes[scope].catch_scope
                                    && self.scopes[scope].bindings.contains_key(id.name.as_str())
                                {
                                    self.opaque = true;
                                }
                                self.scopes[scope].bindings.remove(id.name.as_str());
                            }
                            self.scopes[owner]
                                .bindings
                                .entry(id.name.to_string())
                                .or_default();
                        } else {
                            self.scopes[owner].bindings.insert(
                                id.name.to_string(),
                                Binding {
                                    ready: declarator.span.end,
                                    ..Binding::default()
                                },
                            );
                        }
                    }
                    if let Some(init) = declarator.init.as_ref().and_then(Alias::of) {
                        for id in declarator.id.get_binding_identifiers() {
                            let owner =
                                if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var {
                                    self.variable_scopes.last().copied()
                                } else {
                                    self.stack.last().copied()
                                };
                            if let Some(owner) = owner
                                && let Some(binding) =
                                    self.scopes[owner].bindings.get_mut(id.name.as_str())
                            {
                                binding.potential.push(init.clone());
                            }
                        }
                    } else if let Some(init) = &declarator.init {
                        // An object/array can carry the runner into an otherwise
                        // unresolved data flow; do not establish dormancy.
                        self.escape_value(init);
                    }
                    if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
                        || declaration.declare
                    {
                        continue;
                    }
                    let Some(init) = declarator.init.as_ref().and_then(Alias::of) else {
                        continue;
                    };
                    let Some(index) = self.stack.last().copied() else {
                        continue;
                    };
                    match &declarator.id {
                        BindingPattern::BindingIdentifier(id) => {
                            self.scopes[index].bindings.insert(
                                id.name.to_string(),
                                Binding {
                                    potential: vec![init.clone()],
                                    alias: Some(init),
                                    ready: declarator.span.end,
                                    value: None,
                                },
                            );
                        }
                        BindingPattern::ObjectPattern(pattern) => {
                            for property in &pattern.properties {
                                if property.computed {
                                    continue;
                                }
                                if let BindingPattern::BindingIdentifier(id) = &property.value
                                    && let Some(key) = property.key.static_name()
                                {
                                    let mut path = init.clone();
                                    path.members.push(key.to_string());
                                    self.scopes[index].bindings.insert(
                                        id.name.to_string(),
                                        Binding {
                                            potential: vec![path.clone()],
                                            alias: Some(path),
                                            ready: declarator.span.end,
                                            value: None,
                                        },
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        if matches!(
            kind,
            AstKind::Program(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::StaticBlock(_)
                | AstKind::TSModuleBlock(_)
        ) {
            self.variable_scopes.pop();
        }
        if matches!(
            kind,
            AstKind::Program(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::BlockStatement(_)
                | AstKind::CatchClause(_)
                | AstKind::ForStatement(_)
                | AstKind::ForOfStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::SwitchStatement(_)
                | AstKind::Class(_)
                | AstKind::StaticBlock(_)
                | AstKind::TSModuleBlock(_)
        ) {
            self.stack.pop();
        }
    }
}
