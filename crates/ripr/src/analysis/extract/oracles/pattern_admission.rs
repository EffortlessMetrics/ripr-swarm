//! RIPR-SPEC-0231 rules 2 and 3: a `matches!` or `assert_matches!` assertion
//! is only as strong as its pattern. A binding, a wildcard payload, a range or
//! a side-only constructor does not pin a value, so it never reads as an exact
//! oracle.
//!
//! The reading is syntactic. A path the line cannot resolve (a variant, a
//! unit struct, a constant) counts as pinning a value, which keeps today's
//! reading; only patterns that are shown not to pin a value are weakened.
//! A unique `struct` or single-variant `enum` in the same source the
//! classifier is given is enough to show `Cfg { .. }` or `Only::Value`
//! (#6737); anything the file cannot uniquely name stays pinning.

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::extract::mask_comments_and_strings;
use crate::domain::{OracleKind, OracleStrength};

use super::arguments::{complete_macro_arguments, parenthesized_contents};
use super::classify::OracleClassification;

/// The `Option` and `Result` constructors, which only say which side a value
/// is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Some,
    None,
    Ok,
    Err,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    /// Accepts every value of its type.
    Irrefutable,
    /// Checks only which side of an `Option` or `Result` the value is on.
    SideOnly(Side),
    /// A range, or a constructor whose payload is a range.
    Range { err: bool },
    /// Fixes a shape (a slice length, a tuple of side checks) but no value.
    PinsNothing { err: bool },
    /// Contains a literal, a constant or an unresolved variant.
    Pins,
}

/// Struct and single-variant enum names the same source uniquely declares.
///
/// RIPR-SPEC-0231 rule 3: a path is irrefutable only when it can be shown to
/// name a struct, or the sole variant of an enum shown to have one variant.
/// A second item of the same name, or a path this table does not name,
/// stays unresolved and keeps today's exact reading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalDecls {
    structs: BTreeSet<String>,
    /// Enum name → its only variant.
    single_variant_enums: BTreeMap<String, String>,
}

impl LocalDecls {
    pub(crate) fn from_source(source: &str) -> Self {
        if source.is_empty() {
            return Self::default();
        }
        let masked = mask_comments_and_strings(source);
        let mut kinds: BTreeMap<String, BTreeSet<ItemKind>> = BTreeMap::new();
        let mut enum_variants: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let bytes = masked.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            if !is_identifier_byte(bytes[index]) {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            if start > 0 && is_identifier_byte(bytes[start - 1]) {
                continue;
            }
            let Some(word) = masked.get(start..index) else {
                continue;
            };
            let kind = match word {
                "struct" => ItemKind::Struct,
                "enum" => ItemKind::Enum,
                "union" => ItemKind::Union,
                "type" => ItemKind::Type,
                _ => continue,
            };
            let after = skip_whitespace(&masked, index);
            let Some((name, name_end)) = ident_at(&masked, after) else {
                continue;
            };
            kinds.entry(name.to_string()).or_default().insert(kind);
            if kind == ItemKind::Enum {
                // A second same-named enum cannot uniquely count variants.
                if enum_variants.contains_key(name) {
                    enum_variants.insert(name.to_string(), Vec::new());
                } else {
                    let body_at = skip_whitespace(&masked, skip_generics(&masked, name_end));
                    let variants = next_brace_at_depth_zero(&masked, body_at)
                        .and_then(|open| {
                            let close = matching_close(&masked, open)?;
                            enum_variant_names(masked.get(open + 1..close)?)
                        })
                        .unwrap_or_default();
                    enum_variants.insert(name.to_string(), variants);
                }
            }
            index = name_end;
        }
        let mut structs = BTreeSet::new();
        let mut single_variant_enums = BTreeMap::new();
        for (name, declared) in kinds {
            if declared.len() != 1 {
                continue;
            }
            if declared.contains(&ItemKind::Struct) {
                structs.insert(name);
                continue;
            }
            if declared.contains(&ItemKind::Enum) {
                let Some(variants) = enum_variants.get(&name) else {
                    continue;
                };
                if variants.len() == 1
                    && let Some(variant) = variants.first()
                {
                    single_variant_enums.insert(name, variant.clone());
                }
            }
        }
        Self {
            structs,
            single_variant_enums,
        }
    }

    fn shown_struct(&self, path: &str) -> bool {
        let path = normalized_path(path);
        !path.contains("::") && self.structs.contains(&path)
    }

    fn shown_single_variant(&self, path: &str) -> bool {
        let path = normalized_path(path);
        let Some((ty, variant)) = path.rsplit_once("::") else {
            return false;
        };
        !ty.contains("::")
            && self
                .single_variant_enums
                .get(ty)
                .is_some_and(|only| only == variant)
    }

