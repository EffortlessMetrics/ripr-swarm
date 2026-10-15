//! RIPR-SPEC-0250 rule 1: `toMatchObject({ key: literal })` pins a changed
//! field of the object the owner returns.
//!
//! `return { name, adult: 18 <= age, tags: [] };` with
//! `expect(buildProfile("bo", 18)).toMatchObject({ name: "bo", adult: true })`
//! compares `adult` exactly, as `toEqual` would, and ignores only the fields
//! it does not list. The matcher alone is not an exact discriminator, so it
//! reads as one only for a field-construction change, only on the whole
//! result of an owner call, and only when the changed line is a top-level
//! `key: value` of the one object literal the owner returns and the matcher
//! lists `key`.

use super::*;

/// The top-level keys of a flat object literal such as
/// `{ name: "ana", adult: false }`, or `None` when the text is not one.
/// Callers pass only matcher arguments `is_literal_argument` accepted, whose
/// values are scalar literals.
fn flat_object_keys(object_text: &str) -> Option<Vec<String>> {
    let masked = ts_mask_literals_and_comments(object_text);
    if masked.len() != object_text.len() {
        return None;
    }
    let trimmed = masked.trim();
    let inner_start = masked.find('{')? + 1;
    let inner_end = masked.rfind('}')?;
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') || inner_end < inner_start {
        return None;
    }
    let inner = &masked[inner_start..inner_end];
    if inner.contains(['{', '}', '[', ']']) {
        return None;
    }
    let mut keys = Vec::new();
    let mut offset = inner_start;
    for part in inner.split(',') {
        let start = offset;
        offset += part.len() + 1;
        if part.trim().is_empty() {
            continue;
        }
        let colon = part.find(':')?;
        let raw_key = object_text[start..start + colon].trim();
        let key = raw_key
            .strip_prefix('"')
            .and_then(|key| key.strip_suffix('"'))
            .or_else(|| {
                raw_key
                    .strip_prefix('\'')
                    .and_then(|key| key.strip_suffix('\''))
            })
            .unwrap_or(raw_key);
        if key.is_empty() {
            return None;
        }
        keys.push(key.to_string());
    }
    Some(keys)
}

/// RIPR-SPEC-0250 rule 1 (amended 2026-10-15): a test-body
/// `const <name> = { key: literal, ... };` that `toMatchObject(<name>)` may
/// read in place of the name. `const` stops reassignment, not mutation, so
/// the body must name `<name>` exactly twice, in its binding and in the one
/// matcher argument: any write (`grown.adult = false`), alias, helper call,
/// closure or second assertion spells the name a third time and withholds
/// it. A body containing `eval`, `with` or a `\u` escape can reach the
/// object without spelling it, so it reads no object `const`.
#[derive(Clone, Debug)]
pub(crate) struct MatchObjectConst {
    name: String,
    text: String,
    declared_end: u32,
}

/// The test body's [`MatchObjectConst`]s.
pub(crate) fn match_object_consts(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &str,
) -> Vec<MatchObjectConst> {
    let (Some(first), Some(last)) = (statements.first(), statements.last()) else {
        return Vec::new();
    };
    let Some(body) = source.get(first.span().start as usize..last.span().end as usize) else {
        return Vec::new();
    };
    if whole_word_count(body, "eval") > 0
        || whole_word_count(body, "with") > 0
        || body.contains("\\u")
    {
        return Vec::new();
    }
    let mut consts = Vec::new();
    for stmt in statements {
        let Statement::VariableDeclaration(declaration) = stmt else {
            continue;
        };
        if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const || declaration.declare {
            continue;
        }
        for declarator in &declaration.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            let Some(Expression::ObjectExpression(object)) = declarator
                .init
                .as_ref()
                .map(Expression::get_inner_expression)
            else {
                continue;
            };
            if !safe_mock_expected_object(object) || whole_word_count(body, &id.name) != 2 {
                continue;
            }
            let Some(text) = source.get(object.span.start as usize..object.span.end as usize)
            else {
                continue;
            };
            consts.push(MatchObjectConst {
                name: id.name.to_string(),
                text: text.to_string(),
                declared_end: declaration.span.end,
            });
        }
    }
    consts
}

