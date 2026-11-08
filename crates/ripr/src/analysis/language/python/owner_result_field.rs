//! The unread-owner-result-field fact (the Python analogue of #13700).
//!
//! A changed `self.<field> = ...` in a method that returns `self` (or in
//! `__init__`) changes the instance the call hands back. When the one related
//! test binds that instance once (`r = Req("x").timeout(10)`), reads only other
//! attributes of it and never names the changed field, the strong oracle it
//! has observes a different sink, and the missing assertion has exactly one
//! place to go: `r.<field>`. This module recognizes only that shape and
//! refuses everything it cannot read statically. The unread fact never
//! changes the classification or the sink alignment; the finding carries it as
//! an evidence line, and only the gap ledger's agent-packet eligibility reads
//! it.
//!
//! The same shape with the repair applied (`assert r.<field> == <literal>`)
//! is the one owner-result field read this module credits, so a repair the
//! fact starts can close: without it the after phase would still read the
//! bound result as a different sink.

use super::bool_oracle::count_bindings;
use super::module_constants::visit_exprs_in_statements;
use super::related_tests::{PythonRelatedCandidate, owner_class_locals};
use super::source_facts::parse_module_result;
use super::{PythonFramework, PythonOwner, PythonTest};
use crate::domain::{OracleStrength, OwnerKind, ProbeFamily, RelatedTest};
use rustpython_parser::ast::{self, Expr, ExprContext, Mod, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
use std::path::Path;

/// The fact: the test binds the owner's returned instance as `binding` and
/// never reads `field`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UnreadOwnerResultField {
    pub(super) binding: String,
    pub(super) field: String,
    pub(super) test_name: String,
}

impl UnreadOwnerResultField {
    pub(super) fn evidence_line(&self) -> String {
        crate::domain::python_owner_result_field_unread_line(
            &self.binding,
            &self.field,
            &self.test_name,
        )
    }
}

/// Class-text markers that let an attribute read or write run code, or let a
/// read reach the changed field under another name.
const CLASS_REFUSAL_MARKERS: [&str; 8] = [
    "__getattr",
    "__setattr",
    "__delattr",
    "__slots__",
    "__dict__",
    "__new__",
    "__class__",
    "property",
];

/// Test-text markers that can replace the owner, read the binding's
/// attributes by name, or rebind names dynamically.
const TEST_REFUSAL_MARKERS: [&str; 12] = [
    "patch",
    "mock",
    "Mock",
    "monkeypatch",
    "getattr",
    "setattr",
    "vars(",
    "__dict__",
    "globals(",
    "locals(",
    "exec(",
    "eval(",
];

/// An exact equality assertion on the changed field of the owner result the
/// one related test binds once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnerResultFieldRead {
    pub(super) test_name: String,
    pub(super) test_file: std::path::PathBuf,
    pub(super) assertion_text: String,
}

/// How the test may use the changed field of its binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldUse {
    /// Never named anywhere in the test.
    Unread,
    /// Read only, with one top-level `assert <binding>.<field> == <literal>`.
    ExactRead,
}

