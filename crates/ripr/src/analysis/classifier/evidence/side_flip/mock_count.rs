//! RIPR-SPEC-0295: a mockall call-count expectation observes a guard's
//! side flip.
//!
//! ```text
//! pub fn remind(mailer: &dyn Mailer, user: &str, overdue_days: u32) -> bool {
//!     if overdue_days > GRACE_DAYS {
//!         let body = format!("..");
//!         return mailer.send(user, &body);
//!     }
//!     false
//! }
//! #[test]
//! fn reminds_only_after_the_grace_period() {
//!     let mut mailer = MockMailer::new();
//!     mailer.expect_send().with(eq("ann"), always()).times(1).return_const(true);
//!     assert!(remind(&mailer, "ann", 31));
//!     assert!(!remind(&mailer, "ann", 30));
//! }
//! ```
//!
//! `assert!(!remind(.., 30))` alone cannot see `>` become `>=`: the taken
//! branch returns whatever the mock returns. The mock's `times(1)` can.
//! The guarded block calls `send` once each time it runs, and nothing else
//! in the owner calls it, so the number of `send` calls in the test is the
//! number of owner calls the guard takes. The test takes it once (31) and
//! skips it once (30), and those inputs sit on either side of the edge.
//! Moving the edge in either direction flips exactly the inputs it crosses,
//! all the same way, so the count leaves 1. mockall panics on a call past
//! `times(N)` and, when the mock drops, on fewer than `N` calls.
//!
//! The producer reads only shapes where that count is plain:
//!
//! 1. The owner's first statement is `if <param> <op> <bound> { .. }` with
//!    no `else`, over a built-in integer parameter and an integer literal or
//!    a file-level `const` of an integer type set to one.
//! 2. The owner names one `&dyn T` / `&impl T` parameter exactly once, as
//!    the receiver of one method call inside the guarded block. Nothing
//!    before that call in the block can leave, branch, loop or hide code
//!    (a `format!` whose tokens cannot leave is fine), so the call runs once
//!    per entry. `T` is declared once in the owner's file under an
//!    `automock` attribute and declares that method.
//! 3. One plain `#[test]` builds `let mut m = MockT::new()` (or
//!    `::default()`), sets exactly one `m.expect_<method>()` chain with one
//!    literal `times(N)`, `once()` or `never()` and only matcher and return
//!    configuration besides, and otherwise names `m` only as `&m` in the
//!    owner-call slot of that parameter. Every owner call runs exactly once:
//!    a top-level statement, a `let` initializer, or the condition of an
//!    assertion macro, never a message argument, a branch or a
//!    short-circuit operand. Each passes an integer literal for the compared
//!    parameter.
//! 4. The count the guard predicts equals `N` (the test passes on the
//!    current code), and one taken input and one skipped input are adjacent
//!    integers.
//!
//! The old line must be an ordering comparison, as RIPR-SPEC-0227 rule 3
//! requires: an old `==` agrees with the new guard at the edge pair while
//! differing elsewhere. Any shape outside these rules leaves the evidence
//! unchanged.

use super::guard::{INTEGER_TYPES, flipped, integer_literal, is_equality, line_of, parameter_name};
use super::*;

pub(super) const MOCK_COUNT_SIDE_FLIP: &str = "A mock call-count expectation observes the guard's side flip: one test calls the owner on both sides of this guard's edge, and a mockall `times(N)` on the method only the guarded block calls fails when the edge moves (RIPR-SPEC-0295)";

/// mockall expectation-chain methods that configure matching or returned
/// values and leave the call count alone. `times`, `once` and `never` are
/// read separately; anything else (`in_sequence`, `withf` and the other
/// closure forms, a range `times`) refuses.
const CONFIGURATION_METHODS: &[&str] = &[
    "with",
    "return_const",
    "return_const_st",
    "return_once",
    "return_once_st",
];

/// Names whose presence in a test may hide a missed or extra call from
/// mockall's own count check.
const COUNT_DEFEATING_TOKENS: &[&str] = &[
    "checkpoint",
    "forget",
    "leak",
    "catch_unwind",
    "ManuallyDrop",
    "exit",
    "abort",
];

pub(in crate::analysis::classifier::evidence) fn discrimination(
    context: &ProbeContext<'_>,
    observe: &StageEvidence,
    current: &StageEvidence,
) -> Option<StageEvidence> {
    if context.probe.family != ProbeFamily::Predicate
        || observe.state != StageState::Yes
        || current.state != StageState::Weak
        || !context.workspace_complete
    {
        return None;
    }
    let (owner_name, root, source) = owner_file(context)?;
    let function = unique_plain_free_function(root.syntax(), &owner_name)?;
    let gate = CountGate::read(&function, root.syntax(), &source)?;
    if !gate.is_probe(context) || !old_condition_orders(context.probe.before.as_deref()?) {
        return None;
    }
    plain_related_tests(context, &owner_name, false)
        .any(|test| gate.counted_on_both_sides(&test, &owner_name))
        .then(|| StageEvidence::new(StageState::Yes, Confidence::High, MOCK_COUNT_SIDE_FLIP))
}

