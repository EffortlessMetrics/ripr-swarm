//! Parser-backed owner-result binding for FieldConstruction missing facts.
//!
//! A compatible `RequiredDiscriminator::FieldValue` fact is emitted only
//! when activation is already known and a unique parser-backed test binds
//! a plain local to a *direct* captured owner call, then weakly observes
//! that same binding's constructed field. Presence of a nearby test or
//! field name never manufactures activation.

use super::record_field_name;
use super::related_tests::context::{CompactGripContext, CompactTest};
use super::related_tests::{call_text_contains_named_call, strip_comments_and_strings};
use crate::analysis::seams::{RepoSeam, RequiredDiscriminator, SeamKind};
use crate::analysis::syntax::ra::LineIndex;
use crate::domain::{MissingDiscriminatorFact, OracleKind, OracleStrength, StageState};
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};

pub(super) fn missing_field_value_facts(
    seam: &RepoSeam,
    related: &[&CompactTest<'_>],
    context: &CompactGripContext<'_>,
    owner_name: &str,
    activation: &StageState,
) -> Vec<MissingDiscriminatorFact> {
    if *activation != StageState::Yes || seam.kind() != SeamKind::FieldConstruction {
        return Vec::new();
    }
    if owner_name.is_empty() {
        return Vec::new();
    }
    let RequiredDiscriminator::FieldValue { field } = seam.required_discriminator() else {
        return Vec::new();
    };
    let Some(field_name) = record_field_name(field) else {
        return Vec::new();
    };

    let mut saw_strong = false;
    let mut saw_weak = false;
    for indexed in related {
        match owner_result_field_observation(indexed, context, owner_name, field_name) {
            Some(OwnerResultObservation::Strong) => saw_strong = true,
            Some(OwnerResultObservation::Weak) => saw_weak = true,
            None => {}
        }
        if saw_strong {
            break;
        }
    }
    if saw_strong || !saw_weak {
        return Vec::new();
    }

    vec![MissingDiscriminatorFact {
        value: field.clone(),
        reason: format!(
            "owner-result field `{field_name}` is observed only by a weak oracle; an exact field-value discriminator is missing"
        ),
        flow_sink: None,
    }]
}

enum OwnerResultObservation {
    Strong,
    Weak,
}

fn owner_result_field_observation(
    indexed: &CompactTest<'_>,
    context: &CompactGripContext<'_>,
    owner_name: &str,
    field_name: &str,
) -> Option<OwnerResultObservation> {
    if !indexed.test.calls.iter().any(|call| {
        call.name == owner_name && call_text_contains_named_call(&call.text, owner_name)
    }) {
        return None;
    }
    let facts = context.index.files.get(indexed.test.file.as_path())?;
    if facts.used_lexical_fallback {
        return None;
    }
    let _ = context.unique_evidence_function(
        &indexed.test.file,
        &indexed.test.name,
        indexed.test.start_line,
    )?;
    let parse = context.parsed_source(&indexed.test.file)?;
    let lines = LineIndex::new(&facts.source);
    let function = unique_test_fn(
        &parse.tree(),
        &indexed.test.name,
        indexed.test.start_line,
        &lines,
    )?;
    let bindings = direct_owner_result_bindings(&function, owner_name, &lines);
    if bindings.is_empty() {
        return None;
    }
    let mut kind = None;
    for oracle in &indexed.test.assertions {
        let oracle_text = strip_comments_and_strings(&oracle.text);
        for binding in &bindings {
            if oracle.line <= binding.line {
                continue;
            }
            if binding_invalidated_before(&function, binding, oracle.line, &lines) {
                continue;
            }
            if !reads_binding_field(&oracle_text, &binding.name, field_name) {
                continue;
            }
            match observation_from_oracle(oracle.kind.clone(), oracle.strength.clone()) {
                Some(OwnerResultObservation::Strong) => {
                    return Some(OwnerResultObservation::Strong);
                }
                Some(OwnerResultObservation::Weak) => kind = Some(OwnerResultObservation::Weak),
                None => {}
            }
        }
    }
    kind
}

struct OwnerResultBinding {
    name: String,
    line: usize,
    range_end: ra_ap_syntax::TextSize,
}

fn unique_test_fn(
    source: &SourceFile,
    name: &str,
    start_line: usize,
    lines: &LineIndex,
) -> Option<ast::Fn> {
    let mut matches = source
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .name()
                .is_some_and(|fn_name| fn_name.text() == name)
                && function
                    .fn_token()
                    .is_some_and(|token| lines.line(token.text_range().start()) == start_line)
        });
    let function = matches.next()?;
    matches.next().is_none().then_some(function)
}

