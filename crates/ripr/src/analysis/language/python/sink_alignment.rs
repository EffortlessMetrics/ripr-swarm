use super::assertion_selection::PythonAssertionFocus;
use super::effect_alignment::{
    ChangedEffect, EffectReads, calls_owner_through_module, expected_computed_through_owner,
};
use super::owners_tests::collect_imports_from_statements;
use super::probe_shape::classify_probe_shape;
use super::related_tests::TestLookup;
use super::related_tests::{
    oracle_operand_calls, oracle_operand_names, oracle_operand_protocol_entry,
    owner_module_callees, owner_result_locals,
};
use super::source_facts::parse_module_result;
use super::wrapped_call::owner_call_only_inside_opaque_wrapper;
use super::{
    PythonOwner, PythonTest, assertion_calls_method_only_on_foreign_receiver,
    has_identifier_boundary, import_source_module_matches_owner, parse_attribute_assignment,
    python_dict_field_segment_parts, significant_change_tokens,
    strong_test_calls_owner_method_on_bound_receiver, strong_test_imports_owner_from_module,
    strong_tests_import_only_rival_modules, top_level_python_segments,
};
use crate::domain::{OracleKind, OracleStrength, OwnerKind, ProbeFamily, RelatedTest};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
/// The visible read-out of the sink-alignment decision. `ripr`'s value over
/// coverage is that a strong oracle credits `exposed` only when it *observes the
/// changed sink*, not merely reaches the owner. This carries which token
/// category the strongest oracle matched so a consumer can see *why* a strong
/// oracle did or did not credit `exposed`. It is a pure read-out: the boolean
/// the classifier uses is derived from `oracle_alignment` (see
/// [`SinkAlignment::observes`]), so the surfaced value can never disagree with
/// the decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SinkAlignment {
    pub(super) changed_sink: Option<String>,
    pub(super) observed_sink: Option<String>,
    /// One of `direct | alias | changed_sink_token | orthogonal | unknown`.
    pub(super) oracle_alignment: String,
    pub(super) alignment_reason: String,
    /// The assertion that credited the alignment, when one did. The finding
    /// reports it as its related test's oracle (RIPR-SPEC-0238 rule 4).
    pub(super) credited: Option<CreditedAssertion>,
}

/// The one assertion whose gates all held for the crediting branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CreditedAssertion {
    pub(super) test_name: String,
    pub(super) test_file: std::path::PathBuf,
    pub(super) text: String,
    pub(super) kind: OracleKind,
}

impl SinkAlignment {
    /// The decision the classifier uses, derived from the alignment so the two
    /// can never drift. A strong oracle observes the owner when the alignment is
    /// `direct`/`alias`/`changed_sink_token`. An empty token set cannot establish
    /// observation; an assertion must satisfy one of the existing credit branches.
    pub(super) fn observes(&self) -> bool {
        matches!(
            self.oracle_alignment.as_str(),
            "direct" | "alias" | "changed_sink_token"
        )
    }

    /// The same read-out with a non-delegatable reason: a related row has
    /// no family-relevant assertion (`no_family_relevant_assertion`) or now
    /// shows a different assertion than the strength-only pick
    /// (`other_behavior_assertion_passed_over`), #5572. Only the surfaced
    /// finding fields use it; the classifier decided from `observes()` first,
    /// and the gap ledger never delegates either reason.
    pub(super) fn with_reason(mut self, reason: &str) -> Self {
        self.alignment_reason = reason.to_string();
        self
    }

    /// The alignment surfaced when the classifier did not reach a strong-oracle
    /// branch (no-static-path, static-limit, heuristic-only, or weak-oracle
    /// findings never compute owner alignment). `changed_sink` is retained
    /// because it describes the changed line regardless of the test side.
    pub(super) fn unknown(changed_sink: Option<String>) -> Self {
        SinkAlignment {
            changed_sink,
            observed_sink: None,
            oracle_alignment: "unknown".to_string(),
            alignment_reason: "no_strong_oracle".to_string(),
            credited: None,
        }
    }
}

/// Whether `token` appears in `text` as a whole Python identifier rather than a
/// substring of a larger one. Without this, a common changed-sink token like
/// `buffer` would spuriously "observe" an unrelated oracle that merely contains
/// it (e.g. `buffered_stream` from a different class), over-crediting `exposed` —
/// a confirmed false-exposed vector. Mirrors the identifier-boundary rule of
/// [`has_identifier_boundary`]; whole words such as `key` in `Invalid key` still
/// match, so genuine sink observation is preserved.
pub(super) fn oracle_text_observes_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    text.match_indices(token)
        .any(|(idx, _)| has_identifier_boundary(text, idx, token.len()))
}

/// Classify whether a strong related-test oracle actually observes the changed
/// behavior's sink — and *which* token category matched. Reach-plus-strong-oracle
/// is not enough: a strong oracle that asserts a *different* value (e.g. a
/// wrapper's return) does not discriminate the change. The three token groups
/// (owner name, import alias, changed-sink tokens) are probed in order against
/// the admitted strong assertions. Empty token groups supply no observation
/// credit; the call and effect branches can still observe the actual output.
#[cfg(test)]
pub(super) fn classify_sink_alignment(
    owner: &PythonOwner,
    line_text: &str,
    related: &[RelatedTest],
    all_tests: &[PythonTest],
) -> SinkAlignment {
    classify_sink_alignment_with_old(owner, line_text, None, related, all_tests, false, None)
}

/// Whether a strong related-test oracle actually observes the changed behavior's
/// sink — not merely reaches the owner. Derived from [`classify_sink_alignment`]
/// so the boolean and the surfaced `oracle_alignment` can never disagree. Now a
/// `#[cfg(test)]` regression helper: production code reads the alignment directly
/// via [`SinkAlignment::observes`], and the boolean tests pin that observation
/// requires a crediting branch.
#[cfg(test)]
pub(super) fn strong_oracle_observes_owner(
    owner: &PythonOwner,
    line_text: &str,
    related: &[RelatedTest],
    all_tests: &[PythonTest],
) -> bool {
    classify_sink_alignment(owner, line_text, related, all_tests).observes()
}

/// Whether the changed line is genuinely a control-flow construct (a predicate or
/// error-path), for the empty-token-delta fallback gate. This mirrors the
/// `Predicate` and `ErrorPath` conditions of [`classify_probe_shape`] WITHOUT its
/// default arm (which returns `Control` for any unrecognized line, e.g. a plain
/// `total = base - bonus` assignment). Keying the empty-delta operand fallback on
/// this precise shape — not the default-polluted delta kind — is the #1288 fix:
/// only a real branch / raise change can be discriminated by an outcome oracle that
/// merely matches a line token; a value-producing assignment cannot.
fn is_control_flow_change_line(line_text: &str) -> bool {
    let trimmed = line_text.trim_start();
    (trimmed.contains(" if ") && trimmed.contains(" else "))
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elif ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("case ")
        || trimmed.starts_with("raise ")
        || trimmed == "raise"
        || trimmed.starts_with("try:")
        || trimmed.starts_with("except ")
        || trimmed.starts_with("except* ")
        || trimmed.starts_with("finally:")
        || (trimmed.starts_with("with ") && trimmed.contains("raises("))
}