/// The fact for a changed `self.<field>` write at `line` of `owner`, or
/// `None` whenever any part of the shape is not statically established.
pub(super) fn unread_owner_result_field(
    owner: &PythonOwner,
    owner_source: &str,
    line: usize,
    family: &ProbeFamily,
    related: &[RelatedTest],
    candidates: &[PythonRelatedCandidate<'_>],
) -> Option<UnreadOwnerResultField> {
    let (test, field) = owner_result_shape(owner, owner_source, line, family, related, candidates)?;
    let (binding, _) =
        binding_of_owner_result(test, owner, &owner.class_path, &field, FieldUse::Unread)?;
    Some(UnreadOwnerResultField {
        binding,
        field,
        test_name: test.name.clone(),
    })
}

/// The exact read of the changed field on the bound owner result, the shape
/// [`unread_owner_result_field`] asks the repair to add.
pub(super) fn owner_result_field_read(
    owner: &PythonOwner,
    owner_source: &str,
    line: usize,
    family: &ProbeFamily,
    related: &[RelatedTest],
    candidates: &[PythonRelatedCandidate<'_>],
) -> Option<OwnerResultFieldRead> {
    let (test, field) = owner_result_shape(owner, owner_source, line, family, related, candidates)?;
    let (_, assertion_text) =
        binding_of_owner_result(test, owner, &owner.class_path, &field, FieldUse::ExactRead)?;
    Some(OwnerResultFieldRead {
        test_name: test.name.clone(),
        test_file: test.file.clone(),
        assertion_text: assertion_text?,
    })
}

/// The one strong related test and the changed field, when the owner side of
/// the shape holds.
fn owner_result_shape<'t>(
    owner: &PythonOwner,
    owner_source: &str,
    line: usize,
    family: &ProbeFamily,
    related: &[RelatedTest],
    candidates: &[PythonRelatedCandidate<'t>],
) -> Option<(&'t PythonTest, String)> {
    if *family != ProbeFamily::FieldConstruction
        || owner.owner_kind != Some(OwnerKind::Method)
        || !owner.decorators.is_empty()
    {
        return None;
    }
    let class = owner.class_path.as_str();
    if !plain_identifier(class) || owner.qualified_name != format!("{class}.{}", owner.name) {
        return None;
    }
    let ([row], [candidate]) = (related, candidates) else {
        return None;
    };
    let test = candidate.test;
    if row.oracle_strength != OracleStrength::Strong
        || row.name != test.name
        || row.file != test.file
    {
        return None;
    }
    let field = changed_self_field(owner, owner_source, line)?;
    Some((test, field))
}

/// The field the changed line writes, when the owner is a plain method of a
/// plain top-level class, returns `self` (or is `__init__`), and the changed
/// line is the method's one top-level, unconditional write of `self.<field>`.
fn changed_self_field(owner: &PythonOwner, source: &str, line: usize) -> Option<String> {
    let Ok(Mod::Module(module)) = parse_module_result(&owner.file, source) else {
        return None;
    };
    let class_name = owner.class_path.as_str();
    let mut classes = module.body.iter().filter_map(|stmt| match stmt {
        Stmt::ClassDef(class) if class.name.as_str() == class_name => Some(class),
        _ => None,
    });
    let (Some(class), None) = (classes.next(), classes.next()) else {
        return None;
    };
    if count_bindings(&module.body, class_name) != 1
        || !class.bases.is_empty()
        || !class.keywords.is_empty()
        || !class.decorator_list.is_empty()
    {
        return None;
    }
    let class_text = text_of(source, class.range)?;
    if CLASS_REFUSAL_MARKERS
        .iter()
        .any(|marker| class_text.contains(marker))
    {
        return None;
    }
    // The class body binds the owner name exactly once, as the method.
    let mut methods = class.body.iter().filter_map(|stmt| match stmt {
        Stmt::FunctionDef(function) if function.name.as_str() == owner.name => Some(function),
        _ => None,
    });
    let (Some(method), None) = (methods.next(), methods.next()) else {
        return None;
    };
    if class
        .body
        .iter()
        .filter(|stmt| class_level_binds(stmt, &owner.name))
        .count()
        != 1
    {
        return None;
    }
    if !method.decorator_list.is_empty()
        || !method.args.posonlyargs.is_empty()
        || method.args.args.first().map(|arg| arg.def.arg.as_str()) != Some("self")
        || count_bindings(&method.body, "self") != 0
        || text_of(source, method.range)?.contains("yield")
    {
        return None;
    }
    let returns = count_returns(&method.body);
    if owner.name == "__init__" {
        if returns != 0 {
            return None;
        }
    } else if returns != 1
        || !matches!(method.body.last(), Some(Stmt::Return(ret)) if returns_self(ret))
    {
        return None;
    }
    // The changed line is one top-level `self.<field> = ...` statement.
    let changed = method
        .body
        .iter()
        .find(|stmt| line_of(source, stmt_start(stmt)) == Some(line))?;
    let Stmt::Assign(assign) = changed else {
        return None;
    };
    let [Expr::Attribute(target)] = assign.targets.as_slice() else {
        return None;
    };
    if !matches!(target.value.as_ref(), Expr::Name(name) if name.id.as_str() == "self") {
        return None;
    }
    let field = target.attr.to_string();
    if !plain_identifier(&field) || field.starts_with("__") {
        return None;
    }
    // No class-level binding of the field (a descriptor would intercept the
    // write) and exactly one write of `self.<field>` in the method.
    if class
        .body
        .iter()
        .any(|stmt| class_level_binds(stmt, &field))
    {
        return None;
    }
    let mut writes = 0usize;
    visit_exprs_in_statements(&method.body, &mut |expr| {
        if let Expr::Attribute(attribute) = expr
            && attribute.attr.as_str() == field
            && attribute.ctx != ExprContext::Load
            && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "self")
        {
            writes += 1;
        }
    });
    (writes == 1).then_some(field)
}

/// The local the test binds once to the owner's returned instance, when the
/// test reads only attributes of it and uses `field` as `field_use` allows,
/// with the exact field assertion's text for [`FieldUse::ExactRead`].
fn binding_of_owner_result(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
    field: &str,
    field_use: FieldUse,
) -> Option<(String, Option<String>)> {
    if test.framework != PythonFramework::Pytest
        || !test.decorators.is_empty()
        || !test.fixtures.is_empty()
        || test.parametrized
        || test.qualified_name != test.name
        || test.body_text.contains(field) != (field_use == FieldUse::ExactRead)
        || TEST_REFUSAL_MARKERS
            .iter()
            .any(|marker| test.body_text.contains(marker))
    {
        return None;
    }
    let source = dedent(&test.body_text);
    if !source.trim_start().starts_with("def ") {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<python-test-body>"), &source)
    else {
        return None;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return None;
    };
    let args = &function.args;
    if !args.posonlyargs.is_empty()
        || !args.args.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !function.decorator_list.is_empty()
    {
        return None;
    }
    let body = &function.body;
    // Straight-line statements only: no nested scope, branch, loop, context
    // manager, handler, import or `global`/`nonlocal` can hide a rebinding.
    if !body.iter().all(|stmt| {
        matches!(
            stmt,
            Stmt::Assign(_) | Stmt::Expr(_) | Stmt::Assert(_) | Stmt::Pass(_)
        )
    }) {
        return None;
    }
    let class_locals: Vec<String> = owner_class_locals(test, owner, class)
        .into_iter()
        .filter(|local| plain_identifier(local))
        .collect();
    let mut bindings = body.iter().filter_map(|stmt| {
        let Stmt::Assign(assign) = stmt else {
            return None;
        };
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            return None;
        };
        let class_local = owner_result_call_class(&assign.value, &owner.name)?;
        class_locals
            .iter()
            .any(|local| local == class_local)
            .then(|| (target.id.to_string(), class_local.to_string()))
    });
    let (Some((binding, class_local)), None) = (bindings.next(), bindings.next()) else {
        return None;
    };
    if binding == class_local || count_bindings(body, &binding) != 1 {
        return None;
    }
    let mut uses = BindingUses::default();
    visit_exprs_in_statements(body, &mut |expr| {
        uses.record(expr, &binding, &class_local, owner, field)
    });
    let owner_mentions_ok = owner.name == "__init__" || uses.owner_attribute_mentions == 1;
    let shape_ok = !uses.refused
        && uses.binding_stores == 1
        && uses.binding_loads == uses.other_attribute_reads + uses.field_reads
        && uses.class_local_loads == 1
        && uses.class_local_stores == 0
        && owner_mentions_ok;
    if !shape_ok {
        return None;
    }
    match field_use {
        FieldUse::Unread => {
            (uses.field_reads == 0 && uses.other_attribute_reads >= 1).then_some((binding, None))
        }
        FieldUse::ExactRead => {
            let mut exact = body
                .iter()
                .filter_map(|stmt| exact_field_assertion(stmt, &binding, field));
            let (Some(stmt), None) = (exact.next(), exact.next()) else {
                return None;
            };
            let text = text_of(&source, stmt.range())?.trim().to_string();
            (uses.field_reads == 1).then_some((binding, Some(text)))
        }
    }
}

/// `stmt` when it is `assert <binding>.<field> == <literal>` (either side),
/// with no message.
fn exact_field_assertion<'a>(stmt: &'a Stmt, binding: &str, field: &str) -> Option<&'a Stmt> {
    let Stmt::Assert(assert) = stmt else {
        return None;
    };
    if assert.msg.is_some() {
        return None;
    }
    let Expr::Compare(compare) = assert.test.as_ref() else {
        return None;
    };
    let ([ast::CmpOp::Eq], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return None;
    };
    let is_field = |expr: &Expr| {
        matches!(expr, Expr::Attribute(attribute)
            if attribute.attr.as_str() == field
                && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == binding))
    };
    let left = compare.left.as_ref();
    ((is_field(left) && is_literal(right)) || (is_literal(left) && is_field(right))).then_some(stmt)
}

