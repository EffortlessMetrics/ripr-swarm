//! RIPR-SPEC-0227 rule 3: result-side oracles that observe a guard's side
//! flip.
//!
//! `assert!(check(256).is_err())` reads `broad_error` / weak: alone it
//! cannot tell `len >= 256` from `len >= 255`. One test that also asserts
//! `check(255).is_ok()` can. When the changed predicate is a guard whose
//! taken branch returns `Err` and whose fall-through can only return `Ok`,
//! an `Err` oracle on the edge input and an `Ok` oracle on its neighbour
//! pin the edge: moving it either way flips the side one of them observes.
//!
//! The producer establishes this without evaluating code it cannot read.
//! The guard is the owner's first statement, so every call reaches it
//! (condition 2); it compares one integer parameter with an integer literal,
//! so the test's literal argument decides it; the rest of the body has no
//! `return`, `?`, `Err` or macro and ends in `Ok(..)`, so the untaken guard
//! returns the other side (condition 3). Each assertion must agree with the
//! guard as written (the test passes on the current code), and the `Err` and
//! `Ok` inputs must be adjacent integers on either side of the edge.
//!
//! An oracle on one side only (`>= 255` survives a lone `check(256)`
//! assertion, RIPR-SPEC-0227 example 2 in the verdict corpus), on a far input
//! (example 3), or split across tests (example 4) leaves the evidence
//! unchanged. A change to the returned error (rule 1) never confirms, so the
//! `error_path` probe on the guard's line is confirmed only when the old and
//! new guard return the same `Err` payload.

use super::*;
use ra_ap_syntax::AstToken;

pub(super) const GUARD_SIDE_FLIP: &str = "Result-side oracles observe the guard's side flip: one test asserts the owner returns `Err` on one side of this guard's edge and `Ok` on the other (RIPR-SPEC-0227 rule 3)";

pub(in crate::analysis::classifier::evidence) fn discrimination(
    context: &ProbeContext<'_>,
    observe: &StageEvidence,
    current: &StageEvidence,
) -> Option<StageEvidence> {
    if !matches!(
        context.probe.family,
        ProbeFamily::Predicate | ProbeFamily::ErrorPath
    ) || observe.state != StageState::Yes
        || current.state != StageState::Weak
        || !context.workspace_complete
    {
        return None;
    }
    let (owner_name, root, source) = owner_file(context)?;
    let function = unique_free_function(root.syntax(), &owner_name)?;
    let guard = Guard::read(&function, &source)?;
    if !guard.is_probe(context) || !guard.keeps_error(context.probe.before.as_deref()?) {
        return None;
    }
    plain_related_tests(context, &owner_name)
        .any(|test| guard.observed_on_both_sides(&test, &owner_name))
        .then(|| StageEvidence::new(StageState::Yes, Confidence::High, GUARD_SIDE_FLIP))
}

/// The changed owner's name, parsed file and source, under the gates rule
/// 3b applies: the owner is the probe's own, its name is unique in the
/// index, and the parsed file is the probe's file as read by the parser.
pub(super) fn owner_file(context: &ProbeContext<'_>) -> Option<(String, ast::SourceFile, String)> {
    let owner = context.owner_fn?;
    if context.probe.owner.as_ref() != Some(&owner.id)
        || context
            .index
            .functions()
            .iter()
            .filter(|function| function.name == owner.name)
            .count()
            != 1
    {
        return None;
    }
    let facts = find_file_facts(context.index, &owner.file)?;
    let same_source = find_file_facts(context.index, &context.probe.location.file)
        .is_some_and(|probe_facts| std::ptr::eq(facts.data(), probe_facts.data()))
        || same_current_file(
            context,
            &owner.file,
            &context.probe.location.file,
            &facts.source,
        );
    if !same_source || facts.used_lexical_fallback {
        return None;
    }
    let root = parsed(&facts.source)?;
    Some((owner.name.clone(), root, facts.source.to_string()))
}