/// The literal object `expr`'s `expect(...).toMatchObject(<name>)` argument
/// stands for, when `<name>` is one of `consts` declared before the call.
/// `.not`, `.resolves` and `.rejects` chains read nothing.
pub(crate) fn match_object_const_argument(
    consts: &[MatchObjectConst],
    expr: &Expression<'_>,
) -> Option<String> {
    let Expression::CallExpression(call) = expr.get_inner_expression() else {
        return None;
    };
    let member = call.callee.as_member_expression()?;
    if member.static_property_name()? != "toMatchObject" {
        return None;
    }
    let Expression::CallExpression(subject) = member.object().get_inner_expression() else {
        return None;
    };
    if !matches!(subject.callee.get_inner_expression(), Expression::Identifier(callee) if callee.name == "expect")
    {
        return None;
    }
    let [argument] = call.arguments.as_slice() else {
        return None;
    };
    let Some(Expression::Identifier(ident)) = argument
        .as_expression()
        .map(Expression::get_inner_expression)
    else {
        return None;
    };
    consts
        .iter()
        .find(|known| known.name == ident.name.as_str() && known.declared_end <= call.span.start)
        .map(|known| known.text.clone())
}

/// How many times `word` occurs in `text` as a whole identifier word,
/// strings and comments included (an extra mention only withholds).
fn whole_word_count(text: &str, word: &str) -> usize {
    let part =
        |ch: Option<char>| ch.is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric());
    text.match_indices(word)
        .filter(|(at, _)| {
            !part(text[..*at].chars().next_back()) && !part(text[at + word.len()..].chars().next())
        })
        .count()
}

/// The key of the changed field when `line_text` is a top-level
/// `key: value` line of the one object literal the owner returns
/// (`return {` ... `};`), and nothing else in that literal sets `key`.
pub(crate) fn ts_owner_returned_field_key(owner_source: &str, line_text: &str) -> Option<String> {
    let blank = |text: &str| ts_mask_literals_and_comments(text).replace(['"', '\'', '`'], " ");
    let masked = blank(owner_source);
    let lines: Vec<&str> = masked.lines().collect();
    if lines.len() != owner_source.lines().count() {
        return None;
    }
    // One `return`, and no nested function whose `return` it could be.
    let body = masked_owner_body(owner_source)?;
    if body.contains("=>")
        || !identifier_offsets(&body, "function").is_empty()
        || identifier_offsets(&body, "return").len() != 1
    {
        return None;
    }
    let target = blank(line_text);
    let target = target.trim();
    let key = leading_identifier(target)?;
    let value = target[key.len()..].trim_start().strip_prefix(':')?;
    // The value stays on this line: no nested literal or block opens here.
    // A string value blanks to spaces, so emptiness reads the quotes.
    let quoted = ts_mask_literals_and_comments(line_text);
    let has_value = quoted
        .trim()
        .get(key.len()..)
        .and_then(|rest| rest.trim_start().strip_prefix(':'))
        .is_some_and(|rest| !rest.trim().trim_end_matches(',').trim().is_empty());
    if !has_value || value.contains(['{', '}', '[', ']']) {
        return None;
    }
    let mut matching = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == target)
        .map(|(index, _)| index);
    let changed = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let (open_line, open_column) = enclosing_open_brace(&lines, changed)?;
    let before = lines[open_line][..open_column].trim_end();
    let before_return = before.strip_suffix("return")?;
    if before_return
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
    {
        return None;
    }
    let (close_line, close_column) = matching_close_brace(&lines, open_line, open_column)?;
    // Every top-level entry of the literal, the changed line's own included,
    // must be a plain `name: value`, `name,` or `name()` entry, and exactly
    // one may name `key`: a quoted, computed or numeric key, a spread or an
    // accessor could set `key` again where this scan cannot see it.
    let kept = ts_mask_literals_and_comments(owner_source);
    let kept_lines: Vec<&str> = kept.lines().collect();
    if kept_lines.len() != lines.len() {
        return None;
    }
    let mut literal = String::new();
    for (index, line) in kept_lines
        .iter()
        .enumerate()
        .take(close_line + 1)
        .skip(open_line)
    {
        let start = if index == open_line {
            open_column + 1
        } else {
            0
        };
        let end = if index == close_line {
            close_column
        } else {
            line.len()
        };
        literal.push_str(line.get(start..end)?);
        literal.push('\n');
    }
    // The changed line must hold exactly one entry, the one named `key`,
    // opening at the start of the line and ending on it. A changed line that
    // continues another entry's value (the `adult : x` arm of a multi-line
    // `status: flag ?` ternary) or that also holds a later entry
    // (`adult: x, senior: <changed>`) only looks like the `key` field.
    let mut named = 0usize;
    let mut on_changed_line = 0usize;
    let mut key_owns_changed_line = false;
    for (offset, raw) in top_level_entries(&literal) {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let name = leading_identifier(entry)?;
        let after = entry[name.len()..].trim_start();
        if !(after.is_empty() || after.starts_with([':', '('])) {
            return None; // `get adult()`, `async adult()`, `adult?:`
        }
        let entry_start = offset + (raw.len() - raw.trim_start().len());
        let starts_on_changed = open_line + literal[..entry_start].matches('\n').count() == changed;
        if starts_on_changed {
            on_changed_line += 1;
        }
        if name == key {
            named += 1;
            let line_start = literal[..entry_start].rfind('\n').map_or(0, |at| at + 1);
            key_owns_changed_line = starts_on_changed
                && literal[line_start..entry_start].trim().is_empty()
                && !entry.contains('\n');
        }
    }
    (named == 1 && on_changed_line == 1 && key_owns_changed_line).then(|| key.to_string())
}

