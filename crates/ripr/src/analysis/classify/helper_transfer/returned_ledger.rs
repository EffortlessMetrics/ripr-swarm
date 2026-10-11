//! Identity-only transfer for a helper mutating a caller-local ledger.
//! This establishes a returned binding, never the position of a pushed element.

use super::{HelperChain, caller_rebinds_parameter};
use crate::analysis::facts::FunctionSummary;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericParams, HasName},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReturnedLedger {
    pub(crate) parameter: String,
    pub(crate) local: String,
    pub(crate) caller: crate::domain::SymbolId,
}

/// The first resolved hop borrows a local that its caller returns unchanged
/// after the call. A field initializer must be directly inside a pushed record
/// on a mutable owner parameter. The push itself remains a candidate edge:
/// neither method spelling nor a return proves which element a test observes.
pub(crate) fn returned_ledger(
    owner: &FunctionSummary,
    probe: &Probe,
    chain: &HelperChain,
) -> Option<ReturnedLedger> {
    if probe.family != ProbeFamily::FieldConstruction || probe.owner.as_ref() != Some(&owner.id) {
        return None;
    }
    let owner_parse = parse_clean_source_file(&owner.body)?;
    let function = owner_parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let changed_line = probe.location.line.checked_sub(owner.start_line)?;
    let field = function
        .body()?
        .syntax()
        .descendants()
        .filter_map(ast::RecordExprField::cast)
        .find(|field| {
            let offset = usize::from(field.syntax().text_range().start());
            owner.body[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                == changed_line
                && field_identity(&field.syntax().text().to_string())
                    == field_identity(&probe.expression)
        })?;
    if function.async_token().is_some()
        || field.syntax().ancestors().find_map(ast::Fn::cast).as_ref() != Some(&function)
        || field.syntax().ancestors().any(|node| {
            ast::ClosureExpr::can_cast(node.kind())
                || ast::BlockExpr::cast(node).is_some_and(|block| block.async_token().is_some())
        })
    {
        return None;
    }
    let literal = field.syntax().ancestors().find_map(ast::RecordExpr::cast)?;
    let arguments = literal.syntax().parent().and_then(ast::ArgList::cast)?;
    let call = arguments
        .syntax()
        .parent()
        .and_then(ast::MethodCallExpr::cast)?;
    if call.name_ref()?.text() != "push" || arguments.args().count() != 1 {
        return None;
    }
    let parameter = call.receiver()?.syntax().text().to_string();
    let parameters = function.param_list()?.params().collect::<Vec<_>>();
    let position = parameters.iter().position(|param| {
        param
            .pat()
            .is_some_and(|pat| pat.syntax().text() == parameter.as_str())
            && param
                .ty()
                .is_some_and(|ty| canonical(&ty.syntax().text().to_string()).starts_with("&mut"))
    })?;
    if caller_rebinds_parameter(&owner.body, &parameter) {
        return None;
    }
    let hop = chain.hops.first()?;
    if owner.file != hop.caller.file
        || owner.id.0.rsplit_once("::")?.0 != hop.caller.id.0.rsplit_once("::")?.0
        || function.generic_param_list().is_some()
    {
        return None;
    }
    let target_type = parameters.get(position)?.ty()?;
    let target_type = ast::RefType::cast(target_type.syntax().clone())?.ty()?;
    let argument = canonical(hop.arguments.get(position)?);
    let local = argument.strip_prefix("&mut")?;
    if !super::is_identifier(local) {
        return None;
    }
    let caller_parse = parse_clean_source_file(&hop.caller.body)?;
    let caller_fn = caller_parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if caller_fn.async_token().is_some()
        || caller_fn.generic_param_list().is_some()
        || canonical(&caller_fn.ret_type()?.ty()?.syntax().text().to_string())
            != canonical(&target_type.syntax().text().to_string())
    {
        return None;
    }
    let body = caller_fn.body()?;
    // A block import (including a glob) can shadow this unqualified callee.
    // This bounded transfer does not resolve imports, so refuse them.
    if body
        .syntax()
        .descendants()
        .any(|node| ast::Use::can_cast(node.kind()))
        || body
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .any(|nested| {
                nested
                    .name()
                    .is_some_and(|name| name.text() == owner.name.as_str())
            })
        || body
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .any(|pat| {
                pat.name()
                    .is_some_and(|name| name.text() == owner.name.as_str())
            })
    {
        return None;
    }
    if body.tail_expr()?.syntax().text() != local {
        return None;
    }
    // A single local binding and no assignment prevent shadow/overwrite from
    // supplying a different ledger to the call or return. The call is the last
    // statement in this block, so no subsequent write can discard its effect.
    let bindings = body
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pat| pat.name().is_some_and(|name| name.text() == local))
        .collect::<Vec<_>>();
    if bindings.len() != 1 || bindings[0].mut_token().is_none() {
        return None;
    }
    let declaration = bindings[0].syntax().parent().and_then(ast::LetStmt::cast)?;
    if declaration.syntax().parent().as_ref()
        != body.stmt_list().map(|list| list.syntax().clone()).as_ref()
    {
        return None;
    }
    if body
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .any(|expr| {
            expr.op_token().is_some_and(|token| {
                token.text().ends_with('=')
                    && token.text() != "=="
                    && token.text() != "!="
                    && token.text() != "<="
                    && token.text() != ">="
            }) && expr.lhs().is_some_and(|lhs| lhs.syntax().text() == local)
        })
    {
        return None;
    }
    let last = body.statements().last()?;
    let ast::Stmt::ExprStmt(statement) = last else {
        return None;
    };
    let ast::Expr::CallExpr(last_call) = statement.expr()? else {
        return None;
    };
    if last_call.expr()?.syntax().text() != owner.name.as_str() {
        return None;
    }
    let actual_arguments = last_call
        .arg_list()?
        .args()
        .map(|arg| canonical(&arg.syntax().text().to_string()))
        .collect::<Vec<_>>();
    if actual_arguments
        != hop
            .arguments
            .iter()
            .map(|arg| canonical(arg))
            .collect::<Vec<_>>()
    {
        return None;
    }
    Some(ReturnedLedger {
        parameter,
        local: local.to_string(),
        caller: hop.caller.id.clone(),
    })
}