/// The parsed functions of the tests that call the owner directly, from
/// test files rule 3b would read: no lexical fallback, no inherited parent
/// context, no foreign import of the owner's name, and plain assertion
/// macros and inner `cfg`.
pub(super) fn plain_related_tests<'c>(
    context: &'c ProbeContext<'_>,
    owner: &'c str,
) -> impl Iterator<Item = ast::Fn> + 'c {
    context
        .related_tests
        .iter()
        .filter(|(_, reason)| *reason == RelationReason::DirectOwnerCall)
        .filter_map(move |(test, _)| {
            let test_facts = find_file_facts(context.index, &test.file)?;
            if test_facts.used_lexical_fallback
                || !test_facts.role_provenance.edges.is_empty()
                || test_facts
                    .role_provenance
                    .earliest_unresolved_reason
                    .is_some()
                || context.test_file_imports_foreign_callee_name(
                    &test.file,
                    &test_facts.source,
                    owner,
                )
            {
                return None;
            }
            let test_root = parsed(&test_facts.source)?;
            if !test_file_macros_and_inner_cfg_are_plain(test_root.syntax()) {
                return None;
            }
            unique_function(test_root.syntax(), &test.name)
        })
}

/// The expressions of a plain `#[test]`'s top-level expression statements
/// and tail, when every one of them runs: the same test gates as rule 3b's
/// `asserts_owner_err` (no `return`, closure, nested item or non-assertion
/// macro anywhere in the body, no binding of the owner's name), and no
/// attribute anywhere in the body.
pub(super) fn plain_test_statements(function: &ast::Fn, owner: &str) -> Option<Vec<ast::Expr>> {
    let attributes = function.attrs().collect::<Vec<_>>();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    if attribute.syntax().text() != "#[test]"
        || function.async_token().is_some()
        || function.param_list()?.params().next().is_some()
        || !enclosing_modules_are_test_only(function)
    {
        return None;
    }
    let body = function.body()?;
    for node in body.syntax().descendants() {
        // A macro call parses as an item too; the check below reads it.
        // An attribute anywhere (`#[cfg(any())]` on the tail assertion) may
        // compile an assertion out.
        if ast::ReturnExpr::can_cast(node.kind())
            || ast::Attr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || (ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind()))
        {
            return None;
        }
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && (!call.path().is_some_and(|path| {
                ASSERTION_MACROS.contains(&path.syntax().text().to_string().as_str())
            }) || call
                .token_tree()
                .is_some_and(|tree| may_leave(tree.syntax())))
        {
            return None;
        }
        if ast::IdentPat::cast(node)
            .is_some_and(|pat| pat.name().is_some_and(|n| n.text() == owner))
        {
            return None;
        }
    }
    let statements = body.stmt_list()?;
    Some(
        statements
            .statements()
            .filter_map(|statement| {
                let ast::Stmt::ExprStmt(statement) = statement else {
                    return None;
                };
                // `#[cfg(..)] assert!(..);` may never compile.
                if statement
                    .syntax()
                    .descendants()
                    .any(|node| ast::Attr::can_cast(node.kind()))
                {
                    return None;
                }
                statement.expr()
            })
            .chain(statements.tail_expr())
            .collect(),
    )
}

/// Whether a macro's tokens hold a way out of the test or a block that
/// could hold one: an assertion message is evaluated only when the
/// condition fails, so `assert!(c, "{}", { return; })` passes a test whose
/// condition is false. The tokens are not parsed as expressions, so the
/// body scan cannot see them.
pub(super) fn may_leave(tree: &SyntaxNode) -> bool {
    tree.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| {
            matches!(
                token.text(),
                "return" | "break" | "continue" | "loop" | "?" | "|" | "||" | "async"
            ) || (token.text() == "{" && token.parent().is_some_and(|parent| parent != *tree))
        })
}

/// `if <param> <op> <literal> { return Err(<payload>); }` as the owner's
/// first statement, over a built-in integer parameter.
struct Guard {
    /// Position of the compared parameter in the owner's parameter list.
    parameter: usize,
    parameter_count: usize,
    /// The comparison, normalized to `parameter <op> literal`.
    op: ast::CmpOp,
    literal: i128,
    /// Canonical tokens of the guard's condition and returned payload.
    condition: String,
    payload: String,
    /// 1-based lines of the condition and of the `return`.
    condition_line: usize,
    return_line: usize,
    /// Canonical text of the `return Err(..)` expression.
    return_text: String,
}