/// Parse a dict-literal line (a `return {...}` or `lhs = {...}`) into its top-level
/// `(key, value)` pairs. Keys are unquoted; values keep their source text. Returns
/// `None` if the line has no `{...}` body or no parseable fields.
pub(super) fn parse_dict_literal_fields(line: &str) -> Option<Vec<(String, String)>> {
    let trimmed = line.trim();
    // The dict EXPRESSION must literally START with `{` (after an optional `return `),
    // not merely contain one — otherwise an f-string like `f"{value:.3f}"`, a
    // `.format(...)` call, or any line with `{` inside a string literal is mis-read as
    // a dict literal and wrongly gated (this follow-up fixes a regressed f-string
    // discriminator). A set literal `{1, 2}` is naturally excluded below since it has
    // no top-level `key: value` segments.
    let expr = trimmed
        .strip_prefix("return ")
        .map(str::trim)
        .unwrap_or(trimmed);
    let body = expr.strip_prefix('{')?.strip_suffix('}')?;
    let mut fields = Vec::new();
    for segment in top_level_python_segments(body) {
        if let Some((key, value)) = python_dict_field_segment_parts(segment) {
            fields.push((key.to_string(), value.trim().to_string()));
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

/// For a dict-literal field-construction change, the keys whose value differs
/// between the old and new line (added / removed / re-valued), plus the NEW values
/// of those keys. Returns `None` when the change is not a dict-literal change on
/// both sides, or when nothing localizable changed — in which case the #1290
/// dict-element gate is a pass-through.
pub(super) fn dict_changed_keys_and_values(
    old_line: Option<&str>,
    new_line: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let old = parse_dict_literal_fields(old_line?)?;
    let new = parse_dict_literal_fields(new_line)?;
    let mut changed_keys = Vec::new();
    let mut changed_values = Vec::new();
    for (key, value) in &new {
        let old_value = old.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        if old_value != Some(value) {
            changed_keys.push(key.clone());
            changed_values.push(value.clone());
        }
    }
    // A key present in the old line but removed in the new line is also a change.
    for (key, _) in &old {
        if !new.iter().any(|(k, _)| k == key) {
            changed_keys.push(key.clone());
        }
    }
    if changed_keys.is_empty() {
        None
    } else {
        Some((changed_keys, changed_values))
    }
}

/// Recover only a flat, one-element-per-line return display. Replacing the
/// paired member in that display retains its key/index identity without guessing
/// across nested multiline expressions, opener-line elements or structural edits.
pub(super) fn multiline_literal_change(
    source: &str,
    line: usize,
    new_line: &str,
    old_line: Option<&str>,
) -> Option<(String, String)> {
    let old_line = old_line?;
    let lines: Vec<&str> = source.lines().collect();
    let index = line.checked_sub(1)?;
    if *lines.get(index)? != new_line {
        return None;
    }
    let indent = |text: &str| text.len() - text.trim_start().len();
    if indent(old_line) != indent(new_line) {
        return None;
    }
    let start = (0..index).rev().find(|&i| {
        !lines[i].trim().is_empty()
            && !lines[i].trim_start().starts_with('#')
            && indent(lines[i]) < indent(new_line)
    })?;
    let (open, close) = match lines[start].trim() {
        "return {" => ('{', '}'),
        "return [" => ('[', ']'),
        _ => return None,
    };
    let end = (index + 1..lines.len()).find(|&i| {
        !lines[i].trim().is_empty()
            && !lines[i].trim_start().starts_with('#')
            && indent(lines[i]) <= indent(lines[start])
    })?;
    if lines[end].trim() != close.to_string() {
        return None;
    }
    let display = |old: bool| -> Option<String> {
        let mut members = Vec::new();
        for (i, member) in lines.iter().enumerate().take(end).skip(start + 1) {
            let member = if old && i == index { old_line } else { member };
            if member.trim().is_empty() || member.trim_start().starts_with('#') {
                continue;
            }
            let member = member.trim();
            let mut last = 0;
            for token in rustpython_parser::lexer::lex(member, rustpython_parser::Mode::Expression)
            {
                let (token, range) = token.ok()?;
                if !matches!(
                    token,
                    rustpython_parser::Tok::Newline | rustpython_parser::Tok::EndOfFile
                ) {
                    last = usize::from(range.end());
                }
            }
            let member = member.get(..last)?.trim_end_matches(',');
            let single = format!("{open}{member}{close}");
            match super::approx_band::parse_expression(&single)? {
                Expr::Dict(dict)
                    if open == '{'
                        && dict.keys.len() == 1
                        && dict.keys.first().is_some_and(Option::is_some) => {}
                Expr::List(list) if open == '[' && list.elts.len() == 1 => {}
                _ => return None,
            }
            members.push(member.to_string());
        }
        Some(format!("return {open}{}{close}", members.join(", ")))
    };
    Some((display(true)?, display(false)?))
}

/// A changed literal must be an entire token, not a substring of a sibling's
/// number/string. Keep the existing minimum-length rule for one-line displays.
fn oracle_contains_whole_literal(oracle: &str, value: &str) -> bool {
    let value = value.trim();
    if value.trim_matches(['"', '\'']).len() < 2 {
        return false;
    }
    let Some(wanted) = super::approx_band::parse_expression(value) else {
        return false;
    };
    let width = match &wanted {
        Expr::Constant(_) => 1,
        Expr::UnaryOp(unary)
            if matches!(unary.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                && matches!(unary.operand.as_ref(), Expr::Constant(_)) =>
        {
            2
        }
        _ => return false,
    };
    let equal = |found: Expr| match (&wanted, found) {
        (Expr::Constant(a), Expr::Constant(b)) => a.value == b.value,
        (Expr::UnaryOp(a), Expr::UnaryOp(b)) if a.op == b.op => {
            matches!((a.operand.as_ref(), b.operand.as_ref()),
                (Expr::Constant(a), Expr::Constant(b)) if a.value == b.value)
        }
        _ => false,
    };
    let tokens: Vec<_> = rustpython_parser::lexer::lex(oracle, rustpython_parser::Mode::Module)
        .filter_map(Result::ok)
        .filter(|(token, _)| {
            !matches!(
                token,
                rustpython_parser::Tok::Newline
                    | rustpython_parser::Tok::EndOfFile
                    | rustpython_parser::Tok::Indent
                    | rustpython_parser::Tok::Dedent
            )
        })
        .collect();
    let adjacent = |ch: char| ch.is_alphanumeric() || matches!(ch, '_' | '.');
    tokens.windows(width).any(|window| {
        let start = usize::from(window[0].1.start());
        let end = usize::from(window[width - 1].1.end());
        oracle
            .get(start..end)
            .and_then(super::approx_band::parse_expression)
            .is_some_and(equal)
            && !oracle[..start].chars().next_back().is_some_and(adjacent)
            && !oracle[end..].chars().next().is_some_and(adjacent)
    })
}

/// Compound assertions need collection identity on the compared operand itself:
/// an unrelated display in another conjunct cannot borrow the owner's call.
fn oracle_compares_owner_collection(oracle: &str, test: &PythonTest, owner: &PythonOwner) -> bool {
    fn callee_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Name(name) => Some(name.id.to_string()),
            Expr::Attribute(attr) => Some(format!("{}.{}", callee_name(&attr.value)?, attr.attr)),
            Expr::Call(call) => Some(format!("{}()", callee_name(&call.func)?)),
            _ => None,
        }
    }
    fn observes(expr: &Expr, callees: &[String], locals: &[String]) -> bool {
        let display = |expr: &Expr| matches!(expr, Expr::Dict(_) | Expr::List(_));
        fn result(expr: &Expr, callees: &[String], locals: &[String]) -> bool {
            match expr {
                Expr::Call(call) => {
                    callee_name(&call.func).is_some_and(|name| callees.contains(&name))
                        || (matches!(call.func.as_ref(), Expr::Name(name) if matches!(name.id.as_str(), "list" | "tuple"))
                            && call.keywords.is_empty()
                            && call.args.len() == 1
                            && call
                                .args
                                .first()
                                .is_some_and(|arg| result(arg, callees, locals)))
                }
                Expr::Name(name) => locals.iter().any(|local| local == name.id.as_str()),
                _ => false,
            }
        }
        let result = |expr: &Expr| result(expr, callees, locals);
        match expr {
            Expr::BoolOp(boolean) if boolean.op == ast::BoolOp::And => boolean
                .values
                .iter()
                .any(|value| observes(value, callees, locals)),
            Expr::Compare(compare) => {
                if compare.ops.iter().all(|op| *op == ast::CmpOp::Eq) {
                    let operands: Vec<_> = std::iter::once(compare.left.as_ref())
                        .chain(compare.comparators.iter())
                        .collect();
                    return operands.iter().any(|operand| display(operand))
                        && operands.iter().any(|operand| result(operand));
                }
                let mut left = compare.left.as_ref();
                for (op, right) in compare.ops.iter().zip(&compare.comparators) {
                    if *op == ast::CmpOp::Eq
                        && ((display(left) && result(right)) || (result(left) && display(right)))
                    {
                        return true;
                    }
                    left = right;
                }
                false
            }
            _ => false,
        }
    }
    let expression = oracle
        .trim()
        .strip_prefix("assert ")
        .unwrap_or(oracle.trim());
    let Some(expression) = super::approx_band::parse_expression(expression) else {
        return false;
    };
    let mut callees = owner_module_callees(test, owner);
    if matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    ) && let Some((class, method)) = owner.qualified_name.rsplit_once('.')
    {
        let class_locals = super::related_tests::owner_class_locals(test, owner, class);
        callees.extend(
            class_locals
                .iter()
                .flat_map(|class| [format!("{class}.{method}"), format!("{class}().{method}")]),
        );
        callees.extend(
            super::related_tests::owner_method_bound_locals(test, owner, class, method)
                .into_iter()
                .chain(super::related_tests::setup_bound_receivers(
                    test, owner, class, method,
                ))
                .map(|receiver| format!("{receiver}.{method}")),
        );
    }
    let locals = owner_result_locals(test, &callees);
    observes(&expression, &callees, &locals)
}

/// Whether a strong oracle observes the CHANGED dict element (#1290 / #6585): a
/// subscript or `.get(...)` of a changed key, the changed value literal when the
/// oracle does not solely subscript an unchanged sibling, or a whole-collection
/// comparison. Conservative — when in doubt it returns `true` (credit stands) so a
/// genuine discriminator is never dropped; it only returns `false` for an oracle
/// that observes purely a sibling key or an aggregate.
pub(super) fn oracle_observes_changed_dict_element(
    oracle: &str,
    changed_keys: &[String],
    changed_values: &[String],
) -> bool {
    oracle_observes_changed_dict_element_with_collection(
        oracle,
        changed_keys,
        changed_values,
        legacy_whole_collection_comparison(oracle),
    )
}

fn legacy_whole_collection_comparison(oracle: &str) -> bool {
    ["=={", "== {", "==[", "== ["]
        .iter()
        .any(|needle| oracle.contains(needle))
}

fn oracle_observes_changed_dict_element_with_collection(
    oracle: &str,
    changed_keys: &[String],
    changed_values: &[String],
    whole_collection_observed: bool,
) -> bool {
    // A whole-collection comparison (`== {...}` / `== [...]`) observes every element.
    if whole_collection_observed {
        return true;
    }
    // Observes a changed value literal (e.g. the new `9090` / `"failure"`), but
    // not when that literal only appears beside a sibling-key subscript
    // (`assert build()['timeout'] == 80` after `'port'` changed to `80`).
    for value in changed_values {
        if oracle_contains_whole_literal(oracle, value)
            && !oracle_solely_subscripts_unchanged_dict_key(oracle, changed_keys)
        {
            return true;
        }
    }
    // Subscripts a changed key by literal (`["port"]`, `['port']`, `.get("port")`).
    changed_keys
        .iter()
        .any(|key| oracle_subscripts_dict_key(oracle, key))
}

fn oracle_subscripts_dict_key(oracle: &str, key: &str) -> bool {
    oracle.contains(&format!("[\"{key}\"]"))
        || oracle.contains(&format!("['{key}']"))
        || oracle.contains(&format!(".get(\"{key}\")"))
        || oracle.contains(&format!(".get('{key}')"))
}

fn oracle_has_literal_dict_subscript(oracle: &str) -> bool {
    oracle.contains("[\"")
        || oracle.contains("['")
        || oracle.contains(".get(\"")
        || oracle.contains(".get('")
}

fn oracle_solely_subscripts_unchanged_dict_key(oracle: &str, changed_keys: &[String]) -> bool {
    oracle_has_literal_dict_subscript(oracle)
        && !changed_keys
            .iter()
            .any(|key| oracle_subscripts_dict_key(oracle, key))
}

/// Parse a list-literal line (a `return [...]`) into its top-level element source
/// texts, in order. Like [`parse_dict_literal_fields`], the expression must literally
/// START with `[` (after an optional `return `) so a subscript expression such as
/// `arr[-1]` or an f-string is never mis-read as a list literal.
fn parse_list_literal_elements(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    let expr = trimmed
        .strip_prefix("return ")
        .map(str::trim)
        .unwrap_or(trimmed);
    let body = expr.strip_prefix('[')?.strip_suffix(']')?;
    let elements: Vec<String> = top_level_python_segments(body)
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect();
    if elements.is_empty() {
        None
    } else {
        Some(elements)
    }
}

/// For a list-literal field-construction change, the positions (indices) whose
/// element differs between the old and new line, plus the NEW element source at
/// those positions. Returns `None` when the change is not a list-literal change on
/// both sides, when the lengths differ (a structural change is observed by `len`),
/// or when nothing changed — in which case the list-element gate is a pass-through.
fn list_changed_indices_and_values(
    old_line: Option<&str>,
    new_line: &str,
) -> Option<(Vec<usize>, Vec<String>)> {
    let old = parse_list_literal_elements(old_line?)?;
    let new = parse_list_literal_elements(new_line)?;
    // A length change is discriminated by `len(...)`, so it is NOT gated here.
    if old.len() != new.len() {
        return None;
    }
    let mut changed_indices = Vec::new();
    let mut changed_values = Vec::new();
    for (index, (old_value, new_value)) in old.iter().zip(new.iter()).enumerate() {
        if old_value != new_value {
            changed_indices.push(index);
            changed_values.push(new_value.clone());
        }
    }
    if changed_indices.is_empty() {
        None
    } else {
        Some((changed_indices, changed_values))
    }
}

/// Whether a strong oracle observes the CHANGED list element (#1290 / #6585): a
/// subscript of a changed index, the changed element value literal when the
/// oracle does not solely subscript an unchanged sibling, or a whole-collection
/// comparison. Conservative, mirroring [`oracle_observes_changed_dict_element`]: it
/// returns `false` only for an oracle that observes purely a sibling index or an
/// aggregate (`len(...)`).
fn oracle_observes_changed_list_element(
    oracle: &str,
    changed_indices: &[usize],
    changed_values: &[String],
    whole_collection_observed: bool,
) -> bool {
    if whole_collection_observed {
        return true;
    }
    for value in changed_values {
        if oracle_contains_whole_literal(oracle, value)
            && !oracle_solely_subscripts_unchanged_list_index(oracle, changed_indices)
        {
            return true;
        }
    }
    changed_indices
        .iter()
        .any(|index| oracle_subscripts_list_index(oracle, *index))
}

fn oracle_subscripts_list_index(oracle: &str, index: usize) -> bool {
    oracle.contains(&format!("[{index}]"))
}

fn oracle_has_list_index_subscript(oracle: &str) -> bool {
    let bytes = oracle.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let mut j = i + 1;
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b']' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn oracle_solely_subscripts_unchanged_list_index(oracle: &str, changed_indices: &[usize]) -> bool {
    oracle_has_list_index_subscript(oracle)
        && !changed_indices
            .iter()
            .any(|index| oracle_subscripts_list_index(oracle, *index))
}

/// Split an f-string source (`f"..."`/`f'...'`, optional `r` prefix) into its
/// concatenated literal text and its ordered `{...}` interpolation substrings.
/// Returns `None` if the line (after an optional `return `) is not a single f-string.
pub(super) fn fstring_template(line: &str) -> Option<(String, Vec<String>)> {
    let trimmed = line.trim();
    let expr = trimmed
        .strip_prefix("return ")
        .map(str::trim)
        .unwrap_or(trimmed);
    // Accept f / rf / fr prefixes (case-insensitive), then a quote.
    let lower = expr.to_ascii_lowercase();
    let prefix_len = if lower.starts_with("f\"") || lower.starts_with("f'") {
        1
    } else if lower.starts_with("rf\"")
        || lower.starts_with("rf'")
        || lower.starts_with("fr\"")
        || lower.starts_with("fr'")
    {
        2
    } else {
        return None;
    };
    let rest = &expr[prefix_len..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = rest.strip_prefix(quote)?.strip_suffix(quote)?;
    // Fail open on f-string shapes this simple parser does not model precisely, so the
    // gate never downgrades on a mis-parse: escaped braces (`{{` / `}}`), a leftover
    // quote from a triple-quoted string, or nested interpolation/format-spec
    // (`{value:{width}}`). When unsupported, return `None` and the gate is a no-op.
    if body.contains("{{") || body.contains("}}") || body.starts_with(quote) {
        return None;
    }
    let mut literals = String::new();
    let mut interpolations = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in body.chars() {
        match ch {
            '{' => {
                depth += 1;
                if depth > 1 {
                    // Nested interpolation / format-spec — unsupported, fail open.
                    return None;
                }
                continue;
            }
            '}' if depth >= 1 => {
                depth -= 1;
                interpolations.push(current.clone());
                current.clear();
                continue;
            }
            _ => {}
        }
        if depth == 0 {
            literals.push(ch);
        } else {
            current.push(ch);
        }
    }
    if depth != 0 {
        return None;
    }
    Some((literals, interpolations))
}

/// Whether an f-string change is output-length-invariant: the interpolations are
/// identical (so the runtime-substituted text is unchanged) and the literal text has
/// the same length. When true, a `len(...)` oracle provably cannot discriminate the
/// change, so observing the owner's output only through `len` is not a discriminator.
/// A format-spec change (`:.2f` -> `:.3f`) alters an interpolation, so it is NOT
/// length-invariant and is never gated here.
pub(super) fn fstring_change_is_length_invariant(old_line: &str, new_line: &str) -> bool {
    match (fstring_template(old_line), fstring_template(new_line)) {
        (Some((old_literals, old_interps)), Some((new_literals, new_interps))) => {
            old_interps == new_interps
                && old_literals.chars().count() == new_literals.chars().count()
        }
        _ => false,
    }
}

/// Whether an oracle observes the owner's output ONLY through a `len(...)` aggregate
/// — i.e. it calls `len(` and does NOT also observe the output exactly. An oracle
/// that compares against a string literal, or that contains the changed f-string
/// literal text, observes the changed output and is therefore NOT a pure aggregate
/// (so the #1290 1b gate must not downgrade it). Conservative by design: anything
/// other than a clear length-only observation keeps the credit.
pub(super) fn oracle_is_pure_len_aggregate(oracle: &str, changed_literals: &str) -> bool {
    if !oracle.contains("len(") {
        return false;
    }
    // An exact string-equality comparison observes the produced string, not just its
    // length.
    if oracle.contains("== \"")
        || oracle.contains("== '")
        || oracle.contains("==\"")
        || oracle.contains("=='")
    {
        return false;
    }
    // The changed literal text appearing in the oracle means it observes the change.
    let trimmed = changed_literals.trim();
    if trimmed.len() >= 2 && oracle.contains(trimmed) {
        return false;
    }
    true
}

/// Per-finding state that does not depend on the related rows: the test index
/// and the owner's import-alias locals across every test. Built once so a
/// caller that aligns many rows separately does not rebuild it per row.
pub(super) struct AlignmentPrep<'a> {
    lookup: TestLookup<'a>,
    alias_tokens: Vec<String>,
}

impl<'a> AlignmentPrep<'a> {
    pub(super) fn new(owner: &PythonOwner, all_tests: &'a [PythonTest]) -> Self {
        let mut alias_tokens: Vec<String> = all_tests
            .iter()
            .flat_map(|test| owner_import_aliases(owner, test))
            .collect();
        alias_tokens.retain(|token| token.len() >= 2);
        Self {
            lookup: TestLookup::new(all_tests),
            alias_tokens,
        }
    }
}

/// Import aliases of the owner: `from m import owner as alias` makes the test
/// assert on `alias(...)`, which still observes the owner's output. A method
/// owner's only importable alias is its class's, which rule 9 does not count
/// as identity alone; an aliased class reaches the bound-receiver branch
/// instead (`from m import Cart as C`, `C().total()`).
fn owner_import_aliases(owner: &PythonOwner, test: &PythonTest) -> Vec<String> {
    let is_method_owner = matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    );
    if is_method_owner || super::test_activation::activation_control(test).is_some() {
        return Vec::new();
    }
    let owner_simple = owner
        .qualified_name
        .rsplit('.')
        .next()
        .unwrap_or(owner.name.as_str());
    test.imports
        .iter()
        .filter(|import| {
            // Free-function alias: require module identity, else a same-named
            // function aliased from an unrelated module credits a false-`exposed`.
            (import.imported == owner_simple || import.imported == owner.name)
                && import_source_module_matches_owner(import, owner, &test.file)
        })
        .filter(|import| import.alias.len() >= 2)
        .map(|import| import.alias.clone())
        .collect()
}

pub(super) fn classify_sink_alignment_with_old(
    owner: &PythonOwner,
    line_text: &str,
    old_line_text: Option<&str>,
    related: &[RelatedTest],
    all_tests: &[PythonTest],
    guard_entered: bool,
    literal_lines: Option<&(String, String)>,
) -> SinkAlignment {
    let prep = AlignmentPrep::new(owner, all_tests);
    classify_sink_alignment_prepared(
        owner,
        line_text,
        old_line_text,
        related,
        &prep,
        guard_entered,
        literal_lines,
    )
}

pub(super) fn classify_sink_alignment_prepared(
    owner: &PythonOwner,
    line_text: &str,
    old_line_text: Option<&str>,
    related: &[RelatedTest],
    prep: &AlignmentPrep<'_>,
    guard_entered: bool,
    literal_lines: Option<&(String, String)>,
) -> SinkAlignment {
    // Whether the changed line is genuinely a control-flow construct gates the
    // empty-token-delta fallback below: only a control-flow change may credit
    // `changed_sink_token` from bare line operands when the token delta is empty
    // (see the fallback comment). This is a PRECISE shape check (#1288) — not
    // `classify_probe_shape(..).1 == Control`, whose default arm also returns
    // `Control` for unrecognized lines such as plain local assignments
    // (`total = base - bonus`), which would wrongly keep the operand fallback and
    // re-introduce the false-`exposed`. Value/effect changes are observed via the
    // owner call, not an input operand.
    let changed_line_is_control_flow = is_control_flow_change_line(line_text);
    // `changed_sink` describes the changed line; deduped, joined for display.
    let change_tokens = significant_change_tokens(line_text);
    let mut change_display: Vec<String> = Vec::new();
    for token in &change_tokens {
        if !change_display.contains(token) {
            change_display.push(token.clone());
        }
    }
    let changed_sink = if change_display.is_empty() {
        None
    } else {
        Some(change_display.join(", "))
    };

    // The strongest strong-rank related oracle is the one the decision inspects.
    let strong_tests: Vec<&RelatedTest> = related
        .iter()
        .filter(|test| test.oracle_strength.rank() >= OracleStrength::Strong.rank())
        .collect();
    // A changed side effect (`print`, a file write, a call on a passed-in
    // collaborator, a `self` field) may be observed through its channel.
    let effect = ChangedEffect::of(line_text, old_line_text, owner, guard_entered);
    let mock_tests: Vec<&RelatedTest> = if effect
        .as_ref()
        .is_some_and(ChangedEffect::admits_mock_assertion)
    {
        related
            .iter()
            .filter(|test| matches!(test.oracle_kind, OracleKind::MockExpectation))
            .filter(|test| test.oracle_strength.rank() < OracleStrength::Strong.rank())
            .collect()
    } else {
        Vec::new()
    };
    if strong_tests.is_empty() && mock_tests.is_empty() {
        return SinkAlignment::unknown(changed_sink);
    }
    let observed_sink = strong_tests
        .iter()
        .max_by_key(|test| test.oracle_strength.rank())
        .and_then(|test| test.oracle.clone());
    // RIPR-SPEC-0238 rule 6: an error-path change is observed only by an
    // `exact_error_variant` assertion that itself credits a branch; a strong
    // value assertion never runs the changed raise.
    let family = classify_probe_shape(line_text).0;
    let error_path = matches!(family, ProbeFamily::ErrorPath);
    // RIPR-SPEC-0238 rule 4: every strong assertion of every strong related
    // test is read, and each credit branch must hold on one assertion. The
    // RIPR-SPEC-0224 family filter still applies: an assertion of another
    // behavior family (a `pytest.raises` for a changed `return`) never
    // credits, whichever branch its text would pass.
    let focus = PythonAssertionFocus::for_change(family, line_text, old_line_text);
    let test_lookup = &prep.lookup;
    let mut units = alignment_units(
        &strong_tests,
        test_lookup,
        error_path,
        &focus,
        effect.as_ref(),
    );
    units.extend(alignment_units(
        &mock_tests,
        test_lookup,
        error_path,
        &focus,
        effect.as_ref(),
    ));
    // Each credited list assertion must call this owner in its own test row.
    // Apply the guard to the gate text, after per-assertion expansion.
    units
        .retain(|unit| super::oracles::unittest_list::observes_owner(&unit.text, unit.test, owner));
    if units.is_empty() {
        if strong_tests.is_empty() {
            return SinkAlignment::unknown(changed_sink);
        }
        return SinkAlignment {
            changed_sink,
            observed_sink,
            oracle_alignment: "orthogonal".to_string(),
            alignment_reason: "strong_oracle_observes_different_sink".to_string(),
            credited: None,
        };
    }

    // Owner-name tokens, split for identity-aware crediting. For a method /
    // classmethod owner the bare method name is collision-prone: a same-named
    // method on an unrelated class (`PaymentProcessor.validate` vs owner
    // `TokenValidator.validate`) shares the token `validate` yet is a different
    // entity, so it credits only on a bound receiver. The class token is not
    // identity alone either (RIPR-SPEC-0238 rule 9): `other.total() ==
    // Cart.LIMIT` names the class but runs `total` on another receiver.
    // Free-function names are their own identity.
    let is_method_owner = matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    );
    let owner_class_token: Option<String> = if is_method_owner {
        owner
            .qualified_name
            .rsplit_once('.')
            .map(|(class, _)| class)
            .filter(|class| !class.is_empty() && *class != "<module>")
            .map(str::to_string)
    } else {
        None
    };
    let mut identity_tokens: Vec<String> = if is_method_owner {
        Vec::new()
    } else {
        owner
            .qualified_name
            .split('.')
            .filter(|token| !token.is_empty() && *token != "<module>")
            .map(str::to_string)
            .collect()
    };
    if !is_method_owner && !owner.name.is_empty() && owner.name != "<module>" {
        identity_tokens.push(owner.name.clone());
    }
    // The collision-prone bare method name of a method owner, gated on identity.
    let method_name_token: Option<String> =
        if is_method_owner && !owner.name.is_empty() && owner.name != "<module>" {
            Some(owner.name.clone())
        } else {
            None
        };
    let owner_aliases_in = |test: &PythonTest| owner_import_aliases(owner, test);
    let mut alias_tokens: Vec<String> = prep.alias_tokens.clone();
    let mut change_only = change_tokens.clone();
    identity_tokens.retain(|token| token.len() >= 2);
    let method_name_token = method_name_token.filter(|token| token.len() >= 2);
    alias_tokens.retain(|token| token.len() >= 2);
    change_only.retain(|token| token.len() >= 2);
    // Delta tokens: the changed-sink-token credit must reflect what actually
    // CHANGED on the line, not every operand on it. A token that is unchanged
    // between the old and new line (e.g. `valid_tokens` in `token in valid_tokens`
    // -> `token.strip() in valid_tokens`, or `_balance` in a `max(0, ...)` wrap) is
    // not the behavior delta; an oracle observing only such an operand does not
    // discriminate the change. When the old line is unavailable (a pure addition),
    // every token is part of the delta.
    let delta_tokens: Vec<String> = match old_line_text {
        Some(old) => {
            let old_tokens: std::collections::BTreeSet<String> =
                significant_change_tokens(old).into_iter().collect();
            let delta: Vec<String> = change_only
                .iter()
                .filter(|token| !old_tokens.contains(*token))
                .cloned()
                .collect();
            // An empty token delta means the change is in non-tokenized syntax (an
            // operator like `<=` -> `<`, punctuation, ordering) that token extraction
            // does not capture. Falling back to the full changed-line tokens is only
            // sound for a CONTROL-FLOW change (#1278): for a predicate / error-path
            // operator change the discriminated outcome (a taken branch, a raised
            // exception) can be observed by an outcome oracle that matches a line
            // token — as in `python_cross_file_construct_call`'s `pytest.raises`. For
            // a VALUE/EFFECT change (a `return` or assignment operator edit), every
            // line token is an unchanged INPUT operand; the changed sink is the
            // produced value, which a discriminating test observes via the owner call
            // (the `direct`/`alias` paths), not via an input token. Crediting
            // `changed_sink_token` on an input operand there is the false-`exposed`
            // (e.g. `return count + 1` -> `count - 1` with `assert count == 5`), so
            // for non-control empty deltas we credit nothing rather than the operands.
            if delta.is_empty() {
                if changed_line_is_control_flow {
                    change_only.clone()
                } else {
                    Vec::new()
                }
            } else {
                delta
            }
        }
        None => change_only.clone(),
    };

    let observes = |unit: &AlignmentUnit<'_>, group: &[String]| -> bool {
        group
            .iter()
            .any(|token| oracle_text_observes_token(&unit.text, token))
    };
    // A function-scope stand-in (`fee = Mock(...)`, `fee = lambda ...`,
    // `def fee`) is not the imported owner. Token presence of that name in
    // the assertion is not observation of the production return (#8247).
    let observes_imported_owner = |unit: &AlignmentUnit<'_>, group: &[String]| -> bool {
        group.iter().any(|token| {
            oracle_text_observes_token(&unit.text, token)
                && !unit
                    .test
                    .is_some_and(|test| function_scope_stand_in(test, token, owner))
        })
    };
    // Receiver identity for a method owner: the test holding the assertion must
    // call the owner's method on a receiver statically bound to the owner class
    // (inline construct, local binding, or a classmethod/direct call on the
    // class). A bare method-name match — even with the owner class imported and
    // mentioned — is not identity-bearing, because the asserted `.method(` may
    // run on an unrelated receiver while the class is referenced (or merely
    // named) elsewhere. This is the false-`exposed` guard at the relation layer.
    let binds_method_receiver = per_related_test(|unit: &AlignmentUnit<'_>| {
        strong_test_calls_owner_method_on_bound_receiver(
            owner,
            owner_class_token.as_ref(),
            method_name_token.as_ref(),
            &[unit.related],
            test_lookup,
        )
    });
    // The relation above reads the whole test body; the assertion itself must
    // not call the method only on some other receiver (#8247).
    let asserts_foreign_receiver = |unit: &AlignmentUnit<'_>| match (
        unit.test,
        owner_class_token.as_ref(),
        method_name_token.as_ref(),
    ) {
        (Some(test), Some(class), Some(method)) => {
            assertion_calls_method_only_on_foreign_receiver(test, owner, class, method, &unit.text)
        }
        _ => false,
    };
    // Free-function module identity: a non-method owner's bare function-name token
    // credits `direct` only when the test holding the assertion imports it from
    // the owner's module. A same-named free function imported from a different
    // module (`from src.checker import validate` for owner `src.handler.validate`)
    // is not identity-bearing — the false-`exposed` guard for free functions.
    let free_fn_module_identity = per_related_test(|unit: &AlignmentUnit<'_>| {
        !is_method_owner
            && strong_test_imports_owner_from_module(&[unit.related], test_lookup, owner)
    });
    // Method-owner identity is class and method name; a test whose imports
    // name a same-named module of another project uses that project's class.
    let method_owner_identity = per_related_test(|unit: &AlignmentUnit<'_>| {
        is_method_owner
            && !strong_tests_import_only_rival_modules(owner, &[unit.related], test_lookup)
    });
    // Owner output observed through a module-identified call (#4567): the
    // assertion calls the owner through its module (`utils.sign(0) == 0`,
    // `pkg.utils.sign(0)`, an imported name), or asserts a local the same test
    // bound once to such a call (`result = utils.sign(0)`, `assert result == 0`).
    // The call or local must be an asserted operand: nested in another call
    // (`always_true(utils.sign(0))`) it is not the value the oracle compares.
    let module_call_observed = |unit: &AlignmentUnit<'_>| {
        !is_method_owner
            && unit.test.is_some_and(|test| {
                if let Some(observes) =
                    super::oracles::unittest_list::calls_owner(&unit.text, Some(test), owner)
                {
                    return observes;
                }
                let callees = owner_module_callees(test, owner);
                callees
                    .iter()
                    .any(|callee| oracle_operand_calls(&unit.text, callee))
                    || owner_result_locals(test, &callees)
                        .iter()
                        .any(|local| oracle_operand_names(&unit.text, local))
            })
    };
    // Dunder owner invoked through a builtin protocol entry (`reversed(Range(0))`
    // for `Range.__reversed__`): the entry call must take a receiver bound to the
    // owner class and be an asserted operand, directly or under materializers.
    let protocol_entry_observed = |unit: &AlignmentUnit<'_>| {
        is_method_owner
            && unit
                .test
                .is_some_and(|test| oracle_operand_protocol_entry(&unit.text, test, owner))
    };
    // Receiver/value identity for an attribute-assignment changed sink. The bare
    // attribute token (`status`) is collision-prone: a same-named field on an
    // unrelated receiver (`session.status` changed, oracle `conn.status == ...`)
    // would otherwise credit `changed_sink_token` on token coincidence. For an
    // attribute write `recv.attr = value`, credit only when the assertion
    // observes the receiver-qualified `recv.attr`, OR observes the assigned VALUE
    // together with the attribute name (co-observation defeats a common-literal
    // value coinciding in an unrelated oracle, while keeping legitimate
    // `obj.attr == value` field assertions). Non-attribute changed lines (returns,
    // method calls, comparisons) are not gated and keep prior behavior.
    let attribute_assignment = parse_attribute_assignment(line_text);
    let change_only_credit_ok = |unit: &AlignmentUnit<'_>| match attribute_assignment {
        None => true,
        Some((receiver, attr, rhs)) => {
            let qualified = [format!("{receiver}.{attr}")];
            let mut value_tokens = significant_change_tokens(rhs);
            value_tokens.retain(|token| token.len() >= 2);
            let attr_token = [attr.to_string()];
            observes(unit, &qualified)
                || (!value_tokens.is_empty()
                    && observes(unit, &value_tokens)
                    && observes(unit, &attr_token))
        }
    };
    // Changed-element identity for a dict-literal field-construction change (#1290).
    // A dict-literal change is localized to specific key(s) (`{"port": 8080}` ->
    // `9090` changes only `port`), but an assertion that merely calls the owner
    // and observes a SIBLING key (`build_config()["host"]`) or an aggregate
    // (`len(...)`) does not discriminate the change. Credit only when the
    // assertion observes the CHANGED element: its changed value unless the
    // oracle solely subscripts an unchanged sibling, a subscript of
    // the changed key, or a whole-collection comparison. Non-dict changes (and)
    // changes whose changed keys cannot be localized from the paired old line) are
    // not gated and keep prior behavior (pass-through `true`).
    let (element_old, element_new) = literal_lines
        .map(|(old, new)| (Some(old.as_str()), new.as_str()))
        .unwrap_or((old_line_text, line_text));
    let dict_change = dict_changed_keys_and_values(element_old, element_new);
    let list_change = if dict_change.is_none() {
        list_changed_indices_and_values(element_old, element_new)
    } else {
        None
    };
    let list_equality_discriminates = units
        .iter()
        .any(|unit| super::oracles::unittest_list::literal_text(&unit.text))
        && super::oracles::unittest_list::literal_change_discriminated(element_old, element_new);
    let field_construction_credit_ok = |unit: &AlignmentUnit<'_>| {
        let whole_collection_observed = if literal_lines.is_some() {
            unit.test
                .is_some_and(|test| oracle_compares_owner_collection(&unit.text, test, owner))
        } else {
            legacy_whole_collection_comparison(&unit.text)
        };
        if super::oracles::unittest_list::literal_text(&unit.text) {
            list_equality_discriminates
        } else if let Some((changed_keys, changed_values)) = &dict_change {
            oracle_observes_changed_dict_element_with_collection(
                &unit.text,
                changed_keys,
                changed_values,
                whole_collection_observed,
            )
        } else if let Some((changed_indices, changed_values)) = &list_change {
            oracle_observes_changed_list_element(
                &unit.text,
                changed_indices,
                changed_values,
                whole_collection_observed,
            )
        } else {
            true
        }
    };
    // F-string aggregate gate (#1290 1b): a length-invariant f-string change (only
    // literal text changed, interpolations unchanged) observed through a pure
    // `len(...)` aggregate is not discriminated — the output length is identical,
    // so `len` cannot notice the change. Under rule 11 the gate reads the one
    // assertion that credits: a string-equality assertion (`== "..."`) or a
    // format-spec change (which alters an interpolation, so it is not
    // length-invariant) keeps the credit. Pass-through for any non-f-string change.
    let length_invariant_literals = match old_line_text {
        Some(old) if fstring_change_is_length_invariant(old, line_text) => Some(
            fstring_template(line_text)
                .map(|(literals, _)| literals)
                .unwrap_or_default(),
        ),
        _ => None,
    };
    let fstring_credit_ok = |unit: &AlignmentUnit<'_>| {
        length_invariant_literals
            .as_deref()
            .is_none_or(|literals| !oracle_is_pure_len_aggregate(&unit.text, literals))
    };
    // The literal-element and f-string gates (#1290) apply to EVERY credit branch —
    // like the #1249 every-branch lesson — so a sibling-key / aggregate-only oracle
    // cannot sneak `exposed` through the direct/alias/changed_sink_token path of a
    // localized literal change. Both are pass-through `true` for unrelated changes.
    let owner_reference_tokens: Vec<String> = identity_tokens
        .iter()
        .chain(method_name_token.iter())
        .chain(alias_tokens.iter())
        .cloned()
        .collect();
    // Helper substitution supplies a comparison, not proof that a coroutine
    // or generator body ran. Cover every credit branch, including method-name
    // credit for writes outside ChangedEffect's modeled channels.
    let helper_owner_supported = |unit: &AlignmentUnit<'_>| {
        !unit.helper_derived
            || (!owner.decorators.iter().any(|name| name == "async_def")
                && !owner.term_facts.may_yield())
    };
    let gates_ok = |unit: &AlignmentUnit<'_>| {
        helper_owner_supported(unit)
            && field_construction_credit_ok(unit)
            && fstring_credit_ok(unit)
            && !expected_computed_through_owner(&unit.text, &owner_reference_tokens)
            && !owner_call_only_inside_opaque_wrapper(&unit.text, &owner_reference_tokens)
            && !assertion_calls_stand_in(unit, owner)
    };
    // A mock assertion credits only through the effect branch, where it pins
    // the arguments of the very call the changed line makes.
    let gates_ok = |unit: &AlignmentUnit<'_>| !unit.effect_only && gates_ok(unit);
    // What each related test can read of the changed effect, with its owner
    // identity, is a per-test fact: built once, then checked per assertion.
    let effect_reads = per_related_test(|unit: &AlignmentUnit<'_>| {
        Rc::new(effect.as_ref().zip(unit.test).and_then(|(effect, test)| {
            (method_owner_identity(unit)
                || free_fn_module_identity(unit)
                || calls_owner_through_module(test, owner))
            .then(|| EffectReads::of(effect, test, owner))
        }))
    });
    let effect_observed = |unit: &AlignmentUnit<'_>| {
        helper_owner_supported(unit)
            && effect_reads(unit)
                .as_ref()
                .as_ref()
                .is_some_and(|reads| reads.observed_by(&unit.text, unit.line))
            && field_construction_credit_ok(unit)
            && fstring_credit_ok(unit)
            && !assertion_calls_stand_in(unit, owner)
    };
    let branches: [CreditBranch<'_, '_>; 7] = [
        ("direct", "strong_oracle_observes_owner_name", &|unit| {
            observes_imported_owner(unit, &identity_tokens)
                && (method_owner_identity(unit) || free_fn_module_identity(unit))
                && gates_ok(unit)
        }),
        (
            "direct",
            "strong_oracle_observes_owner_method_on_bound_receiver",
            &|unit| {
                method_name_token
                    .as_ref()
                    .is_some_and(|token| observes(unit, std::slice::from_ref(token)))
                    && binds_method_receiver(unit)
                    && !asserts_foreign_receiver(unit)
                    && gates_ok(unit)
            },
        ),
        ("alias", "strong_oracle_observes_import_alias", &|unit| {
            // An alias binds only in the test that imports it: another test's
            // `from m import owner as quote` does not make this test's `quote`
            // the owner (#8247). Without source facts the pooled set stands.
            let own_aliases = unit.test.map(owner_aliases_in);
            observes_imported_owner(unit, own_aliases.as_deref().unwrap_or(&alias_tokens))
                && gates_ok(unit)
        }),
        (
            "direct",
            "strong_oracle_observes_owner_protocol_entry",
            &|unit| protocol_entry_observed(unit) && method_owner_identity(unit) && gates_ok(unit),
        ),
        (
            "direct",
            "strong_oracle_observes_owner_call_through_module",
            &|unit| module_call_observed(unit) && gates_ok(unit),
        ),
        // Gate the changed-sink-token path with the same free-function module
        // identity as the direct/alias paths: a same-named free function from a
        // different module must not credit `exposed` via this sibling branch
        // either (the #1249 every-branch lesson). Method owners are unaffected.
        (
            "changed_sink_token",
            "strong_oracle_observes_changed_sink_token",
            &|unit| {
                observes(unit, &delta_tokens)
                    && change_only_credit_ok(unit)
                    && gates_ok(unit)
                    && (method_owner_identity(unit) || free_fn_module_identity(unit))
            },
        ),
        (
            "direct",
            "strong_oracle_observes_changed_effect_channel",
            &effect_observed,
        ),
    ];
    // Branches keep their order; within a branch the first crediting assertion
    // in related-test order, then source order, is reported.
    let credit = branches.iter().find_map(|(alignment, reason, credits)| {
        units
            .iter()
            .find(|unit| credits(unit))
            .map(|unit| (*alignment, *reason, unit))
    });
    let (oracle_alignment, alignment_reason, observed_sink, credited) = match credit {
        Some((alignment, reason, unit)) => (
            alignment,
            reason,
            Some(unit.reported.clone()),
            Some(CreditedAssertion {
                test_name: unit.related.name.clone(),
                test_file: unit.related.file.clone(),
                text: unit.reported.clone(),
                kind: unit.kind.clone(),
            }),
        ),
        None => (
            "orthogonal",
            "strong_oracle_observes_different_sink",
            observed_sink,
            None,
        ),
    };
    SinkAlignment {
        changed_sink,
        observed_sink,
        oracle_alignment: oracle_alignment.to_string(),
        alignment_reason: alignment_reason.to_string(),
        credited,
    }
}

