//! A bounded selected literal, local binding and direct-return witness.
//!
//! This is deliberately limited to primitive, literal-input paths. Unknown
//! syntax, unresolved names, transformed outputs and ambiguous test calls
//! produce no witness.

use super::const_flow::expr::{
    Assertion, BinOp, Expr, Machine, Reader, Ty, Value, evaluate_initializer, parse_assertion,
    parse_const_decl, parse_expression, parse_fn_header,
};
use super::owner_pin::{
    OwnerPinSyntax, builtin_type_unshadowed, declared_return_type, targeted_free_call_reaches_owner,
};
use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::{FunctionImplContext, FunctionSummary, TestSummary};
use crate::analysis::rust_index::{RustIndex, find_file_facts};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(in crate::analysis) struct TierFlow {
    pub owner: String,
    pub literal: u32,
    pub test: String,
    pub file: std::path::PathBuf,
    pub line: usize,
    pub assertion: String,
    pub sink: String,
    pub sink_line: usize,
}

fn number(text: &str) -> Option<u32> {
    let text = text.trim();
    (!text.is_empty() && text.bytes().all(|c| c.is_ascii_digit() || c == b'_'))
        .then(|| text.replace('_', "").parse().ok())?
}

/// Read only boolean combinations of primitive values and comparisons.
fn condition(text: &str, names: &mut BTreeSet<String>) -> Option<Expr> {
    fn admit(expr: &Expr, names: &mut BTreeSet<String>) -> bool {
        match expr {
            Expr::Literal(Value::Int(_) | Value::Bool(_)) => true,
            Expr::Var(name) => {
                names.insert(name.clone());
                true
            }
            Expr::Not(inner) => admit(inner, names),
            Expr::Bin(op, left, right)
                if matches!(
                    op,
                    BinOp::And
                        | BinOp::Or
                        | BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                ) =>
            {
                admit(left, names) && admit(right, names)
            }
            _ => false,
        }
    }
    let parsed = parse_expression(text)?;
    admit(&parsed, names).then_some(parsed)
}

// Reject malformed or ambiguous expressions instead of assigning them Rust types.
// An integer comparison needs a declared u32 operand; two unsuffixed literals
// could otherwise use a different default type.
fn typed_trace(expr: &Expr, params: &[(String, Ty)], constant: &str) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Bool,
        U32,
        UntypedInt,
    }
    fn kind(expr: &Expr, params: &[(String, Ty)], constant: &str) -> Option<Kind> {
        match expr {
            Expr::Literal(Value::Bool(_)) => Some(Kind::Bool),
            Expr::Literal(Value::Int(value)) if (0..=u32::MAX as i64).contains(value) => {
                Some(Kind::UntypedInt)
            }
            Expr::Var(name) if !constant.is_empty() && name == constant => Some(Kind::U32),
            Expr::Var(name) => match params.iter().find(|(param, _)| param == name)?.1 {
                Ty::Bool => Some(Kind::Bool),
                Ty::Int {
                    min: 0,
                    max: 4294967295,
                } => Some(Kind::U32),
                _ => None,
            },
            Expr::Not(inner) if kind(inner, params, constant)? == Kind::Bool => Some(Kind::Bool),
            Expr::Bin(BinOp::And | BinOp::Or, left, right)
                if kind(left, params, constant)? == Kind::Bool
                    && kind(right, params, constant)? == Kind::Bool =>
            {
                Some(Kind::Bool)
            }
            Expr::Bin(op, left, right)
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) =>
            {
                let l = kind(left, params, constant)?;
                let r = kind(right, params, constant)?;
                ((l == Kind::U32 && matches!(r, Kind::U32 | Kind::UntypedInt))
                    || (r == Kind::U32 && matches!(l, Kind::U32 | Kind::UntypedInt))
                    || (l == Kind::Bool && r == Kind::Bool && matches!(op, BinOp::Eq | BinOp::Ne)))
                .then_some(Kind::Bool)
            }
            Expr::If(cond, yes, no)
                if kind(cond, params, constant)? == Kind::Bool
                    && kind(yes, params, constant)? == Kind::Bool
                    && kind(no, params, constant)? == Kind::Bool =>
            {
                Some(Kind::Bool)
            }
            _ => None,
        }
    }
    kind(expr, params, constant) == Some(Kind::Bool)
}