impl Guard {
    fn read(function: &ast::Fn, source: &str) -> Option<Self> {
        let parameters = function.param_list()?;
        if parameters.self_param().is_some() {
            return None;
        }
        let parameters = parameters.params().collect::<Vec<_>>();
        let body = function.body()?.stmt_list()?;
        let mut statements = body.statements();
        let ast::Stmt::ExprStmt(first) = statements.next()? else {
            return None;
        };
        let ast::Expr::IfExpr(guard) = first.expr()? else {
            return None;
        };
        if guard.else_branch().is_some()
            || first.syntax().descendants().any(|node| {
                ast::Attr::can_cast(node.kind())
                    || ast::MacroCall::can_cast(node.kind())
                    || ast::ClosureExpr::can_cast(node.kind())
                    || ast::TryExpr::can_cast(node.kind())
                    || ast::Item::can_cast(node.kind())
            })
        {
            return None;
        }
        let returned = sole_return(&guard.then_branch()?)?;
        let ast::Expr::CallExpr(error) = returned.expr()? else {
            return None;
        };
        let arguments = error.arg_list()?.args().collect::<Vec<_>>();
        let [payload] = arguments.as_slice() else {
            return None;
        };
        if error.expr()?.syntax().text() != "Err" {
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
        let (name, literal, op) = match (parameter_name(&lhs), integer_literal(&rhs)) {
            (Some(name), Some(literal)) => (name, literal, op),
            _ => (parameter_name(&rhs)?, integer_literal(&lhs)?, flipped(op)),
        };
        let parameter = parameters.iter().position(|parameter| {
            parameter.pat().is_some_and(|pat| match pat {
                ast::Pat::IdentPat(pat) => {
                    pat.ref_token().is_none() && pat.name().is_some_and(|n| n.text() == name)
                }
                _ => false,
            }) && parameter
                .ty()
                .is_some_and(|ty| INTEGER_TYPES.contains(&ty.syntax().text().to_string().as_str()))
        })?;
        // Condition 3: past the untaken guard only `Ok` can come back.
        let rest = statements
            .map(|statement| statement.syntax().clone())
            .collect::<Vec<_>>();
        let tail = body.tail_expr()?;
        if !rest
            .iter()
            .chain(std::iter::once(tail.syntax()))
            .all(cannot_return_err)
        {
            return None;
        }
        let ast::Expr::CallExpr(ok) = &tail else {
            return None;
        };
        if ok.expr()?.syntax().text() != "Ok" {
            return None;
        }
        Some(Self {
            parameter,
            parameter_count: parameters.len(),
            op,
            literal,
            condition: canonical_tokens(condition.syntax(), None),
            payload: canonical_tokens(payload.syntax(), None),
            condition_line: line_of(source, condition.syntax())?,
            return_line: line_of(source, returned.syntax())?,
            return_text: canonical_tokens(returned.syntax(), None),
        })
    }

    /// The probe is this guard's condition, or (on the guard's own line)
    /// its `return Err(..)`.
    fn is_probe(&self, context: &ProbeContext<'_>) -> bool {
        let probe = context.probe;
        let Some(expression) = parse_expression(&probe.expression) else {
            return false;
        };
        let expression = canonical_tokens(expression.syntax(), None);
        match probe.family {
            ProbeFamily::Predicate => {
                probe.location.line == self.condition_line && expression == self.condition
            }
            ProbeFamily::ErrorPath => {
                probe.location.line == self.return_line
                    && self.return_line == self.condition_line
                    && expression == self.return_text
            }
            _ => false,
        }
    }

    /// Rule 1: the edit left the returned error alone. The old text is
    /// either the whole old guard, whose payload must read the same, or a
    /// smaller subject the edit was confined to: the old condition for the
    /// predicate probe, the same `return` for the error probe.
    fn keeps_error(&self, before: &str) -> bool {
        if let Some((old_condition, old_payload)) = guard_parts(before) {
            return old_payload == self.payload && !is_equality(&old_condition);
        }
        let Some(old) = parse_expression(before.trim().trim_end_matches(';')) else {
            return false;
        };
        match old {
            ast::Expr::BinExpr(comparison) => {
                matches!(comparison.op_kind(), Some(ast::BinaryOp::CmpOp(_)))
                    && !is_equality(&ast::Expr::BinExpr(comparison.clone()))
                    && comparison.syntax().descendants().all(|node| {
                        !ast::MacroCall::can_cast(node.kind())
                            && !ast::BlockExpr::can_cast(node.kind())
                    })
            }
            ast::Expr::ReturnExpr(_) => canonical_tokens(old.syntax(), None) == self.return_text,
            _ => false,
        }
    }

    /// One test asserts `Err` on an input the guard takes and `Ok` on the
    /// adjacent input it does not, and no assertion of that test disagrees
    /// with the guard as written.
    fn observed_on_both_sides(&self, test: &ast::Fn, owner: &str) -> bool {
        let Some(statements) = plain_test_statements(test, owner) else {
            return false;
        };
        let mut taken = Vec::new();
        let mut untaken = Vec::new();
        for expression in statements {
            let Some((call, side)) = side_observation(&expression, owner) else {
                continue;
            };
            let Some(argument) = self.argument(&call) else {
                return false;
            };
            let takes = self.takes(argument);
            match (side, takes) {
                (Side::Err, true) => taken.push(argument),
                (Side::Ok, false) => untaken.push(argument),
                // The assertion would fail on the current code, so ripr is
                // not reading this test the way it runs.
                _ => return false,
            }
        }
        taken
            .iter()
            .any(|a| untaken.iter().any(|b| a.abs_diff(*b) == 1))
    }

    /// The integer literal the call passes for the compared parameter.
    fn argument(&self, call: &ast::CallExpr) -> Option<i128> {
        let arguments = call.arg_list()?.args().collect::<Vec<_>>();
        if arguments.len() != self.parameter_count {
            return None;
        }
        integer_literal(&unwrap_parens(arguments.get(self.parameter)?.clone()))
    }

    fn takes(&self, value: i128) -> bool {
        match self.op {
            ast::CmpOp::Ord {
                ordering: ast::Ordering::Less,
                strict: true,
            } => value < self.literal,
            ast::CmpOp::Ord {
                ordering: ast::Ordering::Less,
                strict: false,
            } => value <= self.literal,
            ast::CmpOp::Ord {
                ordering: ast::Ordering::Greater,
                strict: true,
            } => value > self.literal,
            ast::CmpOp::Ord {
                ordering: ast::Ordering::Greater,
                strict: false,
            } => value >= self.literal,
            ast::CmpOp::Eq { .. } => false,
        }
    }
}

pub(super) const INTEGER_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Err,
    Ok,
}