/// Memoize a fact that depends only on the related test holding an
/// assertion, so a test with many assertions computes it once per finding
/// rather than once per assertion and branch.
fn per_related_test<T: Clone>(
    fact: impl Fn(&AlignmentUnit<'_>) -> T,
) -> impl Fn(&AlignmentUnit<'_>) -> T {
    let cache: RefCell<HashMap<*const RelatedTest, T>> = RefCell::new(HashMap::new());
    move |unit| {
        let key = std::ptr::from_ref(unit.related);
        if let Some(found) = cache.borrow().get(&key) {
            return found.clone();
        }
        let found = fact(unit);
        cache.borrow_mut().insert(key, found.clone());
        found
    }
}

/// A credit branch: its `oracle_alignment`, its `alignment_reason`, and the
/// test it applies to one assertion.
type CreditBranch<'f, 'a> = (
    &'static str,
    &'static str,
    &'f dyn Fn(&AlignmentUnit<'a>) -> bool,
);

/// One strong assertion the alignment gates read (RIPR-SPEC-0238 rule 4).
struct AlignmentUnit<'a> {
    related: &'a RelatedTest,
    test: Option<&'a PythonTest>,
    /// The text every gate reads ([`super::PythonAssertion::gate_text`]).
    text: String,
    /// The assertion text a crediting finding reports.
    reported: String,
    /// The file line the assertion starts on, when it is a source assertion.
    line: Option<usize>,
    kind: OracleKind,
    /// A mock assertion admitted only to observe a changed call's arguments.
    effect_only: bool,
    /// Origin of the actual assertion, independent of its graded strength.
    helper_derived: bool,
}