fn boolean(value: bool) -> Expr {
    Expr::Literal(Value::Bool(value))
}
fn choose(condition: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::If(Box::new(condition), Box::new(yes), Box::new(no))
}
fn only_tail(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let list = block.stmt_list()?;
    list.statements()
        .next()
        .is_none()
        .then(|| list.tail_expr())
        .flatten()
}
fn line_of(node: &ra_ap_syntax::SyntaxNode, owner: &FunctionSummary) -> Option<usize> {
    let offset = usize::from(node.text_range().start());
    Some(
        owner.start_line
            + owner
                .body
                .get(..offset)?
                .bytes()
                .filter(|c| *c == b'\n')
                .count(),
    )
}

/// Map literal leaves of a complete if/else initializer to whether this is
/// the unique changed leaf. Conditions stay as executable bounded expressions.
fn tier_trace(
    expr: ast::Expr,
    owner: &FunctionSummary,
    probe: &Probe,
    names: &mut BTreeSet<String>,
    hits: &mut usize,
) -> Option<Expr> {
    match expr {
        ast::Expr::IfExpr(branch) => {
            let cond = condition(&branch.condition()?.syntax().text().to_string(), names)?;
            let yes = tier_trace(
                only_tail(&branch.then_branch()?)?,
                owner,
                probe,
                names,
                hits,
            )?;
            let no = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => {
                    tier_trace(only_tail(&block)?, owner, probe, names, hits)?
                }
                ast::ElseBranch::IfExpr(other) => {
                    tier_trace(ast::Expr::IfExpr(other), owner, probe, names, hits)?
                }
            };
            Some(choose(cond, yes, no))
        }
        ast::Expr::Literal(literal) => {
            let value = number(&literal.syntax().text().to_string())?;
            let selected = line_of(literal.syntax(), owner)? == probe.location.line;
            if selected {
                if value != number(probe.after.as_deref()?)? {
                    return None;
                }
                *hits += 1;
            }
            Some(boolean(selected))
        }
        _ => None,
    }
}

/// A sink leaf is credited only when it is the binding itself; a different
/// branch is represented as false, never as a transformed observation.
fn sink_trace(expr: ast::Expr, binding: &str, names: &mut BTreeSet<String>) -> Option<Expr> {
    match expr {
        ast::Expr::IfExpr(branch) => {
            let cond = condition(&branch.condition()?.syntax().text().to_string(), names)?;
            let yes = sink_trace(only_tail(&branch.then_branch()?)?, binding, names)?;
            let no = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => sink_trace(only_tail(&block)?, binding, names)?,
                ast::ElseBranch::IfExpr(other) => {
                    sink_trace(ast::Expr::IfExpr(other), binding, names)?
                }
            };
            Some(choose(cond, yes, no))
        }
        ast::Expr::PathExpr(path) => {
            (path.syntax().text().to_string() == binding).then(|| boolean(true))
        }
        // Recognize only a well-typed, simple transformed alternative. It is
        // never credited when selected; it allows the other direct branch.
        ast::Expr::BinExpr(binary) => {
            let parsed = parse_expression(&binary.syntax().text().to_string())?;
            match parsed {
                Expr::Bin(BinOp::Mul, left, right)
                    if matches!(*left, Expr::Var(ref name) if name == binding)
                        && matches!(*right, Expr::Literal(Value::Int(value)) if (0..=u32::MAX as i64).contains(&value)) =>
                {
                    Some(boolean(false))
                }
                _ => None,
            }
        }
        ast::Expr::Literal(literal) => {
            number(&literal.syntax().text().to_string()).map(|_| boolean(false))
        }
        _ => None,
    }
}

fn prefix_guard(statement: &ast::Stmt, names: &mut BTreeSet<String>) -> Option<Expr> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    let ast::Expr::IfExpr(branch) = statement.expr()? else {
        return None;
    };
    if branch.else_branch().is_some() {
        return None;
    }
    let list = branch.then_branch()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [ast::Stmt::ExprStmt(exit)] = statements.as_slice() else {
        return None;
    };
    if list.tail_expr().is_some() {
        return None;
    }
    let ast::Expr::ReturnExpr(returned) = exit.expr()? else {
        return None;
    };
    number(&returned.expr()?.syntax().text().to_string())?;
    condition(&branch.condition()?.syntax().text().to_string(), names)
}