    fn shown_irrefutable_constructor(&self, path: &str) -> bool {
        self.shown_struct(path) || self.shown_single_variant(path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ItemKind {
    Struct,
    Enum,
    Union,
    Type,
}

/// The classification a whole-pattern assertion earns from its pattern, or
/// `None` when the pattern pins a value and the ordinary chain decides.
#[cfg(test)]
pub(super) fn pattern_assertion_classification(line: &str) -> Option<OracleClassification> {
    pattern_assertion_classification_in(line, &LocalDecls::default())
}

/// The classification a whole-pattern assertion earns from its pattern, using
/// the test file's own type declarations when they uniquely show a struct or
/// single-variant enum, or `None` when the pattern pins a value.
pub(super) fn pattern_assertion_classification_in(
    line: &str,
    decls: &LocalDecls,
) -> Option<OracleClassification> {
    let (scrutinee, pattern) = asserted_pattern(line)?;
    pattern_pair_classification(&scrutinee, &pattern, decls)
}

/// RIPR-SPEC-0231 rule 3 (#7715): whether some `matches!` invocation in
/// `text` pins a value. A `matches!` nested in a call or closure
/// (`xs.iter().any(|x| matches!(x, Some(_)))`), compared with a bool
/// (`matches!(r, Ok(_)) == true`), or named only in a comment or string
/// earns no exact credit unless its own pattern pins a value. An invocation
/// this cannot read counts as pinning, which keeps today's reading.
pub(super) fn some_matches_pins_a_value(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    masked.match_indices("matches!").any(|(index, name)| {
        let before = &masked[..index];
        if before
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            // A user macro whose name ends in `matches!` (`expect_matches!`)
            // is not the std pattern test, so it keeps today's reading.
            return true;
        }
        // A user macro (`custom::matches!`, `helpers::std::matches!`) is not
        // the std pattern test, so it keeps today's reading. A spaced or
        // commented std path (`std :: matches!`, `::core::matches!`) is
        // still std.
        if before.trim_end().ends_with("::") && !is_std_macro_path(before) {
            return true;
        }
        let after = index + name.len();
        let Some((offset, '(' | '[' | '{')) = masked[after..]
            .char_indices()
            .find(|(_, ch)| !ch.is_whitespace())
        else {
            return false;
        };
        super::arguments::delimited_contents_at(text, after + offset).is_none_or(|contents| {
            match super::arguments::split_top_level_commas(&contents).as_slice() {
                [scrutinee, pattern, ..] => {
                    pattern_pair_classification(scrutinee, pattern, &LocalDecls::default())
                        .is_none()
                }
                _ => true,
            }
        })
    })
}

/// Whether the macro path ending `before` (masked text up to `matches!`,
/// ending in `::`) is exactly `std::` or `core::`, with an optional leading
/// `::` and whitespace around each `::`.
fn is_std_macro_path(before: &str) -> bool {
    let mut rest = before.trim_end();
    let mut segments = Vec::new();
    while let Some(prefix) = rest.strip_suffix("::") {
        let prefix = prefix.trim_end();
        let start = prefix
            .char_indices()
            .rev()
            .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let segment = prefix.get(start..).unwrap_or_default();
        if segment.is_empty() {
            // A leading `::` ends the path.
            break;
        }
        segments.push(segment);
        rest = prefix.get(..start).unwrap_or_default().trim_end();
    }
    matches!(segments.as_slice(), ["std" | "core"])
}

/// Whether `text` is one complete `matches!` invocation whose pattern pins
/// no value (`matches!(r, Ok(_))`), as RIPR-SPEC-0231 rule 3 reads it.
pub(super) fn is_unpinned_pattern(text: &str) -> bool {
    complete_macro_arguments(text, "matches!").is_some_and(|arguments| {
        matches!(arguments.as_slice(), [scrutinee, pattern, ..]
            if pattern_pair_classification(scrutinee, pattern, &LocalDecls::default()).is_some())
    })
}

/// The reading a `matches!` with this scrutinee and pattern earns, or `None`
/// when the pattern pins a value and the ordinary chain decides.
fn pattern_pair_classification(
    scrutinee: &str,
    pattern: &str,
    decls: &LocalDecls,
) -> Option<OracleClassification> {
    let (pattern, guard) = split_guard(pattern);
    // A guard that equates the scrutinee, a pattern binding, or a field /
    // deref / tuple projection of either with a value pins that value
    // (`_ if value == 2`, `Some(v) if v.id == 3`; RIPR-SPEC-0108's
    // runtime-controlled fixtures), so the ordinary chain keeps its reading
    // (decision 7, #7726).
    if guard.is_some_and(|guard| guard_pins_value(guard, scrutinee, pattern)) {
        return None;
    }
    let guarded = guard.is_some();
    let alternatives = split_top_level(pattern, '|');
    if alternatives.len() > 1 {
        return or_pattern_classification(&alternatives, guarded, decls);
    }
    reading_classification(read_pattern(pattern, decls), guarded)
}

/// The pattern operand of an `assert_matches!` / `debug_assert_matches!`, or
/// of a `matches!` that is the whole condition of `assert!`, `debug_assert!`,
/// `prop_assert!` or `ensure!`. A compound condition is not a pattern
/// assertion.
fn asserted_pattern(line: &str) -> Option<(String, String)> {
    let pair = |arguments: Vec<String>| match arguments.as_slice() {
        [scrutinee, pattern] => Some((scrutinee.clone(), pattern.clone())),
        _ => None,
    };
    if let Some(arguments) = ["assert_matches!", "debug_assert_matches!"]
        .into_iter()
        .find_map(|name| complete_macro_arguments(line, name))
    {
        return pair(arguments);
    }
    let condition = ["assert!", "debug_assert!", "prop_assert!", "ensure!"]
        .into_iter()
        .find_map(|name| complete_macro_arguments(line, name)?.into_iter().next())?;
    let mut expression = condition.trim();
    while let Some(inner) = parenthesized_contents(expression) {
        expression = inner.trim();
    }
    pair(complete_macro_arguments(expression, "matches!")?)
}

/// A guard conjunct `a == b` where exactly one side names the matched
/// value: the scrutinee, a name the pattern binds, or a field / deref /
/// tuple projection rooted at one of those (`v.id`, `(*v).field`, `v.0`).
/// Method calls (`e.len() == 3`), a foreign name (`other.id == 3`),
/// `flag == true`, and self-comparisons (`e == e`, `v.id == v.id`,
/// `v.a == v.b`, `(a, b) if a == b`) pin nothing. A disjunct (`||`)
/// does not pin.
fn guard_pins_value(guard: &str, scrutinee: &str, pattern: &str) -> bool {
    let bound = pattern_bindings(pattern);
    let subject = comparable(scrutinee);
    split_conjuncts(guard).into_iter().any(|conjunct| {
        let masked = mask_comments_and_strings(conjunct);
        // A disjunct (`x > 1 || x == 0`) does not pin the value.
        if masked.contains("||") {
            return false;
        }
        let Some(index) = top_level_equality(&masked) else {
            return false;
        };
        // One side names the matched value and the other is a value
        // expression: `e == e` or `a == b` over two bindings compares the
        // match with itself and pins nothing.
        let names_match = |side: &str| side_names_the_match(side, &subject, &bound);
        let left = comparable(&conjunct[..index]);
        let right = comparable(&conjunct[index + 2..]);
        names_match(&left) != names_match(&right)
    })
}

/// Whether `side` (already comparable) is the scrutinee, a pattern binding,
/// or a field / deref / tuple projection of either. Method calls and
/// indexing do not name the match.
fn side_names_the_match(side: &str, subject: &str, bound: &[String]) -> bool {
    if side == subject || bound.iter().any(|name| name == side) {
        return true;
    }
    match_root_ident(side).is_some_and(|(root, has_projection)| {
        has_projection && (root == subject || bound.iter().any(|name| name == root))
    })
}

/// Root identifier of a field, deref, or tuple projection, and whether at
/// least one `.field` / `.0` projection remains (`v.id`, `(*v).field`).
fn match_root_ident(side: &str) -> Option<(&str, bool)> {
    let mut s = side;
    let mut projected = false;
    loop {
        if s.is_empty() {
            return None;
        }
        if s.starts_with('(') {
            let close = matching_close(s, 0)?;
            if close + 1 == s.len() {
                s = s.get(1..close)?;
                continue;
            }
            let rest = s.get(close + 1..)?;
            if !is_field_suffix(rest) {
                return None;
            }
            projected = true;
            s = s.get(1..close)?;
            continue;
        }
        if s.starts_with(['*', '&']) {
            s = s.get(1..)?;
            continue;
        }
        break;
    }
    let ident_end = s
        .char_indices()
        .find(|(_, ch)| !is_ident_char(*ch))
        .map_or(s.len(), |(index, _)| index);
    if ident_end == 0 {
        return None;
    }
    let ident = s.get(..ident_end)?;
    if !is_ident(ident) {
        return None;
    }
    let rest = s.get(ident_end..)?;
    if rest.is_empty() {
        return Some((ident, projected));
    }
    is_field_suffix(rest).then_some((ident, true))
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_') && chars.all(is_ident_char)
}

/// `.field` / `.0` chain and nothing else. A method call (`.len()`) or
/// index (`[0]`) is not a field projection.
fn is_field_suffix(rest: &str) -> bool {
    let mut rest = rest;
    if !rest.starts_with('.') {
        return false;
    }
    while let Some(after) = rest.strip_prefix('.') {
        let len = after
            .char_indices()
            .find(|(_, ch)| !is_ident_char(*ch))
            .map_or(after.len(), |(index, _)| index);
        if len == 0 {
            return false;
        }
        let Some(field) = after.get(..len) else {
            return false;
        };
        let digits = !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit());
        if !digits && !is_ident(field) {
            return false;
        }
        let Some(rest_after) = after.get(len..) else {
            return false;
        };
        rest = rest_after;
    }
    rest.is_empty()
}

fn comparable(expression: &str) -> String {
    expression
        .split_whitespace()
        .collect::<String>()
        .trim_start_matches(['&', '*'])
        .to_string()
}

/// Top-level `&&` conjuncts of a guard.
fn split_conjuncts(guard: &str) -> Vec<&str> {
    let masked = mask_comments_and_strings(guard);
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = masked.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'&' if depth == 0 && bytes.get(index + 1) == Some(&b'&') => {
                parts.push(guard[start..index].trim());
                start = index + 2;
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(guard[start..].trim());
    parts
}

/// The byte index of a top-level `==` that is not part of `!=`, `<=`, `>=`.
fn top_level_equality(masked: &str) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'=' if depth == 0
                && bytes.get(index + 1) == Some(&b'=')
                && !(index > 0 && matches!(bytes[index - 1], b'!' | b'<' | b'>' | b'=')) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

/// Names a pattern binds: lowercase-initial identifiers that are not a path
/// segment, a call or a keyword.
fn pattern_bindings(pattern: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(pattern);
    let bytes = masked.as_bytes();
    let mut names = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if is_identifier_byte(bytes[index]) && (index == 0 || !is_identifier_byte(bytes[index - 1]))
        {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            let name = &masked[start..index];
            let path_segment = masked[..start].ends_with("::") || masked[index..].starts_with("::");
            let call = masked[index..].trim_start().starts_with(['(', '{', '!']);
            if !path_segment
                && !call
                && name.starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
                && !matches!(name, "ref" | "mut" | "box" | "true" | "false" | "_")
            {
                names.push(name.to_string());
            }
        } else {
            index += 1;
        }
    }
    names
}

/// Split `pattern if guard` at its top-level `if`.
fn split_guard(pattern: &str) -> (&str, Option<&str>) {
    let masked = mask_comments_and_strings(pattern);
    let mut depth = 0usize;
    let bytes = masked.as_bytes();
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            'i' if depth == 0
                && masked[index..].starts_with("if")
                && (index == 0 || !is_identifier_byte(bytes[index - 1]))
                && !bytes
                    .get(index + 2)
                    .copied()
                    .is_some_and(is_identifier_byte) =>
            {
                return (pattern[..index].trim(), Some(pattern[index + 2..].trim()));
            }
            _ => {}
        }
    }
    (pattern.trim(), None)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Split on a separator outside delimiters, comments and literals. `|` is