/// Every strong assertion of every strong related test, in related-test order
/// then source order. For an error-path change only `exact_error_variant`
/// assertions are kept (rule 6). A related test whose source facts are not in
/// `all_tests` contributes its one reported oracle.
fn alignment_units<'a>(
    strong_tests: &[&'a RelatedTest],
    all_tests: &TestLookup<'a>,
    error_path: bool,
    focus: &PythonAssertionFocus,
    effect: Option<&ChangedEffect>,
) -> Vec<AlignmentUnit<'a>> {
    let admits = |kind: &OracleKind| !error_path || matches!(kind, OracleKind::ExactErrorVariant);
    let admits_mock = effect.is_some_and(ChangedEffect::admits_mock_assertion);
    let mut units = Vec::new();
    for related in strong_tests {
        let test = all_tests.matching(related).find(|test| {
            test.line == related.line && super::test_activation::activation_control(test).is_none()
        });
        let before = units.len();
        if let Some(test) = test {
            units.extend(test.assertions.iter().filter_map(|assertion| {
                let strong = assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                    && admits(&assertion.oracle_kind)
                    && focus.admits(assertion);
                let mock =
                    admits_mock && matches!(assertion.oracle_kind, OracleKind::MockExpectation);
                (strong || mock).then(|| AlignmentUnit {
                    related,
                    test: Some(test),
                    text: assertion.gate_text.clone(),
                    reported: assertion.text.clone(),
                    line: Some(assertion.line),
                    kind: assertion.oracle_kind.clone(),
                    effect_only: !strong,
                    helper_derived: assertion.helper_derived,
                })
            }));
        }
        // A related test whose source facts carry no admitted assertion (one
        // built without them) contributes its reported oracle.
        if units.len() == before
            && let Some(text) = related.oracle.as_ref()
            && admits(&related.oracle_kind)
            && related.oracle_strength.rank() >= OracleStrength::Strong.rank()
        {
            units.push(AlignmentUnit {
                related,
                test,
                text: text.clone(),
                reported: text.clone(),
                line: None,
                kind: related.oracle_kind.clone(),
                effect_only: false,
                // A legacy reported oracle must not wash out known helper
                // origin or borrow it from another assertion in this test.
                helper_derived: test.is_some_and(|test| {
                    test.assertions.iter().any(|assertion| {
                        assertion.helper_derived
                            && (assertion.text == *text || assertion.gate_text == *text)
                    })
                }),
            });
        }
    }
    units
}

