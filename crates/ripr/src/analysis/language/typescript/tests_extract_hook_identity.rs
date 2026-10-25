//! Registration-time hook callable identity, shared by receiver and Mocha setup.
//! Only ordinary statements and admitted synchronous suite callbacks have a
//! modeled order. Helpers and repeated/conditional registrations stay conservative.

use super::*;
use oxc_ast::ast::{AssignmentTargetPropertyIdentifier, FormalParameters, SimpleAssignmentTarget};
use oxc_ast::ast_kind::AstKind;
use oxc_ast_visit::{Visit, walk};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct HookCalls(HashSet<u32>);

impl HookCalls {
    pub(super) fn contains(&self, call: &oxc_ast::ast::CallExpression<'_>) -> bool {
        self.0.contains(&call.span.start)
    }

    pub(super) fn of_program(
        program: &oxc_ast::ast::Program<'_>,
        imports: &[TypeScriptImport],
        admission: &TypeScriptAdmissionContext,
    ) -> Self {
        let mut names = Vec::new();
        collect_registration_scope_names(
            &program.body,
            &mut names,
            admission.block_functions_are_lexical,
        );
        names.extend(imports.iter().map(|import| import.local.clone()));
        let frame = Frame {
            anchor: 0,
            names,
            literal_object_roots: literal_object_roots(&program.body),
            previous_mode: Mode::Registration,
            previous_strict: admission.block_functions_are_lexical,
        };
        let mut scan = Scan {
            frames: vec![frame.clone()],
            mode: Mode::Registration,
            strict: admission.block_functions_are_lexical,
            admission,
            active_writes: HashSet::new(),
            deferred_writes: HashSet::new(),
            all_writes: HashSet::new(),
            synchronous_callbacks: HashSet::new(),
            calls: HashSet::new(),
            statement_call: None,
            record_calls: false,
            collecting_region: false,
        };
        // A helper declared later can be called earlier. Do not assign its
        // body an execution time from its lexical offset.
        scan.visit_program(program);
        scan.frames = vec![frame];
        scan.mode = Mode::Registration;
        scan.strict = admission.block_functions_are_lexical;
        scan.active_writes.clear();
        scan.synchronous_callbacks.clear();
        scan.record_calls = true;
        scan.visit_program(program);
        Self(scan.calls)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Binding {
    frame: Option<u64>,
    name: String,
}

#[derive(Clone, Copy)]
enum Mode {
    Registration,
    Deferred,
}

#[derive(Clone)]
struct Frame {
    anchor: u64,
    names: Vec<String>,
    literal_object_roots: Vec<String>,
    previous_mode: Mode,
    previous_strict: bool,
}

#[derive(Clone)]
struct Scan<'s> {
    frames: Vec<Frame>,
    mode: Mode,
    strict: bool,
    admission: &'s TypeScriptAdmissionContext,
    active_writes: HashSet<Binding>,
    deferred_writes: HashSet<Binding>,
    all_writes: HashSet<Binding>,
    synchronous_callbacks: HashSet<u32>,
    calls: HashSet<u32>,
    statement_call: Option<u32>,
    record_calls: bool,
    collecting_region: bool,
}

impl Scan<'_> {
    fn binding(&self, name: &str) -> Binding {
        Binding {
            frame: self
                .frames
                .iter()
                .rev()
                .find(|frame| frame.names.iter().any(|bound| bound == name))
                .map(|frame| frame.anchor),
            name: name.to_string(),
        }
    }

    fn push(
        &mut self,
        anchor: u64,
        names: Vec<String>,
        mode: Mode,
        strict: bool,
        literal_object_roots: Vec<String>,
    ) {
        self.frames.push(Frame {
            anchor,
            names,
            literal_object_roots,
            previous_mode: self.mode,
            previous_strict: self.strict,
        });
        self.mode = mode;
        self.strict = strict;
    }

    fn write(&mut self, binding: Binding) {
        self.all_writes.insert(binding.clone());
        match self.mode {
            Mode::Registration => {
                self.active_writes.insert(binding);
            }
            Mode::Deferred => {
                self.deferred_writes.insert(binding);
            }
        }
    }