/// A plain literal: a constant, or a negated numeric constant.
fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(_) => true,
        Expr::UnaryOp(unary) => {
            unary.op == ast::UnaryOp::USub && matches!(unary.operand.as_ref(), Expr::Constant(_))
        }
        _ => false,
    }
}

/// Counts of how the test body uses the binding, the class local and the
/// owner name.
#[derive(Default)]
struct BindingUses {
    refused: bool,
    binding_stores: usize,
    binding_loads: usize,
    other_attribute_reads: usize,
    field_reads: usize,
    class_local_loads: usize,
    class_local_stores: usize,
    owner_attribute_mentions: usize,
}

impl BindingUses {
    fn record(
        &mut self,
        expr: &Expr,
        binding: &str,
        class_local: &str,
        owner: &PythonOwner,
        field: &str,
    ) {
        match expr {
            Expr::Lambda(_) | Expr::NamedExpr(_) => self.refused = true,
            Expr::Name(name) if name.id.as_str() == binding => {
                if name.ctx == ExprContext::Load {
                    self.binding_loads += 1;
                } else {
                    self.binding_stores += 1;
                }
            }
            Expr::Name(name) if name.id.as_str() == class_local => {
                if name.ctx == ExprContext::Load {
                    self.class_local_loads += 1;
                } else {
                    self.class_local_stores += 1;
                }
            }
            Expr::Attribute(attribute) => {
                if attribute.attr.as_str() == owner.name {
                    self.owner_attribute_mentions += 1;
                }
                if matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == binding)
                {
                    let attr = attribute.attr.as_str();
                    if attribute.ctx != ExprContext::Load || attr.starts_with("__") {
                        self.refused = true;
                    } else if attr == field {
                        self.field_reads += 1;
                    } else {
                        self.other_attribute_reads += 1;
                    }
                }
            }
            // A method call on the binding may read the field internally.
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = call.func.as_ref()
                    && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == binding)
                {
                    self.refused = true;
                }
            }
            _ => {}
        }
    }
}