/// The block holds exactly one `return <expr>`, as its only statement or
/// its tail.
fn sole_return(block: &ast::BlockExpr) -> Option<ast::ReturnExpr> {
    let list = block.stmt_list()?;
    let statements = list.statements().collect::<Vec<_>>();
    let expression = match (statements.as_slice(), list.tail_expr()) {
        ([ast::Stmt::ExprStmt(statement)], None) => statement.expr()?,
        ([], Some(tail)) => tail,
        _ => return None,
    };
    match expression {
        ast::Expr::ReturnExpr(returned) => Some(returned),
        _ => None,
    }
}

/// Nothing in the node can leave the owner other than by the `Ok(..)` tail,
/// or hide a way to.
fn cannot_return_err(node: &SyntaxNode) -> bool {
    std::iter::once(node.clone())
        .chain(node.descendants())
        .all(|node| {
            !ast::ReturnExpr::can_cast(node.kind())
                && !ast::TryExpr::can_cast(node.kind())
                && !ast::ClosureExpr::can_cast(node.kind())
                && !ast::MacroCall::can_cast(node.kind())
                && !ast::Item::can_cast(node.kind())
                && !ast::BlockExpr::cast(node.clone())
                    .is_some_and(|block| block.async_token().is_some() || block.label().is_some())
                && !ast::PathExpr::cast(node).is_some_and(|path| path.syntax().text() == "Err")
        })
}

/// A bare single-segment path, read as a parameter name.
pub(super) fn parameter_name(expression: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expression else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}

