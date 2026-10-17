//! A single primitive field read through one exact free-function wrapper.
//!
//! This receipt extends the existing local-field witness with observer identity;
//! it does not claim that the wrapper returns the owner's whole record.

use super::{callee_is_unique, helper_only_reach, is_exact_call, split_call_arguments_text};
use crate::analysis::classify::owner_pin::{
    field_builtin_integer, targeted_free_call_reaches_owner,
};
use crate::analysis::classify::propagation_witness::{
    PropagationWitnessV1, complete_direct_witness, normalize_semantic_text,
};
use crate::analysis::classify::{OwnerPinSyntax, ProbeContext};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, OracleFact, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{OracleKind, OracleStrength, Probe, ProbeFamily, RelationReason, SymbolId};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericParams, HasName},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectionAssertion {
    file: PathBuf,
    test: String,
    test_line: usize,
    line: usize,
    text: String,
}

/// Private producer receipt, carried by `PropagationWitnessV1`. Every credited
/// assertion is the same admitted assertion that reads this exact field path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct FieldProjectionReceipt {
    owner: SymbolId,
    helper: SymbolId,
    expression: String,
    line: usize,
    field: String,
    call: String,
    source_hash: String,
    assertions: Vec<ProjectionAssertion>,
    pub(in crate::analysis) semantic_digest: String,
    validation_digest: String,
}

fn digest(fields: &[String]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("sha256:{:x}", hash.finalize())
}

impl FieldProjectionReceipt {
    fn semantic_fields(&self) -> Vec<String> {
        let mut fields = vec![
            "one-hop-usize-field-v1".to_owned(),
            self.owner.0.clone(),
            self.helper.0.clone(),
            self.expression.clone(),
            self.field.clone(),
            self.call.clone(),
        ];
        for assertion in &self.assertions {
            fields.extend([
                assertion.file.to_string_lossy().into_owned(),
                assertion.test.clone(),
                normalize_semantic_text(&assertion.text),
            ]);
        }
        fields
    }

    fn validation_fields(&self) -> Vec<String> {
        let mut fields = self.semantic_fields();
        fields.extend([self.source_hash.clone(), self.line.to_string()]);
        for assertion in &self.assertions {
            fields.extend([
                assertion.test_line.to_string(),
                assertion.line.to_string(),
                assertion.text.clone(),
            ]);
        }
        fields
    }

    pub(in crate::analysis) fn valid_for(&self, probe: &Probe) -> bool {
        probe.family == ProbeFamily::FieldConstruction
            && probe.delta.as_str() == "value"
            && probe.owner.as_ref() == Some(&self.owner)
            && self.line == probe.location.line
            && self.expression == normalize_semantic_text(&probe.expression)
            && !self.assertions.is_empty()
            && self.semantic_digest == digest(&self.semantic_fields())
            && self.validation_digest == digest(&self.validation_fields())
    }

    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        self.assertions.iter().any(|selected| {
            selected.file == test.file
                && selected.test == test.name
                && selected.test_line == test.start_line
                && selected.line == assertion.line
                && selected.text == assertion.text
        })
    }
}