/// never split inside `||`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == separator && depth == 0 => {
                let doubled = (separator == '|'
                    && (bytes.get(index + 1) == Some(&b'|')
                        || (index > 0 && bytes[index - 1] == b'|')))
                    || (separator == ':'
                        && (bytes.get(index + 1) == Some(&b':')
                            || (index > 0 && bytes[index - 1] == b':')));
                if !doubled {
                    parts.push(text[start..index].trim());
                    start = index + separator.len_utf8();
                }
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    // A leading `|` in an or-pattern leaves an empty first alternative.
    if parts.len() > 1 && parts.first().is_some_and(|part| part.is_empty()) {
        parts.remove(0);
    }
    parts
}

fn reading_classification(reading: Reading, guarded: bool) -> Option<OracleClassification> {
    let (kind, strength) = match reading {
        Reading::Irrefutable => (OracleKind::RelationalCheck, OracleStrength::Weak),
        // Rule 2: an `Err` side check is a broad error, guard or not.
        Reading::SideOnly(Side::Err) | Reading::Range { err: true } => {
            (OracleKind::BroadError, OracleStrength::Weak)
        }
        Reading::PinsNothing { err: true } => (OracleKind::BroadError, OracleStrength::Weak),
        Reading::SideOnly(_) if guarded => (OracleKind::RelationalCheck, OracleStrength::Weak),
        Reading::SideOnly(_) => (OracleKind::SmokeOnly, OracleStrength::Smoke),
        Reading::Range { err: false } | Reading::PinsNothing { err: false } => {
            (OracleKind::RelationalCheck, OracleStrength::Weak)
        }
        Reading::Pins => return None,
    };
    Some(OracleClassification { kind, strength })
}

