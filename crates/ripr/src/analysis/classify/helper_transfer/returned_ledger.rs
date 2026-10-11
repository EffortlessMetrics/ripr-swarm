//! Identity-only transfer for a helper mutating a caller-local ledger.
//! This establishes a returned binding, never the position of a pushed element.

use super::{HelperChain, caller_rebinds_parameter};
use crate::analysis::facts::FunctionSummary;
use crate::analysis::syntax::{fn_name_binding_count, parse_clean_source_file};
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
    source: &str,
) -> Option<ReturnedLedger> {
    if probe.family != ProbeFamily::FieldConstruction || probe.owner.as_ref() != Some(&owner.id) {
        return None;
    }
    let hop = chain.hops.first()?;
    // Fact bodies start at `fn`, excluding qualifiers such as `async`. Bind
    // both definitions to the indexed source before claiming execution.
    if !synchronous_definitions(source, &[owner, &hop.caller]) {
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
    // The sole binding must be the mutable parameter, including raw spelling.
    // Opaque statement expansions may introduce a same-name local invisibly.
    let name = parameter.trim_start_matches("r#");
    if fn_name_binding_count(&owner.body, name)?
        + fn_name_binding_count(&owner.body, &format!("r#{name}"))?
        != 1
        || has_statement_macro(&function.body()?)
        || caller_rebinds_parameter(&owner.body, &parameter)
    {
        return None;
    }
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
    // Use the existing full-function binding authority: signature patterns,
    // all body patterns and named items can replace this unqualified callee.
    // Check both source spellings of a raw-capable identifier, failing closed
    // when the authority cannot parse the declaration.
    let name = owner.name.trim_start_matches("r#");
    if fn_name_binding_count(&hop.caller.body, name)? != 0
        || fn_name_binding_count(&hop.caller.body, &format!("r#{name}"))? != 0
    {
        return None;
    }
    let body = caller_fn.body()?;
    // Imports, value/item declarations and opaque statement expansions can
    // replace the unqualified callee. Refuse before establishing an owner edge.
    if body
        .syntax()
        .descendants()
        .any(|node| ast::Use::can_cast(node.kind()))
        || has_statement_macro(&body)
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

fn has_statement_macro(body: &ast::BlockExpr) -> bool {
    body.syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .any(|call| crate::analysis::syntax::macro_call_is_statement(&call))
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

fn synchronous_definitions(source: &str, facts: &[&FunctionSummary]) -> bool {
    let Some(parsed) = parse_clean_source_file(source) else {
        return false;
    };
    facts.iter().all(|fact| {
        let mut matches = parsed
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                let Some(token) = function.fn_token() else {
                    return false;
                };
                let start = usize::from(token.text_range().start());
                let end = usize::from(function.syntax().text_range().end());
                function
                    .name()
                    .is_some_and(|name| name.text() == fact.name.as_str())
                    && source[..start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                        + 1
                        == fact.start_line
                    && source.get(start..end) == Some(fact.body.as_str())
            });
        // Same-line identical siblings have indistinguishable fact bodies.
        // Reject ambiguity before checking the matched declaration's qualifier.
        matches
            .next()
            .is_some_and(|function| function.async_token().is_none())
            && matches.next().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::classify::helper_transfer::HelperHop;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::domain::{DeltaKind, ProbeId, SourceLocation, SymbolId};
    use std::path::PathBuf;

    fn returned_ledger(
        owner: &FunctionSummary,
        probe: &Probe,
        chain: &HelperChain,
    ) -> Option<ReturnedLedger> {
        let source = format!("{}\n{}", owner.body, chain.hops.first()?.caller.body);
        let mut chain = chain.clone();
        chain.hops.first_mut()?.caller.start_line = owner.body.lines().count() + 1;
        super::returned_ledger(owner, probe, &chain, &source)
    }

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
    fn returned_ledger_requires_unique_source_declarations() {
        let owner = function("record", "fn record(errors: &mut Vec<PolicyError>) {}");
        let source = format!(
            "mod a {{ async {} }} mod b {{ {} }}",
            owner.body, owner.body
        );
        assert!(!synchronous_definitions(&source, &[&owner]));
        let source = format!(
            "mod a {{ {} }} mod b {{ async {} }}",
            owner.body, owner.body
        );
        assert!(!synchronous_definitions(&source, &[&owner]));
        assert!(synchronous_definitions(&owner.body, &[&owner]));
    }

    #[test]
    fn returned_ledger_refuses_async_qualifiers_omitted_from_fact_bodies() -> Result<(), String> {
        let (mut owner, probe, chain) = route(
            "fn collect() -> Vec<PolicyError> {\n    let mut errors = Vec::new();\n    record(&mut errors);\n    errors\n}",
        );
        let source = format!("async {}\n{}", owner.body, chain.hops[0].caller.body);
        let mut chain = chain;
        chain.hops[0].caller.start_line = owner.body.lines().count() + 1;
        assert!(super::returned_ledger(&owner, &probe, &chain, &source).is_none());
        let source = format!("{}\nasync {}", owner.body, chain.hops[0].caller.body);
        assert!(super::returned_ledger(&owner, &probe, &chain, &source).is_none());
        owner.start_line += 1;
        assert!(super::returned_ledger(&owner, &probe, &chain, &source).is_none());
        Ok(())
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
    fn caller_signature_pattern_shadows_have_no_transfer() {
        for parameters in [
            "record: fn(&mut Vec<PolicyError>)",
            "r#record: fn(&mut Vec<PolicyError>)",
            "mut record: fn(&mut Vec<PolicyError>)",
            "(record, _): (fn(&mut Vec<PolicyError>), bool)",
            "(r#record, _): (fn(&mut Vec<PolicyError>), bool)",
            "Callbacks { record }: Callbacks",
            "Callbacks { r#record }: Callbacks",
            "Callbacks { other: record }: Callbacks",
        ] {
            let caller = format!(
                "fn collect({parameters}) -> Vec<PolicyError> {{ let mut errors = Vec::new(); record(&mut errors); errors }}"
            );
            let (owner, probe, chain) = route(&caller);
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{parameters}"
            );
        }
        for parameters in [
            "callback: fn(&mut Vec<PolicyError>)",
            "Callbacks { record: callback }: Callbacks",
        ] {
            let caller = format!(
                "fn collect({parameters}) -> Vec<PolicyError> {{ let mut errors = Vec::new(); record(&mut errors); errors }}"
            );
            let (owner, probe, chain) = route(&caller);
            assert!(
                returned_ledger(&owner, &probe, &chain).is_some(),
                "{parameters}"
            );
        }
    }

    #[test]
    fn callee_item_and_macro_shadows_have_no_transfer() {
        for declaration in [
            "const record: fn(&mut Vec<PolicyError>) = |_| {};",
            "static record: fn(&mut Vec<PolicyError>) = |_| {};",
            "const r#record: fn(&mut Vec<PolicyError>) = |_| {};",
            "macro_rules! bind { ($name:ident) => { let $name: fn(&mut Vec<PolicyError>) = |_| {}; }; } bind!(record);",
            "macro_rules! bind { ($name:ident) => { static $name: fn(&mut Vec<PolicyError>) = |_| {}; }; } bind!{record}",
            "opaque!();",
        ] {
            let caller = format!(
                "fn collect() -> Vec<PolicyError> {{ {declaration} let mut errors = Vec::new(); record(&mut errors); errors }}"
            );
            let (owner, probe, chain) = route(&caller);
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{declaration}"
            );
        }
        // Formatting inside an expression does not add a caller-scope binding.
        let (owner, probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let _message = format!(\"diagnostic\"); let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        assert!(returned_ledger(&owner, &probe, &chain).is_some());
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
    fn owner_parameter_shadows_have_no_transfer() -> Result<(), String> {
        let (mut owner, mut probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        for declaration in [
            "let mut errors = Vec::new();",
            "let mut r#errors = Vec::new();",
            "let (mut errors, _) = (Vec::new(), true);",
            "let Ledgers { mut errors } = ledgers;",
            "macro_rules! bind { ($name:ident) => { let mut $name = Vec::new(); }; } bind!(errors);",
            "opaque!();",
        ] {
            let source = format!(
                "fn record(errors: &mut Vec<PolicyError>) {{ {declaration}\n    errors.push(PolicyError {{\n        detail: format!(\"diagnostic\"),\n    }});\n}}"
            );
            let offset = source.find("detail:").ok_or("fixture field missing")?;
            probe.location.line = owner.start_line
                + source[..offset]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count();
            owner.body = source.into();
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{declaration}"
            );
        }
        Ok(())
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