/// The only function of this name in the file, outside any `impl`, `trait`
/// or function, with no attributes and no `async` or `const`.
fn unique_plain_free_function(root: &SyntaxNode, name: &str) -> Option<ast::Fn> {
    let function = unique_function(root, name)?;
    let nested_in_item = function.syntax().ancestors().skip(1).any(|node| {
        ast::Impl::can_cast(node.kind())
            || ast::Trait::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
    });
    (!nested_in_item
        && function.attrs().next().is_none()
        && function.async_token().is_none()
        && function.const_token().is_none()
        && function.generic_param_list().is_none())
    .then_some(function)
}

/// The old text is an ordering comparison: the old condition alone, or the
/// old guard's opening line `if <condition> {`.
fn old_condition_orders(before: &str) -> bool {
    let text = before.trim();
    let text = text.strip_prefix("if ").unwrap_or(text);
    let text = text.strip_suffix('{').unwrap_or(text).trim();
    let Some(old) = parse_expression(text) else {
        return false;
    };
    let old = unwrap_parens(old);
    matches!(&old, ast::Expr::BinExpr(comparison)
        if matches!(comparison.op_kind(), Some(ast::BinaryOp::CmpOp(ast::CmpOp::Ord { .. }))))
        && !is_equality(&old)
        && old.syntax().descendants().all(|node| {
            !ast::MacroCall::can_cast(node.kind()) && !ast::BlockExpr::can_cast(node.kind())
        })
}

/// Rules 1 and 2: the owner's first-statement guard and the one mocked call
/// it gates.
struct CountGate {
    /// Position of the compared integer parameter, and of the mocked
    /// receiver parameter.
    parameter: usize,
    receiver: usize,
    parameter_count: usize,
    /// The comparison, normalized to `parameter <op> bound`.
    op: ast::CmpOp,
    bound: i128,
    /// `Mock<Trait>` and `expect_<method>`.
    mock_type: String,
    expectation: String,
    condition: String,
    condition_line: usize,
}

impl CountGate {
    fn read(function: &ast::Fn, root: &SyntaxNode, source: &str) -> Option<Self> {
        let parameter_list = function.param_list()?;
        if parameter_list.self_param().is_some() {
            return None;
        }
        let parameters = parameter_list.params().collect::<Vec<_>>();
        let body = function.body()?.stmt_list()?;
        let ast::Stmt::ExprStmt(first) = body.statements().next()? else {
            return None;
        };
        let ast::Expr::IfExpr(guard) = first.expr()? else {
            return None;
        };
        if guard.else_branch().is_some() {
            return None;
        }
        let condition = unwrap_parens(guard.condition()?);
        let ast::Expr::BinExpr(comparison) = &condition else {
            return None;
        };
        let Some(ast::BinaryOp::CmpOp(op @ ast::CmpOp::Ord { .. })) = comparison.op_kind() else {
            return None;
        };
        let (lhs, rhs) = (
            unwrap_parens(comparison.lhs()?),
            unwrap_parens(comparison.rhs()?),
        );
        let (name, bound, op) = match (parameter_name(&lhs), bound_value(&rhs, root, function)) {
            (Some(name), Some(bound)) if parameter_named(&parameters, &name).is_some() => {
                (name, bound, op)
            }
            _ => (
                parameter_name(&rhs)?,
                bound_value(&lhs, root, function)?,
                flipped(op),
            ),
        };
        let parameter = parameters.iter().position(|parameter| {
            parameter_named(std::slice::from_ref(parameter), &name).is_some()
                && parameter.ty().is_some_and(|ty| {
                    INTEGER_TYPES.contains(&ty.syntax().text().to_string().as_str())
                })
        })?;
        let (receiver, trait_name) = parameters
            .iter()
            .enumerate()
            .find_map(|(index, parameter)| Some((index, mocked_trait(parameter)?)))?;
        let receiver_name = match parameters.get(receiver)?.pat()? {
            ast::Pat::IdentPat(pat) if pat.ref_token().is_none() && pat.mut_token().is_none() => {
                pat.name()?.text().to_string()
            }
            _ => return None,
        };
        let method = sole_gated_call(function, &guard, &receiver_name)?;
        if !trait_is_automocked_with(root, &trait_name, &method) {
            return None;
        }
        Some(Self {
            parameter,
            receiver,
            parameter_count: parameters.len(),
            op,
            bound,
            mock_type: format!("Mock{trait_name}"),
            expectation: format!("expect_{method}"),
            condition: canonical_tokens(condition.syntax(), None),
            condition_line: line_of(source, condition.syntax())?,
        })
    }

    fn is_probe(&self, context: &ProbeContext<'_>) -> bool {
        let probe = context.probe;
        parse_expression(&probe.expression).is_some_and(|expression| {
            probe.location.line == self.condition_line
                && canonical_tokens(expression.syntax(), None) == self.condition
        })
    }

    fn takes(&self, value: i128) -> bool {
        let ast::CmpOp::Ord { ordering, strict } = self.op else {
            return false;
        };
        match (ordering, strict) {
            (ast::Ordering::Less, true) => value < self.bound,
            (ast::Ordering::Less, false) => value <= self.bound,
            (ast::Ordering::Greater, true) => value > self.bound,
            (ast::Ordering::Greater, false) => value >= self.bound,
        }
    }