fn or_pattern_classification(
    alternatives: &[&str],
    guarded: bool,
    decls: &LocalDecls,
) -> Option<OracleClassification> {
    let readings = alternatives
        .iter()
        .map(|alternative| read_pattern(alternative, decls))
        .collect::<Vec<_>>();
    let covers = |a: Side, b: Side| {
        readings.contains(&Reading::SideOnly(a)) && readings.contains(&Reading::SideOnly(b))
    };
    if readings.contains(&Reading::Irrefutable)
        || covers(Side::Some, Side::None)
        || covers(Side::Ok, Side::Err)
    {
        return reading_classification(Reading::Irrefutable, guarded);
    }
    // Alternatives on both sides do not observe the side
    // (`Ok(_) | Err(E::X)`), so the assertion is only a relation.
    let sides = alternatives
        .iter()
        .filter_map(|alternative| top_side(alternative))
        .collect::<Vec<_>>();
    let both = |a: Side, b: Side| sides.contains(&a) && sides.contains(&b);
    if both(Side::Some, Side::None) || both(Side::Ok, Side::Err) {
        return Some(OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        });
    }
    // Otherwise the or-pattern reads as its weakest alternative. An
    // alternative that pins a value keeps the ordinary chain's (strong)
    // reading, so only when every alternative pins does the chain decide.
    readings
        .into_iter()
        .filter_map(|reading| reading_classification(reading, guarded))
        .min_by_key(|classification| classification.strength.rank())
}

/// The `Option`/`Result` constructor at the top of a pattern, if any.
fn top_side(pattern: &str) -> Option<Side> {
    let pattern = strip_wrappers(pattern);
    let (path, _) = constructor_parts(pattern);
    side_of_path(&path)
}

/// The `Option`/`Result` constructor a path names: bare (`Some`) or
/// qualified through `Option`/`Result` (`std::option::Option::Some`). A
/// user enum's `Mode::None` or `Reply::Ok` is a variant, not a side.
fn side_of_path(path: &str) -> Option<Side> {
    let path = path.split_whitespace().collect::<String>();
    let path = path.trim_start_matches("::");
    let (qualifier, last) = path.rsplit_once("::").unwrap_or(("", path));
    let (side, owner) = match last {
        "Some" => (Side::Some, "Option"),
        "None" => (Side::None, "Option"),
        "Ok" => (Side::Ok, "Result"),
        "Err" => (Side::Err, "Result"),
        _ => return None,
    };
    let module = owner.to_ascii_lowercase();
    let qualified = qualifier.is_empty()
        || qualifier == owner
        || ["std", "core"]
            .iter()
            .any(|krate| qualifier == format!("{krate}::{module}::{owner}"));
    qualified.then_some(side)
}

/// `&p`, `&mut p`, `box p` and `name @ p` read through to `p`.
fn strip_wrappers(pattern: &str) -> &str {
    let mut pattern = pattern.trim();
    loop {
        if let Some(rest) = pattern.strip_prefix('&') {
            pattern = rest.trim_start();
            if let Some(rest) = pattern.strip_prefix("mut ") {
                pattern = rest.trim_start();
            }
        } else if let Some(rest) = pattern.strip_prefix("box ") {
            pattern = rest.trim_start();
        } else if let Some(sub) = binding_subpattern(pattern) {
            pattern = sub;
        } else {
            return pattern;
        }
    }
}

/// The sub-pattern of `name @ p` (with optional `ref` / `mut`).
fn binding_subpattern(pattern: &str) -> Option<&str> {
    let parts = split_top_level(pattern, '@');
    let [binding, sub] = parts.as_slice() else {
        return None;
    };
    is_binding(binding).then_some(*sub)
}