/// Whether this assertion calls a function-scope stand-in of the owner
/// (or of an import alias) rather than the imported production function.
fn assertion_calls_stand_in(unit: &AlignmentUnit<'_>, owner: &PythonOwner) -> bool {
    let Some(test) = unit.test else {
        return false;
    };
    std::iter::once(owner.name.as_str())
        .chain(test.imports.iter().filter_map(|import| {
            (import.imported == owner.name
                && import_source_module_matches_owner(import, owner, &test.file))
            .then_some(import.alias.as_str())
        }))
        .any(|name| {
            oracle_operand_calls(&unit.text, name) && function_scope_stand_in(test, name, owner)
        })
}

/// Whether `name` is bound in the test function's own scope by a substitute
/// (assignment, local `def`/`class`, loop/`as` target, or a non-owner import).
/// Nested `def`/`class` bodies are a different scope, so a helper-local
/// shadow does not hide an outer assertion on the imported owner.
pub(super) fn function_scope_stand_in(test: &PythonTest, name: &str, owner: &PythonOwner) -> bool {
    if name.is_empty() {
        return false;
    }
    let Some((parameter_names, body)) = parse_test_function_scope(test) else {
        return false;
    };
    parameter_names.iter().any(|parameter| parameter == name)
        || suite_binds_stand_in(&body, name, test, owner)
        || binds_by_walrus(&test.body_text, name)
        || test.constant_rebinding.module_rebinds(name)
}

