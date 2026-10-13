//! RIPR-SPEC-0254: a changed entry of a lookup table the owner returns from.
//!
//! `const table = { apple: 120, pear: 95 };` with `return table[sku] ?? 0;`
//! changes only what the owner returns for the keys whose value moved. An
//! exact assertion on `fetchUnitPrice("pear")` sees the change; one on
//! `fetchUnitPrice("apple")` passes on both versions. The change reads as a
//! return-value change, credited only when a strong assertion observes a
//! whole owner call whose key argument is a changed key.

use super::*;

/// The facts of a lookup-table change: which owner parameter selects the
/// entry, and the keys whose value the change moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LookupTableChange {
    pub(crate) param_index: usize,
    pub(crate) changed_keys: Vec<String>,
}

/// The `(name, entries)` of a one-line `const name = { k: v, ... };` (or
/// `let`, with an optional type annotation) whose entries are all
/// `key: scalar literal`, or `None`.
fn table_declaration(line_text: &str) -> Option<(String, Vec<(String, String)>)> {
    let masked = ts_mask_literals_and_comments(line_text);
    if masked.len() != line_text.len() {
        return None;
    }
    let trimmed_start = masked.len() - masked.trim_start().len();
    let rest = &masked[trimmed_start..];
    let after_keyword = rest
        .strip_prefix("const ")
        .or_else(|| rest.strip_prefix("let "))?;
    let after_keyword = after_keyword.trim_start();
    let name = leading_identifier(after_keyword)?.to_string();
    let open = masked.find('{')?;
    let close = masked.rfind('}')?;
    // Only an optional type annotation sits between the name and `=`, and
    // the literal is the whole initializer.
    let header_end = open.checked_sub(masked.len() - after_keyword.len())?;
    let annotation = after_keyword.get(name.len()..header_end)?.trim();
    let annotation = annotation.strip_suffix('=')?.trim();
    if !(annotation.is_empty() || annotation.starts_with(':')) || annotation.contains('{') {
        return None;
    }
    if masked[close + 1..].trim() != ";" {
        return None;
    }
    let inner = &masked[open + 1..close];
    if inner.contains(['{', '}', '[', ']', '(', ')']) {
        return None;
    }
    let mut entries = Vec::new();
    let mut offset = open + 1;
    for part in inner.split(',') {
        let start = offset;
        offset += part.len() + 1;
        if part.trim().is_empty() {
            continue;
        }
        let colon = part.find(':')?;
        let raw_key = line_text[start..start + colon].trim();
        let key = table_key(raw_key)?;
        let value = line_text[start + colon + 1..start + part.len()].trim();
        if !is_scalar_literal(value) {
            return None;
        }
        if entries.iter().any(|(existing, _)| existing == key) {
            return None;
        }
        entries.push((key.to_string(), value.to_string()));
    }
    (!entries.is_empty()).then_some((name, entries))
}

/// The property name a table key stores, or `None` when the stored name
/// could differ from the source spelling. JavaScript stores `{ 1e3: 5 }`
/// under `"1000"` and `{ 0x10: 5 }` under `"16"`, and `__proto__: v` sets
/// the prototype instead of a property, so an unquoted key must be an
/// identifier or a plain decimal integer, and `__proto__` is refused.
fn table_key(raw_key: &str) -> Option<&str> {
    let key = match string_literal_content(raw_key) {
        Some(quoted) => quoted
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '-'))
            .then_some(quoted)?,
        None => {
            let identifier = raw_key
                .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
                && raw_key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$'));
            // Past 15 digits a number key may round (`12345678901234567890`
            // is stored as `"12345678901234567000"`).
            let integer = raw_key == "0"
                || (raw_key.starts_with(|c: char| matches!(c, '1'..='9'))
                    && raw_key.len() <= 15
                    && raw_key.chars().all(|c| c.is_ascii_digit()));
            (identifier || integer).then_some(raw_key)?
        }
    };
    (!key.is_empty() && key != "__proto__").then_some(key)
}

/// The contents of a one-quote string literal with no quote or escape
/// inside (`"a" + x + "b"` is not one), or `None`.
fn string_literal_content(value: &str) -> Option<&str> {
    ['"', '\''].iter().find_map(|quote| {
        value
            .strip_prefix(*quote)?
            .strip_suffix(*quote)
            .filter(|inner| !inner.contains(['"', '\'', '`', '\\']))
    })
}