    /// Rules 3 and 4 for one test.
    fn counted_on_both_sides(&self, test: &ast::Fn, owner: &str) -> bool {
        self.count_reading(test, owner)
            .is_some_and(|(expected, inputs)| {
                let taken = inputs.iter().filter(|value| self.takes(**value));
                let skipped = inputs.iter().filter(|value| !self.takes(**value));
                taken.clone().count() == expected
                    && taken
                        .clone()
                        .any(|a| skipped.clone().any(|b| a.abs_diff(*b) == 1))
            })
    }

    /// The expected call count and the compared input of every owner call,
    /// when the test meets rule 3.
    fn count_reading(&self, test: &ast::Fn, owner: &str) -> Option<(usize, Vec<i128>)> {
        let statements = plain_test_statements(test, owner)?;
        let body = test.body()?;
        if body.syntax().descendants().any(|node| {
            ast::IfExpr::can_cast(node.kind())
                || ast::MatchExpr::can_cast(node.kind())
                || ast::ForExpr::can_cast(node.kind())
                || ast::WhileExpr::can_cast(node.kind())
                || ast::LoopExpr::can_cast(node.kind())
                || ast::TryExpr::can_cast(node.kind())
                || ast::BreakExpr::can_cast(node.kind())
                || ast::ContinueExpr::can_cast(node.kind())
                // An `async` block that is never awaited, or a labeled
                // block left early, holds calls that never run.
                || ast::BlockExpr::cast(node.clone()).is_some_and(|block| {
                    block.async_token().is_some() || block.label().is_some()
                })
                || is_short_circuit(&node)
        }) || body
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| COUNT_DEFEATING_TOKENS.contains(&identifier_text(&token).as_str()))
        {
            return None;
        }
        let mock = self.mock_binding(&body)?;
        let mut expected = None;
        let mut inputs = Vec::new();
        let mut mock_uses = 0usize;
        let lets = body
            .stmt_list()?
            .statements()
            .filter_map(|statement| match statement {
                ast::Stmt::LetStmt(statement) => statement.initializer(),
                _ => None,
            })
            .collect::<Vec<_>>();
        // A `let` that binds an expectation chain hands it a second name the
        // naming count cannot follow (`let e = m.expect_m().times(1);
        // e.times(1..3);`).
        if lets
            .iter()
            .any(|initializer| self.expectation_count(initializer, &mock).is_some())
        {
            return None;
        }
        for expression in statements.into_iter().chain(lets) {
            match self.expectation_count(&expression, &mock) {
                Some(Expectation::Counted(count)) => {
                    if expected.replace(count).is_some() {
                        return None;
                    }
                    mock_uses += 1;
                    continue;
                }
                Some(Expectation::OtherMethod) => {
                    mock_uses += 1;
                    continue;
                }
                Some(Expectation::Uncounted) => return None,
                None => {}
            }
            for call in self.executed_owner_calls(&expression, owner)? {
                inputs.push(self.compared_input(&call, &mock)?);
                mock_uses += 1;
            }
        }
        // Every naming of the mock, in the body and inside macro tokens, is
        // its `let`, the one expectation or an owner-call argument.
        let named = body
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT && identifier_text(token) == mock)
            .count();
        (named == mock_uses + 1).then_some((expected?, inputs))
    }

    /// `let mut m = MockT::new();` (or `::default()`), the test's only
    /// binding of that name.
    fn mock_binding(&self, body: &ast::BlockExpr) -> Option<String> {
        let mut found = None;
        for statement in body.stmt_list()?.statements() {
            let ast::Stmt::LetStmt(statement) = statement else {
                continue;
            };
            let Some(ast::Expr::CallExpr(call)) = statement.initializer() else {
                continue;
            };
            let callee = call.expr()?.syntax().text().to_string();
            if callee != format!("{}::new", self.mock_type)
                && callee != format!("{}::default", self.mock_type)
            {
                continue;
            }
            if call.arg_list()?.args().next().is_some() || statement.ty().is_some() {
                return None;
            }
            let ast::Pat::IdentPat(pat) = statement.pat()? else {
                return None;
            };
            if found.replace(pat.name()?.text().to_string()).is_some() {
                return None;
            }
        }
        let mock = found?;
        let bindings = body
            .syntax()
            .descendants()
            .filter_map(ast::Name::cast)
            .filter(|name| name.text() == mock)
            .count();
        (bindings == 1).then_some(mock)
    }

    /// How an expression statement rooted at the mock sets an expectation;
    /// `None` when it is not an `expect_*` chain on the mock.
    fn expectation_count(&self, expression: &ast::Expr, mock: &str) -> Option<Expectation> {
        let mut methods = Vec::new();
        let mut current = expression.clone();
        while let ast::Expr::MethodCallExpr(call) = current {
            methods.push(call.clone());
            current = call.receiver()?;
        }
        let ast::Expr::PathExpr(root) = current else {
            return None;
        };
        if root.syntax().text() != mock {
            return None;
        }
        methods.reverse();
        let (first, rest) = methods.split_first()?;
        let name = first.name_ref()?.text().to_string();
        if !name.starts_with("expect_") {
            // Any other use of the mock stays a stray naming.
            return None;
        }
        if name != self.expectation {
            return Some(Expectation::OtherMethod);
        }
        if first.arg_list()?.args().next().is_some() || first.generic_arg_list().is_some() {
            return Some(Expectation::Uncounted);
        }
        let mut count = None;
        for method in rest {
            let name = method.name_ref()?.text().to_string();
            let arguments = method.arg_list()?.args().collect::<Vec<_>>();
            let this = match (name.as_str(), arguments.as_slice()) {
                ("times", [argument]) => integer_literal(&unwrap_parens(argument.clone()))
                    .and_then(|n| usize::try_from(n).ok()),
                ("once", []) => Some(1),
                ("never", []) => Some(0),
                (other, _) if CONFIGURATION_METHODS.contains(&other) => continue,
                _ => return Some(Expectation::Uncounted),
            };
            let Some(this) = this else {
                return Some(Expectation::Uncounted);
            };
            if count.replace(this).is_some() {
                return Some(Expectation::Uncounted);
            }
        }
        Some(count.map_or(Expectation::Uncounted, Expectation::Counted))
    }

    /// The owner calls the expression runs exactly once each, or `None`
    /// when it holds an owner call that may run zero or several times.
    fn executed_owner_calls(
        &self,
        expression: &ast::Expr,
        owner: &str,
    ) -> Option<Vec<ast::CallExpr>> {
        if let ast::Expr::MacroExpr(macro_expression) = expression {
            let call = macro_expression.macro_call()?;
            let name = call.path()?.syntax().text().to_string();
            let arguments = macro_arguments(expression, &name)?;
            let checked = match name.as_str() {
                "assert" => 1,
                "assert_eq" | "assert_ne" => 2,
                _ => 0,
            };
            let mut calls = Vec::new();
            for (index, argument) in arguments.iter().enumerate() {
                let parsed = parse_expression(argument)?;
                let found = owner_calls_in(&parsed, owner);
                if index >= checked && !found.is_empty() {
                    // A message argument runs only when the check fails.
                    return None;
                }
                if parsed
                    .syntax()
                    .descendants()
                    .any(|node| is_short_circuit(&node) || ast::IfExpr::can_cast(node.kind()))
                {
                    return None;
                }
                calls.extend(found);
            }
            return Some(calls);
        }
        Some(owner_calls_in(expression, owner))
    }

    /// The compared parameter's input, when the call passes `&m` for the
    /// mocked parameter, an integer literal for the compared one, and names the mock nowhere else.
    fn compared_input(&self, call: &ast::CallExpr, mock: &str) -> Option<i128> {
        let arguments = call.arg_list()?.args().collect::<Vec<_>>();
        if arguments.len() != self.parameter_count {
            return None;
        }
        let ast::Expr::RefExpr(reference) = unwrap_parens(arguments.get(self.receiver)?.clone())
        else {
            return None;
        };
        if reference.mut_token().is_some() || reference.expr()?.syntax().text() != mock {
            return None;
        }
        let compared = unwrap_parens(arguments.get(self.parameter)?.clone());
        // Only a literal: the bound's `const` name may be rebound where the
        // test reads it (a glob import, a macro-defined item), which the
        // owner-side binding count cannot see.
        integer_literal(&compared)
    }
}