/// The class local of `value` when it is `Cls(...).method(...)` for the owner
/// method, or `Cls(...)` for an `__init__` owner.
fn owner_result_call_class<'a>(value: &'a Expr, owner_name: &str) -> Option<&'a str> {
    let Expr::Call(call) = value else {
        return None;
    };
    if owner_name == "__init__" {
        return match call.func.as_ref() {
            Expr::Name(name) => Some(name.id.as_str()),
            _ => None,
        };
    }
    let Expr::Attribute(method) = call.func.as_ref() else {
        return None;
    };
    if method.attr.as_str() != owner_name {
        return None;
    }
    let Expr::Call(construction) = method.value.as_ref() else {
        return None;
    };
    match construction.func.as_ref() {
        Expr::Name(name) => Some(name.id.as_str()),
        _ => None,
    }
}

/// Whether a class-body statement binds `name` at class level.
fn class_level_binds(stmt: &Stmt, name: &str) -> bool {
    let target = |expr: &Expr| matches!(expr, Expr::Name(bound) if bound.id.as_str() == name);
    match stmt {
        Stmt::FunctionDef(function) => function.name.as_str() == name,
        Stmt::AsyncFunctionDef(function) => function.name.as_str() == name,
        Stmt::ClassDef(class) => class.name.as_str() == name,
        Stmt::Assign(assign) => assign.targets.iter().any(target),
        Stmt::AnnAssign(assign) => target(&assign.target),
        Stmt::AugAssign(assign) => target(&assign.target),
        // Anything else at class level (imports, branches, loops) is not
        // read; refuse rather than guess what it binds.
        Stmt::Expr(_) | Stmt::Pass(_) => false,
        _ => true,
    }
}

/// Return statements of a function body, not counting nested scopes.
fn count_returns(body: &[Stmt]) -> usize {
    body.iter()
        .map(|stmt| match stmt {
            Stmt::Return(_) => 1,
            Stmt::If(stmt) => count_returns(&stmt.body) + count_returns(&stmt.orelse),
            Stmt::For(stmt) => count_returns(&stmt.body) + count_returns(&stmt.orelse),
            Stmt::AsyncFor(stmt) => count_returns(&stmt.body) + count_returns(&stmt.orelse),
            Stmt::While(stmt) => count_returns(&stmt.body) + count_returns(&stmt.orelse),
            Stmt::With(stmt) => count_returns(&stmt.body),
            Stmt::AsyncWith(stmt) => count_returns(&stmt.body),
            Stmt::Try(stmt) => {
                try_returns(&stmt.body, &stmt.handlers, &stmt.orelse, &stmt.finalbody)
            }
            Stmt::TryStar(stmt) => {
                try_returns(&stmt.body, &stmt.handlers, &stmt.orelse, &stmt.finalbody)
            }
            Stmt::Match(stmt) => stmt
                .cases
                .iter()
                .map(|case| count_returns(&case.body))
                .sum(),
            _ => 0,
        })
        .sum()
}

fn try_returns(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) -> usize {
    count_returns(body)
        + count_returns(orelse)
        + count_returns(finalbody)
        + handlers
            .iter()
            .map(|ast::ExceptHandler::ExceptHandler(handler)| count_returns(&handler.body))
            .sum::<usize>()
}

fn returns_self(ret: &ast::StmtReturn) -> bool {
    matches!(ret.value.as_deref(), Some(Expr::Name(name)) if name.id.as_str() == "self")
}

fn stmt_start(stmt: &Stmt) -> usize {
    usize::from(stmt.range().start())
}

fn text_of(source: &str, range: TextRange) -> Option<&str> {
    source.get(usize::from(range.start())..usize::from(range.end()))
}

/// The 1-based line of byte `offset` in `source`.
fn line_of(source: &str, offset: usize) -> Option<usize> {
    Some(source.get(..offset)?.matches('\n').count() + 1)
}

fn plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn dedent(source: &str) -> String {
    let indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    source
        .lines()
        .map(|line| line.get(indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}
