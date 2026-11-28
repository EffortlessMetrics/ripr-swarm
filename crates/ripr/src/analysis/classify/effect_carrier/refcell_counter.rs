//! Assertion-local receipt for a fresh standard RefCell<u32> unit counter.
//! This does not weaken the opaque-state refusal of EffectStateCarrier.
use super::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::classify::builder_override::{
    getter_dispatch_body, indexed_getter_body, inherent_method_body,
};
use crate::analysis::classify::owner_pin::{
    OwnerScope, test_receiver_named_type, wrapper_may_be_rebound,
};
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::{ImplicitExpansionContext, OwnerPinSyntax};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::{fn_signature::local_type_equality, parse_clean_source_file};
use crate::domain::{ExposureClass, Finding, Probe, ProbeFamily, ProbeId, StaticLimitKind};
use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind,
    ast::{self, HasArgList, HasGenericParams, HasName},
};

pub(in crate::analysis) struct RefCellCounter {
    ty: String,
    field: String,
}

/// Producer-only refusal: local source binding is present, while the shared
/// workspace expansion proof is unavailable. It never supplies sink identity.
pub(in crate::analysis) struct RefCellObserverDispatchRefusal {
    probe: ProbeId,
    test: String,
    reason: ObserverDispatchRefusalReason,
}

#[derive(Clone, Copy)]
enum ObserverDispatchRefusalReason {
    WorkspaceExpansionUnresolved,
}