/// A catch-all binding: a lowercase-initial identifier other than `true`
/// and `false`, optionally with `ref`, `mut` or `ref mut`.
fn is_binding(text: &str) -> bool {
    let mut words = text.split_whitespace().collect::<Vec<_>>();
    let Some(name) = words.pop() else {
        return false;
    };
    let modifiers_ok = matches!(words.as_slice(), [] | ["ref"] | ["mut"] | ["ref", "mut"]);
    modifiers_ok
        && name != "true"
        && name != "false"
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `(path, delimited payload)` for `Path(..)`, `Path { .. }` or a bare path.
/// The path is read from the masked text, so a comment around it
/// (`/* , */ Some(_)`, #7772) never hides the constructor it names.
fn constructor_parts(pattern: &str) -> (String, Option<(char, &str)>) {
    let masked = mask_comments_and_strings(pattern);
    let Some(open) = masked.find(['(', '{', '[']) else {
        return (masked.trim().to_string(), None);
    };
    let delimiter = masked[open..].chars().next().unwrap_or('(');
    // The first delimiter must close at the very end: `A(x) | B(y)` or
    // `(a) .. (b)` is not one constructor.
    let end = masked.trim_end().len().saturating_sub(1);
    if matching_close(&masked, open) != Some(end) {
        return (masked.trim().to_string(), None);
    }
    (
        masked[..open].trim().to_string(),
        Some((delimiter, pattern.get(open + 1..end).unwrap_or_default())),
    )
}

/// The byte index of the delimiter that closes the one at `open`.
fn matching_close(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn skip_whitespace(text: &str, mut index: usize) -> usize {
    while text
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn ident_at(text: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    if !bytes.get(start).copied().is_some_and(is_identifier_byte) {
        return None;
    }
    let mut end = start + 1;
    while bytes.get(end).copied().is_some_and(is_identifier_byte) {
        end += 1;
    }
    Some((text.get(start..end)?, end))
}

fn skip_generics(text: &str, start: usize) -> usize {
    let start = skip_whitespace(text, start);
    if text.as_bytes().get(start) != Some(&b'<') {
        return start;
    }
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate().skip(start) {
        match byte {
            b'<' => depth = depth.saturating_add(1),
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index.saturating_add(1);
                }
            }
            _ => {}
        }
    }
    start
}

fn next_brace_at_depth_zero(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate().skip(start) {
        match byte {
            b'<' | b'(' | b'[' => depth = depth.saturating_add(1),
            b'>' | b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth == 0 => return Some(index),
            b'{' => depth = depth.saturating_add(1),
            b'}' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

fn enum_variant_names(body: &str) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for chunk in split_top_level(body, ',') {
        let rest = skip_leading_attrs_and_vis(chunk)?;
        if rest.is_empty() {
            continue;
        }
        let (name, _) = ident_at(rest, 0)?;
        if matches!(name, "ref" | "mut" | "const" | "true" | "false") {
            return None;
        }
        names.push(name.to_string());
    }
    Some(names)
}

fn skip_leading_attrs_and_vis(text: &str) -> Option<&str> {
    let mut rest = text.trim_start();
    loop {
        rest = rest.trim_start();
        let Some(after_hash) = rest.strip_prefix('#') else {
            break;
        };
        let after_hash = after_hash.trim_start();
        let after_hash = after_hash
            .strip_prefix('!')
            .unwrap_or(after_hash)
            .trim_start();
        if !after_hash.starts_with('[') {
            return None;
        }
        let close = matching_close(after_hash, 0)?;
        rest = after_hash.get(close.saturating_add(1)..)?;
    }
    rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix("pub")
        && after
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_whitespace() || ch == '(')
    {
        let after = after.trim_start();
        rest = if after.starts_with('(') {
            let close = matching_close(after, 0)?;
            after.get(close.saturating_add(1)..)?.trim_start()
        } else {
            after
        };
    }
    Some(rest)
}

fn normalized_path(path: &str) -> String {
    path.split_whitespace()
        .collect::<String>()
        .trim_start_matches("::")
        .to_string()
}

fn read_pattern(pattern: &str, decls: &LocalDecls) -> Reading {
    let pattern = strip_wrappers(pattern);
    // Comments and literal contents never make a binding or wildcard.
    let masked = mask_comments_and_strings(pattern);
    let code = masked.trim();
    if code == "_" || code == ".." || is_binding(code) {
        return Reading::Irrefutable;
    }
    let alternatives = split_top_level(pattern, '|');
    if alternatives.len() > 1 {
        return nested_or_reading(&alternatives, decls);
    }
    if is_range(pattern) {
        return Reading::Range { err: false };
    }
    if is_literal(pattern) {
        return Reading::Pins;
    }
    let (path, payload) = constructor_parts(pattern);
    match payload {
        Some(('(', inner)) if path.is_empty() => tuple_reading(inner, decls),
        Some(('[', inner)) if path.is_empty() => slice_reading(inner, decls),
        Some(('(', inner)) => match side_of_path(&path) {
            Some(side) => side_payload_reading(side, inner, decls),
            None if decls.shown_irrefutable_constructor(&path) => tuple_reading(inner, decls),
            // A tuple-struct or variant the line cannot resolve.
            None => Reading::Pins,
        },
        Some(('{', inner)) if decls.shown_irrefutable_constructor(&path) => {
            struct_payload_reading(inner, decls)
        }
        // A struct-like path is read as a variant unless it can be shown to
        // name a struct or the sole variant of a single-variant enum.
        Some(_) => Reading::Pins,
        None => match side_of_path(&path) {
            Some(Side::None) => Reading::SideOnly(Side::None),
            _ if decls.shown_irrefutable_constructor(&path) => Reading::Irrefutable,
            _ => Reading::Pins,
        },
    }
}

/// An or-pattern inside a constructor or tuple (`Some(Ok(_) | Err(_))`).
fn nested_or_reading(alternatives: &[&str], decls: &LocalDecls) -> Reading {
    let readings = alternatives
        .iter()
        .map(|alternative| read_pattern(alternative, decls))
        .collect::<Vec<_>>();
    let covers = |a: Side, b: Side| {
        readings.contains(&Reading::SideOnly(a)) && readings.contains(&Reading::SideOnly(b))
    };
    if readings.contains(&Reading::Irrefutable)
        || covers(Side::Some, Side::None)
        || covers(Side::Ok, Side::Err)
    {
        return Reading::Irrefutable;
    }
    // The same order as `or_pattern_classification`: alternatives on both
    // sides observe only a relation, and otherwise the disjunction reads as
    // its weakest alternative, pinning only when every alternative pins.
    let sides = alternatives
        .iter()
        .filter_map(|alternative| top_side(alternative))
        .collect::<Vec<_>>();
    let both = |a: Side, b: Side| sides.contains(&a) && sides.contains(&b);
    if both(Side::Some, Side::None) || both(Side::Ok, Side::Err) {
        return Reading::PinsNothing { err: false };
    }
    let weaker = readings
        .iter()
        .filter(|reading| **reading != Reading::Pins)
        .collect::<Vec<_>>();
    if weaker.is_empty() {
        Reading::Pins
    } else if let [Reading::SideOnly(side), rest @ ..] = weaker.as_slice()
        && rest
            .iter()
            .all(|reading| **reading == Reading::SideOnly(*side))
    {
        Reading::SideOnly(*side)
    } else if weaker
        .iter()
        .any(|reading| matches!(reading, Reading::Range { .. }))
    {
        Reading::Range { err: false }
    } else {
        Reading::PinsNothing { err: false }
    }
}

fn side_payload_reading(side: Side, inner: &str, decls: &LocalDecls) -> Reading {
    let err = side == Side::Err;
    let elements = split_top_level(inner, ',');
    let elements = elements
        .iter()
        .filter(|element| !element.is_empty())
        .collect::<Vec<_>>();
    let readings = elements
        .iter()
        .map(|element| {
            if element.trim() == ".." {
                Reading::Irrefutable
            } else {
                read_pattern(element, decls)
            }
        })
        .collect::<Vec<_>>();
    if readings.contains(&Reading::Pins) {
        Reading::Pins
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::Range { .. }))
    {
        Reading::Range { err }
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::PinsNothing { .. }))
    {
        Reading::PinsNothing { err }
    } else {
        Reading::SideOnly(side)
    }
}