fn changed_literal(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
) -> Option<Option<u32>> {
    if probe.family != ProbeFamily::StaticUnknown {
        return None;
    }
    let now = number(probe.after.as_deref()?)?;
    if !changed
        .added_lines
        .iter()
        .any(|line| line.new_side_line == probe.location.line && number(&line.text) == Some(now))
    {
        return None;
    }
    if changed.removed_lines.is_empty() {
        let lines: BTreeSet<_> = changed
            .added_lines
            .iter()
            .map(|line| line.new_side_line)
            .collect();
        return (owner.start_line..=owner.end_line)
            .all(|line| lines.contains(&line))
            .then_some(None);
    }
    if changed.added_lines.len() != 1 || changed.removed_lines.len() != 1 {
        return None;
    }
    let paired = changed.replaced_line_counterpart(probe.location.line)?;
    let old = number(&paired.text)?;
    if number(probe.before.as_deref()?) != Some(old) {
        return None;
    }
    (old != now).then_some(Some(old))
}

fn plain_assertion(assertion: &Assertion, name: &str, now: u32, old: Option<u32>) -> bool {
    match assertion {
        Assertion::Eq(Expr::Call(callee, args), Expr::Literal(Value::Int(expected)))
            if callee == name
                && args
                    .iter()
                    .all(|arg| matches!(arg, Expr::Literal(Value::Int(_) | Value::Bool(_)))) =>
        {
            *expected == i64::from(now) || old.is_some_and(|old| *expected == i64::from(old))
        }
        _ => false,
    }
}

/// The existing occurrence reader does not associate multiple owner calls on
/// one line with individual assertions (#13373). Refuse that line here.
fn one_owner_call_on_line(index: &RustIndex, test: &TestSummary, line: usize, name: &str) -> bool {
    let Some(facts) = find_file_facts(index, &test.file) else {
        return false;
    };
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&facts.source);
    let Some(text) = line.checked_sub(1).and_then(|at| masked.lines().nth(at)) else {
        return false;
    };
    let mut count = 0usize;
    for (at, _) in text.match_indices(name) {
        let before = text[..at].chars().next_back();
        let after = &text[at + name.len()..];
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        if after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if after.trim_start().starts_with('(') {
            count += 1;
        }
    }
    count == 1
}