enum Expectation {
    /// The one chain on the gated method, with its literal count.
    Counted(usize),
    /// A chain on the gated method this rule cannot count: no count, a
    /// second count, a range, `in_sequence` or a closure form.
    Uncounted,
    /// A chain on another method, which leaves the gated count alone.
    OtherMethod,
}

/// An identifier's name without a raw `r#` prefix: `r#mailer` names
/// `mailer`.
fn identifier_text(token: &ra_ap_syntax::SyntaxToken) -> String {
    let text = token.text();
    text.strip_prefix("r#").unwrap_or(text).to_string()
}

/// Owner calls inside an expression, by the owner's bare name.
fn owner_calls_in(expression: &ast::Expr, owner: &str) -> Vec<ast::CallExpr> {
    // `descendants` starts with the node itself.
    expression
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| {
            call.expr()
                .is_some_and(|callee| callee.syntax().text() == owner)
        })
        .collect()
}

fn is_short_circuit(node: &SyntaxNode) -> bool {
    ast::BinExpr::cast(node.clone())
        .is_some_and(|expression| matches!(expression.op_kind(), Some(ast::BinaryOp::LogicOp(_))))
}

fn parameter_named(parameters: &[ast::Param], name: &str) -> Option<usize> {
    parameters.iter().position(|parameter| {
        parameter.pat().is_some_and(|pat| match pat {
            ast::Pat::IdentPat(pat) => {
                pat.ref_token().is_none() && pat.name().is_some_and(|n| n.text() == name)
            }
            _ => false,
        })
    })
}

/// An integer literal, or a file-level `const NAME: <integer> = <literal>;`
/// that is the file's only binding of `NAME` and that the owner does not
/// shadow.
fn bound_value(expression: &ast::Expr, root: &SyntaxNode, function: &ast::Fn) -> Option<i128> {
    if let Some(literal) = integer_literal(expression) {
        return Some(literal);
    }
    let name = parameter_name(expression)?;
    let constant = root
        .children()
        .filter_map(ast::Const::cast)
        .find(|constant| constant.name().is_some_and(|n| n.text() == name))?;
    let declared = constant.ty()?.syntax().text().to_string();
    if !INTEGER_TYPES.contains(&declared.as_str()) {
        return None;
    }
    let value = integer_literal(&unwrap_parens(constant.body()?))?;
    let bindings = root
        .descendants()
        .filter(|node| super::binds_name(node, &name))
        .count();
    if bindings != 1
        || function
            .syntax()
            .descendants()
            .any(|node| super::binds_name(&node, &name))
    {
        return None;
    }
    Some(value)
}