    fn identifier(&mut self, name: &str) {
        if matches!(name, "before" | "beforeEach" | "beforeAll") {
            self.write(self.binding(name));
        }
    }

    fn member(&mut self, object: &Expression<'_>, property: Option<&str>) {
        let global = match runtime_expression(object) {
            Expression::Identifier(identifier) => {
                matches!(
                    identifier.name.as_str(),
                    "globalThis" | "global" | "window" | "self"
                ) && !self
                    .frames
                    .iter()
                    .rev()
                    .find(|frame| {
                        frame
                            .names
                            .iter()
                            .any(|name| name == identifier.name.as_str())
                    })
                    .is_some_and(|frame| {
                        frame
                            .literal_object_roots
                            .iter()
                            .any(|name| name == identifier.name.as_str())
                    })
            }
            Expression::ThisExpression(_) => true,
            _ => false,
        };
        if global {
            for hook in ["before", "beforeEach", "beforeAll"] {
                if property.is_none_or(|property| property == hook) {
                    // A local root may still alias the real global object.
                    // Its possible property write affects ambient hooks, not
                    // a distinct local/imported hook identifier.
                    self.write(Binding {
                        frame: None,
                        name: hook.to_string(),
                    });
                }
            }
        }
    }

    fn expression(&mut self, expression: &Expression<'_>) {
        match runtime_expression(expression) {
            Expression::Identifier(identifier) => self.identifier(identifier.name.as_str()),
            Expression::StaticMemberExpression(member) => {
                self.member(&member.object, Some(member.property.name.as_str()));
            }
            Expression::ComputedMemberExpression(member) => {
                self.member(&member.object, literal_key(&member.expression));
            }
            _ => {}
        }
    }

    fn function_frame(
        &mut self,
        span: u32,
        params: &FormalParameters<'_>,
        name: Option<&str>,
        strict: bool,
    ) {
        let mut names: Vec<String> = params
            .items
            .iter()
            .flat_map(|param| param.pattern.get_binding_identifiers())
            .chain(
                params
                    .rest
                    .iter()
                    .flat_map(|rest| rest.rest.argument.get_binding_identifiers()),
            )
            .map(|identifier| identifier.name.to_string())
            .collect();
        names.extend(name.map(str::to_string));
        let mode = if self.synchronous_callbacks.contains(&span) {
            self.mode
        } else {
            Mode::Deferred
        };
        self.push((u64::from(span) << 3) | 1, names, mode, strict, Vec::new());
    }
}

/// Only an immutable empty literal proves a disjoint root here. A local name,
/// parameter, mutable initializer or other value is not a non-alias proof.
/// Nonempty literals can define forwarding setters; no value evaluator is added.
fn literal_object_roots(body: &[Statement<'_>]) -> Vec<String> {
    body.iter()
        .filter_map(|statement| match statement {
            Statement::VariableDeclaration(declaration) => Some(declaration.as_ref()),
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(oxc_ast::ast::Declaration::VariableDeclaration(declaration)) => {
                    Some(declaration.as_ref())
                }
                _ => None,
            },
            _ => None,
        })
        .filter(|declaration| {
            declaration.kind == oxc_ast::ast::VariableDeclarationKind::Const && !declaration.declare
        })
        .flat_map(|declaration| declaration.declarations.iter())
        .filter_map(|declaration| {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(identifier) = &declaration.id
            else {
                return None;
            };
            matches!(declaration.init.as_ref().map(runtime_expression),
                Some(Expression::ObjectExpression(object)) if object.properties.is_empty())
            .then(|| identifier.name.to_string())
        })
        .collect()
}