/// Establish a concrete field/call/observer path. Unsupported syntax retains
/// the existing helper-result refusal; an unrelated strong oracle supplies none
/// of the receipt's identity.
pub(in crate::analysis) fn field_projection_receipt(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
    witness: &PropagationWitnessV1,
) -> Option<FieldProjectionReceipt> {
    let probe = context.probe;
    if probe.family != ProbeFamily::FieldConstruction
        || probe.delta.as_str() != "value"
        || !context.workspace_complete
        || !helper_only_reach(&context.related_tests)
        || !complete_direct_witness(probe, Some(witness))
    {
        return None;
    }
    let owner = context.owner_fn?;
    let chain = context.helper_chain.as_ref()?;
    let [hop] = chain.hops.as_slice() else {
        return None;
    };
    let helper = &hop.caller;
    if hop.through_macro
        || helper.source_role.is_evidence_role()
        || owner.file != helper.file
        || !callee_is_unique(&owner.name, context.index)
        || !callee_is_unique(&helper.name, context.index)
    {
        return None;
    }
    let facts = context.index.files().get(&owner.file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let parse = parse_clean_source_file(&facts.source)?;
    let file = parse.tree();
    let owner_fn = root_function(&file, owner)?;
    let helper_fn = root_function(&file, helper)?;
    let owner_parameters = usize_parameters(&owner_fn)?;
    let helper_parameters = usize_parameters(&helper_fn)?;
    if owner_parameters.len() != helper_parameters.len() || return_type(&helper_fn)? != "usize" {
        return None;
    }
    let ast::Expr::RecordExpr(record) = sole_tail(&owner_fn)? else {
        return None;
    };
    let type_name = record.path()?.syntax().text().to_string();
    if !super::is_identifier(&type_name) || return_type(&owner_fn)? != type_name {
        return None;
    }
    let list = record.record_expr_field_list()?;
    if list.spread().is_some() || list.dotdot_token().is_some() {
        return None;
    }
    let mut changed_field = None;
    let mut names = std::collections::BTreeSet::new();
    for field in list.fields() {
        let name = field.field_name()?.text().to_string();
        if !names.insert(name.clone())
            || field.attrs().next().is_some()
            || field_builtin_integer(owner, &type_name, &name, context.index, syntax)? != "usize"
            || !primitive_expression(&field.expr()?, &owner_parameters, 16)
        {
            return None;
        }
        let line = 1 + facts
            .source
            .get(..usize::from(field.syntax().text_range().start()))?
            .matches('\n')
            .count();
        if line == probe.location.line
            && squeeze(&field.syntax().text().to_string()) == squeeze(&probe.expression)
        {
            changed_field = Some(name);
        }
    }
    let field = changed_field?;
    let ast::Expr::FieldExpr(projection) = sole_tail(&helper_fn)? else {
        return None;
    };
    if projection.name_ref()?.text() != field {
        return None;
    }
    let ast::Expr::CallExpr(call) = projection.expr()? else {
        return None;
    };
    let call_text = call.syntax().text().to_string();
    if call.expr()?.syntax().text() != owner.name.as_str() {
        return None;
    }
    let arguments: Vec<String> = call
        .arg_list()?
        .args()
        .map(|arg| arg.syntax().text().to_string())
        .collect();
    if arguments != helper_parameters || arguments != hop.arguments {
        return None;
    }
    // Reuse the existing closed-field premise. Arbitrary impls/Drop and
    // external or generated primitive aliases are outside this initial slice.
    if !syntax.field_projection_workspace_closed(|| {
        super::super::element_field_pin::field_workspace_is_closed(context.index)
            && context.index.files().values().all(|file| {
                parse_clean_source_file(&file.source).is_some_and(|parse| {
                    !parse.tree().syntax().descendants().any(|node| {
                        ast::Impl::can_cast(node.kind())
                            || ast::Attr::cast(node).is_some_and(|attribute| {
                                attribute
                                    .syntax()
                                    .text()
                                    .to_string()
                                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                                    .any(|word| {
                                        matches!(
                                            word,
                                            "proc_macro"
                                                | "proc_macro_attribute"
                                                | "proc_macro_derive"
                                        )
                                    })
                            })
                    })
                })
            })
    }) {
        return None;
    }
    let mut assertions = Vec::new();
    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::HelperOwnerCall
            || test.file != owner.file
            || test.attrs.len() != 1
            || squeeze(&test.attrs[0]) != "#[test]"
            || !targeted_free_call_reaches_owner(test, helper, None, context.index, syntax)
            || !targeted_free_call_reaches_owner(test, owner, None, context.index, syntax)
        {
            continue;
        }
        for assertion in &test.assertions {
            if assertion.kind != OracleKind::ExactValue
                || assertion.strength != OracleStrength::Strong
                || !syntax.admits(test, assertion, context.index)
                || !pins_projection(&assertion.text, &helper.name, helper_parameters.len())
            {
                continue;
            }
            assertions.push(ProjectionAssertion {
                file: test.file.clone(),
                test: test.name.clone(),
                test_line: test.start_line,
                line: assertion.line,
                text: assertion.text.clone(),
            });
        }
    }
    if assertions.is_empty() {
        return None;
    }
    let mut receipt = FieldProjectionReceipt {
        owner: owner.id.clone(),
        helper: helper.id.clone(),
        expression: normalize_semantic_text(&probe.expression),
        line: probe.location.line,
        field,
        call: squeeze(&call_text),
        source_hash: format!("{:x}", Sha256::digest(facts.source.as_bytes())),
        assertions,
        semantic_digest: String::new(),
        validation_digest: String::new(),
    };
    receipt.semantic_digest = digest(&receipt.semantic_fields());
    receipt.validation_digest = digest(&receipt.validation_fields());
    Some(receipt)
}

fn root_function(file: &ast::SourceFile, fact: &FunctionSummary) -> Option<ast::Fn> {
    if !matches!(fact.item.container, FunctionContainer::Free) || !fact.attrs.is_empty() {
        return None;
    }
    let mut functions = file
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .name()
                .is_some_and(|name| name.text() == fact.name.as_str())
        });
    let function = functions.next()?;
    if functions.next().is_some()
        || function.generic_param_list().is_some()
        || function.where_clause().is_some()
        || function.async_token().is_some()
        || function.unsafe_token().is_some()
        || function.abi().is_some()
    {
        return None;
    }
    Some(function)
}

fn return_type(function: &ast::Fn) -> Option<String> {
    Some(function.ret_type()?.ty()?.syntax().text().to_string())
}

fn usize_parameters(function: &ast::Fn) -> Option<Vec<String>> {
    let list = function.param_list()?;
    if list.self_param().is_some() {
        return None;
    }
    list.params()
        .map(|parameter| {
            if parameter.ty()?.syntax().text() != "usize" {
                return None;
            }
            let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
                return None;
            };
            if pattern.mut_token().is_some()
                || pattern.ref_token().is_some()
                || pattern.pat().is_some()
            {
                return None;
            }
            Some(pattern.name()?.text().to_string())
        })
        .collect()
}