fn is_scalar_literal(value: &str) -> bool {
    // `numeric_literal_value` drops `_` separators first, so `_1` (an
    // identifier) would read as `1`; a number starts with a digit. It also
    // drops a BigInt's `n`, so `95` and `95n` (a different type) would read
    // as one value; a BigInt is not a scalar here.
    (value.starts_with(|c: char| c.is_ascii_digit())
        && !value.ends_with('n')
        && numeric_literal_value(value).is_some())
        || matches!(value, "true" | "false" | "null")
        || string_literal_content(value).is_some()
}

/// The type of a scalar literal: number, string, boolean or `null`.
fn literal_type(value: &str) -> u8 {
    if numeric_literal_value(value).is_some() {
        0
    } else if string_literal_content(value).is_some() {
        1
    } else if value == "null" {
        3
    } else {
        2
    }
}

/// Whether two literal values differ: numerically for two numbers, by
/// contents for two strings (`'x'` and `"x"` are the same), by text
/// otherwise. A move between a number, a string and a boolean does not
/// count: loose equality (`assert.equal`) can read `95` and `"95"`, or `1`
/// and `true`, as the same value.
fn values_differ(old: &str, new: &str) -> bool {
    let (old_type, new_type) = (literal_type(old), literal_type(new));
    if old_type != new_type && old_type != 3 && new_type != 3 {
        return false;
    }
    if let (Some(old), Some(new)) = (
        numeric_literal_value(old).and_then(|v| v.parse::<f64>().ok()),
        numeric_literal_value(new).and_then(|v| v.parse::<f64>().ok()),
    ) {
        // Exact comparison is the intent: `1` and `1.0` name the same value,
        // `0.1` and `0.10000001` do not.
        return old.total_cmp(&new).is_ne();
    }
    if let (Some(old), Some(new)) = (string_literal_content(old), string_literal_content(new)) {
        return old != new;
    }
    old != new
}

/// RIPR-SPEC-0254 rule 1: the lookup-table change on `line_text`, when the
/// old and new lines declare the same table with the same keys in the same
/// order, at least one value moved, and the owner reads the table exactly
/// once, as `return table[param]` (optionally `?? <literal>`), with `param`
/// a parameter the body never rebinds.
pub(crate) fn ts_lookup_table_change(
    owner: &TypeScriptOwner,
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<LookupTableChange> {
    let (name, new_entries) = table_declaration(line_text)?;
    let (old_name, old_entries) = table_declaration(old_line_text?)?;
    if name != old_name
        || new_entries.len() != old_entries.len()
        || new_entries
            .iter()
            .zip(&old_entries)
            .any(|((new_key, _), (old_key, _))| new_key != old_key)
    {
        return None;
    }
    // Each moved key with its old and new values.
    let moved: Vec<(String, String, String)> = new_entries
        .iter()
        .zip(&old_entries)
        .filter(|((_, new_value), (_, old_value))| values_differ(old_value, new_value))
        .map(|((key, new_value), (_, old_value))| {
            (key.clone(), old_value.clone(), new_value.clone())
        })
        .collect();
    if moved.is_empty() {
        return None;
    }
    let source = owner.source_text.as_deref()?;
    if source.lines().filter(|line| *line == line_text).count() != 1 {
        return None;
    }
    let body = masked_owner_body(source)?;
    // An arrow owner's body starts after `=>`; step into its block. A
    // function body starts inside its block already, so a leading `{` there
    // opens an inner block and stays.
    let masked_source = ts_mask_literals_and_comments(source);
    let arrow = masked_source
        .get(..masked_source.len().saturating_sub(body.len()))
        .is_some_and(|head| head.trim_end().ends_with("=>"));
    let body = match body.trim_start().strip_prefix('{') {
        Some(block) if arrow => block.to_string(),
        _ => body,
    };
    // One `return`, no nested function, and the table named only by its
    // declaration and that one read.
    if body.contains("=>")
        || !identifier_offsets(&body, "function").is_empty()
        || identifier_offsets(&body, "return").len() != 1
        || identifier_offsets(&body, &name).len() != 2
    {
        return None;
    }
    // The declaration must be a top-level statement of the body: a table
    // declared in an inner block leaves the `return` reading another one.
    let declaration = *identifier_offsets(&body, &name).first()?;
    let before = &body[..declaration];
    if before.matches('{').count() != before.matches('}').count() {
        return None;
    }
    let returned = body
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("return "))?;
    let read = returned
        .strip_prefix("return ")?
        .trim_end()
        .strip_suffix(';')?
        .trim();
    // The value `?? fallback` returns for a `null` entry, when the masked
    // text spells it: a number, `true`, `false` or `null` (a string's
    // contents are masked, and `undefined` is not a table value).
    let mut fallback_value: Option<Option<String>> = None;
    let read = match read.split_once("??") {
        Some((read, fallback)) => {
            let fallback = fallback.trim();
            // `fallback` is masked, so a string's contents are blank here and
            // must not be compared.
            fallback_value = Some(
                (is_scalar_literal(fallback) && string_literal_content(fallback).is_none())
                    .then(|| fallback.to_string()),
            );
            // The masked fallback blanks a string's contents but keeps its
            // quotes, so a whole string literal is one quote pair with no
            // quote inside; `"x" ? a : b` or `"x", 0` is an expression.
            if !(numeric_literal_value(fallback).is_some()
                || matches!(fallback, "true" | "false" | "null" | "undefined")
                || masked_string_literal(fallback))
            {
                return None;
            }
            read.trim()
        }
        None => read,
    };
    let key_param = read
        .strip_prefix(name.as_str())?
        .trim_start()
        .strip_prefix('[')?
        .strip_suffix(']')?
        .trim();
    let param_index = owner.params.iter().position(|param| param == key_param)?;
    // `?? fallback` turns a `null` entry into the fallback, so a move to or
    // from `null` counts only when the values the owner returns still
    // differ with the fallback in its place (`95` to `null` under `?? 0`
    // returns 95, then 0); an unread fallback refuses the move.
    let changed_keys: Vec<String> = moved
        .into_iter()
        .filter(|(_, old_value, new_value)| match &fallback_value {
            Some(_) if old_value != "null" && new_value != "null" => true,
            None => true,
            Some(None) => false,
            Some(Some(fallback)) => {
                let returned = |value: &str| {
                    if value == "null" {
                        fallback.clone()
                    } else {
                        value.to_string()
                    }
                };
                values_differ(&returned(old_value), &returned(new_value))
            }
        })
        .map(|(key, _, _)| key)
        .collect();
    if changed_keys.is_empty() {
        return None;
    }
    ts_parameter_is_stable(source, key_param).then_some(LookupTableChange {
        param_index,
        changed_keys,
    })
}