/// Whether `body_text` binds `name` with an assignment expression
/// (`name := ...`), which binds in the enclosing function scope wherever the
/// expression sits. Textual on purpose: a comment or string match only
/// withholds credit.
pub(super) fn binds_by_walrus(body_text: &str, name: &str) -> bool {
    body_text.match_indices(name).any(|(idx, _)| {
        let before_ok = body_text[..idx]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '.'));
        let rest = &body_text[idx + name.len()..];
        let after_ok = rest
            .chars()
            .next()
            .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
        before_ok && after_ok && rest.trim_start().starts_with(":=")
    })
}

/// Whether a `match` case pattern captures `name` (`case fee:`,
/// `case [*fee]`, `case {**fee}`, `case Cls() as fee`).
pub(super) fn pattern_binds_name(pattern: &ast::Pattern, name: &str) -> bool {
    let named =
        |bound: &Option<ast::Identifier>| bound.as_ref().is_some_and(|id| id.as_str() == name);
    let any =
        |patterns: &[ast::Pattern]| patterns.iter().any(|inner| pattern_binds_name(inner, name));
    match pattern {
        ast::Pattern::MatchValue(_) | ast::Pattern::MatchSingleton(_) => false,
        ast::Pattern::MatchSequence(sequence) => any(&sequence.patterns),
        ast::Pattern::MatchMapping(mapping) => named(&mapping.rest) || any(&mapping.patterns),
        ast::Pattern::MatchClass(class) => any(&class.patterns) || any(&class.kwd_patterns),
        ast::Pattern::MatchStar(star) => named(&star.name),
        ast::Pattern::MatchAs(alias) => {
            named(&alias.name)
                || alias
                    .pattern
                    .as_ref()
                    .is_some_and(|inner| pattern_binds_name(inner, name))
        }
        ast::Pattern::MatchOr(or) => any(&or.patterns),
    }
}

