//! Unreached `match` arm and `dict.get` default evidence for the Python
//! preview adapter (#9429; extends RIPR-SPEC-0260).
//!
//! `guard_activation.rs` proves that a changed line inside an `if` arm never
//! runs under any observed input. Two more branch shapes hide an edit from
//! every observed input the same way:
//!
//! - a `case` arm of a `match` statement whose pattern no observed literal
//!   input matches (or that an earlier arm already took);
//! - the fallback argument of `TABLE.get(key, default)` when every observed
//!   key is one the module-level literal table holds.
//!
//! A third shape hides the edit behind a further method call (#9429): a
//! classmethod's `return cls(0, 0.0)` -> `cls(0, 0.2)` is observed only through
//! `Invoice.free().total()`, which prices an amount of 0 to 0 at any rate.
//! `python_chained_receiver_gap` reads that bounded form.
//!
//! In the first two, reach plus a strong exact-value oracle does not discriminate the
//! change, because the edited expression is never evaluated. Like the guard
//! rule, this one engages only when static evidence reads everything it
//! needs and otherwise fails open and keeps the existing verdict: a subject
//! that is not a bare owner parameter, a pattern outside the small literal
//! and builtin-class vocabulary, a table that is not one literal dict, a test
//! that mentions the table, or a strong call no row binds.

use super::PythonOwner;
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::boundary::{
    CallRow, literal_value, resolve_operand, strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::guard_activation::{
    GuardedArmGap, compare_literal_values, leading_whitespace, line_rebinds_parameter,
    parameter_rebound_before,
};
use super::module_constants::mentions_identifier;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::{OracleStrength, OwnerKind, ValueContext, ValueFact};

/// Whether one row reaches the changed expression; `Some(true)` means the row
/// provably never evaluates it, `None` that static evidence cannot tell.
type Exclusion = Option<bool>;

/// A changed line that sits directly in the body of one `case` arm.
struct MatchArm {
    subject: String,
    /// 0-based index of the `match` header line.
    header_index: usize,
    /// Patterns of the `case` arms that precede the changed arm.
    earlier: Vec<String>,
    /// The changed arm's pattern.
    pattern: String,
}

/// A changed `TABLE.get(key, default)` whose edit is only the default.
struct DefaultSite {
    table: String,
    key: String,
    /// 0-based index of the changed line.
    line_index: usize,
}

/// Evidence that the edit sits on an arm or default no strong related call
/// reaches. Returns `None` when the line is neither shape, when any observed
/// input may reach it, or when static evidence cannot read the structure.
pub(super) fn python_unreached_arm_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    new_line: &str,
    old_line: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<GuardedArmGap> {
    let arm = match_arm(owner, owner_source, changed_line);
    let default = old_line.and_then(|old| {
        default_site(
            old,
            new_line,
            changed_line.checked_sub(1)?,
            owner,
            owner_source,
            related_candidates,
        )
    });
    if arm.is_none() && default.is_none() {
        return None;
    }
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    if rows.is_empty() || gaps {
        // The gate's premise is "EVERY strong call provably misses the
        // edit", which readable rows alone cannot establish.
        return None;
    }
    let (discriminator, reason) = if let Some(arm) = &arm {
        let every_row_misses = rows
            .iter()
            .all(|row| arm_excludes(arm, row, owner, owner_source) == Some(true));
        if !every_row_misses {
            return None;
        }
        let observed = observed_subject_values(&arm.subject, &rows);
        (
            format!("{} matching `case {}`", arm.subject, arm.pattern),
            format!(
                "no strong related test input reaches `case {}`; observed {} values: {}; the changed line never executes under an observed input",
                arm.pattern,
                arm.subject,
                observed.join(", ")
            ),
        )
    } else {
        let default = default.as_ref()?;
        let table = literal_table_keys(owner_source, &default.table)?;
        let every_row_hits = rows.iter().all(|row| {
            default_excludes(default, &table, row, &constants, owner, owner_source) == Some(true)
        });
        if !every_row_hits {
            return None;
        }
        let observed = observed_subject_values(&default.key, &rows);
        (
            format!("{} not in {}", default.key, default.table),
            format!(
                "every strong related test looks up a key `{}` holds{}; the `.get` default never runs under an observed input",
                default.table,
                if observed.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (observed {} values: {})",
                        default.key,
                        observed.join(", ")
                    )
                }
            ),
        )
    };
    let subject = arm
        .as_ref()
        .map(|arm| arm.subject.as_str())
        .or(default.as_ref().map(|default| default.key.as_str()));
    let mut observed_values: Vec<ValueFact> = rows
        .iter()
        .flat_map(|row| {
            row.bindings
                .iter()
                .filter(|(parameter, binding)| {
                    !binding.from_default && subject.is_some_and(|name| name == parameter.as_str())
                })
                .map(|(parameter, binding)| ValueFact {
                    file: None,
                    line: row.line,
                    text: row.text.clone(),
                    value: format!("{parameter} = {}", binding.value),
                    context: ValueContext::FunctionArgument,
                })
        })
        .collect();
    observed_values.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.value.cmp(&right.value))
            .then(left.text.cmp(&right.text))
    });
    observed_values.dedup();
    Some(GuardedArmGap {
        discriminator,
        observed_values,
        reason,
        chained_receiver: false,
    })
}