fn combine(readings: &[Reading], all_irrefutable: Reading) -> Reading {
    if readings.contains(&Reading::Pins) {
        Reading::Pins
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::Range { .. }))
    {
        Reading::Range { err: false }
    } else if readings
        .iter()
        .all(|reading| *reading == Reading::Irrefutable)
    {
        all_irrefutable
    } else {
        Reading::PinsNothing { err: false }
    }
}

fn tuple_reading(inner: &str, decls: &LocalDecls) -> Reading {
    let elements = split_top_level(inner, ',');
    // `(p)` is a parenthesized pattern, not a one-tuple.
    if elements.len() == 1 {
        return read_pattern(inner, decls);
    }
    let readings = elements
        .iter()
        .filter(|element| !element.is_empty())
        .map(|element| {
            if *element == ".." {
                Reading::Irrefutable
            } else {
                read_pattern(element, decls)
            }
        })
        .collect::<Vec<_>>();
    combine(&readings, Reading::Irrefutable)
}

fn struct_payload_reading(inner: &str, decls: &LocalDecls) -> Reading {
    let readings = split_top_level(inner, ',')
        .into_iter()
        .filter(|element| !element.is_empty())
        .map(|element| {
            if element == ".." {
                return Reading::Irrefutable;
            }
            let fields = split_top_level(element, ':');
            let pattern = if fields.len() == 2 {
                fields[1]
            } else {
                element
            };
            read_pattern(pattern, decls)
        })
        .collect::<Vec<_>>();
    combine(&readings, Reading::Irrefutable)
}

fn slice_reading(inner: &str, decls: &LocalDecls) -> Reading {
    let elements = split_top_level(inner, ',')
        .into_iter()
        .filter(|element| !element.is_empty())
        .collect::<Vec<_>>();
    let is_rest = |element: &str| {
        element == ".."
            || split_top_level(element, '@')
                .as_slice()
                .get(1)
                .is_some_and(|rest| *rest == "..")
    };
    // `[..]` and `[name @ ..]` accept every slice; any other slice pattern
    // checks a length.
    if let [only] = elements.as_slice()
        && is_rest(only)
    {
        return Reading::Irrefutable;
    }
    let readings = elements
        .iter()
        .map(|element| {
            if is_rest(element) {
                Reading::Irrefutable
            } else {
                read_pattern(element, decls)
            }
        })
        .collect::<Vec<_>>();
    combine(&readings, Reading::PinsNothing { err: false })
}

fn is_range(pattern: &str) -> bool {
    let masked = mask_comments_and_strings(pattern);
    let mut depth = 0usize;
    let bytes = masked.as_bytes();
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '.' if depth == 0 && bytes.get(index + 1) == Some(&b'.') => {
                return masked.trim() != "..";
            }
            _ => {}
        }
    }
    false
}

fn is_literal(pattern: &str) -> bool {
    let pattern = pattern.strip_prefix('-').unwrap_or(pattern).trim_start();
    pattern == "true"
        || pattern == "false"
        || pattern.starts_with(|ch: char| ch.is_ascii_digit() || ch == '"' || ch == '\'')
        || pattern.starts_with("b\"")
        || pattern.starts_with("b'")
        || pattern.starts_with("r\"")
        || pattern.starts_with("r#")
}

#[cfg(test)]
mod tests {
    use super::pattern_assertion_classification;
    use crate::domain::{OracleKind, OracleStrength};