fn field_identity(text: &str) -> String {
    crate::analysis::classify::propagation_witness::normalize_semantic_text(
        text.trim().trim_end_matches(','),
    )
}

fn canonical(text: &str) -> String {
    crate::analysis::language::mask_rust_comments_and_strings(text)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .trim_end_matches(',')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::classify::helper_transfer::HelperHop;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::domain::{DeltaKind, ProbeId, SourceLocation, SymbolId};
    use std::path::PathBuf;

    fn function(name: &str, body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn route(caller: &str) -> (FunctionSummary, Probe, HelperChain) {
        let owner = function(
            "record",
            "fn record(errors: &mut Vec<PolicyError>) {\n    errors.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    });\n}",
        );
        let probe = Probe {
            id: ProbeId("probe:ledger".to_string()),
            location: SourceLocation::new("src/lib.rs", 3, 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::FieldConstruction,
            delta: DeltaKind::Value,
            before: None,
            after: None,
            expression: "detail: format!(\"diagnostic\"),".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let chain = HelperChain {
            hops: vec![HelperHop {
                caller: function("collect", caller),
                call_text: "record(&mut errors);".to_string(),
                arguments: vec!["&mut errors".to_string()],
            }],
            stop_above: None,
        };
        (owner, probe, chain)
    }

    #[test]
    fn returned_ledger_establishes_the_mutable_argument_and_tail_binding_only() -> Result<(), String>
    {
        let (owner, probe, chain) = route(
            "fn collect() -> Vec<PolicyError> {\n    let mut errors = Vec::new();\n    record(&mut errors);\n    errors\n}",
        );
        let ledger =
            returned_ledger(&owner, &probe, &chain).ok_or("returned ledger not established")?;
        assert_eq!(ledger.parameter, "errors");
        assert_eq!(ledger.local, "errors");
        assert_eq!(ledger.caller, chain.hops[0].caller.id);
        let sinks = crate::analysis::classify::local_flow_sinks(&probe, Some(&owner));
        let mut witness = crate::analysis::classify::returned_ledger_witness(&probe, &sinks)
            .ok_or("local candidate witness missing")?;
        crate::analysis::classify::retain_returned_ledger(
            &mut witness,
            &owner.id,
            &ledger.parameter,
            &ledger.caller,
            &ledger.local,
        );
        assert!(witness.digest_matches());
        assert_eq!(
            witness.completeness,
            crate::analysis::classify::propagation_witness::PathCompleteness::Partial
        );
        assert!(
            !crate::analysis::classify::propagation_witness::complete_direct_witness(
                &probe,
                Some(&witness)
            )
        );
        Ok(())
    }

    #[test]
    fn discarded_wrong_overwritten_shadowed_and_modified_ledgers_have_no_transfer() {
        for caller in [
            "fn collect() { let mut errors = Vec::new(); record(&mut errors); }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); Vec::new() }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); let other = Vec::new(); record(&mut errors); other }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); errors = Vec::new(); record(&mut errors); errors }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); let errors = Vec::new(); errors }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors.clear(); errors }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); if true { record(&mut errors); } errors }",
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors.clone() }",
            "fn collect() -> Vec<PolicyError> { fn record(_: &mut Vec<PolicyError>) {} let mut errors = Vec::new(); record(&mut errors); errors }",
            "fn collect() -> Vec<PolicyError> { let record = |_| {}; let mut errors = Vec::new(); record(&mut errors); errors }",
            "fn collect() -> Vec<PolicyError> { use other::record; let mut errors = Vec::new(); record(&mut errors); errors }",
            "fn collect() -> Vec<PolicyError> { use other::*; let mut errors = Vec::new(); record(&mut errors); errors }",
            "async fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
            "fn collect() -> OtherLedger { let mut errors = OtherLedger::new(); record(&mut errors); errors }",
        ] {
            let (owner, probe, chain) = route(caller);
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{caller}"
            );
        }
    }

    #[test]
    fn deferred_owner_pushes_have_no_transfer() -> Result<(), String> {
        let (mut owner, mut probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        for source in [
            "async fn record(errors: &mut Vec<PolicyError>) {\n    errors.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    });\n}",
            "fn record(errors: &mut Vec<PolicyError>) {\n    let _ = || { errors.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    }); };\n}",
            "fn record(errors: &mut Vec<PolicyError>) {\n    let _ = async { errors.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    }); };\n}",
        ] {
            let offset = source.find("detail:").ok_or("fixture field missing")?;
            probe.location.line = owner.start_line
                + source[..offset]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count();
            owner.body = source.into();
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{source}"
            );
        }
        Ok(())
    }

    #[test]
    fn sibling_field_wrong_owner_and_non_parameter_push_have_no_transfer() {
        let (mut owner, mut probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        probe.expression = "section: format!(\"diagnostic\"),".to_string();
        assert!(returned_ledger(&owner, &probe, &chain).is_none());
        probe.expression = "detail: format!(\"diagnostic\"),".to_string();
        probe.owner = Some(SymbolId("src/lib.rs::unrelated".to_string()));
        assert!(returned_ledger(&owner, &probe, &chain).is_none());
        probe.owner = Some(owner.id.clone());
        owner.body = "fn record(errors: &mut Vec<PolicyError>) {\n    let mut wrong = Vec::new(); wrong.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    });\n}".into();
        assert!(returned_ledger(&owner, &probe, &chain).is_none());
    }
}