/// A decimal integer literal, optionally negated, with an integer suffix
/// or none.
pub(super) fn integer_literal(expression: &ast::Expr) -> Option<i128> {
    let (negative, literal) = match expression {
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
            let ast::Expr::Literal(literal) = prefix.expr()? else {
                return None;
            };
            (true, literal)
        }
        ast::Expr::Literal(literal) => (false, literal.clone()),
        _ => return None,
    };
    let ast::LiteralKind::IntNumber(number) = literal.kind() else {
        return None;
    };
    let text = number.text().replace('_', "");
    let digits = INTEGER_TYPES
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<i128>().ok()?;
    Some(if negative { -value } else { value })
}

/// `a < b` is `b > a`.
pub(super) fn flipped(op: ast::CmpOp) -> ast::CmpOp {
    match op {
        ast::CmpOp::Ord { ordering, strict } => ast::CmpOp::Ord {
            ordering: match ordering {
                ast::Ordering::Less => ast::Ordering::Greater,
                ast::Ordering::Greater => ast::Ordering::Less,
            },
            strict,
        },
        other => other,
    }
}

pub(super) fn line_of(source: &str, node: &SyntaxNode) -> Option<usize> {
    let offset = u32::from(node.text_range().start()) as usize;
    Some(
        source
            .get(..offset)?
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1,
    )
}

/// An `==` or `!=` comparison. An old guard written that way agrees with
/// the new ordering guard at the edge pair while differing elsewhere
/// (`len == 256` to `len >= 256` changes only `257..`), so the edge
/// oracles do not observe what changed.
pub(super) fn is_equality(condition: &ast::Expr) -> bool {
    matches!(
        unwrap_parens(condition.clone()),
        ast::Expr::BinExpr(comparison)
            if matches!(comparison.op_kind(), Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { .. })))
    )
}

/// The condition and canonical payload of an old guard line
/// `if <condition> { return Err(<payload>); }`.
fn guard_parts(line: &str) -> Option<(ast::Expr, String)> {
    let source = format!("fn __guard() {{ {} }}", line.trim());
    let root = parsed(&source)?;
    let function = root.syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    let guard = match (
        body.statements().collect::<Vec<_>>().as_slice(),
        body.tail_expr(),
    ) {
        ([ast::Stmt::ExprStmt(statement)], None) => statement.expr()?,
        ([], Some(tail)) => tail,
        _ => return None,
    };
    let ast::Expr::IfExpr(guard) = guard else {
        return None;
    };
    if guard.else_branch().is_some() {
        return None;
    }
    let ast::Expr::CallExpr(error) = sole_return(&guard.then_branch()?)?.expr()? else {
        return None;
    };
    if error.expr()?.syntax().text() != "Err" {
        return None;
    }
    let arguments = error.arg_list()?.args().collect::<Vec<_>>();
    let [payload] = arguments.as_slice() else {
        return None;
    };
    Some((guard.condition()?, canonical_tokens(payload.syntax(), None)))
}

/// The direct owner call an assertion statement observes, and the side it
/// requires: `assert!(owner(..).is_err())`, `assert!(!owner(..).is_ok())`,
/// `assert!(matches!(owner(..), Err(_)))`, `owner(..).unwrap_err()` or
/// `.expect_err(..)` for `Err`; their mirrors and `owner(..).unwrap()` or
/// `.expect(..)` for `Ok`.
fn side_observation(expression: &ast::Expr, owner: &str) -> Option<(ast::CallExpr, Side)> {
    if let ast::Expr::MethodCallExpr(call) = expression {
        let side = match call.name_ref()?.text().to_string().as_str() {
            "unwrap_err" | "expect_err" => Side::Err,
            "unwrap" | "expect" => Side::Ok,
            _ => return None,
        };
        return Some((direct_owner_call(call.receiver()?, owner)?, side));
    }
    let arguments = macro_arguments(expression, "assert")?;
    condition_side(&parse_expression(arguments.first()?)?, owner)
}