fn getter_view<'a>(
    index: &'a RustIndex,
    ty: &str,
    name: &str,
    expansion: Option<&ImplicitExpansionContext<'_>>,
) -> Option<(&'a FunctionSummary, &'a str)> {
    match expansion {
        Some(expansion) => getter_dispatch_body(index, ty, name, expansion),
        None => indexed_getter_body(index, ty, name),
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}
fn function(text: &str) -> Option<ast::Fn> {
    parse_clean_source_file(text)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn expression(text: &str) -> Option<ast::Expr> {
    function(&format!("fn operand() {{ {text} }}"))?
        .body()?
        .stmt_list()?
        .tail_expr()
}
fn statement_expression(statement: &ast::Stmt) -> Option<ast::Expr> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    statement.expr()
}
fn bare(expr: ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expr else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}
fn borrowed_self_only(function: &ast::Fn) -> bool {
    function.async_token().is_none()
        && function.unsafe_token().is_none()
        && function.generic_param_list().is_none()
        && function.param_list().is_some_and(|params| {
            params.params().next().is_none()
                && params.self_param().is_some_and(|receiver| {
                    receiver.amp_token().is_some() && receiver.mut_token().is_none()
                })
        })
}
fn mentions(statement: &ast::Stmt, root: &str) -> bool {
    statement
        .syntax()
        .descendants_with_tokens()
        .filter_map(|node| node.into_token())
        .any(|token| {
            token.kind() == SyntaxKind::IDENT && token.text().trim_start_matches("r#") == root
        })
}

impl RefCellCounter {
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        complete: bool,
        expansion: &ImplicitExpansionContext<'_>,
    ) -> Option<Self> {
        Self::establish_view(probe, owner, index, complete, Some(expansion))
    }

    // None is used exclusively for the non-crediting named-refusal query.
    fn establish_view(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        complete: bool,
        expansion: Option<&ImplicitExpansionContext<'_>>,
    ) -> Option<Self> {
        // Most effect probes are not interior counters. Refuse their syntax
        // before consulting the whole dispatch/type surface.
        if !probe.expression.trim_start().starts_with('*')
            || !probe.expression.contains("borrow_mut")
        {
            return None;
        }
        if !complete
            || !matches!(probe.family, ProbeFamily::CallDeletion)
            || index.files().get(&owner.file)?.used_lexical_fallback
        {
            return None;
        }
        let FunctionContainer::Inherent { self_ty: ty } = &owner.item.container else {
            return None;
        };
        let (resolved, body) = getter_view(index, ty, &owner.name, expansion)?;
        if resolved.file != owner.file || resolved.start_line != owner.start_line {
            return None;
        }
        let method = function(body)?;
        if !borrowed_self_only(&method) || method.ret_type().is_some() {
            return None;
        }
        let list = method.body()?.stmt_list()?;
        let statements: Vec<_> = list.statements().collect();
        let [statement] = statements.as_slice() else {
            return None;
        };
        if list.tail_expr().is_some() {
            return None;
        }
        let selected = compact(&statement_expression(statement)?.syntax().text().to_string());
        if selected != compact(probe.expression.trim_end_matches(';')) {
            return None;
        }
        let field = selected
            .strip_prefix("*self.")?
            .strip_suffix(".borrow_mut()+=1u32")
            .or_else(|| {
                selected
                    .strip_prefix("*self.")?
                    .strip_suffix(".borrow_mut()+=1")
            })?;
        let source = &index.files().get(&owner.file)?.source;
        let shape = local_type_equality(source, ty)?;
        let [slot] = shape.fields.as_slice() else {
            return None;
        };
        if shape.generic_params
            || shape.other_attributes
            || slot.has_attributes
            || slot.name.as_deref() != Some(field)
            || compact(&slot.ty) != "RefCell<u32>"
            || ["RefCell", "u32", "borrow", "borrow_mut"]
                .into_iter()
                .any(|name| wrapper_may_be_rebound(name, index))
        {
            return None;
        }
        let tree = parse_clean_source_file(source)?.tree();
        if !crate::analysis::classify::implicit_effect::standard_handles_with_required(
            &tree,
            &["RefCell"],
        ) {
            return None;
        }
        Some(Self {
            ty: ty.clone(),
            field: field.to_string(),
        })
    }

    pub(in crate::analysis) fn admits(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        admission: (&OwnerPinSyntax, &ImplicitExpansionContext<'_>),
    ) -> bool {
        self.bound(
            owner,
            index,
            test,
            assertion,
            (admission.0, Some(admission.1)),
        )
        .is_some()
    }

    pub(in crate::analysis) fn observer_dispatch_refusal(
        context: &crate::analysis::classify::ProbeContext<'_>,
        test: &TestSummary,
        assertion: &OracleFact,
        admission: (&OwnerPinSyntax, &ImplicitExpansionContext<'_>),
    ) -> Option<RefCellObserverDispatchRefusal> {
        if crate::analysis::classify::is_ignored(test)
            || test
                .attrs
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(test.body.as_str()))
                .any(|text| text.contains("should_panic"))
        {
            return None;
        }
        let owner = context.owner_fn?;
        let local = Self::establish_view(
            context.probe,
            owner,
            context.index,
            context.workspace_complete,
            None,
        )?;
        if !admission.1.getter_expansion_unresolved_only(&local.ty) {
            return None;
        }
        local.bound(owner, context.index, test, assertion, (admission.0, None))?;
        Some(RefCellObserverDispatchRefusal {
            probe: context.probe.id.clone(),
            test: format!("{} ({}:{})", test.name, test.file.display(), assertion.line),
            reason: ObserverDispatchRefusalReason::WorkspaceExpansionUnresolved,
        })
    }

    /// Same named-gap-withholding mechanism as the other Rust limitations;
    /// only typed, exact-probe local observer refusals license this rewrite.
    pub(in crate::analysis) fn withhold_observer_dispatch_gap(
        finding: &mut Finding,
        refusals: &[RefCellObserverDispatchRefusal],
    ) {
        if finding.class != ExposureClass::WeaklyExposed || finding.static_limit_kind.is_some() {
            return;
        }
        let judged: Vec<String> = refusals
            .iter()
            .filter(|refusal| {
                refusal.probe == finding.probe.id
                    && matches!(
                        refusal.reason,
                        ObserverDispatchRefusalReason::WorkspaceExpansionUnresolved
                    )
            })
            .map(|refusal| refusal.test.clone())
            .collect();
        if judged.is_empty() {
            return;
        }
        super::super::gap_admission::withhold_gap(
            finding,
            StaticLimitKind::RustObserverDispatchUnresolved,
            "whether the locally matched counter getter is the method executed under the unread workspace expansion surface",
            "A related test already contains an independently expected counter observation. Resolve getter dispatch and workspace expansion, then use real verification to determine whether the existing assertion detects the changed write; no missing or weak assertion is established.".to_string(),
            &judged,
        );
    }

    fn bound(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        admission: (&OwnerPinSyntax, Option<&ImplicitExpansionContext<'_>>),
    ) -> Option<()> {
        let (syntax, expansion) = admission;
        if !syntax.admits_effect_assertion(test, assertion, index) {
            return None;
        }
        let text = assertion.text.trim().trim_start_matches("::");
        let text = text
            .strip_prefix("std::")
            .or_else(|| text.strip_prefix("core::"))
            .unwrap_or(text);
        if !text.starts_with("assert_eq!") {
            return None;
        }
        let operands = assertion_comparison_operands(&assertion.text)?;
        let pair: (String, String, usize) =
            [(operands[0], operands[1]), (operands[1], operands[0])]
                .into_iter()
                .find_map(|(actual, expected)| {
                    let ast::Expr::MethodCallExpr(read) = expression(actual)? else {
                        return None;
                    };
                    if read.arg_list()?.args().next().is_some() {
                        return None;
                    }
                    let root = bare(read.receiver()?)?;
                    let getter = read.name_ref()?.text().to_string();
                    let ast::Expr::Literal(literal) = expression(expected)? else {
                        return None;
                    };
                    let ast::LiteralKind::IntNumber(number) = literal.kind() else {
                        return None;
                    };
                    let expected = number
                        .text()
                        .strip_suffix("u32")
                        .unwrap_or(number.text())
                        .replace('_', "")
                        .parse::<usize>()
                        .ok()?;
                    (expected > 0 && expected <= 16).then_some((root, getter, expected))
                })?;
        let (root, getter, expected) = pair;
        let (_, getter_body) = getter_view(index, &self.ty, &getter, expansion)?;
        let method = function(getter_body)?;
        let list = method.body()?.stmt_list()?;
        if !borrowed_self_only(&method)
            || list.statements().next().is_some()
            || compact(&method.ret_type()?.ty()?.syntax().text().to_string()) != "u32"
            || compact(&list.tail_expr()?.syntax().text().to_string())
                != format!("*self.{}.borrow()", self.field)
        {
            return None;
        }
        if test_receiver_named_type(
            test,
            &root,
            OwnerScope::new(&owner.name, owner.start_line, &owner.file),
            index,
        )
        .as_deref()
            != Some(self.ty.as_str())
        {
            return None;
        }
        let method = function(&test.body)?;
        let statements: Vec<_> = method.body()?.stmt_list()?.statements().collect();
        let assertion_at = statements.iter().position(|statement| {
            let at = usize::from(statement.syntax().text_range().start());
            test.start_line + test.body[..at].matches('\n').count() == assertion.line
                && compact(&statement.syntax().text().to_string()).trim_end_matches(';')
                    == compact(&assertion.text).trim_end_matches(';')
        })?;
        let uses: Vec<_> = statements[..assertion_at]
            .iter()
            .filter(|s| mentions(s, &root))
            .collect();
        if uses.len() != assertion_at {
            return None;
        }
        let (definition, calls) = uses.split_first()?;
        if calls.len() != expected {
            return None;
        }
        let ast::Stmt::LetStmt(definition) = definition else {
            return None;
        };
        let ast::Pat::IdentPat(binding) = definition.pat()? else {
            return None;
        };
        if binding.name()?.text() != root.as_str()
            || binding.ref_token().is_some()
            || binding.pat().is_some()
            || definition.let_else().is_some()
        {
            return None;
        }
        let ast::Expr::CallExpr(ctor) = definition.initializer()? else {
            return None;
        };
        if ctor.arg_list()?.args().next().is_some() {
            return None;
        }
        let ctor_path = compact(&ctor.expr()?.syntax().text().to_string());
        let (ty, name) = ctor_path.split_once("::")?;
        if ty != self.ty || name.contains("::") {
            return None;
        }
        let ctor_body = inherent_method_body(index, ty, name)?;
        let definition = index.functions().iter().find(|candidate| {
            candidate.body.as_str() == ctor_body
                && candidate.name == name
                && matches!(&candidate.item.container,
                    FunctionContainer::Inherent { self_ty } if self_ty == ty)
        })?;
        if !definition.attrs.is_empty() || !definition.impl_attrs.is_empty() {
            return None;
        }
        let ctor = function(ctor_body)?;
        let list = ctor.body()?.stmt_list()?;
        let parameters = ctor.param_list()?;
        if parameters.params().next().is_some() || parameters.self_param().is_some() {
            return None;
        }
        let returned = compact(&ctor.ret_type()?.ty()?.syntax().text().to_string());
        if returned != self.ty && returned != "Self" {
            return None;
        }
        if list.statements().next().is_some()
            || ctor.async_token().is_some()
            || ctor.unsafe_token().is_some()
            || ctor.generic_param_list().is_some()
            || compact(&list.tail_expr()?.syntax().text().to_string())
                != format!("{}{{{}:RefCell::new(0)}}", self.ty, self.field)
        {
            return None;
        }
        calls
            .iter()
            .all(|statement| {
                let Some(ast::Expr::MethodCallExpr(call)) = statement_expression(statement) else {
                    return false;
                };
                call.name_ref()
                    .is_some_and(|name| name.text() == owner.name)
                    && call.receiver().and_then(bare).as_deref() == Some(root.as_str())
                    && call
                        .arg_list()
                        .is_some_and(|args| args.args().next().is_none())
            })
            .then_some(())
    }
}
