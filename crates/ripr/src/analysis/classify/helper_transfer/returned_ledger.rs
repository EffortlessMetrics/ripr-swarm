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
    let hop = chain.hops.first().filter(|hop| !hop.through_macro)?;
    let owner_parse = parse_clean_source_file(&owner.body)?;
    let function = owner_parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let changed_line = probe.location.line.checked_sub(owner.start_line)?;
    let mut matching_fields = function
        .body()?
        .syntax()
        .descendants()
        .filter_map(ast::RecordExprField::cast)
        .filter(|field| {
            let offset = usize::from(field.syntax().text_range().start());
            owner.body[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                == changed_line
                && field_identity(&field.syntax().text().to_string())
                    == field_identity(&probe.expression)
        });
    // Probe locations retain a line, not an occurrence range. Identical fields
    // on that line cannot establish which receiver carries the changed value.
    let field = matching_fields.next()?;
    if matching_fields.next().is_some() {
        return None;
    }
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
                .and_then(|ty| ast::RefType::cast(ty.syntax().clone()))
                .is_some_and(|ty| ty.mut_token().is_some())
    })?;
    // The sole binding must be the mutable parameter, including raw spelling.
    // Opaque statement expansions may introduce a same-name local invisibly.
    if name_binding_count(&owner.body, &parameter)? != 1
        || has_opaque_bindings(&function.body()?)
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
    if name_binding_count(&hop.caller.body, &owner.name)? != 0 {
        return None;
    }
    let body = caller_fn.body()?;
    // Imports, value/item declarations and opaque statement expansions can
    // replace the unqualified callee. Refuse before establishing an owner edge.
    if has_opaque_bindings(&body) {
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
    let arguments = last_call.arg_list()?.args().collect::<Vec<_>>();
    // Mutability is a grammar token, never a whitespace-stripped prefix:
    // `&mut_errors` is shared, and `&mutLedger` names a shared reference type.
    let ast::Expr::RefExpr(borrowed) = arguments.get(position)? else {
        return None;
    };
    if borrowed.mut_token().is_none() || borrowed.raw_token().is_some() {
        return None;
    }
    let ast::Expr::PathExpr(path) = borrowed.expr()? else {
        return None;
    };
    let local_text = path.syntax().text().to_string();
    let local = local_text.as_str();
    if !super::is_identifier(local) {
        return None;
    }
    let actual_arguments = arguments
        .iter()
        .map(|arg| canonical(&arg.syntax().text().to_string()))
        .collect::<Vec<_>>();
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
    if name_binding_count(&hop.caller.body, local)? != 1
        || bindings.len() != 1
        || bindings[0].mut_token().is_none()
    {
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
    if actual_arguments
        != hop
            .arguments
            .iter()
            .map(|arg| canonical(arg))
            .collect::<Vec<_>>()
    {
        return None;
    }
    // Fact bodies start at `fn`, excluding qualifiers such as `async`. Bind
    // both definitions to the indexed source before claiming execution.
    if !synchronous_definitions(source, &[owner, &hop.caller]) {
        return None;
    }
    Some(ReturnedLedger {
        parameter,
        local: local.to_string(),
        caller: hop.caller.id.clone(),
    })
}

// Shared authority for each value-namespace endpoint: the helper receiver,
// caller callee and returned local. Imports are unresolved by the name counter;
// opaque statement/item expansions may introduce names absent from the AST.
fn name_binding_count(function: &str, identifier: &str) -> Option<usize> {
    let name = identifier.trim_start_matches("r#");
    Some(
        fn_name_binding_count(function, name)?
            + fn_name_binding_count(function, &format!("r#{name}"))?,
    )
}

fn has_opaque_bindings(body: &ast::BlockExpr) -> bool {
    if body
        .syntax()
        .descendants()
        .any(|node| ast::Use::can_cast(node.kind()))
    {
        return true;
    }
    body.syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .any(|call| {
            // A block-tail MacroExpr cannot introduce a binding visible to a
            // subsequent push/call: its expression block has already ended.
            // Keep expression tails such as `if ... { format!(...) }` allowed.
            let block_tail = call.syntax().parent().is_some_and(|parent| {
                ast::MacroExpr::can_cast(parent.kind())
                    && parent
                        .parent()
                        .is_some_and(|node| ast::StmtList::can_cast(node.kind()))
            });
            !block_tail && crate::analysis::syntax::macro_call_is_statement(&call)
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

fn synchronous_definitions(source: &str, facts: &[&FunctionSummary]) -> bool {
    let Some(parsed) = parse_clean_source_file(source) else {
        return false;
    };
    // File-level lint/doc attributes are inert for declaration identity. Other
    // inner attributes can exclude or transform this source scope; no cfg
    // evaluation or macro expansion is claimed here.
    if ast::attrs_including_inner(&parsed.tree()).any(|attr| {
        !matches!(
            attr.simple_name().as_deref(),
            Some("allow" | "warn" | "deny" | "forbid" | "doc")
        )
    }) {
        return false;
    }
    let mut declaration_scope = None;
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
        let Some(function) = matches.next() else {
            return false;
        };
        if matches.next().is_some()
            || function.async_token().is_some()
            || ast::attrs_including_inner(&function).next().is_some()
            || function
                .syntax()
                .ancestors()
                .filter_map(ast::Module::cast)
                .any(|module| ast::attrs_including_inner(&module).next().is_some())
        {
            return false;
        }
        // Free-function facts also include declarations inside const/block
        // expressions. Only direct, unconditional module items share the bare
        // call's namespace; associated/local declarations are unsupported.
        let Some(scope) = function.syntax().parent() else {
            return false;
        };
        if !(ast::SourceFile::can_cast(scope.kind())
            || (ast::ItemList::can_cast(scope.kind())
                && scope
                    .parent()
                    .is_some_and(|parent| ast::Module::can_cast(parent.kind()))))
            || declaration_scope
                .as_ref()
                .is_some_and(|previous| previous != &scope)
        {
            return false;
        }
        let count = scope
            .children()
            .filter_map(ast::AnyHasName::cast)
            .filter_map(|item| item.name())
            .filter(|name| {
                name.text().trim_start_matches("r#") == fact.name.trim_start_matches("r#")
            })
            .count();
        if count != 1 {
            return false;
        }
        declaration_scope = Some(scope);
        true
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
                through_macro: false,
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
    fn returned_ledger_requires_shared_lexical_module_scope() {
        let owner = function("record", "fn record(errors: &mut Vec<PolicyError>) {}");
        let caller = function("collect", "fn collect() {}");
        for source in [
            format!("{} {}", owner.body, caller.body),
            format!("mod same {{ {} {} }}", owner.body, caller.body),
            format!("#![warn(dead_code)] {} {}", owner.body, caller.body),
        ] {
            assert!(synchronous_definitions(&source, &[&owner, &caller]));
        }
        for source in [
            format!(
                "const record: fn(&mut Vec<PolicyError>) = |_| {{}}; const _: () = {{ {} }}; {}",
                owner.body, caller.body
            ),
            format!("static HIDDEN: () = {{ {} }}; {}", owner.body, caller.body),
            format!("fn outer() {{ {} }} {}", owner.body, caller.body),
            format!("impl Collector {{ {} {} }}", owner.body, caller.body),
            format!("mod a {{ {} }} mod b {{ {} }}", owner.body, caller.body),
            format!("#[rewrite] mod same {{ {} {} }}", owner.body, caller.body),
            format!(
                "mod same {{ #![cfg(any())] {} {} }}",
                owner.body, caller.body
            ),
            format!("#![cfg(any())] {} {}", owner.body, caller.body),
            format!(
                "{} {}",
                owner.body.replacen("{", "{ #![cfg(any())]", 1),
                caller.body
            ),
            format!(
                "{} {}",
                owner.body,
                caller.body.replacen("{", "{ #![cfg(any())]", 1)
            ),
            format!(
                "#[cfg_attr(feature = \"rewrite\", rewrite)] mod outer {{ mod same {{ {} {} }} }}",
                owner.body, caller.body
            ),
            format!(
                "#[cfg(any())] {} const record: fn(&mut Vec<PolicyError>) = |_| {{}}; {}",
                owner.body, caller.body
            ),
        ] {
            assert!(
                !synchronous_definitions(&source, &[&owner, &caller]),
                "{source}"
            );
        }
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
    fn same_line_field_identity_requires_a_unique_occurrence() {
        let (mut owner, mut probe, mut chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); let mut discarded = Vec::new(); record(&mut errors, &mut discarded); errors }",
        );
        chain.hops[0].arguments.push("&mut discarded".to_string());
        probe.location.line = owner.start_line;
        for sibling in ["diagnostic", "unrelated"] {
            owner.body = format!(
                "fn record(errors: &mut Vec<PolicyError>, discarded: &mut Vec<PolicyError>) {{ errors.push(PolicyError {{ detail: format!(\"diagnostic\"), }}); discarded.push(PolicyError {{ detail: format!(\"{sibling}\"), }}); }}"
            ).into();
            assert_eq!(
                returned_ledger(&owner, &probe, &chain).is_some(),
                sibling == "unrelated",
                "{sibling}"
            );
        }
    }

    #[test]
    fn shared_reference_type_prefixes_have_no_transfer() {
        for ty in ["mutLedger", "mutVec<PolicyError>", "mut_ledger"] {
            let caller = format!(
                "fn collect() -> {ty} {{ let mut errors = Vec::new(); record(&mut errors); errors }}"
            );
            let (mut owner, probe, chain) = route(&caller);
            owner.body = format!(
                "fn record(errors: &{ty}) {{\n    errors.push(PolicyError {{\n        detail: format!(\"diagnostic\"),\n    }});\n}}"
            ).into();
            assert!(returned_ledger(&owner, &probe, &chain).is_none(), "{ty}");
        }
        let (mut owner, probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        owner.body = owner
            .body
            .replace("&mut Vec", "& /*comment*/ mut Vec")
            .into();
        assert!(returned_ledger(&owner, &probe, &chain).is_some());
    }

    #[test]
    fn mutable_argument_identity_uses_reference_tokens() {
        for argument in ["&mut_errors", "&muterrors", "&raw mut errors", "&(errors)"] {
            let caller = format!(
                "fn collect() -> Vec<PolicyError> {{ let mut _errors = Vec::new(); let mut errors = Vec::new(); let mut_errors = Vec::new(); record({argument}); _errors }}"
            );
            let (owner, probe, mut chain) = route(&caller);
            chain.hops[0].arguments = vec![argument.to_string()];
            assert!(
                returned_ledger(&owner, &probe, &chain).is_none(),
                "{argument}"
            );
        }
        let (owner, probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(& /*comment*/ mut errors); errors }",
        );
        assert!(returned_ledger(&owner, &probe, &chain).is_some());
    }

    #[test]
    fn owner_expression_tail_macros_preserve_the_parameter_identity() -> Result<(), String> {
        let (mut owner, mut probe, chain) = route(
            "fn collect() -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }",
        );
        let source = "fn record(errors: &mut Vec<PolicyError>) {\n    let _message = if true { format!(\"other\") } else { format!(\"other\") };\n    errors.push(PolicyError {\n        detail: format!(\"diagnostic\"),\n    });\n}";
        let offset = source.find("detail:").ok_or("fixture field missing")?;
        probe.location.line = owner.start_line
            + source[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
        owner.body = source.into();
        assert!(returned_ledger(&owner, &probe, &chain).is_some());
        Ok(())
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
            "use crate::shadow::errors;",
            "use crate::shadow::*;",
            "use crate::shadow::other as errors;",
            "use crate::shadow::{errors};",
            "use crate::shadow::{other as errors};",
            "use crate::shadow::unrelated;",
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