fn condition_side(condition: &ast::Expr, owner: &str) -> Option<(ast::CallExpr, Side)> {
    match condition {
        ast::Expr::MethodCallExpr(call) => {
            if call.arg_list()?.args().next().is_some() {
                return None;
            }
            let side = match call.name_ref()?.text().to_string().as_str() {
                "is_err" => Side::Err,
                "is_ok" => Side::Ok,
                _ => return None,
            };
            Some((direct_owner_call(call.receiver()?, owner)?, side))
        }
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Not) => {
            let ast::Expr::MethodCallExpr(_) = prefix.expr()? else {
                return None;
            };
            let (call, side) = condition_side(&prefix.expr()?, owner)?;
            Some((
                call,
                match side {
                    Side::Err => Side::Ok,
                    Side::Ok => Side::Err,
                },
            ))
        }
        _ => {
            let arguments = macro_arguments(condition, "matches")?;
            let [scrutinee, pattern] = arguments.as_slice() else {
                return None;
            };
            let side = match pattern.trim() {
                "Err(_)" | "Err(..)" => Side::Err,
                "Ok(_)" | "Ok(..)" => Side::Ok,
                _ => return None,
            };
            Some((
                direct_owner_call(parse_expression(scrutinee)?, owner)?,
                side,
            ))
        }
    }
}