fn observed_subject_values(subject: &str, rows: &[CallRow]) -> Vec<String> {
    let mut values: Vec<String> = rows
        .iter()
        .filter_map(|row| row.bindings.get(subject).map(|b| b.value.clone()))
        .collect();
    values.sort();
    values.dedup();
    values
}

// ---------------------------------------------------------------- match arms

/// The `case` arm whose body directly holds `changed_line`, with the arms
/// before it. None when the line is not a direct arm body line, the subject
/// is not a bare name, or the nesting cannot be read.
fn match_arm(owner: &PythonOwner, owner_source: &str, changed_line: usize) -> Option<MatchArm> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_index = changed_line.checked_sub(1)?;
    let changed = lines.get(changed_index)?;
    if owner.start_line == 0 || changed.trim().is_empty() {
        return None;
    }
    let body_indent = leading_whitespace(changed);
    // The nearest preceding line that is less indented opens the changed
    // line's block; it must be the `case` header.
    let case_index = (owner.start_line..changed_index).rev().find(|index| {
        let raw = lines[*index];
        !raw.trim().is_empty()
            && !raw.trim_start().starts_with('#')
            && leading_whitespace(raw) < body_indent
    })?;
    let case_indent = leading_whitespace(lines[case_index]);
    let pattern = case_pattern(lines[case_index])?;
    let mut earlier = Vec::new();
    let mut header_index = None;
    for index in (owner.start_line..case_index).rev() {
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_whitespace(raw);
        if indent > case_indent {
            continue;
        }
        if indent == case_indent {
            earlier.push(case_pattern(raw)?);
            continue;
        }
        header_index = Some(index);
        break;
    }
    let header_index = header_index?;
    let subject = lines[header_index]
        .trim()
        .strip_prefix("match ")?
        .trim_end()
        .strip_suffix(':')?
        .trim();
    let bare = !subject.is_empty()
        && subject
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !subject.chars().next()?.is_ascii_digit();
    if !bare {
        return None;
    }
    earlier.reverse();
    Some(MatchArm {
        subject: subject.to_string(),
        header_index,
        earlier,
        pattern,
    })
}

/// The pattern text of a `case PATTERN:` header line.
fn case_pattern(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix("case ")?;
    Some(rest.trim_end().strip_suffix(':')?.trim().to_string())
}

/// Whether `row` provably never enters the arm: the subject value matches no
/// pattern of the arm, or matches an earlier unguarded arm first.
fn arm_excludes(
    arm: &MatchArm,
    row: &CallRow,
    owner: &PythonOwner,
    owner_source: &str,
) -> Exclusion {
    if parameter_rebound_before(
        owner_source,
        owner.start_line,
        arm.header_index,
        &arm.subject,
    ) {
        return None;
    }
    let value = &row.bindings.get(&arm.subject)?.value;
    let builtins = builtin_classes_intact(owner, owner_source);
    for earlier in &arm.earlier {
        if pattern_matches(earlier, value, builtins) == Some(true) {
            return Some(true);
        }
    }
    (pattern_matches(&arm.pattern, value, builtins) == Some(false)).then_some(true)
}