/// `&dyn T`, `&impl T`: the trait's single-segment name.
fn mocked_trait(parameter: &ast::Param) -> Option<String> {
    let ast::Type::RefType(reference) = parameter.ty()? else {
        return None;
    };
    if reference.mut_token().is_some() || reference.lifetime().is_some() {
        return None;
    }
    let bounds = match reference.ty()? {
        ast::Type::DynTraitType(dynamic) => dynamic.type_bound_list()?,
        ast::Type::ImplTraitType(implemented) => implemented.type_bound_list()?,
        _ => return None,
    };
    let bounds = bounds.bounds().collect::<Vec<_>>();
    let [bound] = bounds.as_slice() else {
        return None;
    };
    let ast::Type::PathType(path) = bound.ty()? else {
        return None;
    };
    let path = path.path()?;
    let segment = path.segment()?;
    (path.qualifier().is_none() && segment.generic_arg_list().is_none())
        .then(|| segment.name_ref().map(|name| name.text().to_string()))
        .flatten()
}

/// Rule 2: the receiver parameter is named once in the owner, as the
/// receiver of one method call inside the guard's block, and that call
/// runs exactly once whenever the block does.
fn sole_gated_call(function: &ast::Fn, guard: &ast::IfExpr, receiver: &str) -> Option<String> {
    let body = function.body()?;
    let mentions = body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && identifier_text(token) == receiver)
        .count();
    if mentions != 1 {
        return None;
    }
    let block = guard.then_branch()?;
    let call = block
        .syntax()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
        .find(|call| {
            call.receiver()
                .is_some_and(|object| object.syntax().text() == receiver)
        })?;
    let statements = block.stmt_list()?;
    let mut before_call = Vec::new();
    let mut holder = None;
    for statement in statements.statements() {
        if statement
            .syntax()
            .text_range()
            .contains_range(call.syntax().text_range())
        {
            holder = Some(statement.syntax().clone());
            break;
        }
        before_call.push(statement.syntax().clone());
    }
    let holder = match holder {
        Some(holder) => holder,
        None => {
            let tail = statements.tail_expr()?;
            if !tail
                .syntax()
                .text_range()
                .contains_range(call.syntax().text_range())
            {
                return None;
            }
            tail.syntax().clone()
        }
    };
    // The call is the whole statement's work: `r.m(..);`, `return r.m(..);`,
    // `let x = r.m(..);` or the block's tail.
    let reached = *call.syntax() == holder
        || call
            .syntax()
            .ancestors()
            .skip(1)
            .take_while(|node| *node != holder)
            .all(|node| {
                ast::ReturnExpr::can_cast(node.kind()) || ast::ExprStmt::can_cast(node.kind())
            });
    if !reached
        || call.generic_arg_list().is_some()
        || call
            .arg_list()?
            .args()
            .any(|argument| !runs_plainly(argument.syntax()))
        || !before_call.iter().all(runs_plainly)
    {
        return None;
    }
    Some(call.name_ref()?.text().to_string())
}

/// Nothing in the node can leave the block, branch, loop or hide code. A
/// `format!` whose tokens cannot leave only builds a string.
fn runs_plainly(node: &SyntaxNode) -> bool {
    node.descendants().all(|node| {
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            return call
                .path()
                .is_some_and(|path| path.syntax().text() == "format")
                && call
                    .token_tree()
                    .is_some_and(|tree| !may_leave(tree.syntax()));
        }
        !ast::ReturnExpr::can_cast(node.kind())
            && !ast::TryExpr::can_cast(node.kind())
            && !ast::BreakExpr::can_cast(node.kind())
            && !ast::ContinueExpr::can_cast(node.kind())
            && !ast::ClosureExpr::can_cast(node.kind())
            && !ast::IfExpr::can_cast(node.kind())
            && !ast::MatchExpr::can_cast(node.kind())
            && !ast::ForExpr::can_cast(node.kind())
            && !ast::WhileExpr::can_cast(node.kind())
            && !ast::LoopExpr::can_cast(node.kind())
            && !ast::AwaitExpr::can_cast(node.kind())
            && !is_short_circuit(&node)
            && (!ast::Item::can_cast(node.kind()) || ast::MacroCall::can_cast(node.kind()))
            && !ast::BlockExpr::cast(node).is_some_and(|block| {
                block.async_token().is_some()
                    || block.label().is_some()
                    || block.unsafe_token().is_some()
            })
    })
}