fn direct_owner_result_bindings(
    function: &ast::Fn,
    owner_name: &str,
    lines: &LineIndex,
) -> Vec<OwnerResultBinding> {
    let Some(statements) = function.body().and_then(|body| body.stmt_list()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for statement in statements.statements() {
        let ast::Stmt::LetStmt(binding) = statement else {
            continue;
        };
        let Some(name) = plain_binding_name(&binding) else {
            continue;
        };
        let Some(initializer) = binding.initializer() else {
            continue;
        };
        if !initializer_is_direct_owner_call(&initializer, owner_name) {
            continue;
        }
        let line = binding
            .let_token()
            .map(|token| lines.line(token.text_range().start()))
            .unwrap_or_else(|| lines.line(binding.syntax().text_range().start()));
        found.push(OwnerResultBinding {
            name,
            line,
            range_end: binding.syntax().text_range().end(),
        });
    }
    found
}

fn plain_binding_name(statement: &ast::LetStmt) -> Option<String> {
    let pattern = ast::IdentPat::cast(statement.pat()?.syntax().clone())?;
    if pattern.ref_token().is_some() || pattern.at_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    valid_identifier(&name).then_some(name)
}

fn initializer_is_direct_owner_call(expression: &ast::Expr, owner_name: &str) -> bool {
    let Some(call) = ast::CallExpr::cast(expression.syntax().clone()) else {
        return false;
    };
    let Some(callee) = call.expr() else {
        return false;
    };
    direct_name(&callee).as_deref() == Some(owner_name)
}

fn direct_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    let text = path.syntax().text().to_string();
    valid_identifier(&text).then_some(text)
}

fn valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn binding_invalidated_before(
    function: &ast::Fn,
    binding: &OwnerResultBinding,
    assertion_line: usize,
    lines: &LineIndex,
) -> bool {
    let assertion_cutoff = function
        .syntax()
        .descendants()
        .find(|node| lines.line(node.text_range().start()) == assertion_line)
        .map(|node| node.text_range().start())
        .unwrap_or_else(|| function.syntax().text_range().end());
    let start = binding.range_end;
    if start >= assertion_cutoff {
        return true;
    }
    for node in function.syntax().descendants() {
        let range = node.text_range();
        if range.start() < start || range.start() >= assertion_cutoff {
            continue;
        }
        if let Some(pattern) = ast::IdentPat::cast(node.clone())
            && pattern
                .name()
                .is_some_and(|name| name.text() == binding.name)
        {
            return true;
        }
        if let Some(bin) = ast::BinExpr::cast(node.clone())
            && is_assignment_op(
                bin.op_token()
                    .as_ref()
                    .map(|token| token.text())
                    .unwrap_or(""),
            )
            && let Some(lhs) = bin.lhs()
            && (path_is_binding(&lhs, &binding.name)
                || field_receiver_is_binding(&lhs, &binding.name))
        {
            return true;
        }
        if let Some(reference) = ast::RefExpr::cast(node)
            && reference.mut_token().is_some()
            && reference
                .expr()
                .is_some_and(|expr| path_is_binding(&expr, &binding.name))
        {
            return true;
        }
    }
    false
}

fn is_assignment_op(token: &str) -> bool {
    matches!(
        token,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
    )
}

fn path_is_binding(expression: &ast::Expr, binding: &str) -> bool {
    direct_name(expression).as_deref() == Some(binding)
}

fn field_receiver_is_binding(expression: &ast::Expr, binding: &str) -> bool {
    ast::FieldExpr::cast(expression.syntax().clone())
        .and_then(|field| field.expr())
        .is_some_and(|receiver| path_is_binding(&receiver, binding))
}

fn reads_binding_field(code: &str, binding: &str, field: &str) -> bool {
    let pattern = format!("{binding}.{field}");
    code.match_indices(&pattern).any(|(start, matched)| {
        let before = code[..start].chars().next_back();
        if before.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
            return false;
        }
        let after = code[start + matched.len()..].chars().next();
        after.is_none_or(|ch| ch != '_' && !ch.is_ascii_alphanumeric())
    })
}

fn observation_from_oracle(
    kind: OracleKind,
    strength: OracleStrength,
) -> Option<OwnerResultObservation> {
    match (kind, strength) {
        (
            OracleKind::ExactValue | OracleKind::WholeObjectEquality,
            OracleStrength::Strong | OracleStrength::Medium,
        ) => Some(OwnerResultObservation::Strong),
        (
            OracleKind::RelationalCheck | OracleKind::Snapshot | OracleKind::SmokeOnly,
            OracleStrength::Weak | OracleStrength::Smoke | OracleStrength::Medium,
        ) => Some(OwnerResultObservation::Weak),
        (OracleKind::RelationalCheck, OracleStrength::Strong) => {
            Some(OwnerResultObservation::Strong)
        }
        _ => None,
    }
}