/// Whether the literal `value` matches the `case` pattern: `Some(true)` only
/// when the arm is certainly taken (no guard), `Some(false)` only when it is
/// certainly not. Anything outside the literal, builtin-class, wildcard and
/// capture vocabulary is `None`.
fn pattern_matches(pattern: &str, value: &str, builtins: bool) -> Option<bool> {
    let (pattern, guarded) = split_guard(pattern)?;
    let mut any_unknown = false;
    let mut matched = false;
    for alternative in pattern.split('|') {
        match alternative_matches(alternative.trim(), value, builtins) {
            Some(true) => matched = true,
            Some(false) => {}
            None => any_unknown = true,
        }
    }
    if matched {
        // A guard may still reject the value, so only the unguarded arm is
        // certainly taken.
        return (!guarded).then_some(true);
    }
    (!any_unknown).then_some(false)
}

/// The pattern without a trailing `if GUARD`. None when the text has quotes
/// and a guard keyword, which this reader does not split.
fn split_guard(pattern: &str) -> Option<(&str, bool)> {
    match pattern.find(" if ") {
        Some(position) => {
            if pattern.contains(['"', '\'']) {
                return None;
            }
            Some((pattern[..position].trim(), true))
        }
        None => Some((pattern.trim(), false)),
    }
}