fn sole_tail(function: &ast::Fn) -> Option<ast::Expr> {
    let statements = function.body()?.stmt_list()?;
    if statements.statements().next().is_some() {
        return None;
    }
    statements.tail_expr()
}

fn primitive_expression(expr: &ast::Expr, parameters: &[String], depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    match expr {
        ast::Expr::Literal(literal) => integer_literal(&literal.syntax().text().to_string()),
        ast::Expr::PathExpr(path) => parameters
            .iter()
            .any(|name| path.syntax().text() == name.as_str()),
        ast::Expr::ParenExpr(paren) => paren
            .expr()
            .is_some_and(|expr| primitive_expression(&expr, parameters, depth - 1)),
        ast::Expr::BinExpr(binary) => {
            matches!(
                binary.op_kind(),
                Some(ast::BinaryOp::ArithOp(
                    ast::ArithOp::Add | ast::ArithOp::Sub
                ))
            ) && binary
                .lhs()
                .is_some_and(|expr| primitive_expression(&expr, parameters, depth - 1))
                && binary
                    .rhs()
                    .is_some_and(|expr| primitive_expression(&expr, parameters, depth - 1))
        }
        _ => false,
    }
}

fn integer_literal(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| ch.is_ascii_digit()) && text.parse::<usize>().is_ok()
}

fn pins_projection(text: &str, helper: &str, arity: usize) -> bool {
    let text = text.trim();
    let text = text.strip_suffix(';').unwrap_or(text).trim_end();
    let Some(rest) = text.strip_prefix("assert_eq!") else {
        return false;
    };
    let operands = super::super::reveal::macro_arguments(rest);
    let [left, right] = operands.as_slice() else {
        return false;
    };
    [(*left, *right), (*right, *left)]
        .into_iter()
        .any(|(call, expected)| {
            integer_literal(expected.trim())
                && is_exact_call(call.trim(), helper)
                && split_call_arguments_text(call.trim(), helper).is_some_and(|arguments| {
                    arguments.len() == arity
                        && arguments
                            .iter()
                            .all(|argument| integer_literal(argument.trim()))
                })
        })
}

fn squeeze(text: &str) -> String {
    text.trim()
        .trim_end_matches(',')
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_pin_requires_a_whole_bare_call_and_literal_operands() {
        for text in [
            "assert_eq!(projected(20, 1), 19);",
            "assert_eq!(19, projected(20, 1));",
        ] {
            assert!(pins_projection(text, "projected", 2), "{text}");
        }
        for text in [
            "assert_eq!(sentinel(), 19);",
            "assert_eq!(projected(20, 1) + 1, 20);",
            "assert_eq!(projected(20 + 1, 1), 20);",
            "assert_eq!(projected(20, 1), expected);",
            "assert_eq!(other::projected(20, 1), 19);",
            "assert_eq!(projected(20, 1), 19, \"note\");",
        ] {
            assert!(!pins_projection(text, "projected", 2), "{text}");
        }
    }

    #[test]
    fn receipt_rejects_mutated_identity_and_validation_bytes() {
        let probe = Probe {
            id: crate::domain::ProbeId("fixture".into()),
            location: crate::domain::SourceLocation {
                file: "src/lib.rs".into(),
                line: 5,
                column: 1,
            },
            owner: Some(SymbolId("src/lib.rs::stats".into())),
            family: ProbeFamily::FieldConstruction,
            delta: crate::domain::DeltaKind::Value,
            before: None,
            after: None,
            expression: "missing_duration_count: total - known".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let mut receipt = FieldProjectionReceipt {
            owner: SymbolId("src/lib.rs::stats".into()),
            helper: SymbolId("src/lib.rs::projected".into()),
            expression: normalize_semantic_text(&probe.expression),
            line: 5,
            field: "missing_duration_count".into(),
            call: "stats(total,known)".into(),
            source_hash: "source-bytes".into(),
            assertions: vec![ProjectionAssertion {
                file: "src/lib.rs".into(),
                test: "pin".into(),
                test_line: 12,
                line: 13,
                text: "assert_eq!(projected(20, 1), 19);".into(),
            }],
            semantic_digest: String::new(),
            validation_digest: String::new(),
        };
        receipt.semantic_digest = digest(&receipt.semantic_fields());
        receipt.validation_digest = digest(&receipt.validation_fields());
        assert!(receipt.valid_for(&probe));
        let mut wrong_field = receipt.clone();
        wrong_field.field = "sibling".into();
        assert!(!wrong_field.valid_for(&probe));
        let mut wrong_call = receipt.clone();
        wrong_call.helper = SymbolId("other::projected".into());
        assert!(!wrong_call.valid_for(&probe));
        receipt.source_hash = "different-source".into();
        assert!(!receipt.valid_for(&probe));
    }
}