fn literal_key<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
    match runtime_expression(expression) {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

fn loop_names(declaration: &oxc_ast::ast::VariableDeclaration<'_>) -> Vec<String> {
    if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var || declaration.declare {
        Vec::new()
    } else {
        declaration_binding_names(declaration)
    }
}

/// A direct runner call does not prove single invocation when its callback
/// refers to itself. Keep that body under the existing unknown-order fence.
fn callback_can_reenter(function: &oxc_ast::ast::Function<'_>) -> bool {
    struct References<'s> {
        name: Option<&'s str>,
        found: bool,
    }
    impl<'a> Visit<'a> for References<'_> {
        fn visit_identifier_reference(
            &mut self,
            identifier: &oxc_ast::ast::IdentifierReference<'a>,
        ) {
            self.found |= self.name == Some(identifier.name.as_str());
        }
        fn visit_static_member_expression(
            &mut self,
            member: &oxc_ast::ast::StaticMemberExpression<'a>,
        ) {
            self.found |= matches!(runtime_expression(&member.object), Expression::Identifier(id) if id.name == "arguments")
                && member.property.name == "callee";
            walk::walk_static_member_expression(self, member);
        }
        fn visit_computed_member_expression(
            &mut self,
            member: &oxc_ast::ast::ComputedMemberExpression<'a>,
        ) {
            self.found |= matches!(runtime_expression(&member.object), Expression::Identifier(id) if id.name == "arguments")
                && literal_key(&member.expression).is_none_or(|key| key == "callee");
            walk::walk_computed_member_expression(self, member);
        }
    }
    let mut references = References {
        name: function.id.as_ref().map(|id| id.name.as_str()),
        found: false,
    };
    if let Some(body) = &function.body {
        references.visit_function_body(body);
    }
    references.found
}