/// Whether `int`, `float`, `str` and `bool` still name the builtins: no
/// parameter, assignment, `def`/`class`, import or star import of the file
/// rebinds one, so a `case int()` class pattern means what it spells.
fn builtin_classes_intact(owner: &PythonOwner, owner_source: &str) -> bool {
    const BUILTINS: [&str; 4] = ["int", "float", "str", "bool"];
    if owner
        .parameters
        .iter()
        .any(|parameter| BUILTINS.contains(&parameter.name.as_str()))
    {
        return false;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    BUILTINS.iter().all(|name| {
        !lines.iter().any(|line| binds_name(line.trim(), name))
            && !lines.iter().any(|line| {
                let trimmed = line.trim();
                let names_it = mentions_identifier(trimmed, name);
                (names_it
                    && [
                        "def ",
                        "async def ",
                        "class ",
                        "import ",
                        "from ",
                        "global ",
                    ]
                    .iter()
                    .any(|prefix| trimmed.starts_with(prefix)))
                    || (trimmed.starts_with("from ") && trimmed.ends_with("import *"))
            })
    })
}

fn alternative_matches(pattern: &str, value: &str, builtins: bool) -> Option<bool> {
    if pattern.is_empty() || pattern.contains(['[', '{', ',', ' ']) && !is_quoted(pattern) {
        return None;
    }
    if pattern == "_" {
        return Some(true);
    }
    // `None` / `True` / `False` patterns compare by identity.
    if matches!(pattern, "None" | "True" | "False") {
        scalar_kind(value)?;
        return Some(value == pattern);
    }
    if let Some(class) = pattern.strip_suffix("()") {
        if !builtins {
            return None;
        }
        let kind = scalar_kind(value)?;
        return match class {
            "int" => Some(matches!(kind, ScalarKind::Int | ScalarKind::Bool)),
            "float" => Some(matches!(kind, ScalarKind::Float)),
            "str" => Some(matches!(kind, ScalarKind::Str)),
            "bool" => Some(matches!(kind, ScalarKind::Bool)),
            _ => None,
        };
    }
    if pattern.contains(['(', ')', '.', '=', '*', ':']) && !is_quoted(pattern) {
        return None;
    }
    if is_capture_name(pattern) {
        // A capture binds any value, so the arm is always taken.
        return Some(true);
    }
    let literal = literal_value(pattern)?;
    scalar_kind(value)?;
    scalar_kind(&literal)?;
    compare_literal_values(value, "==", &literal)
}

fn is_quoted(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && matches!(bytes[0], b'"' | b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
        && !text[1..text.len() - 1].contains(bytes[0] as char)
}

fn is_capture_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !matches!(text, "None" | "True" | "False")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    None,
    Bool,
    Int,
    Float,
    Str,
}

fn scalar_kind(value: &str) -> Option<ScalarKind> {
    match value {
        "None" => return Some(ScalarKind::None),
        "True" | "False" => return Some(ScalarKind::Bool),
        _ => {}
    }
    if is_quoted(value) {
        // Different escape spellings denote one string (`'\n'` and `'\x0a'`),
        // so text equality proves nothing once a backslash is present.
        return (!value.contains('\\')).then_some(ScalarKind::Str);
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(ScalarKind::Int);
    }
    let (whole, fraction) = digits.split_once('.')?;
    (whole.chars().all(|ch| ch.is_ascii_digit())
        && fraction.chars().all(|ch| ch.is_ascii_digit())
        && !(whole.is_empty() && fraction.is_empty()))
    .then_some(ScalarKind::Float)
}

// ------------------------------------------------------------ dict defaults

/// The `TABLE.get(key, default)` call on the changed line when the edit
/// replaces only the (literal) default, and no test can rebind the table.
fn default_site(
    old: &str,
    new: &str,
    line_index: usize,
    owner: &PythonOwner,
    owner_source: &str,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<DefaultSite> {
    let old_call = get_call(old)?;
    let new_call = get_call(new)?;
    if old_call.before != new_call.before
        || old_call.after != new_call.after
        || old_call.table != new_call.table
        || old_call.key != new_call.key
        || old_call.default == new_call.default
        || literal_value(&old_call.default).is_none()
        || literal_value(&new_call.default).is_none()
    {
        return None;
    }
    let table = new_call.table;
    // The table must be one that no test and no other owner code can change.
    let mutated_by_test = related_candidates.iter().any(|candidate| {
        let test = candidate.test;
        mentions_identifier(&test.body_text, &table)
            || test.constant_rebinding.assigns_attribute(&table)
            || test.constant_rebinding.rebinds(&table)
    });
    // An owner parameter or local of the same name replaces the module table
    // inside the owner, so its keys say nothing about the lookup.
    let shadowed_in_owner = owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == table)
        || parameter_rebound_before(owner_source, owner.start_line, line_index, &table);
    if mutated_by_test || shadowed_in_owner || table_mutated(owner_source, &table) {
        return None;
    }
    Some(DefaultSite {
        table,
        key: new_call.key,
        line_index,
    })
}

struct GetCall {
    before: String,
    table: String,
    key: String,
    default: String,
    after: String,
}

/// The only `NAME.get(key, default)` call on a line, split around its
/// arguments. None for no call, several, or any other argument count.
fn get_call(line: &str) -> Option<GetCall> {
    let marker = line.find(".get(")?;
    if line[marker + 1..].contains(".get(") {
        return None;
    }
    // Step past the delimiter's full UTF-8 width: a non-ASCII identifier
    // character is a delimiter here, and `position + 1` would split it.
    let table_start = line[..marker]
        .char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_'))
        .map_or(0, |(position, ch)| position + ch.len_utf8());
    let table = &line[table_start..marker];
    if table.is_empty() || line[..table_start].ends_with('.') {
        return None;
    }
    let args_start = marker + ".get(".len();
    let args_end = matching_paren(line, args_start)?;
    let arguments = split_top_level(&line[args_start..args_end]);
    let [key, default] = arguments.as_slice() else {
        return None;
    };
    Some(GetCall {
        before: line[..table_start].to_string(),
        table: table.to_string(),
        key: key.trim().to_string(),
        default: default.trim().to_string(),
        after: line[args_end..].to_string(),
    })
}

/// Index of the `)` closing the call whose arguments start at `start`.
fn matching_paren(text: &str, start: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote: Option<u8> = None;
    for (offset, byte) in text.as_bytes()[start..].iter().enumerate() {
        if let Some(active) = quote {
            if *byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(*byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split at top-level commas, outside quotes and brackets.
fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut current = String::new();
    for ch in text.chars() {
        if let Some(active) = quote {
            current.push(ch);
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// Whether the file touches `table` other than by its definition or a
/// `.get(` read (a subscript store, `update`, `pop`, a rebinding).
fn table_mutated(source: &str, table: &str) -> bool {
    let mut definitions = 0usize;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let defines = trimmed
            .strip_prefix(table)
            .is_some_and(|rest| rest.trim_start().starts_with('=') || rest.starts_with(':'))
            && leading_whitespace(line) == 0;
        if defines {
            // `TABLE = {..}; TABLE.pop(k)` mutates on the definition line;
            // a `;` there cannot be read safely, so fail open.
            if line.contains(';') {
                return true;
            }
            definitions += 1;
            continue;
        }
        for (start, _) in line.match_indices(table) {
            let end = start + table.len();
            let word = |ch: char| ch.is_alphanumeric() || ch == '_';
            if line[..start].chars().next_back().is_some_and(word)
                || line[end..].chars().next().is_some_and(word)
            {
                continue;
            }
            // Inside a string (`__all__ = ["TABLE"]`) it is a name, not a use.
            if matches!(line[..start].chars().next_back(), Some('"' | '\'')) {
                continue;
            }
            // A `from .table import TABLE` re-export only names it.
            if trimmed.starts_with("from ") || trimmed.starts_with("import ") {
                continue;
            }
            if !line[end..].starts_with(".get(") {
                return true;
            }
        }
    }
    definitions != 1
}

/// The literal keys of the module-level dict assigned to `table`, or None
/// when the table is not one readable dict literal of scalar keys.
fn literal_table_keys(source: &str, table: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines.iter().position(|line| {
        leading_whitespace(line) == 0
            && line.strip_prefix(table).is_some_and(|rest| {
                let rest = rest.trim_start();
                rest.starts_with("= {") || rest.starts_with("={") || rest.starts_with(':')
            })
    })?;
    let mut text = String::new();
    for line in &lines[start..] {
        text.push_str(line);
        text.push('\n');
        let open = text.find('{')?;
        if matching_paren(&text, open + 1).is_some() {
            break;
        }
    }
    let open = text.find('{')?;
    let close = matching_paren(&text, open + 1)?;
    // The dict must be the whole assigned expression: nothing but a comment
    // may follow it (`{..} if cond else {}`, `{..} | other`, `{..}; T.pop(k)`).
    let after = text[close + 1..].lines().next().unwrap_or("").trim();
    if !(after.is_empty() || after.starts_with('#')) {
        return None;
    }
    let before = text[..open].trim_end();
    let target = before.strip_suffix('=')?.trim_end();
    if !(target == table || target.starts_with(&format!("{table}:"))) || before.ends_with("==") {
        return None;
    }
    let body: String = text[open + 1..close]
        .lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    if body.contains(" for ") || body.trim_start().starts_with("**") {
        return None;
    }
    let mut keys = Vec::new();
    for entry in split_top_level(&body) {
        let entry = entry.trim();
        if entry.is_empty() || entry.starts_with("**") {
            return None;
        }
        let (key, _) = entry.split_once(':')?;
        let key = literal_value(key.trim())?;
        scalar_kind(&key)?;
        keys.push(key);
    }
    (!keys.is_empty()).then_some(keys)
}

/// Whether `row` looks up a key the table holds, so the default never runs.
fn default_excludes(
    site: &DefaultSite,
    table_keys: &[String],
    row: &CallRow,
    constants: &[super::module_constants::PythonModuleConstant],
    owner: &PythonOwner,
    owner_source: &str,
) -> Exclusion {
    let key = &site.key;
    let is_name = key
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
    if is_name && parameter_rebound_before(owner_source, owner.start_line, site.line_index, key) {
        return None;
    }
    let value = resolve_operand(key, row, constants)?;
    scalar_kind(&value)?;
    let mut unknown = false;
    for held in table_keys {
        match compare_literal_values(&value, "==", held) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    if unknown { None } else { Some(false) }
}

/// True when `line` assigns `name` or uses it as a `for` target.
fn binds_name(line: &str, name: &str) -> bool {
    line_rebinds_parameter(line, name)
        || line
            .strip_prefix("for ")
            .and_then(|rest| rest.split(" in ").next())
            .is_some_and(|targets| {
                targets.split(',').any(|target| {
                    target
                        .trim()
                        .trim_start_matches(['(', '[', '*'])
                        .trim_end_matches([')', ']'])
                        == name
                })
            })
}

/// What the one further method a strong assertion calls on the owner's result
/// does with the edited constructor field.
enum ChainedMethod {
    /// `return self.<field>` for the edited field: it observes the edit.
    ReturnsEditedField,
    /// Anything else: computed from other state or returning another field.
    DoesNotReturnEditedField,
}

/// Evidence that a classmethod's `return cls(...)` edit is observed only
/// through a further method call that does not return the edited field.
///
/// Engages only on a `@dataclass` class without bases, an edit to exactly one
/// argument of `return cls(...)` / `return Class(...)` that names a declared
/// field, and strong related tests whose selected assertion calls the owner
/// only as the receiver of `.method(...)`. A method that is a plain getter of
/// the edited field, an assertion that reads an attribute or the object itself,
/// or anything unreadable keeps the existing verdict.
pub(super) fn python_chained_receiver_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    new_line: &str,
    old_line: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<GuardedArmGap> {
    if !matches!(owner.owner_kind, Some(OwnerKind::ClassMethod))
        || owner
            .decorators
            .iter()
            .any(|decorator| decorator.ends_with("staticmethod"))
    {
        return None;
    }
    let receiver = owner.parameters.first()?.name.as_str();
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed = lines.get(changed_line.checked_sub(1)?)?;
    if changed.trim() != new_line.trim() {
        return None;
    }
    let class = DataclassBody::read(owner, &lines)?;
    let (ctor, new_args) = constructor_arguments(new_line)?;
    let (old_ctor, old_args) = constructor_arguments(old_line?)?;
    if ctor != old_ctor
        || (ctor != receiver && ctor != class.name)
        || new_args.len() != old_args.len()
    {
        return None;
    }
    let mut edits =
        (0..new_args.len()).filter(|index| new_args[*index].trim() != old_args[*index].trim());
    let index = edits.next()?;
    if edits.next().is_some() {
        return None;
    }
    let field = match keyword_name(&new_args[index]) {
        // A keyword edit must name a declared field.
        Some(name) => class.fields.iter().find(|field| *field == name)?.clone(),
        // A positional edit: no keyword argument may precede it.
        None if new_args[..index]
            .iter()
            .any(|arg| keyword_name(arg).is_some()) =>
        {
            return None;
        }
        None => class.fields.get(index)?.clone(),
    };
    let mut methods: Vec<String> = Vec::new();
    for candidate in related_candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        let selected = select_relevant_assertion(&candidate.test.assertions, Some(focus));
        let Some(assertion) = selected.assertion() else {
            continue;
        };
        if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
            continue;
        }
        methods.extend(chained_methods(&assertion.gate_text, &owner.name)?);
    }
    methods.sort();
    methods.dedup();
    if methods.is_empty() {
        return None;
    }
    for method in &methods {
        match class.method_reads(method, &field)? {
            ChainedMethod::ReturnsEditedField => return None,
            ChainedMethod::DoesNotReturnEditedField => {}
        }
    }
    let called = methods
        .iter()
        .map(|method| format!("`.{method}()`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(GuardedArmGap {
        discriminator: format!("an assertion on the `{field}` state the edit sets"),
        observed_values: Vec::new(),
        reason: format!(
            "every strong related assertion reads `{}` only through {called}, which does not return `{field}` directly, so a different `{field}` can leave the same value",
            owner.name
        ),
        chained_receiver: true,
    })
}

/// The name of a `name=value` keyword argument (not a `==` comparison).
fn keyword_name(arg: &str) -> Option<&str> {
    let (name, value) = arg.split_once('=')?;
    (is_identifier(name.trim()) && !value.starts_with('=')).then_some(name.trim())
}

/// The constructor name and top-level arguments of a `return Ctor(args)` line.
fn constructor_arguments(line: &str) -> Option<(String, Vec<String>)> {
    let rest = line.trim().strip_prefix("return ")?.trim();
    let open = rest.find('(')?;
    let ctor = rest[..open].trim();
    if !is_identifier(ctor) || matching_paren(rest, open + 1)? != rest.len() - 1 {
        return None;
    }
    Some((
        ctor.to_string(),
        split_top_level(&rest[open + 1..rest.len() - 1]),
    ))
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Every method the assertion calls on the result of an owner call. `None`
/// when any mention of the owner is not exactly `owner(...).method(`: a bare
/// reference, an attribute read, a property or the object compared directly.
fn chained_methods(assertion: &str, owner_name: &str) -> Option<Vec<String>> {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut methods = Vec::new();
    for (start, _) in assertion.match_indices(owner_name) {
        let after = &assertion[start + owner_name.len()..];
        if assertion[..start].chars().next_back().is_some_and(is_word)
            || after.chars().next().is_some_and(is_word)
        {
            continue;
        }
        let args = after.strip_prefix('(')?;
        let close = matching_paren(args, 0)?;
        let tail = args[close + 1..].strip_prefix('.')?;
        let name: String = tail.chars().take_while(|ch| is_word(*ch)).collect();
        if name.is_empty() || !tail[name.len()..].starts_with('(') {
            return None;
        }
        methods.push(name);
    }
    (!methods.is_empty()).then_some(methods)
}

/// The `@dataclass` class around a classmethod owner, read as text.
struct DataclassBody {
    name: String,
    /// Declared fields in constructor order.
    fields: Vec<String>,
    /// Non-blank, non-comment lines of the class body.
    members: Vec<String>,
    member_indent: usize,
}

impl DataclassBody {
    fn read(owner: &PythonOwner, lines: &[&str]) -> Option<Self> {
        let def_index = (owner.start_line.checked_sub(1)?..lines.len()).find(|index| {
            let trimmed = lines[*index].trim_start();
            trimmed.starts_with("def ") || trimmed.starts_with("async def ")
        })?;
        let member_indent = leading_whitespace(lines[def_index]);
        let class_index = (0..def_index).rev().find(|index| {
            let raw = lines[*index];
            !raw.trim().is_empty()
                && !raw.trim_start().starts_with('#')
                && leading_whitespace(raw) < member_indent
        })?;
        let class_indent = leading_whitespace(lines[class_index]);
        let name = lines[class_index]
            .trim()
            .strip_prefix("class ")?
            .strip_suffix(':')?
            .trim_end_matches("()")
            .trim();
        if !is_identifier(name) {
            return None;
        }
        // Decorators directly above the class: `@dataclass` and nothing that
        // reorders or hides fields.
        let mut dataclass = false;
        let mut index = class_index;
        while index > 0 && lines[index - 1].trim_start().starts_with('@') {
            index -= 1;
            let decorator = lines[index].trim();
            if decorator.contains("kw_only") || decorator.contains("init=False") {
                return None;
            }
            dataclass |= decorator == "@dataclass"
                || decorator == "@dataclasses.dataclass"
                || decorator.starts_with("@dataclass(")
                || decorator.starts_with("@dataclasses.dataclass(");
        }
        if !dataclass {
            return None;
        }
        let mut fields = Vec::new();
        let mut members = Vec::new();
        for raw in &lines[class_index + 1..] {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = leading_whitespace(raw);
            if indent <= class_indent {
                break;
            }
            members.push((*raw).to_string());
            if indent != member_indent || trimmed.starts_with('@') || trimmed.starts_with("def ") {
                continue;
            }
            if let Some((field, annotation)) = trimmed.split_once(':')
                && is_identifier(field.trim())
            {
                if ["ClassVar", "InitVar", "KW_ONLY", "init=False", "kw_only"]
                    .iter()
                    .any(|marker| annotation.contains(marker))
                {
                    return None;
                }
                fields.push(field.trim().to_string());
            }
        }
        Some(Self {
            name: name.to_string(),
            fields,
            members,
            member_indent,
        })
    }

    /// How the one `def method(self)` of this class relates to `field`.
    fn method_reads(&self, method: &str, field: &str) -> Option<ChainedMethod> {
        let prefix = format!("def {method}(");
        let mut headers = self.members.iter().enumerate().filter(|(_, raw)| {
            leading_whitespace(raw) == self.member_indent && raw.trim_start().starts_with(&prefix)
        });
        let (position, header) = headers.next()?;
        if headers.next().is_some() {
            return None;
        }
        // Undecorated, so `.method(` is an ordinary bound-method call.
        if position > 0 && self.members[position - 1].trim_start().starts_with('@') {
            return None;
        }
        let header = header.trim();
        let receiver = header
            .strip_prefix(&prefix)?
            .split([',', ')'])
            .next()?
            .trim();
        if !is_identifier(receiver) || !header.ends_with(':') {
            return None;
        }
        let mut body: Vec<&str> = self.members[position + 1..]
            .iter()
            .take_while(|raw| leading_whitespace(raw) > self.member_indent)
            .map(|raw| raw.trim())
            .collect();
        if let Some(first) = body.first().copied()
            && (first.starts_with("\"\"\"") || first.starts_with("'''"))
        {
            // A one-line docstring is skipped; a longer one is unreadable.
            let quote = &first[..3];
            if first.len() >= 6 && first.ends_with(quote) {
                body.remove(0);
            } else {
                return None;
            }
        }
        let getter = body
            .first()
            .filter(|_| body.len() == 1)
            .and_then(|line| line.strip_prefix("return "))
            .and_then(|expression| expression.trim().strip_prefix(receiver))
            .and_then(|rest| rest.strip_prefix('.'))
            .filter(|attribute| is_identifier(attribute));
        Some(match getter {
            Some(attribute) if attribute == field => ChainedMethod::ReturnsEditedField,
            _ => ChainedMethod::DoesNotReturnEditedField,
        })
    }
}