/// The entries of an object literal's body, split at its top-level commas,
/// each with its byte offset in `literal`.
fn top_level_entries(literal: &str) -> Vec<(usize, &str)> {
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, c) in literal.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push((start, &literal[start..index]));
                start = index + 1;
            }
            _ => {}
        }
    }
    entries.push((start, &literal[start..]));
    entries
}

/// The line and column of the `{` that opens the literal holding line
/// `changed`, when every line between them balances.
fn enclosing_open_brace(lines: &[&str], changed: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for index in (0..changed).rev() {
        let line = lines[index];
        for (column, c) in line.char_indices().rev() {
            match c {
                '}' => depth += 1,
                '{' if depth == 0 => return Some((index, column)),
                '{' => depth -= 1,
                _ => {}
            }
        }
    }
    None
}

/// The line and column of the `}` that closes the `{` at (`open_line`, `open_column`)
/// when only `;` follows it there, or nothing follows and the next line
/// closes the owner.
fn matching_close_brace(
    lines: &[&str],
    open_line: usize,
    open_column: usize,
) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for (index, line) in lines.iter().enumerate().skip(open_line) {
        let start = if index == open_line { open_column } else { 0 };
        for (column, c) in line[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        let after = line[start + column + 1..].trim();
                        let closes_owner = || {
                            lines[index + 1..]
                                .iter()
                                .find(|next| !next.trim().is_empty())
                                .is_some_and(|next| next.trim().starts_with('}'))
                        };
                        return (after == ";" || (after.is_empty() && closes_owner()))
                            .then_some((index, start + column));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Collect the credited `toMatchObject` assertions for a field-construction
/// change: each on an oracle relation, on the whole result of one owner call
/// (`owner(...)`, `ns.owner(...)`, awaited, or a local bound once to that
/// call), with a flat literal object that lists the changed field's key.
pub(crate) fn ts_match_object_facts(
    probe_family: &ProbeFamily,
    line_text: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    if !matches!(probe_family, ProbeFamily::FieldConstruction) {
        return facts;
    }
    let pins = |assertion: &TypeScriptAssertion| {
        assertion.matcher == "toMatchObject"
            && matches!(assertion.oracle_kind, OracleKind::RelationalCheck)
            && assertion.error_payload.is_none()
            && (assertion.expected_value_or_variant.is_some()
                || assertion.expected_object_const.is_some())
    };
    if !candidates
        .iter()
        .any(|candidate| candidate.test.assertions.iter().any(pins))
    {
        return facts;
    }
    let Some(key) = owner
        .source_text
        .as_deref()
        .and_then(|source| ts_owner_returned_field_key(source, line_text))
    else {
        return facts;
    };
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name) {
            continue;
        }
        let receivers =
            unshadowed_owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
        for assertion in candidate.test.assertions.iter().filter(|a| pins(a)) {
            let lists_key = assertion
                .expected_value_or_variant
                .as_deref()
                .or(assertion.expected_object_const.as_deref())
                .and_then(flat_object_keys)
                .is_some_and(|keys| keys.contains(&key));
            if lists_key
                && ts_observes_whole_owner_result_through_identity(
                    candidate.test,
                    assertion,
                    owner,
                    &receivers,
                )
            {
                facts.credit(&candidate.test.file, assertion);
            }
        }
    }
    facts
}