/// The trait is declared once in the owner's file, under an attribute that
/// names `automock`, without generics, and declares the method.
fn trait_is_automocked_with(root: &SyntaxNode, trait_name: &str, method: &str) -> bool {
    let mut traits = root
        .descendants()
        .filter_map(ast::Trait::cast)
        .filter(|item| item.name().is_some_and(|name| name.text() == trait_name));
    let (Some(item), None) = (traits.next(), traits.next()) else {
        return false;
    };
    item.generic_param_list().is_none()
        && item
            .attrs()
            .any(|attribute| attribute.syntax().text().to_string().contains("automock"))
        && item.assoc_item_list().is_some_and(|list| {
            list.assoc_items().any(|assoc| match assoc {
                ast::AssocItem::Fn(function) => function.name().is_some_and(|n| n.text() == method),
                _ => false,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "#[cfg_attr(test, mockall::automock)]\npub trait Mailer {\n    fn send(&self, to: &str, body: &str) -> bool;\n}\n\npub const GRACE_DAYS: u32 = 30;\n\npub fn remind(mailer: &dyn Mailer, user: &str, overdue_days: u32) -> bool {\n    if GRACE_DAYS < overdue_days {\n        let body = format!(\"{user}, your book is {overdue_days} days late\");\n        return mailer.send(user, &body);\n    }\n    false\n}\n";

    const TEST: &str = "    let mut mailer = MockMailer::new();\n    mailer.expect_send().with(eq(\"ann\"), always()).times(1).return_const(true);\n    assert!(remind(&mailer, \"ann\", 31));\n    assert!(!remind(&mailer, \"ann\", 30));";

    fn gate(source: &str) -> Option<CountGate> {
        let root = parsed(source)?;
        let function = unique_plain_free_function(root.syntax(), "remind")?;
        CountGate::read(&function, root.syntax(), source)
    }

    /// Whether the test is counted on both sides. A source or gate that does
    /// not read is an error, so a control cannot pass on a broken premise.
    fn counted(owner: &str, body: &str) -> Result<bool, String> {
        let source = format!("{owner}#[test]\nfn t() {{\n{body}\n}}\n");
        let root = parsed(&source).ok_or("premise: the source parses")?;
        let gate = gate(&source).ok_or("premise: the owner gate reads")?;
        let test = unique_function(root.syntax(), "t").ok_or("premise: one test `t`")?;
        Ok(gate.counted_on_both_sides(&test, "remind"))
    }

    #[test]
    fn the_corpus_shape_reads_and_counts() -> Result<(), String> {
        let read = gate(OWNER).ok_or("premise: the owner gate reads")?;
        assert_eq!(
            (
                read.parameter,
                read.receiver,
                read.bound,
                read.mock_type.as_str(),
                read.expectation.as_str(),
                read.condition_line,
            ),
            (2, 0, 30, "MockMailer", "expect_send", 9)
        );
        assert!(read.takes(31) && !read.takes(30));
        assert!(counted(OWNER, TEST)?);
        Ok(())
    }

    #[test]
    fn other_owner_shapes_the_rule_reads() -> Result<(), String> {
        for (label, owner) in [
            (
                "literal bound",
                OWNER.replace("GRACE_DAYS < overdue_days", "overdue_days > 30"),
            ),
            ("impl trait", OWNER.replace("&dyn Mailer", "&impl Mailer")),
            (
                "plain automock",
                OWNER.replace("#[cfg_attr(test, mockall::automock)]", "#[automock]"),
            ),
            (
                "statement call",
                OWNER.replace(
                    "return mailer.send(user, &body);",
                    "mailer.send(user, &body);\n        return true;",
                ),
            ),
            (
                "tail call",
                OWNER.replace("-> bool;\n}", ";\n}").replace(
                    "        return mailer.send(user, &body);\n",
                    "        mailer.send(user, &body)\n",
                ),
            ),
            (
                "let call",
                OWNER.replace(
                    "return mailer.send(user, &body);",
                    "let sent = mailer.send(user, &body);\n        return sent;",
                ),
            ),
        ] {
            assert!(counted(&owner, TEST)?, "{label}");
        }
        Ok(())
    }

    #[test]
    fn other_test_shapes_the_rule_counts() -> Result<(), String> {
        for body in [
            TEST.replace(".times(1)", ".once()"),
            TEST.replace("MockMailer::new()", "MockMailer::default()"),
            TEST.replace("return_const(true)", "return_once(|| true)").replace("|| true", "true"),
            TEST.replace("return_const(true)", "return_const_st(true)"),
            // Example 6: the count alone, with no assertion.
            "    let mut mailer = MockMailer::new();\n    mailer.expect_send().times(1).return_const(true);\n    remind(&mailer, \"ann\", 31);\n    remind(&mailer, \"ann\", 30);".to_string(),
            TEST.replace(
                "assert!(remind(&mailer, \"ann\", 31));",
                "assert_eq!(remind(&mailer, \"ann\", 31), true);",
            ),
            TEST.replace(
                "assert!(remind(&mailer, \"ann\", 31));",
                "let sent = remind(&mailer, \"ann\", 31);",
            ),
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "remind(&mailer, \"ann\", 30);",
            ),
            // A far input on the taken side counts toward `times`.
            TEST.replace(".times(1)", ".times(2)") + "\n    assert!(remind(&mailer, \"ann\", 90));",
            // A far skipped input adds no call.
            TEST.to_string() + "\n    assert!(!remind(&mailer, \"ann\", 2));",
            // Another method's expectation leaves this count alone.
            TEST.replace(
                "    assert!(remind",
                "    mailer.expect_close().times(3).return_const(());\n    assert!(remind",
            ),
            // A message argument that names neither the owner nor the mock.
            TEST.replace("\"ann\", 30));", "\"ann\", 30), \"day {}\", 30);"),
        ] {
            assert!(counted(OWNER, &body)?, "{body}");
        }
        Ok(())
    }

    #[test]
    fn tests_the_count_cannot_vouch_for_are_not_counted() -> Result<(), String> {
        for body in [
            // One side only, or far from the edge.
            TEST.replace("    assert!(!remind(&mailer, \"ann\", 30));", ""),
            TEST.replace("    assert!(remind(&mailer, \"ann\", 31));", "")
                .replace(".times(1)", ".times(0)"),
            TEST.replace("31", "40").replace("30));", "3));"),
            // A count the current code does not meet: ripr misreads the test.
            TEST.replace(".times(1)", ".times(2)"),
            // Counts mockall reads as a range, or none at all.
            TEST.replace(".times(1)", ".times(1..3)"),
            TEST.replace(".times(1)", ""),
            TEST.replace(".times(1)", ".times(1).times(1)"),
            TEST.replace(".times(1)", ".times(1).in_sequence(&mut seq)"),
            // A second expectation on the same method.
            TEST.replace(
                "    assert!(remind",
                "    mailer.expect_send().times(1).return_const(true);\n    assert!(remind",
            ),
            // A call outside mockall's count, or a mock that never drops.
            TEST.to_string() + "\n    mailer.checkpoint();",
            TEST.to_string() + "\n    std::mem::forget(mailer);",
            TEST.replace(
                "let mut mailer = MockMailer::new();",
                "let mut mailer = std::mem::ManuallyDrop::new(MockMailer::new());",
            ),
            // A second name for the mock, or a call through another mock.
            TEST.to_string() + "\n    let other = &mailer;",
            TEST.replace(
                "remind(&mailer, \"ann\", 30)",
                "remind(&spare, \"ann\", 30)",
            ),
            // Calls that may not run, or not once.
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "if true { remind(&mailer, \"ann\", 30); }",
            ),
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "assert!(true || remind(&mailer, \"ann\", 30));",
            ),
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "assert!(true, \"{}\", remind(&mailer, \"ann\", 30));",
            ),
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "for _ in 0..1 { remind(&mailer, \"ann\", 30); }",
            ),
            // An input the rule cannot read.
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "let day = 30;\n    assert!(!remind(&mailer, \"ann\", day));",
            ),
            // A second mock binding.
            TEST.to_string() + "\n    let mut spare = MockMailer::new();",
            // Names that defeat mockall's count without naming the mock.
            TEST.to_string() + "\n    let _ = std::panic::catch_unwind;",
            TEST.to_string() + "\n    let _ = std::process::exit;",
            TEST.to_string() + "\n    let _ = Box::leak;",
            // A raw identifier still names the mock.
            TEST.to_string() + "\n    let _keep = std::mem::MaybeUninit::new(r#mailer);",
            // Calls that never run: an unawaited `async` block, a labeled
            // block left early.
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "let _ = async { remind(&mailer, \"ann\", 30) };",
            ),
            TEST.replace(
                "assert!(!remind(&mailer, \"ann\", 30));",
                "'skip: {\n        break 'skip;\n        remind(&mailer, \"ann\", 30);\n    }",
            ),
            // An expectation bound to a second name, then widened.
            TEST.replace(
                "    mailer.expect_send().with(eq(\"ann\"), always()).times(1).return_const(true);",
                "    let e = mailer.expect_send().times(1).return_const(true);\n    e.times(1..3);",
            ),
            // Arity, `&mut` and a typed mock binding.
            TEST.replace("remind(&mailer, \"ann\", 30)", "remind(&mailer, 30)"),
            TEST.replace(
                "remind(&mailer, \"ann\", 30)",
                "remind(&mut mailer, \"ann\", 30)",
            ),
            TEST.replace(
                "let mut mailer = MockMailer::new();",
                "let mut mailer: MockMailer = MockMailer::new();",
            ),
        ] {
            assert!(!counted(OWNER, &body)?, "{body}");
        }
        // The shared test gates refuse these.
        for body in [
            TEST.replace("return_const(true)", "returning(|_, _| true)"),
            TEST.to_string() + "\n    return;",
            TEST.replace(
                "    assert!(!remind",
                "    #[cfg(any())]\n    assert!(!remind",
            ),
            "    skip!();\n".to_string() + TEST,
        ] {
            assert!(!counted(OWNER, &body)?, "{body}");
        }
        let should_panic = format!("{OWNER}#[test]\n#[should_panic]\nfn t() {{\n{TEST}\n}}\n");
        let root = parsed(&should_panic).ok_or("premise: the source parses")?;
        let read = gate(&should_panic).ok_or("premise: the owner gate reads")?;
        let test = unique_function(root.syntax(), "t").ok_or("premise: one test `t`")?;
        assert!(!read.counted_on_both_sides(&test, "remind"));
        Ok(())
    }

    #[test]
    fn owners_whose_call_count_is_not_plain_are_not_read() {
        for (label, owner) in [
            (
                "else branch",
                OWNER.replace(
                    "    }\n    false",
                    "    } else {\n        return false;\n    }\n    false",
                ),
            ),
            (
                "earlier statement",
                OWNER.replace("    if GRACE", "    let today = 1;\n    if GRACE"),
            ),
            (
                "equality",
                OWNER.replace("GRACE_DAYS < overdue_days", "overdue_days == GRACE_DAYS"),
            ),
            (
                "non-integer parameter",
                OWNER.replace("overdue_days: u32", "overdue_days: Days"),
            ),
            (
                "non-integer const",
                OWNER.replace("GRACE_DAYS: u32", "GRACE_DAYS: Days"),
            ),
            ("computed const", OWNER.replace("= 30;", "= 15 * 2;")),
            (
                "shadowed const",
                OWNER.replace("    if GRACE", "    let GRACE_DAYS = 1;\n    if GRACE"),
            ),
            (
                "second const binding",
                OWNER.replace(
                    "pub fn remind",
                    "mod m { const GRACE_DAYS: u32 = 9; }\npub fn remind",
                ),
            ),
            (
                "mutable reference",
                OWNER.replace("&dyn Mailer", "&mut dyn Mailer"),
            ),
            (
                "boxed receiver",
                OWNER.replace("&dyn Mailer", "Box<dyn Mailer>"),
            ),
            (
                "second call",
                OWNER.replace("    false\n}", "    mailer.send(user, \"\");\n    false\n}"),
            ),
            (
                "receiver passed on",
                OWNER.replace("    false\n}", "    audit(mailer);\n    false\n}"),
            ),
            (
                "call in a loop",
                OWNER.replace(
                    "return mailer.send(user, &body);",
                    "for _ in 0..2 { mailer.send(user, &body); }\n        return true;",
                ),
            ),
            (
                "call in a branch",
                OWNER.replace(
                    "return mailer.send(user, &body);",
                    "if user.is_empty() { return mailer.send(user, &body); }\n        return true;",
                ),
            ),
            (
                "early return",
                OWNER.replace(
                    "        let body",
                    "        if user.is_empty() { return false; }\n        let body",
                ),
            ),
            (
                "try before",
                OWNER.replace(
                    "        let body",
                    "        let n = parse(user)?;\n        let body",
                ),
            ),
            ("other macro", OWNER.replace("format!", "log!")),
            (
                "closure call",
                OWNER.replace(
                    "return mailer.send(user, &body);",
                    "let send = || mailer.send(user, &body);\n        return send();",
                ),
            ),
            (
                "not automocked",
                OWNER.replace("#[cfg_attr(test, mockall::automock)]\n", ""),
            ),
            ("method not declared", OWNER.replace("fn send(", "fn post(")),
            (
                "generic trait",
                OWNER
                    .replace("pub trait Mailer {", "pub trait Mailer<T> {")
                    .replace("&dyn Mailer", "&dyn Mailer<u8>"),
            ),
            (
                "async owner",
                OWNER.replace("pub fn remind", "pub async fn remind"),
            ),
            (
                "lifetime receiver",
                OWNER.replace("&dyn Mailer", "&'a dyn Mailer"),
            ),
            (
                "bound list",
                OWNER.replace("&dyn Mailer", "&(dyn Mailer + Send)"),
            ),
            (
                "qualified trait",
                OWNER.replace("&dyn Mailer", "&dyn crate::Mailer"),
            ),
            (
                "mutable receiver binding",
                OWNER.replace("mailer: &dyn", "mut mailer: &dyn"),
            ),
            (
                "trait declared twice",
                OWNER.replace(
                    "pub const",
                    "mod m { pub trait Mailer { fn send(&self); } }\npub const",
                ),
            ),
            (
                "await before",
                OWNER.replace(
                    "        let body",
                    "        ready().await;\n        let body",
                ),
            ),
            (
                "shadowed const after the guard",
                OWNER.replace("    false\n}", "    let GRACE_DAYS = 1;\n    false\n}"),
            ),
            (
                "raw receiver after",
                OWNER.replace(
                    "    false\n}",
                    "    r#mailer.send(user, \"\");\n    false\n}",
                ),
            ),
        ] {
            // The owner still parses and is the plain free function, so
            // the refusal is the gate's own.
            let root = parsed(&owner);
            assert!(
                root.as_ref()
                    .is_some_and(|root| unique_function(root.syntax(), "remind").is_some()),
                "premise: {label} parses"
            );
            assert!(gate(&owner).is_none(), "{label}");
        }
    }

    #[test]
    fn a_const_input_is_not_read_as_its_value() -> Result<(), String> {
        // In the owner's own file too: a glob import or a macro-defined
        // item can rebind the name where the test reads it.
        let body = TEST.replace("\"ann\", 30)", "\"ann\", GRACE_DAYS)");
        assert!(!counted(OWNER, &body)?);
        assert!(counted(OWNER, TEST)?);
        Ok(())
    }

    #[test]
    fn the_old_line_must_order() {
        assert!(old_condition_orders("overdue_days > GRACE_DAYS"));
        assert!(old_condition_orders("    if overdue_days > GRACE_DAYS {"));
        assert!(!old_condition_orders("overdue_days == GRACE_DAYS"));
        assert!(!old_condition_orders("if overdue_days != 30 {"));
        assert!(!old_condition_orders("check(overdue_days)"));
        assert!(!old_condition_orders("overdue_days > limit!()"));
    }
}