pub(in crate::analysis) fn evaluate_tier_flow(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    tests: &[&TestSummary],
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
    workspace_complete: bool,
) -> Option<TierFlow> {
    let old = changed_literal(probe, owner, changed)?;
    let now = number(probe.after.as_deref()?)?;
    if !workspace_complete
        || !matches!(owner.impl_context, FunctionImplContext::Free)
        || !owner.attrs.is_empty()
        || tests.is_empty()
        || declared_return_type(&owner.body).as_deref() != Some("u32")
        || !builtin_type_unshadowed("u32", index, syntax)
        || !builtin_type_unshadowed("bool", index, syntax)
    {
        return None;
    }
    let (name, params) = parse_fn_header(&owner.body)?;
    if name != owner.name
        || params.iter().any(|(_, ty)| {
            !matches!(
                ty,
                Ty::Bool
                    | Ty::Int {
                        min: 0,
                        max: 4294967295
                    }
            )
        })
    {
        return None;
    }
    let param_names: BTreeSet<String> = params.iter().map(|(name, _)| name.clone()).collect();
    if param_names.len() != params.len() {
        return None;
    }
    let parse = parse_clean_source_file(&owner.body)?;
    let mut items = parse.tree().syntax().children().filter_map(ast::Item::cast);
    let ast::Item::Fn(function) = items.next()? else {
        return None;
    };
    if items.next().is_some()
        || function.attrs().next().is_some()
        || function.name()?.text() != owner.name
    {
        return None;
    }
    if function.syntax().descendants().skip(1).any(|node| {
        ast::MacroCall::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::TryExpr::can_cast(node.kind())
            || ast::Item::can_cast(node.kind())
    }) {
        return None;
    }
    let body = function.body()?.stmt_list()?;
    let statements: Vec<_> = body.statements().collect();
    let (guard, binding) = match statements.as_slice() {
        [ast::Stmt::LetStmt(binding)] => (None, binding),
        [guard, ast::Stmt::LetStmt(binding)] => (Some(guard), binding),
        _ => return None,
    };
    if binding.attrs().next().is_some() || binding.ty().is_some() || binding.let_else().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(local) = binding.pat()? else {
        return None;
    };
    if local.mut_token().is_some() || local.ref_token().is_some() || local.at_token().is_some() {
        return None;
    }
    let binding = local.name()?.text().to_string();
    if param_names.contains(&binding) {
        return None;
    }
    let mut names = BTreeSet::new();
    let mut hits = 0;
    let tier = tier_trace(
        binding_initializer(&statements)?,
        owner,
        probe,
        &mut names,
        &mut hits,
    )?;
    if hits != 1 {
        return None;
    }
    let tail = body.tail_expr()?;
    let sink_line = line_of(tail.syntax(), owner)?;
    let sink_text = tail.syntax().text().to_string();
    let sink = sink_trace(tail, &binding, &mut names)?;
    let mut trace = Expr::Bin(BinOp::And, Box::new(tier), Box::new(sink));
    if let Some(guard) = guard {
        trace = choose(prefix_guard(guard, &mut names)?, boolean(false), trace);
    }
    let facts = find_file_facts(index, &owner.file)?;
    let free: Vec<_> = names.difference(&param_names).collect();
    let (constant_name, constant_value) = match free.as_slice() {
        [] => (String::new(), Value::Int(0)),
        [name] => {
            if syntax.macro_may_declare(name, index) {
                return None;
            }
            let mut declarations = facts.source.lines().enumerate().filter_map(|(at, line)| {
                let decl = parse_const_decl(line)?;
                (decl.name == **name).then_some((at + 1, decl))
            });
            let (line, decl) = declarations.next()?;
            if declarations.next().is_some()
                || decl.ty
                    != (Ty::Int {
                        min: 0,
                        max: 4294967295,
                    })
                || !super::const_flow::shares_module_scope(&facts.source, line, &[owner.start_line])
            {
                return None;
            }
            let parsed_file = parse_clean_source_file(&facts.source)?;
            let constants: Vec<_> = parsed_file
                .tree()
                .syntax()
                .children()
                .filter_map(ast::Const::cast)
                .filter(|item| {
                    item.name()
                        .is_some_and(|item_name| item_name.text() == name.as_str())
                })
                .collect();
            if constants.len() != 1 || constants[0].attrs().next().is_some() {
                return None;
            }
            (name.to_string(), evaluate_initializer(&decl.init, decl.ty)?)
        }
        _ => return None,
    };
    if !typed_trace(&trace, &params, &constant_name) {
        return None;
    }
    let reader = Reader::from_branch_parts(name.clone(), params, trace)?;
    let readers = [reader];
    let machine = Machine {
        readers: &readers,
        constant: &constant_name,
        constant_ty: Ty::Int {
            min: 0,
            max: 4294967295,
        },
        constant_value: &constant_value,
    };
    for test in tests {
        if test.generated.is_some()
            || test.attrs.iter().any(|attr| !attr.trim().eq("#[test]"))
            || !targeted_free_call_reaches_owner(test, owner, None, index, syntax)
        {
            continue;
        }
        for oracle in &test.assertions {
            if !syntax.admits_tier_return_assertion(test, oracle, index) {
                continue;
            }
            if !one_owner_call_on_line(index, test, oracle.line, &name) {
                continue;
            }
            let Some(assertion) = parse_assertion(&oracle.text) else {
                continue;
            };
            if !plain_assertion(&assertion, &name, now, old) {
                continue;
            }
            if machine.call_result(&assertion) != Some(Value::Bool(true)) {
                continue;
            }
            return Some(TierFlow {
                owner: name,
                literal: now,
                test: test.name.clone(),
                file: test.file.clone(),
                line: test.start_line,
                assertion: oracle.text.clone(),
                sink: sink_text.clone(),
                sink_line,
            });
        }
    }
    None
}

fn binding_initializer(statements: &[ast::Stmt]) -> Option<ast::Expr> {
    let binding = statements.iter().find_map(|statement| match statement {
        ast::Stmt::LetStmt(binding) => Some(binding),
        _ => None,
    })?;
    binding.initializer()
}