impl<'a> Visit<'a> for Scan<'_> {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::Function(function) => {
                self.function_frame(
                    function.span.start,
                    &function.params,
                    function.id.as_ref().map(|id| id.name.as_str()),
                    self.strict || function.has_use_strict_directive(),
                );
            }
            AstKind::ArrowFunctionExpression(arrow) => self.function_frame(
                arrow.span.start,
                &arrow.params,
                None,
                self.strict || arrow.has_use_strict_directive(),
            ),
            AstKind::FunctionBody(body) => {
                // Oxc visits parameter defaults before entering the body.
                // Body declarations and root proofs cannot shield writes
                // made in that earlier parameter/outer environment.
                let mut names = Vec::new();
                collect_registration_scope_names(&body.statements, &mut names, self.strict);
                if let Some(frame) = self.frames.last_mut() {
                    frame.names.extend(names);
                    frame.literal_object_roots = literal_object_roots(&body.statements);
                }
            }
            AstKind::BlockStatement(block) => {
                let mut names = Vec::new();
                collect_registration_scope_names(&block.body, &mut names, self.strict);
                self.push(
                    (u64::from(block.span.start) << 3) | 2,
                    names,
                    self.mode,
                    self.strict,
                    literal_object_roots(&block.body),
                );
            }
            AstKind::ForStatement(loop_) => {
                let names = match &loop_.init {
                    Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) => {
                        loop_names(declaration)
                    }
                    _ => Vec::new(),
                };
                self.push(
                    (u64::from(loop_.span.start) << 3) | 4,
                    names,
                    self.mode,
                    self.strict,
                    Vec::new(),
                );
            }
            AstKind::ForOfStatement(loop_) => {
                let names = match &loop_.left {
                    oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) => {
                        loop_names(declaration)
                    }
                    _ => Vec::new(),
                };
                self.push(
                    (u64::from(loop_.span.start) << 3) | 4,
                    names,
                    self.mode,
                    self.strict,
                    Vec::new(),
                );
            }
            AstKind::ForInStatement(loop_) => {
                let names = match &loop_.left {
                    oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) => {
                        loop_names(declaration)
                    }
                    _ => Vec::new(),
                };
                self.push(
                    (u64::from(loop_.span.start) << 3) | 4,
                    names,
                    self.mode,
                    self.strict,
                    Vec::new(),
                );
            }
            AstKind::CatchClause(catch) => {
                let names = catch
                    .param
                    .iter()
                    .flat_map(|param| param.pattern.get_binding_identifiers())
                    .map(|id| id.name.to_string())
                    .collect();
                self.push(
                    (u64::from(catch.span.start) << 3) | 3,
                    names,
                    self.mode,
                    self.strict,
                    Vec::new(),
                );
            }
            _ => {}
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        if matches!(
            kind,
            AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::BlockStatement(_)
                | AstKind::CatchClause(_)
                | AstKind::ForStatement(_)
                | AstKind::ForOfStatement(_)
                | AstKind::ForInStatement(_)
        ) && let Some(frame) = self.frames.pop()
        {
            self.mode = frame.previous_mode;
            self.strict = frame.previous_strict;
        }
    }

    fn visit_statement(&mut self, statement: &Statement<'a>) {
        let previous = self.statement_call;
        self.statement_call = statement_call(statement).map(|call| call.span.start);
        if self.record_calls
            && !self.collecting_region
            && matches!(
                statement,
                Statement::IfStatement(_)
                    | Statement::ForStatement(_)
                    | Statement::ForOfStatement(_)
                    | Statement::ForInStatement(_)
                    | Statement::WhileStatement(_)
                    | Statement::DoWhileStatement(_)
                    | Statement::SwitchStatement(_)
                    | Statement::TryStatement(_)
            )
        {
            // A later write in a repeated/uncertain body may precede another
            // registration in that body. Discover affected bindings first.
            let mut possible = self.clone();
            possible.collecting_region = true;
            walk::walk_statement(&mut possible, statement);
            self.active_writes.extend(possible.active_writes);
        }
        walk::walk_statement(self, statement);
        self.statement_call = previous;
    }

    fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        let mut captured = None;
        if let Expression::Identifier(callee) = &call.callee
            && matches!(callee.name.as_str(), "before" | "beforeEach" | "beforeAll")
        {
            let binding = self.binding(callee.name.as_str());
            let known = match self.mode {
                Mode::Registration => {
                    !self.active_writes.contains(&binding)
                        && !self.deferred_writes.contains(&binding)
                }
                Mode::Deferred => !self.all_writes.contains(&binding),
            };
            // Capture before evaluating arguments: later writes cannot revoke
            // the callback already passed to this callable.
            if known && self.record_calls && !self.collecting_region {
                self.calls.insert(call.span.start);
                captured = Some(binding);
            }
        }
        if self.statement_call == Some(call.span.start)
            && call_callee_is_active_declaration(call, TestDeclarationRoot::Describe)
            && !call_callee_is_active_each_declaration(call, TestDeclarationRoot::Describe)
            && declaration_options_are_active_with_undefined(
                call,
                self.binding("undefined").frame.is_none(),
            )
            && self.admission.registration_is_known(
                call,
                true,
                self.frames.iter().enumerate().flat_map(|(depth, frame)| {
                    frame
                        .names
                        .iter()
                        .map(move |name| (name.clone(), depth == 0))
                }),
            )
            && let Some(argument) = call.arguments.get(declaration_callback_index(call))
        {
            let callback = match argument {
                oxc_ast::ast::Argument::ArrowFunctionExpression(arrow)
                    if !arrow.r#async && !arrow.expression =>
                {
                    Some(arrow.span.start)
                }
                oxc_ast::ast::Argument::FunctionExpression(function)
                    if !function.r#async
                        && !function.generator
                        && !callback_can_reenter(function) =>
                {
                    Some(function.span.start)
                }
                _ => None,
            };
            if let Some(callback) = callback {
                self.synchronous_callbacks.insert(callback);
            }
        }
        walk::walk_call_expression(self, call);
        // Inline writes during argument evaluation are outside the simple
        // registration boundary: runner overload/callback selection is not
        // interpreted here. Retain that existing unknown case.
        if let Some(binding) = captured
            && self.active_writes.contains(&binding)
        {
            self.calls.remove(&call.span.start);
        }
    }

    fn visit_simple_assignment_target(&mut self, target: &SimpleAssignmentTarget<'a>) {
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                self.identifier(identifier.name.as_str())
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                self.member(&member.object, Some(member.property.name.as_str()))
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                self.member(&member.object, literal_key(&member.expression))
            }
            SimpleAssignmentTarget::TSAsExpression(inner) => self.expression(&inner.expression),
            SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
                self.expression(&inner.expression)
            }
            SimpleAssignmentTarget::TSNonNullExpression(inner) => {
                self.expression(&inner.expression)
            }
            SimpleAssignmentTarget::TSTypeAssertion(inner) => self.expression(&inner.expression),
            _ => {}
        }
        walk::walk_simple_assignment_target(self, target);
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        property: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.identifier(property.binding.name.as_str());
        walk::walk_assignment_target_property_identifier(self, property);
    }
}