fn parse_test_function_scope(test: &PythonTest) -> Option<(Vec<String>, Vec<Stmt>)> {
    let source = dedent_python(&test.body_text);
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<python-test-body>"), &source)
    else {
        return None;
    };
    match module.body.len() {
        1 => match module.body.into_iter().next()? {
            Stmt::FunctionDef(function) => Some((argument_names(&function.args), function.body)),
            Stmt::AsyncFunctionDef(function) => {
                Some((argument_names(&function.args), function.body))
            }
            other => Some((Vec::new(), vec![other])),
        },
        _ => Some((Vec::new(), module.body)),
    }
}

fn argument_names(args: &ast::Arguments) -> Vec<String> {
    args.posonlyargs
        .iter()
        .chain(&args.args)
        .chain(&args.kwonlyargs)
        .map(|arg| arg.def.arg.to_string())
        .chain(args.vararg.iter().map(|arg| arg.arg.to_string()))
        .chain(args.kwarg.iter().map(|arg| arg.arg.to_string()))
        .filter(|name| !matches!(name.as_str(), "self" | "cls"))
        .collect()
}

fn suite_binds_stand_in(
    statements: &[Stmt],
    name: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> bool {
    statements.iter().any(|stmt| match stmt {
        Stmt::FunctionDef(function) => function.name.as_str() == name,
        Stmt::AsyncFunctionDef(function) => function.name.as_str() == name,
        Stmt::ClassDef(class) => class.name.as_str() == name,
        Stmt::Assign(assign) => assign
            .targets
            .iter()
            .any(|target| target_binds_name(target, name)),
        Stmt::AnnAssign(assign) => target_binds_name(&assign.target, name),
        Stmt::AugAssign(assign) => target_binds_name(&assign.target, name),
        Stmt::Delete(delete) => delete
            .targets
            .iter()
            .any(|target| target_binds_name(target, name)),
        Stmt::For(stmt) => {
            target_binds_name(&stmt.target, name)
                || suite_binds_stand_in(&stmt.body, name, test, owner)
                || suite_binds_stand_in(&stmt.orelse, name, test, owner)
        }
        Stmt::AsyncFor(stmt) => {
            target_binds_name(&stmt.target, name)
                || suite_binds_stand_in(&stmt.body, name, test, owner)
                || suite_binds_stand_in(&stmt.orelse, name, test, owner)
        }
        Stmt::While(stmt) => {
            suite_binds_stand_in(&stmt.body, name, test, owner)
                || suite_binds_stand_in(&stmt.orelse, name, test, owner)
        }
        Stmt::If(stmt) => {
            suite_binds_stand_in(&stmt.body, name, test, owner)
                || suite_binds_stand_in(&stmt.orelse, name, test, owner)
        }
        Stmt::With(stmt) => {
            stmt.items.iter().any(|item| {
                item.optional_vars
                    .as_ref()
                    .is_some_and(|target| target_binds_name(target, name))
            }) || suite_binds_stand_in(&stmt.body, name, test, owner)
        }
        Stmt::AsyncWith(stmt) => {
            stmt.items.iter().any(|item| {
                item.optional_vars
                    .as_ref()
                    .is_some_and(|target| target_binds_name(target, name))
            }) || suite_binds_stand_in(&stmt.body, name, test, owner)
        }
        Stmt::Try(stmt) => try_binds_stand_in(
            &stmt.body,
            &stmt.handlers,
            &stmt.orelse,
            &stmt.finalbody,
            name,
            test,
            owner,
        ),
        Stmt::TryStar(stmt) => try_binds_stand_in(
            &stmt.body,
            &stmt.handlers,
            &stmt.orelse,
            &stmt.finalbody,
            name,
            test,
            owner,
        ),
        Stmt::Match(stmt) => stmt.cases.iter().any(|case| {
            pattern_binds_name(&case.pattern, name)
                || suite_binds_stand_in(&case.body, name, test, owner)
        }),
        Stmt::Import(_) | Stmt::ImportFrom(_) => {
            let imports = collect_imports_from_statements(&test.file, std::slice::from_ref(stmt));
            imports.iter().any(|import| {
                import.alias == name
                    && !(import.imported == owner.name
                        && import_source_module_matches_owner(import, owner, &test.file))
            })
        }
        _ => false,
    })
}

fn try_binds_stand_in(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    name: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> bool {
    suite_binds_stand_in(body, name, test, owner)
        || handlers
            .iter()
            .any(|ast::ExceptHandler::ExceptHandler(handler)| {
                handler
                    .name
                    .as_ref()
                    .is_some_and(|bound| bound.as_str() == name)
                    || suite_binds_stand_in(&handler.body, name, test, owner)
            })
        || suite_binds_stand_in(orelse, name, test, owner)
        || suite_binds_stand_in(finalbody, name, test, owner)
}

fn target_binds_name(target: &Expr, name: &str) -> bool {
    match target {
        Expr::Name(bound) => bound.id.as_str() == name,
        Expr::Tuple(tuple) => tuple
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        Expr::List(list) => list
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        Expr::Starred(starred) => target_binds_name(&starred.value, name),
        _ => false,
    }
}

fn dedent_python(source: &str) -> String {
    let min_indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    if min_indent == 0 {
        return source.to_string();
    }
    source
        .lines()
        .map(|line| line.get(min_indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod helper_origin_tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn helper_origin_survives_reported_oracle_fallback_without_tainting_raw_assertions()
    -> Result<(), String> {
        let file = PathBuf::from("tests/test_stats.py");
        let source = "from src.stats import Stats\n\ndef check_writes(obj, want):\n    assert obj.writes >= want\n\ndef test_writes():\n    stats = Stats()\n    check_writes(stats, 0)\n    assert stats.hits >= 0\n";
        let facts = super::super::source_facts::extract_source_facts(&file, source);
        let mut tests = facts.tests;
        let modules = BTreeMap::from([(file, facts.helper_module)]);
        super::super::helper_assertions::apply_helper_assertions(&mut tests, &modules);
        assert_eq!(tests.len(), 1);
        let test = &tests[0];
        let helper = test
            .assertions
            .iter()
            .find(|assertion| assertion.helper_derived)
            .ok_or("the existing helper pass must supply the control assertion")?;
        let raw = test
            .assertions
            .iter()
            .find(|assertion| !assertion.helper_derived)
            .ok_or("an unrelated raw assertion must remain in the same test")?;
        let lookup = TestLookup::new(&tests);
        for (oracle, expected_helper) in [
            (helper.text.clone(), true),
            (helper.gate_text.clone(), true),
            (raw.text.clone(), false),
        ] {
            // A legacy reported row can retain stronger grading than current
            // source facts. Neither weak source assertion enters the ordinary
            // unit list, so the reported-oracle fallback must preserve origin.
            let related = RelatedTest {
                name: test.name.clone(),
                file: test.file.clone(),
                line: test.line,
                oracle: Some(oracle),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
                miss_support: None,
                role: crate::domain::RelatedTestRole::Contributing,
            };
            let units = alignment_units(
                &[&related],
                &lookup,
                false,
                &PythonAssertionFocus::for_family(ProbeFamily::Predicate),
                None,
            );
            assert_eq!(units.len(), 1);
            assert_eq!(units[0].line, None, "control must reach fallback");
            assert_eq!(units[0].helper_derived, expected_helper);
        }
        Ok(())
    }
}