/// Whether a masked expression is exactly one quoted string literal.
fn masked_string_literal(masked: &str) -> bool {
    ['"', '\''].iter().any(|quote| {
        masked
            .strip_prefix(*quote)
            .and_then(|rest| rest.strip_suffix(*quote))
            .is_some_and(|inner| !inner.contains(['"', '\'', '`']))
    })
}

/// The string a quoted literal argument spells, or `None`.
fn string_literal_argument(argument: &str) -> Option<&str> {
    let argument = argument.trim();
    ['"', '\''].iter().find_map(|quote| {
        argument
            .strip_prefix(*quote)?
            .strip_suffix(*quote)
            .filter(|inner| !inner.contains(['\\', *quote]))
    })
}

/// RIPR-SPEC-0254 rule 2: whether a strong exact assertion of an oracle
/// relation observes a whole owner call (`owner(...)`, a rename local, or a
/// local bound once to that call) whose key argument is a string literal
/// naming a changed key.
pub(crate) fn ts_lookup_table_witnessed(
    change: &LookupTableChange,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .filter(|candidate| {
            !local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name)
        })
        .any(|candidate| {
            let receivers =
                owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
            let names = ts_owner_call_names(candidate, owner, alias_map, workspace_root);
            candidate.test.assertions.iter().any(|assertion| {
                if assertion.oracle_strength.rank() < OracleStrength::Strong.rank()
                    || !matches!(
                        assertion.oracle_kind,
                        OracleKind::ExactValue | OracleKind::WholeObjectEquality
                    )
                    // An unpinned expected side (`expect.any(Number)`, or the
                    // owner call itself) cannot tell the two versions apart.
                    || !assertion_expected_side_is_pinned(assertion)
                {
                    return false;
                }
                ts_observed_owner_call_arguments(
                    candidate.test,
                    assertion,
                    &names,
                    owner,
                    &receivers,
                )
                .into_iter()
                .any(|arguments| {
                    arguments
                        .get(change.param_index)
                        .and_then(|argument| string_literal_argument(argument))
                        .is_some_and(|key| change.changed_keys.iter().any(|k| k == key))
                })
            })
        })
}