fn direct_owner_call(expression: ast::Expr, owner: &str) -> Option<ast::CallExpr> {
    if !is_direct_owner_call(&expression, owner) {
        return None;
    }
    match expression {
        ast::Expr::CallExpr(call) => Some(call),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "pub fn check(len: usize) -> Result<usize, E> {\n    if len >= 256 { return Err(E::TooLong); }\n    Ok(len)\n}\n";

    fn guard(source: &str) -> Option<Guard> {
        let root = parsed(source)?;
        Guard::read(&unique_free_function(root.syntax(), "check")?, source)
    }

    /// Whether the test observes both sides; `None` when the test gates
    /// refuse it. A source or guard that does not read is an error, so a
    /// control cannot pass on a broken premise.
    fn both_sides(body: &str) -> Result<Option<bool>, String> {
        let source = format!("{OWNER}#[test]\nfn t() {{\n{body}\n}}\n");
        let root = parsed(&source).ok_or("premise: the source parses")?;
        let guard = guard(&source).ok_or("premise: the owner guard reads")?;
        let test = unique_function(root.syntax(), "t").ok_or("premise: one test `t`")?;
        Ok(plain_test_statements(&test, "check")
            .is_some()
            .then(|| guard.observed_on_both_sides(&test, "check")))
    }

    #[test]
    fn a_first_statement_integer_guard_over_an_ok_tail_is_read() {
        let read = guard(OWNER);
        assert!(read.as_ref().is_some_and(|guard| {
            guard.parameter == 0
                && guard.literal == 256
                && guard.condition_line == 2
                && guard.return_line == 2
                && guard.takes(256)
                && !guard.takes(255)
        }));
        // `256 <= len` reads as `len >= 256`.
        let flipped = guard(&OWNER.replace("len >= 256", "256 <= len"));
        assert!(flipped.is_some_and(|guard| guard.takes(256) && !guard.takes(255)));
    }

    #[test]
    fn a_guard_the_rule_cannot_establish_is_not_read() {
        for (label, source) in [
            (
                "earlier statement",
                OWNER.replace("    if len", "    let n = len;\n    if len"),
            ),
            ("else branch", OWNER.replace("); }", "); } else { }")),
            (
                "later Err",
                OWNER.replace(
                    "    Ok(len)",
                    "    if len == 7 { return Err(E::Bad); }\n    Ok(len)",
                ),
            ),
            (
                "later ?",
                OWNER.replace("    Ok(len)", "    Ok(other(len)?)"),
            ),
            (
                "non-Ok tail",
                OWNER.replace("    Ok(len)", "    other(len)"),
            ),
            (
                "macro after",
                OWNER.replace("    Ok(len)", "    log!(len);\n    Ok(len)"),
            ),
            (
                "Ok in guard",
                OWNER.replace("return Err(E::TooLong)", "return Ok(0)"),
            ),
            ("equality", OWNER.replace(">=", "==")),
            (
                "call in condition",
                OWNER.replace("len >= 256", "len.max(1) >= 256"),
            ),
            (
                "non-integer parameter",
                OWNER.replace("len: usize", "len: Len"),
            ),
            ("float literal", OWNER.replace("256", "256.0")),
            ("hex literal", OWNER.replace("256", "0x100")),
        ] {
            assert!(guard(&source).is_none(), "{label}");
        }
    }

    #[test]
    fn err_on_the_edge_and_ok_beside_it_observe_both_sides() -> Result<(), String> {
        for body in [
            "assert!(check(256).is_err());\nassert!(check(255).is_ok());",
            "assert!(check(255).is_ok());\nassert!(!check(256).is_ok());",
            "assert!(matches!(check(256), Err(_)));\ncheck(255).unwrap();",
            "check(256).unwrap_err();\nassert!(!check(255).is_err());",
            // A far assertion that agrees with the guard adds nothing and
            // takes nothing away.
            "assert!(check(300).is_err());\nassert!(check(256).is_err());\nassert!(check(255).is_ok());",
            // A message that cannot leave is fine.
            "assert!(check(256).is_err(), \"edge {}\", 256);\nassert!(check(255).is_ok());",
            // A tail assertion is read too.
            "assert!(check(256).is_err());\nassert!(check(255).is_ok())",
        ] {
            assert_eq!(both_sides(body)?, Some(true), "{body}");
        }
        Ok(())
    }

    #[test]
    fn one_side_far_inputs_and_disagreeing_assertions_do_not() -> Result<(), String> {
        // Each body is read by the test gates, so the refusal is the
        // observation's own.
        for body in [
            // RIPR-SPEC-0227 example 2: `len >= 255` survives.
            "assert!(check(256).is_err());",
            "assert!(check(255).is_ok());",
            // Example 3: far from the edge.
            "assert!(check(300).is_err());\nassert!(check(3).is_ok());",
            // Adjacent, but both on the untaken side.
            "assert!(check(254).is_ok());\nassert!(check(255).is_ok());",
            // An assertion the current code fails: ripr misreads the test,
            // as a statement or as the tail.
            "assert!(check(256).is_err());\nassert!(check(255).is_ok());\nassert!(check(9).is_err());",
            "assert!(check(256).is_err());\nassert!(check(255).is_ok());\nassert!(check(9).is_err())",
            // Non-literal argument.
            "let n = 256;\nassert!(check(n).is_err());\nassert!(check(255).is_ok());",
            // Inside a branch: may not run.
            "if true { assert!(check(256).is_err()); }\nassert!(check(255).is_ok());",
        ] {
            assert_eq!(both_sides(body)?, Some(false), "{body}");
        }
        // The test gates refuse these outright.
        for body in [
            // A skip macro may return first.
            "skip!();\nassert!(check(256).is_err());\nassert!(check(255).is_ok());",
            // A nested item can rebind the owner's name.
            "use other::check;\nassert!(check(256).is_err());\nassert!(check(255).is_ok());",
            "return;\nassert!(check(256).is_err());\nassert!(check(255).is_ok());",
            // An attribute may compile a side out, on a statement or the tail.
            "assert!(check(256).is_err());\n#[cfg(any())]\nassert!(check(255).is_ok());",
            "assert!(check(256).is_err());\n#[cfg(any())]\nassert!(check(255).is_ok())",
            // A message runs only when the condition fails, and may leave.
            "assert!(check(256).is_err(), \"{}\", { if true { return; } 0 });\nassert!(check(255).is_ok());",
            "assert!(check(256).is_err(), \"{}\", f()?);\nassert!(check(255).is_ok());",
        ] {
            assert_eq!(both_sides(body)?, None, "{body}");
        }
        Ok(())
    }

    #[test]
    fn the_old_guard_must_return_the_same_error() -> Result<(), String> {
        let read = guard(OWNER).ok_or("premise: the owner guard reads")?;
        assert!(read.keeps_error("    if len > 256 { return Err(E::TooLong); }"));
        assert!(read.keeps_error("len > 256"));
        assert!(read.keeps_error("return Err(E::TooLong)"));
        assert!(!read.keeps_error("    if len > 256 { return Err(E::Bad); }"));
        assert!(!read.keeps_error("return Err(E::Bad)"));
        assert!(!read.keeps_error("Ok(len)"));
        // An old equality guard differs from the new one away from the edge.
        assert!(!read.keeps_error("    if len == 256 { return Err(E::TooLong); }"));
        assert!(!read.keeps_error("len == 256"));
        assert!(!read.keeps_error("len != 256"));

        Ok(())
    }
}