    #[test]
    fn spec_0231_rules_2_and_3_read_the_pattern() -> Result<(), String> {
        use OracleKind::{BroadError, RelationalCheck, SmokeOnly};
        use OracleStrength::{Smoke, Weak};
        let weakened = [
            // Acceptance examples 3, 4, 7, 17, 18, 21, 23, 24, 25 and 26.
            (
                "assert!(matches!(check(20), Err(e) if !e.is_empty()))",
                BroadError,
                Weak,
            ),
            ("assert!(matches!(check(20), Err(_)))", BroadError, Weak),
            ("assert!(matches!(check(5), Ok(_)))", SmokeOnly, Smoke),
            ("assert!(matches!(check(20), Err(_e)))", BroadError, Weak),
            ("ensure!(matches!(check(5), Ok(_)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(check(20), Err(ref e) if e.len() > 1))",
                BroadError,
                Weak,
            ),
            ("assert!(matches!(check(20), Err(mut e)))", BroadError, Weak),
            ("assert!(matches!(check(20), Err(e @ _)))", BroadError, Weak),
            (
                "assert!(matches!(lookup(1), Some(ref x)))",
                SmokeOnly,
                Smoke,
            ),
            ("assert!(matches!(check(5), Ok(x @ _)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(lookup(1), Some(_) | None))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(check(5), Ok(_) | Err(_)))",
                RelationalCheck,
                Weak,
            ),
            ("assert!(matches!(pair(), (_, _)))", RelationalCheck, Weak),
            (
                "assert!(matches!(lookup(1), Some(1..=5)))",
                RelationalCheck,
                Weak,
            ),
            ("assert!(matches!(parse(), Some(Ok(_))))", SmokeOnly, Smoke),
            ("assert!(matches!(lookup(1), &Some(_)))", SmokeOnly, Smoke),
            ("assert!(matches!(items(), [_, ..]))", RelationalCheck, Weak),
            (
                "assert!(matches!(check(20), Ok(_) | Err(E::Bad)))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(3) | Some(_)))",
                SmokeOnly,
                Smoke,
            ),
            ("assert!(matches!(items(), [..]))", RelationalCheck, Weak),
            (
                "assert!(matches!(items(), [rest @ ..]))",
                RelationalCheck,
                Weak,
            ),
            // Nested or-patterns, `(..)`, comments and qualified sides.
            (
                "assert!(matches!(lookup(1), Some(Ok(_) | Err(_))))",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(matches!(lookup(1), Some(None | Some(_))))",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(matches!(lookup(1), Some(_ | _)))",
                SmokeOnly,
                Smoke,
            ),
            // A nested disjunction reads as its weakest alternative, and
            // alternatives on both sides observe only a relation.
            (
                "assert!(matches!(lookup(1), Some(3 | 4..=9)))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(Ok(_) | Err(E::X))))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(Some(3) | Some(_))))",
                SmokeOnly,
                Smoke,
            ),
            ("assert!(matches!(pair(), (..)))", RelationalCheck, Weak),
            (
                "assert!(matches!(lookup(1), Some(x /* any */)))",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(matches!(lookup(1), std::option::Option::Some(_)))",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(matches!(check(5), Result::Ok(_)))",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(matches!(lookup(1), Some(x) if !(x == 3)))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(x) if x > 1 || x == 0))",
                RelationalCheck,
                Weak,
            ),
            // Other forms the rules name. A guard `==` that does not equate
            // the scrutinee, a binding, or a field/deref/tuple projection
            // of either pins no value (decision 7, #7726).
            (
                "assert!(matches!(check(20), Err(e) if e.len() == 3))",
                BroadError,
                Weak,
            ),
            (
                "assert!(matches!(check(5), Ok(_) if flag == true))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(check(20), Err(e) if e == e))",
                BroadError,
                Weak,
            ),
            (
                "assert!(matches!(value, _ if value == value))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(pair(), (a, b) if a == b))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(v) if v.id == v.id))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(v) if v.a == v.b))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(v) if other.id == 3))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(v) if v.len() == 3))",
                RelationalCheck,
                Weak,
            ),
            ("assert!(matches!(check(20), Err(1..=5)))", BroadError, Weak),
            (
                "assert!(matches!(lookup(1), Some(x) if x > 3))",
                RelationalCheck,
                Weak,
            ),
            ("assert_matches!(check(5), Ok(_))", SmokeOnly, Smoke),
            ("debug_assert_matches!(lookup(1), None)", SmokeOnly, Smoke),
            ("assert_matches!(check(20), Err(_e))", BroadError, Weak),
            ("assert!((matches!(check(5), Ok(_))))", SmokeOnly, Smoke),
            ("assert!(matches!(check(5), Ok(..)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(pair(), (Some(_), _)))",
                RelationalCheck,
                Weak,
            ),
        ];
        for (text, kind, strength) in weakened {
            let actual = pattern_assertion_classification(text);
            if actual.as_ref().map(|c| (&c.kind, &c.strength)) != Some((&kind, &strength)) {
                return Err(format!(
                    "{text}: expected {kind:?}/{strength:?}, got {actual:?}"
                ));
            }
        }
        // Patterns that pin a value keep the ordinary chain's reading, and
        // compound conditions are not pattern assertions. `Only::Value` and
        // `Cfg { .. }` pin nothing per examples 24 and 25, but a line with
        // no local declarations cannot show them, so they keep today's
        // exact reading until [`LocalDecls`] names the type (#6737).
        for text in [
            "assert!(matches!(check(5), Ok(5)))",
            "assert!(matches!(check(20), Err(E::Bad)))",
            "assert!(matches!(check(20), Err(e @ E::Bad)))",
            "assert!(matches!(check(20), Err(E::Bad(..))))",
            "assert!(matches!(check(20), Err(E::Bad { .. })))",
            "assert!(matches!(lookup(1), Some(x @ 3)))",
            "assert!(matches!(items(), [1, ..]))",
            "assert!(matches!(lookup(1), Some(\"a\")))",
            "assert!(matches!(make(), Only::Value))",
            "assert!(matches!(cfg(), Cfg { .. }))",
            "assert!(matches!(check(20), Err(E::A) | Err(E::B)))",
            "assert!(matches!(check(5), Ok(_)) && score(2) == 4)",
            "assert_eq!(check(20), Err(e))",
            // A guard equating the scrutinee, a binding, or a field /
            // deref / tuple projection of either pins a value
            // (decision 7, #7726).
            "assert!(matches!(value, _ if value == 2))",
            "assert_matches!(value, _ if value == 2)",
            "assert!(matches!(lookup(1), Some(x) if x == 3))",
            "assert!(matches!(lookup(1), Some(x) if ready && *x == 3))",
            "assert!(matches!(check(20), Err(e) if e == E::Bad))",
            "assert!(matches!(check(20), Err(e) if E::Bad == e))",
            "assert!(matches!(lookup(1), Some(v) if v.id == 3))",
            "assert!(matches!(lookup(1), Some(v) if 3 == v.id))",
            "assert!(matches!(lookup(1), Some(v) if (*v).id == 3))",
            "assert!(matches!(lookup(1), Some(v) if v.0 == 3))",
            "assert!(matches!(value, _ if value.id == 3))",
            // A user enum's `None`, `Ok` or `Err` variant is a variant pin.
            "assert!(matches!(mode(), Mode::None))",
            "assert!(matches!(reply(), Reply::Ok(_)))",
            "assert!(matches!(reply(), Reply::Err(_)))",
            "assert!(matches!(lookup(1), Some(Foo::A | Foo::None)))",
            "assert!(matches!(lookup(1), Some(3 | 4)))",
            "assert!(matches!(lookup(1), Some(3 /* three */)))",
        ] {
            let actual = pattern_assertion_classification(text);
            if actual.is_some() {
                return Err(format!(
                    "{text}: expected no pattern reading, got {actual:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn spec_0231_local_struct_and_single_variant_enum_are_weak() -> Result<(), String> {
        use super::super::classify::classify_assertion_in;
        use super::{LocalDecls, pattern_assertion_classification_in};
        use OracleKind::{ExactValue, RelationalCheck};
        use OracleStrength::{Strong, Weak};

        let source = r#"
pub struct Cfg { pub retries: u32 }
pub fn cfg() -> Cfg { Cfg { retries: 2 } }
enum Only { Value }
fn make() -> Only { Only::Value }
enum Wrap { Value(u32) }
fn wrap() -> Wrap { Wrap::Value(1) }
enum Two { A, B }
"#;
        let decls = LocalDecls::from_source(source);
        let weakened = [
            "assert!(matches!(cfg(), Cfg { .. }))",
            "assert!(matches!(cfg(), Cfg { retries: _ }))",
            "assert!(matches!(make(), Only::Value))",
            "assert!(matches!(wrap(), Wrap::Value(_)))",
            "assert_matches!(cfg(), Cfg { .. })",
        ];
        for text in weakened {
            let actual = pattern_assertion_classification_in(text, &decls);
            if actual.as_ref().map(|c| (&c.kind, &c.strength)) != Some((&RelationalCheck, &Weak)) {
                return Err(format!(
                    "{text}: expected RelationalCheck/Weak from local decls, got {actual:?}"
                ));
            }
            let classified = classify_assertion_in(text, source);
            if classified.kind != RelationalCheck || classified.strength != Weak {
                return Err(format!(
                    "{text}: expected RelationalCheck/Weak through classify_assertion_in, got {:?}/{:?}",
                    classified.kind, classified.strength
                ));
            }
        }

        let still_pin = [
            "assert!(matches!(cfg(), Cfg { retries: 2 }))",
            "assert!(matches!(make(), Two::A))",
            "assert!(matches!(make(), crate::Only::Value))",
            "assert!(matches!(cfg(), other::Cfg { .. }))",
        ];
        for text in still_pin {
            if pattern_assertion_classification_in(text, &decls).is_some() {
                return Err(format!(
                    "{text}: expected no pattern ceiling with local decls"
                ));
            }
            let classified = classify_assertion_in(text, source);
            if classified.kind != ExactValue || classified.strength != Strong {
                return Err(format!(
                    "{text}: expected ExactValue/Strong when the path is not shown, got {:?}/{:?}",
                    classified.kind, classified.strength
                ));
            }
        }

        let empty = LocalDecls::from_source("");
        for text in [
            "assert!(matches!(cfg(), Cfg { .. }))",
            "assert!(matches!(make(), Only::Value))",
        ] {
            if pattern_assertion_classification_in(text, &empty).is_some() {
                return Err(format!(
                    "{text}: empty decls must keep today's exact reading"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn guard_projection_of_a_pattern_binding_pins_a_value() -> Result<(), String> {
        use crate::analysis::extract::oracles::classify_assertion;
        use OracleKind::{BroadError, ExactValue, RelationalCheck};
        use OracleStrength::{Strong, Weak};

        let whole_pattern_pins = [
            "assert!(matches!(x, Some(v) if v.id == 3))",
            "assert!(matches!(x, Some(v) if 3 == v.id))",
            "assert!(matches!(x, Some(v) if (*v).id == 3))",
            "assert!(matches!(x, Some(v) if ready && v.id == 3))",
            "assert!(matches!(x, Some(v) if v.inner.id == 3))",
            "assert!(matches!(x, Some(v) if v.0 == 3))",
            "assert!(matches!(value, _ if value.id == 3))",
        ];
        for text in whole_pattern_pins {
            if let Some(ceiling) = pattern_assertion_classification(text) {
                return Err(format!(
                    "{text}: expected a pinning guard (no pattern ceiling), got {ceiling:?}"
                ));
            }
            let actual = classify_assertion(text);
            if actual.kind != ExactValue || actual.strength != Strong {
                return Err(format!(
                    "{text}: expected ExactValue/Strong through classify_assertion, got {:?}/{:?}",
                    actual.kind, actual.strength
                ));
            }
        }
        // Nested `matches!` is not a whole-pattern assertion; the same
        // guard reader still lets `some_matches_pins_a_value` keep exact
        // credit. The closure-body `*r == Ok(3)` form is out of this claim.
        let nested = "assert!(xs.iter().any(|x| matches!(x, Some(v) if v.id == 3)))";
        if pattern_assertion_classification(nested).is_some() {
            return Err(format!(
                "{nested}: nested matches! is not a whole-pattern assertion, got {:?}",
                pattern_assertion_classification(nested)
            ));
        }
        let nested_actual = classify_assertion(nested);
        if nested_actual.kind != ExactValue || nested_actual.strength != Strong {
            return Err(format!(
                "{nested}: expected ExactValue/Strong through classify_assertion, got {:?}/{:?}",
                nested_actual.kind, nested_actual.strength
            ));
        }

        let still_weak = [
            (
                "assert!(matches!(x, Some(v) if v.id == v.id))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(x, Some(v) if v.a == v.b))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(x, Some(v) if v.id == 3 || v.id == 4))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(x, Some(v) if other.id == 3))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(x, Some(w) if v.id == 3))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(x, Some(v) if v.len() == 3))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(check(20), Err(e) if e.len() == 3))",
                BroadError,
                Weak,
            ),
        ];
        for (text, kind, strength) in still_weak {
            let ceiling = pattern_assertion_classification(text);
            if ceiling.as_ref().map(|c| (&c.kind, &c.strength)) != Some((&kind, &strength)) {
                return Err(format!(
                    "{text}: expected pattern ceiling {kind:?}/{strength:?}, got {ceiling:?}"
                ));
            }
            let actual = classify_assertion(text);
            if actual.kind != kind || actual.strength != strength {
                return Err(format!(
                    "{text}: expected {kind:?}/{strength:?} through classify_assertion, got {:?}/{:?}",
                    actual.kind, actual.strength
                ));
            }
        }
        Ok(())
    }
}
