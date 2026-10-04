//! Which arm of a changed `match` a related test's owner call selects
//! (RIPR-SPEC-0229, #5432).
//!
//! A changed match arm is observed only by a test whose input reaches that
//! arm. The token rule in `reveal` confirms a qualified variant
//! (`Mode::Frozen`) wherever an assertion names it, but a glob-imported
//! variant (`LowerCase =>`), an `Option` arm (`None =>`) or an integer arm
//! (`2 =>`) has no qualified token, so the arm stays unconfirmed whichever
//! arm the test selects.
//!
//! This module reads the arm's pattern and the input a direct owner call
//! passes at the match scrutinee's position, and judges the pair:
//!
//! - `Selects`: the input's constructor or literal equals one of the arm's
//!   alternatives, and that alternative binds its payload irrefutably;
//! - `SelectsOther`: every alternative names a constructor or literal, and
//!   the input's differs from all of them;
//! - `Unknown`: anything else (a variable input, a wildcard or binding
//!   alternative, a range, a guard, a const-looking name, mixed kinds).
//!
//! The scrutinee must be the owner's `self` receiver or one of its named
//! parameters, read from the enclosing `match <scrutinee> {` with no
//! intervening rebinding. Every other shape is `Unknown`, so the judgment
//! fails closed: it never names an arm that a computed value could select.

use super::super::rust_index::{FunctionSummary, TestSummary};
use super::activation::function_parameters;
use super::reveal::{
    assertion_comparison_operands, find_fat_arrow, matching_parenthesis, split_top_level_arguments,
    string_span_ranges,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::domain::Probe;

/// Where the owner's `match` reads its scrutinee from.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScrutineeBinding {
    /// `match self { .. }` in a method: the call's receiver.
    Receiver,
    /// `match <param> { .. }`: the call's argument at this index (the
    /// `self` receiver, if any, is not counted).
    Parameter(usize),
}

/// The changed arm, the scrutinee binding, and the owner call shape that
/// supplies the scrutinee. Built once per probe.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct ArmSelector {
    alternatives: Vec<PatternHead>,
    /// The alternatives of each arm before the changed one, in source
    /// order. `None` for an arm that cannot be read (a guard): such an arm
    /// may take any input first.
    earlier: Vec<Option<Vec<PatternHead>>>,
    pattern_text: String,
    scrutinee: String,
    /// The scrutinee's type name (`Mode` for `mode: Mode`, the impl's self
    /// type for `self`), when the signature or impl names one. A qualified
    /// input must be qualified by it.
    scrutinee_type: Option<String>,
    binding: ScrutineeBinding,
    owner: String,
    method: bool,
    /// Whether every call of the owner runs the enclosing `match`: it is
    /// the first thing the body does, after `let` statements with no
    /// control flow. A match nested in another arm, an `if`, a loop, or
    /// after a possible early `return`/`?` may be skipped, so an input
    /// that fits the arm does not show the arm ran.
    reached: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum ArmSelection {
    Selects,
    SelectsOther,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LiteralValue {
    Int(i128),
    Str(String),
    Char(String),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PatternHead {
    Literal(LiteralValue),
    /// A tuple/struct/unit variant. `irrefutable` is true when the payload
    /// (if any) is made only of `_`, `..` and plain bindings, so the
    /// variant name alone decides selection.
    Variant {
        name: String,
        irrefutable: bool,
    },
    Opaque,
}

/// One related test's owner-call inputs at the scrutinee position, judged
/// against the changed arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct ObservedArmInputs {
    /// The input text of every direct owner call, in source order.
    pub(in crate::analysis) inputs: Vec<String>,
    pub(in crate::analysis) selection: ArmSelection,
}

impl ArmSelector {
    /// The selector for a `MatchArm` probe whose owner reads the arm's
    /// scrutinee from `self` or a named parameter. `None` when the arm has
    /// a guard, the enclosing `match` is not found, or its scrutinee is
    /// computed or rebound.
    pub(in crate::analysis) fn establish(probe: &Probe, owner: &FunctionSummary) -> Option<Self> {
        if owner.name.is_empty() {
            return None;
        }
        let pattern_text = arm_pattern_text(&probe.expression)?;
        let alternatives = top_level_alternatives(&pattern_text)?
            .into_iter()
            .map(pattern_head)
            .collect::<Vec<_>>();
        if alternatives.is_empty() {
            return None;
        }
        let method = owner.item.has_self_param || owner_has_self_parameter(owner);
        let EnclosingMatch {
            scrutinee,
            earlier_patterns,
            reached,
        } = enclosing_match(owner, probe.location.line)?;
        let earlier = earlier_patterns
            .iter()
            .map(|pattern| earlier_arm(pattern))
            .collect::<Vec<_>>();
        let (binding, scrutinee_type) = if method && matches!(scrutinee.as_str(), "self" | "*self")
        {
            if receiver_may_change(owner) {
                return None;
            }
            let self_type = match &owner.impl_context {
                crate::analysis::facts::FunctionImplContext::Impl { self_type } => {
                    Some(self_type.clone())
                }
                _ => None,
            };
            (ScrutineeBinding::Receiver, self_type)
        } else {
            let parameters = function_parameters(owner);
            let index = parameters
                .iter()
                .position(|parameter| parameter_name(parameter) == scrutinee)?;
            let declared = parameter_declaration(owner, &scrutinee)?;
            if parameter_may_change(owner, &scrutinee, declared.mutable) {
                return None;
            }
            (ScrutineeBinding::Parameter(index), declared.type_name)
        };
        Some(Self {
            alternatives,
            earlier,
            pattern_text,
            scrutinee,
            scrutinee_type,
            binding,
            owner: owner.name.clone(),
            method,
            reached,
        })
    }

    /// The scrutinee as the owner's `match` names it (`x`, `self`).
    pub(in crate::analysis) fn scrutinee(&self) -> &str {
        &self.scrutinee
    }

    /// The arm pattern as written, without the `=>`.
    pub(in crate::analysis) fn pattern_text(&self) -> &str {
        &self.pattern_text
    }

    /// Whether either compared operand of an `assert_eq!`/`assert_ne!` is
    /// exactly a direct owner call whose scrutinee input selects this arm.
    /// Diagnostic arguments and every other assertion shape never select.
    pub(in crate::analysis) fn assertion_selects(&self, assertion_text: &str) -> bool {
        let Some(operands) = assertion_comparison_operands(assertion_text) else {
            return false;
        };
        operands.iter().any(|operand| {
            let operand = operand.trim();
            owner_calls_in(operand, &self.owner, self.method)
                .into_iter()
                .any(|call| {
                    call.span == (0, operand.len())
                        && self
                            .call_input(&call)
                            .is_some_and(|input| self.judge(input) == ArmSelection::Selects)
                })
        })
    }

    /// Every direct owner call in a test body, judged against this arm.
    /// `None` when the body names the owner anywhere other than a call
    /// this module can read (a function pointer, a multi-line call, a
    /// differently shaped call), since such a use may select any arm.
    pub(in crate::analysis) fn observed_inputs(
        &self,
        test: &TestSummary,
    ) -> Option<ObservedArmInputs> {
        let masked = mask_comments_and_strings(&test.body);
        let mentions = whole_word_offsets(&masked, &self.owner);
        if mentions.is_empty() {
            return None;
        }
        let mut inputs = Vec::new();
        let mut selection = ArmSelection::SelectsOther;
        for line_start in line_starts_of(&test.body, &mentions) {
            let line_end = test.body[line_start..]
                .find('\n')
                .map_or(test.body.len(), |offset| line_start + offset);
            let line = &test.body[line_start..line_end];
            let calls = owner_calls_in(line, &self.owner, self.method);
            let line_mentions = mentions
                .iter()
                .filter(|offset| (line_start..line_end).contains(*offset))
                .count();
            if calls.len() != line_mentions {
                return None;
            }
            for call in calls {
                let input = self.call_input(&call)?;
                inputs.push(input.to_string());
                selection = combine(selection, self.judge(input));
            }
        }
        Some(ObservedArmInputs { inputs, selection })
    }

    fn call_input<'a>(&self, call: &OwnerCall<'a>) -> Option<&'a str> {
        match self.binding {
            ScrutineeBinding::Receiver => call.receiver,
            ScrutineeBinding::Parameter(index) => call.arguments.get(index).copied(),
        }
    }

    /// First-match selection: the changed arm is selected only when its own
    /// alternatives select the input, every earlier arm provably does not,
    /// and the call always runs the enclosing `match`. Not matching the
    /// changed arm's own pattern is enough for `SelectsOther`, whatever the
    /// earlier arms are and whether the `match` runs at all.
    fn judge(&self, input: &str) -> ArmSelection {
        // `Priority::Low` is not `Level::Low`: a qualifier other than the
        // scrutinee's type (or `Self`) names another enum's variant.
        if let Some(qualifier) = input_qualifier(input)
            && qualifier != "Self"
            && self.scrutinee_type.as_deref() != Some(qualifier)
        {
            return ArmSelection::Unknown;
        }
        let input = input_head(input);
        if input == PatternHead::Opaque {
            return ArmSelection::Unknown;
        }
        match judge_alternatives(&self.alternatives, &input) {
            ArmSelection::Selects => {}
            other => return other,
        }
        for arm in &self.earlier {
            let Some(alternatives) = arm else {
                return ArmSelection::Unknown;
            };
            if judge_alternatives(alternatives, &input) != ArmSelection::SelectsOther {
                return ArmSelection::Unknown;
            }
        }
        if self.reached {
            ArmSelection::Selects
        } else {
            ArmSelection::Unknown
        }
    }
}

fn judge_alternatives(alternatives: &[PatternHead], input: &PatternHead) -> ArmSelection {
    let mut selection = ArmSelection::SelectsOther;
    for alternative in alternatives {
        match judge_alternative(alternative, input) {
            ArmSelection::Selects => return ArmSelection::Selects,
            ArmSelection::Unknown => selection = ArmSelection::Unknown,
            ArmSelection::SelectsOther => {}
        }
    }
    selection
}

/// An earlier arm's alternatives, or `None` when it carries a guard.
fn earlier_arm(pattern: &str) -> Option<Vec<PatternHead>> {
    let masked = mask_comments_and_strings(pattern);
    if !whole_word_offsets(&masked, "if").is_empty() {
        return None;
    }
    Some(
        top_level_alternatives(pattern)?
            .into_iter()
            .map(pattern_head)
            .collect(),
    )
}

/// Across calls: any `Selects` wins, then any `Unknown`.
fn combine(left: ArmSelection, right: ArmSelection) -> ArmSelection {
    match (left, right) {
        (ArmSelection::Selects, _) | (_, ArmSelection::Selects) => ArmSelection::Selects,
        (ArmSelection::Unknown, _) | (_, ArmSelection::Unknown) => ArmSelection::Unknown,
        _ => ArmSelection::SelectsOther,
    }
}

fn judge_alternative(alternative: &PatternHead, input: &PatternHead) -> ArmSelection {
    match (alternative, input) {
        (PatternHead::Literal(pattern), PatternHead::Literal(value)) => {
            if std::mem::discriminant(pattern) != std::mem::discriminant(value) {
                ArmSelection::Unknown
            } else if pattern == value {
                ArmSelection::Selects
            } else {
                ArmSelection::SelectsOther
            }
        }
        (
            PatternHead::Variant { name, irrefutable },
            PatternHead::Variant {
                name: input_name, ..
            },
        ) => {
            if name != input_name {
                ArmSelection::SelectsOther
            } else if *irrefutable {
                ArmSelection::Selects
            } else {
                ArmSelection::Unknown
            }
        }
        _ => ArmSelection::Unknown,
    }
}

/// The pattern before `=>`, without comments, trimmed, with no guard.
/// `None` without a fat arrow or with an `if` guard: a guard's truth is
/// never established here.
fn arm_pattern_text(expression: &str) -> Option<String> {
    let arrow = find_fat_arrow(expression)?;
    // Two arms on one line: which one changed is not known.
    if find_fat_arrow(&expression[arrow + 2..]).is_some() {
        return None;
    }
    let pattern = without_comments(&expression[..arrow])?;
    let pattern = pattern.trim();
    let masked = mask_comments_and_strings(pattern);
    if pattern.is_empty() || !whole_word_offsets(&masked, "if").is_empty() {
        return None;
    }
    Some(pattern.to_string())
}

/// `text` with every comment byte blanked and string literals kept, so a
/// `/* "x" */` beside a pattern never reads as part of it.
fn without_comments(text: &str) -> Option<String> {
    let masked = mask_comments_and_strings(text);
    let strings = string_span_ranges(text);
    let bytes = text
        .bytes()
        .zip(masked.bytes())
        .enumerate()
        .map(|(index, (original, masked))| {
            let in_string = strings
                .iter()
                .any(|(start, end)| index >= *start && index < *end);
            if original != masked && !in_string {
                b' '
            } else {
                original
            }
        })
        .collect::<Vec<_>>();
    String::from_utf8(bytes).ok()
}

/// Split a pattern at top-level `|`, dropping a leading `|`.
fn top_level_alternatives(pattern: &str) -> Option<Vec<&str>> {
    let masked = mask_comments_and_strings(pattern);
    let mut alternatives = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 => {
                alternatives.push(pattern[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    if depth != 0 {
        return None;
    }
    alternatives.push(pattern[start..].trim());
    if alternatives.first().is_some_and(|first| first.is_empty()) {
        alternatives.remove(0);
    }
    if alternatives
        .iter()
        .any(|alternative| alternative.is_empty())
    {
        return None;
    }
    Some(alternatives)
}

fn pattern_head(alternative: &str) -> PatternHead {
    if let Some(literal) = literal_value(alternative) {
        return PatternHead::Literal(literal);
    }
    match variant_parts(alternative) {
        Some((name, payload)) => PatternHead::Variant {
            name: name.to_string(),
            irrefutable: payload.is_none_or(payload_is_irrefutable),
        },
        None => PatternHead::Opaque,
    }
}

/// The same reading applied to a call's input expression. A variant input
/// selects by name whatever its payload is.
fn input_head(input: &str) -> PatternHead {
    let input = input.trim();
    if let Some(literal) = literal_value(input) {
        return PatternHead::Literal(literal);
    }
    match variant_parts(input) {
        Some((name, _)) => PatternHead::Variant {
            name: name.to_string(),
            irrefutable: true,
        },
        None => PatternHead::Opaque,
    }
}

/// A path whose last segment reads as a variant (`Some`, `LowerCase`,
/// `Mode::Frozen`), optionally followed by one `(..)` or `{..}` payload.
/// The payload text is returned without its delimiters.
fn variant_parts(text: &str) -> Option<(&str, Option<&str>)> {
    let text = text.trim();
    let path_end = text
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(text.len());
    let path = &text[..path_end];
    let rest = text[path_end..].trim_start();
    let segments = path.split("::").collect::<Vec<_>>();
    if segments.iter().any(|segment| !is_identifier(segment)) {
        return None;
    }
    let name = segments.last()?;
    if !is_variant_name(name) {
        return None;
    }
    if rest.is_empty() {
        return Some((name, None));
    }
    let (open, close) = match rest.as_bytes().first() {
        Some(b'(') => ('(', ')'),
        Some(b'{') => ('{', '}'),
        _ => return None,
    };
    let inner = rest.strip_prefix(open)?.strip_suffix(close)?;
    // The delimiters must enclose the whole payload: `Some(a)(b)` is not
    // one variant.
    let masked = mask_comments_and_strings(inner);
    let mut depth = 0i32;
    for byte in masked.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    (depth == 0).then_some((name, Some(inner)))
}

/// `_`, `..`, plain bindings (`v`, `ref v`, `mut v`) and struct shorthand
/// or `field: binding` entries. Anything that could refute (a literal, a
/// nested variant, a range) is not irrefutable.
fn payload_is_irrefutable(payload: &str) -> bool {
    let Some(entries) = split_top_level_arguments(payload) else {
        return false;
    };
    entries.into_iter().all(|entry| {
        let entry = entry.trim();
        let binding = entry
            .rsplit_once(':')
            .map_or(entry, |(_, value)| value)
            .trim();
        let binding = binding
            .strip_prefix("ref ")
            .or_else(|| binding.strip_prefix("mut "))
            .unwrap_or(binding)
            .trim();
        // `true`/`false` are literals, and a path (`limits::MAX`) is a
        // constant, not a binding: both refute.
        let is_binding = is_identifier(binding)
            && binding.starts_with(|ch: char| ch.is_ascii_lowercase())
            && !matches!(binding, "true" | "false");
        !entry.contains("::")
            && (binding.is_empty() || binding == "_" || binding == ".." || is_binding)
    })
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// CamelCase only: an uppercase first letter and at least one lowercase
/// letter. `SCREAMING` names are constants (which a pattern compares by
/// value, not by name), single letters are ambiguous, and lowercase names
/// are bindings or functions.
fn is_variant_name(name: &str) -> bool {
    name.starts_with(|ch: char| ch.is_ascii_uppercase())
        && name.chars().any(|ch| ch.is_ascii_lowercase())
}

fn literal_value(text: &str) -> Option<LiteralValue> {
    let text = text.trim();
    match text {
        "true" => return Some(LiteralValue::Bool(true)),
        "false" => return Some(LiteralValue::Bool(false)),
        _ => {}
    }
    if let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return (!inner.contains(['"', '\\'])).then(|| LiteralValue::Str(inner.to_string()));
    }
    if let Some(inner) = text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return (!inner.contains(['\'', '\\']) && inner.chars().count() == 1)
            .then(|| LiteralValue::Char(inner.to_string()));
    }
    integer_value(text).map(LiteralValue::Int)
}

/// Decimal, hex, octal or binary integer literals with `_` separators and an
/// optional integer type suffix. `0x07FF` and `0x7FF` are the same value.
fn integer_value(text: &str) -> Option<i128> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, text),
    };
    let (radix, digits) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, digits)
    };
    let digits = strip_integer_suffix(digits, radix);
    let cleaned = digits.replace('_', "");
    if cleaned.is_empty() || !cleaned.chars().all(|ch| ch.is_digit(radix)) {
        return None;
    }
    let value = i128::from_str_radix(&cleaned, radix).ok()?;
    Some(if negative { -value } else { value })
}

fn strip_integer_suffix(digits: &str, radix: u32) -> &str {
    for suffix in [
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        // A hex literal's own digits can end in what looks like a suffix
        // only through `u`/`i`, which are not hex digits, so stripping is
        // safe in every radix.
        if let Some(rest) = digits.strip_suffix(suffix)
            && (radix != 16 || !rest.is_empty())
        {
            return rest;
        }
    }
    digits
}

fn owner_has_self_parameter(owner: &FunctionSummary) -> bool {
    let signature = owner.body.lines().next().unwrap_or_default();
    let Some(open) = signature.find('(') else {
        return false;
    };
    let first = signature[open + 1..]
        .split([',', ')'])
        .next()
        .unwrap_or_default();
    // `self`, `mut self`, `&self`, `&mut self`, `&'a self`, `self: Box<Self>`.
    let name = first.split(':').next().unwrap_or_default().trim();
    let name = name.strip_prefix('&').unwrap_or(name).trim_start();
    let name = match name.strip_prefix('\'') {
        Some(rest) => rest
            .split_once(char::is_whitespace)
            .map_or("", |(_, rest)| rest)
            .trim_start(),
        None => name,
    };
    name.strip_prefix("mut ").unwrap_or(name).trim() == "self"
}

/// `name` from `name`, `mut name`, or a destructuring-free parameter.
fn parameter_name(parameter: &str) -> &str {
    parameter
        .trim()
        .strip_prefix("mut ")
        .unwrap_or(parameter.trim())
        .trim()
}

/// The enclosing `match` of the arm that starts on `arm_line`: its
/// scrutinee and the patterns of the arms before it, in source order.
/// `None` when the arm's enclosing brace is not a `match`, the line does
/// not start an arm, or an earlier arm cannot be delimited.
struct EnclosingMatch {
    scrutinee: String,
    earlier_patterns: Vec<String>,
    reached: bool,
}

fn enclosing_match(owner: &FunctionSummary, arm_line: usize) -> Option<EnclosingMatch> {
    let relative = arm_line.checked_sub(owner.start_line)?;
    let masked = mask_comments_and_strings(&owner.body);
    let source = without_comments(&owner.body)?;
    let arm_offset = masked
        .split_inclusive('\n')
        .take(relative)
        .map(str::len)
        .sum::<usize>();
    if arm_offset == 0 || arm_offset > masked.len() {
        return None;
    }
    let before = &masked[..arm_offset];
    let mut depth = 0usize;
    let mut brace = None;
    for (index, byte) in before.bytes().enumerate().rev() {
        match byte {
            b'}' | b')' | b']' => depth += 1,
            b'{' | b'(' | b'[' => {
                if depth == 0 {
                    brace = (byte == b'{').then_some(index);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let brace = brace?;
    let head = &before[..brace];
    let keyword = *whole_word_offsets(head, "match").last()?;
    let scrutinee = head[keyword + "match".len()..].trim();
    if scrutinee.is_empty() || scrutinee.contains(['{', '}', ';']) {
        return None;
    }
    if !(scrutinee == "*self" || is_identifier(scrutinee) && !is_variant_name(scrutinee)) {
        return None;
    }
    let earlier_patterns = earlier_arm_patterns(&masked, &source, brace + 1, arm_offset)?;
    Some(EnclosingMatch {
        scrutinee: scrutinee.to_string(),
        earlier_patterns,
        reached: match_always_runs(&masked[..keyword]),
    })
}

/// Whether the owner's body always reaches a `match` whose keyword ends
/// `masked_before_match`: everything between the body's opening `{` and
/// the keyword is `let` statements (or a `let x =` head for the match
/// itself) with no block, closure, branch, loop, early exit or macro.
fn match_always_runs(masked_before_match: &str) -> bool {
    let Some(body_open) = masked_before_match.find('{') else {
        return false;
    };
    let prefix = &masked_before_match[body_open + 1..];
    let control = [
        "if", "else", "match", "loop", "while", "for", "return", "break", "continue",
    ];
    !prefix.contains(['{', '}', '?', '|', '!', '#'])
        && !prefix.contains("=>")
        && control
            .iter()
            .all(|keyword| whole_word_offsets(prefix, keyword).is_empty())
        && prefix
            .split(';')
            .map(str::trim)
            .all(|statement| statement.is_empty() || statement.starts_with("let "))
}

/// Arm patterns in `masked[start..end]`, the part of a match body before
/// the changed arm. Each arm is `pattern => expression` ended by a
/// top-level `,`, or `pattern => { block }` with an optional `,`. The scan
/// must end exactly at an arm boundary, or the changed line does not start
/// an arm of this match.
fn earlier_arm_patterns(
    masked: &str,
    source: &str,
    start: usize,
    end: usize,
) -> Option<Vec<String>> {
    let bytes = masked.as_bytes();
    let mut patterns = Vec::new();
    let mut arm_start = start;
    let mut index = start;
    let mut depth = 0usize;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b'=' if depth == 0 && bytes.get(index + 1) == Some(&b'>') => {
                patterns.push(source.get(arm_start..index)?.trim().to_string());
                index = arm_expression_end(masked, index + 2, end)?;
                arm_start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    (depth == 0 && masked.get(arm_start..end)?.trim().is_empty()).then_some(patterns)
}

/// The offset just past an arm expression that starts at `from`: past its
/// block (and an optional `,`) or past its top-level `,`.
fn arm_expression_end(masked: &str, from: usize, end: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut index = from;
    while index < end && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let block = bytes.get(index) == Some(&b'{');
    let mut depth = 0usize;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if block && depth == 0 {
                    let mut next = index + 1;
                    while next < end && bytes[next].is_ascii_whitespace() {
                        next += 1;
                    }
                    if bytes.get(next) == Some(&b',') {
                        next += 1;
                    }
                    return Some(next);
                }
            }
            b',' if depth == 0 => return Some(index + 1),
            // A block-like body (`match`, `if .. else`, `unsafe`, a labeled
            // block) may end without a comma; a second `=>` before the
            // comma means the next arm was swallowed.
            b'=' if depth == 0 && !block && bytes.get(index + 1) == Some(&b'>') => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

/// The type segment before the variant in a qualified input path
/// (`Mode` in `Mode::Hot`, `Mode::Hot(1)`), or `None` when unqualified.
fn input_qualifier(input: &str) -> Option<&str> {
    let input = input.trim();
    let path_end = input
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(input.len());
    let mut segments = input[..path_end].rsplit("::");
    segments.next()?;
    segments.next()
}

struct ParameterDeclaration {
    mutable: bool,
    type_name: Option<String>,
}

/// How `name` is declared in the owner's one-line signature: whether it is
/// `mut name` or `&mut T`, and the last path segment of its type with
/// references and generic arguments dropped (`Option` for `Option<i32>`).
fn parameter_declaration(owner: &FunctionSummary, name: &str) -> Option<ParameterDeclaration> {
    let signature = owner.body.lines().next()?;
    let open = signature.find('(')?;
    let close = matching_parenthesis(signature, open)?;
    let entries = split_top_level_arguments(&signature[open + 1..close])?;
    let entry = entries.into_iter().find(|entry| {
        entry
            .split_once(':')
            .is_some_and(|(pattern, _)| parameter_name(pattern) == name)
    })?;
    let (pattern, ty) = entry.split_once(':')?;
    let ty = ty.trim();
    let mutable = pattern.trim().starts_with("mut ")
        || ty.starts_with("&mut")
        || ty.starts_with("&'") && ty.contains(" mut ");
    let base = ty.trim_start_matches('&');
    let base = match base.strip_prefix('\'') {
        Some(rest) => rest.split_once(' ').map_or("", |(_, rest)| rest),
        None => base,
    };
    let base = base.trim().strip_prefix("mut ").unwrap_or(base.trim());
    let base = base.split('<').next().unwrap_or(base).trim();
    let type_name = base
        .rsplit("::")
        .next()
        .filter(|name| is_identifier(name))
        .map(str::to_string);
    Some(ParameterDeclaration { mutable, type_name })
}

/// The owner's body after its signature line, masked.
fn owner_body_after_signature(owner: &FunctionSummary) -> String {
    let masked = mask_comments_and_strings(&owner.body);
    masked
        .split_once('\n')
        .map_or(String::new(), |(_, rest)| rest.to_string())
}

/// Whether the parameter may hold something other than the caller's
/// argument when the match reads it. The whole body is read, not only the
/// part before the arm, because a loop can reassign it after the match. A
/// binding position (`let x`, `Some(x) =`, `for x in`, `|x|`, `W { f: x }`),
/// an assignment, or, for a `mut`/`&mut` parameter, any method call or
/// further use refuses. Plain reads of an immutable parameter do not. A use
/// inside call arguments (`f(x)`) also refuses: telling it from a pattern
/// position needs a parser.
fn parameter_may_change(owner: &FunctionSummary, name: &str, mutable: bool) -> bool {
    let body = owner_body_after_signature(owner);
    whole_word_offsets(&body, name).into_iter().any(|offset| {
        let prefix = body[..offset].trim_end();
        let suffix = body[offset + name.len()..].trim_start();
        if prefix.ends_with("match") && suffix.starts_with('{') {
            return false;
        }
        mutable
            || [
                "let", "mut", "ref", "for", "|", "(", ",", "{", "@", "[", "&",
            ]
            .iter()
            .any(|marker| prefix.ends_with(marker))
            || (prefix.ends_with(':') && !prefix.ends_with("::"))
            || (suffix.starts_with('=') && !suffix.starts_with("=="))
            || [
                "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>=", "@", "in ",
            ]
            .iter()
            .any(|marker| suffix.starts_with(marker))
            || (suffix.starts_with(':') && !suffix.starts_with("::"))
    })
}

/// Whether a `mut self`/`&mut self` method may change `self` before or
/// between reads of the match: any use of `self` other than the match's own
/// scrutinee refuses. An immutable receiver cannot be reassigned.
fn receiver_may_change(owner: &FunctionSummary) -> bool {
    let signature = owner.body.lines().next().unwrap_or_default();
    let masked_signature = mask_comments_and_strings(signature);
    let mutable = whole_word_offsets(&masked_signature, "self")
        .into_iter()
        .any(|offset| masked_signature[..offset].trim_end().ends_with("mut"));
    if !mutable {
        return false;
    }
    let body = owner_body_after_signature(owner);
    whole_word_offsets(&body, "self").into_iter().any(|offset| {
        let prefix = body[..offset].trim_end().trim_end_matches('*').trim_end();
        let suffix = body[offset + "self".len()..].trim_start();
        !(prefix.ends_with("match") && suffix.starts_with('{'))
    })
}

/// One direct owner call: `owner(..)`, `Path::owner(..)` or
/// `<receiver>.owner(..)`, with its byte span in the scanned text.
struct OwnerCall<'a> {
    span: (usize, usize),
    receiver: Option<&'a str>,
    arguments: Vec<&'a str>,
}

/// Owner calls in one line or operand. A method owner is read only through
/// `<receiver>.owner(..)` with a plain path or literal receiver; a free
/// owner only through `owner(..)` or `Path::owner(..)`. Calls of the other
/// form, or whose receiver is computed, are left out, so a caller that
/// counts mentions sees the gap.
fn owner_calls_in<'a>(text: &'a str, owner: &str, method: bool) -> Vec<OwnerCall<'a>> {
    let masked = mask_comments_and_strings(text);
    let mut calls = Vec::new();
    for offset in whole_word_offsets(&masked, owner) {
        let after = offset + owner.len();
        let open = after + (masked[after..].len() - masked[after..].trim_start().len());
        if masked.as_bytes().get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_parenthesis(text, open) else {
            continue;
        };
        let Some(arguments) = split_top_level_arguments(&text[open + 1..close]) else {
            continue;
        };
        let arguments = arguments
            .into_iter()
            .filter(|argument| !argument.is_empty())
            .collect::<Vec<_>>();
        let prefix = masked[..offset].trim_end();
        if method {
            let Some(receiver_end) = prefix.strip_suffix('.').map(str::len) else {
                continue;
            };
            let receiver_start = path_start(&masked[..receiver_end]);
            // Masking keeps byte length but not character boundaries inside
            // strings and comments, so the original text is sliced checked.
            let Some(receiver) = text.get(receiver_start..receiver_end).map(str::trim) else {
                continue;
            };
            let preceding = masked[..receiver_start].trim_end();
            if receiver.is_empty() || preceding.ends_with(['.', ')', ']', '?']) {
                continue;
            }
            calls.push(OwnerCall {
                span: (receiver_start, close + 1),
                receiver: Some(receiver),
                arguments,
            });
        } else {
            if prefix.ends_with('.') {
                continue;
            }
            let start = if prefix.ends_with("::") {
                path_start(&masked[..prefix.len()])
            } else {
                offset
            };
            calls.push(OwnerCall {
                span: (start, close + 1),
                receiver: None,
                arguments,
            });
        }
    }
    calls
}

/// Where the ASCII path (`a::B`) that ends `text` starts: just past the last
/// character outside `[A-Za-z0-9_:]`, on a character boundary.
fn path_start(text: &str) -> usize {
    text.char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_' || *ch == ':'))
        .map_or(0, |(index, ch)| index + ch.len_utf8())
}

fn whole_word_offsets(text: &str, word: &str) -> Vec<usize> {
    if word.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    text.match_indices(word)
        .map(|(offset, _)| offset)
        .filter(|offset| {
            let before = offset.checked_sub(1).and_then(|index| bytes.get(index));
            let after = bytes.get(offset + word.len());
            !before.is_some_and(|byte| is_word(*byte)) && !after.is_some_and(|byte| is_word(*byte))
        })
        .collect()
}

/// The distinct line starts of the lines holding `offsets`, in order.
fn line_starts_of(text: &str, offsets: &[usize]) -> Vec<usize> {
    let mut starts = offsets
        .iter()
        .map(|offset| text[..*offset].rfind('\n').map_or(0, |index| index + 1))
        .collect::<Vec<_>>();
    starts.dedup();
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation, SymbolId};

    fn owner(body: &str, name: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.to_string(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn arm_probe(expression: &str, line: usize) -> Probe {
        Probe {
            id: ProbeId(format!("probe:src_lib_rs:{line}:match_arm")),
            location: SourceLocation::new("src/lib.rs", line, 1),
            owner: Some(SymbolId("src/lib.rs::reason".to_string())),
            family: ProbeFamily::MatchArm,
            delta: DeltaKind::Value,
            before: None,
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn test_with(body: &str) -> TestSummary {
        TestSummary {
            name: "t".to_string(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.to_string(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    const REASON: &str = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";

    fn reason_selector() -> Result<ArmSelector, String> {
        ArmSelector::establish(&arm_probe("None => 0,", 4), &owner(REASON, "reason"))
            .ok_or_else(|| "premise: the None arm reads its scrutinee from x".to_string())
    }

    #[test]
    fn sibling_variant_input_selects_other_arm() -> Result<(), String> {
        let selector = reason_selector()?;
        let observed = selector
            .observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n}\n",
            ))
            .ok_or_else(|| "premise: the owner call is readable".to_string())?;
        assert_eq!(observed.inputs, vec!["Some(5)".to_string()]);
        assert_eq!(observed.selection, ArmSelection::SelectsOther);
        Ok(())
    }

    #[test]
    fn matching_input_selects_and_any_selecting_call_wins() -> Result<(), String> {
        let selector = reason_selector()?;
        let observed = selector
            .observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n    assert_eq!(reason(None), 0);\n}\n",
            ))
            .ok_or_else(|| "premise: both owner calls are readable".to_string())?;
        assert_eq!(observed.selection, ArmSelection::Selects);
        assert!(selector.assertion_selects("assert_eq!(reason(None), 0);"));
        assert!(!selector.assertion_selects("assert_eq!(reason(Some(5)), 6);"));
        // A variant named only on the expected side is not an input.
        assert!(!selector.assertion_selects("assert_eq!(reason(Some(5)), None);"));
        Ok(())
    }

    #[test]
    fn opaque_or_unreadable_inputs_never_select_other() -> Result<(), String> {
        let selector = reason_selector()?;
        let variable = selector
            .observed_inputs(&test_with(
                "fn t() {\n    let x = Some(5);\n    assert_eq!(reason(x), 6);\n}\n",
            ))
            .ok_or_else(|| "premise: the call is readable".to_string())?;
        assert_eq!(variable.selection, ArmSelection::Unknown);
        // A function pointer may be called with any input.
        assert_eq!(
            selector.observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n    let _ = [None].map(reason);\n}\n",
            )),
            None
        );
        // A call split across lines is not read.
        assert_eq!(
            selector.observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(\n        Some(5)), 6);\n}\n",
            )),
            None
        );
        // No owner call at all: the test may reach the arm another way.
        assert_eq!(
            selector.observed_inputs(&test_with("fn t() {\n    assert_eq!(other(1), 2);\n}\n")),
            None
        );
        Ok(())
    }

    #[test]
    fn receiver_scrutinee_reads_glob_imported_variants() -> Result<(), String> {
        let body = "pub fn apply_to_variant(self, variant: &str) -> String {\n    match self {\n        None | PascalCase => variant.to_owned(),\n        LowerCase => str::to_ascii_lowercase(variant),\n        UpperCase => variant.to_ascii_uppercase(),\n    }\n}\n";
        let mut apply = owner(body, "apply_to_variant");
        apply.impl_context = crate::analysis::facts::FunctionImplContext::Impl {
            self_type: "RenameRule".to_string(),
        };
        let selector = ArmSelector::establish(
            &arm_probe("LowerCase => str::to_ascii_lowercase(variant),", 4),
            &apply,
        )
        .ok_or_else(|| "premise: the arm reads its scrutinee from self".to_string())?;
        assert!(
            selector.assertion_selects("assert_eq!(LowerCase.apply_to_variant(original), lower);")
        );
        assert!(selector.assertion_selects(
            "assert_eq!(RenameRule::LowerCase.apply_to_variant(original), lower);"
        ));
        assert!(
            !selector.assertion_selects("assert_eq!(UpperCase.apply_to_variant(original), upper);")
        );
        // A same-named variant of another enum is not this scrutinee's input.
        assert!(!selector.assertion_selects(
            "assert_eq!(OtherRule::LowerCase.apply_to_variant(original), lower);"
        ));
        // A computed receiver is not an input this module reads.
        assert!(
            !selector.assertion_selects("assert_eq!(rule().apply_to_variant(original), lower);")
        );
        Ok(())
    }

    #[test]
    fn integer_arms_compare_by_value() -> Result<(), String> {
        let body = "fn max_scalar_value(nbytes: usize) -> u32 {\n    match nbytes {\n        1 => 0x007F,\n        2 => 0x7FF,\n        3 => 0xFFFF,\n        _ => unreachable!(),\n    }\n}\n";
        let selector = ArmSelector::establish(
            &arm_probe("2 => 0x7FF,", 4),
            &owner(body, "max_scalar_value"),
        )
        .ok_or_else(|| "premise: the arm reads its scrutinee from nbytes".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(max_scalar_value(2), 0x07FF);"));
        assert!(selector.assertion_selects("assert_eq!(max_scalar_value(0x2usize), 2047);"));
        assert!(!selector.assertion_selects("assert_eq!(max_scalar_value(3), 0xFFFF);"));
        assert_eq!(
            selector
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(max_scalar_value(3), 0xFFFF);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        Ok(())
    }

    #[test]
    fn refutable_or_unprovable_patterns_stay_unknown() -> Result<(), String> {
        let body = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(0) => 9,\n        Some(v) => v + 1,\n        _ => 0,\n    }\n}\n";
        // `Some(0)` refutes on its payload: `Some(5)` is not provably selected.
        let some_zero =
            ArmSelector::establish(&arm_probe("Some(0) => 9,", 3), &owner(body, "reason"))
                .ok_or_else(|| "premise: the Some(0) arm is readable".to_string())?;
        assert!(!some_zero.assertion_selects("assert_eq!(reason(Some(0)), 9);"));
        // A wildcard arm can be selected by any input.
        let wildcard = ArmSelector::establish(&arm_probe("_ => 0,", 5), &owner(body, "reason"))
            .ok_or_else(|| "premise: the wildcard arm is readable".to_string())?;
        assert_eq!(
            wildcard
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::Unknown)
        );
        // A guarded arm never establishes selection.
        let guarded = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        None if flag() => 0,\n        _ => 1,\n    }\n}\n";
        assert!(
            ArmSelector::establish(
                &arm_probe("None if flag() => 0,", 3),
                &owner(guarded, "reason")
            )
            .is_none()
        );
        // A SCREAMING name may be a constant compared by value.
        let constant = "pub fn reason(x: u8) -> u8 {\n    match x {\n        LIMIT => 0,\n        _ => 1,\n    }\n}\n";
        let limit =
            ArmSelector::establish(&arm_probe("LIMIT => 0,", 3), &owner(constant, "reason"))
                .ok_or_else(|| "premise: the LIMIT arm is readable".to_string())?;
        assert_eq!(
            limit
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(reason(OTHER), 1);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::Unknown)
        );
        Ok(())
    }

    #[test]
    fn computed_or_rebound_scrutinee_establishes_nothing() {
        let computed = "pub fn reason(x: Option<i32>) -> i32 {\n    match x.map(|v| v * 2) {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 4), &owner(computed, "reason"))
                .is_none()
        );
        let rebound = "pub fn reason(x: Option<i32>) -> i32 {\n    let x = x.filter(|v| *v > 0);\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(rebound, "reason"))
                .is_none()
        );
        let if_let = "pub fn reason(x: Option<Option<i32>>) -> i32 {\n    if let Some(x) = x {\n        return match x {\n            Some(v) => v + 1,\n            None => 0,\n        };\n    }\n    1\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(if_let, "reason")).is_none()
        );
        // A plain read before the match is not a rebinding.
        let read = "pub fn reason(x: Option<i32>) -> i32 {\n    if x.is_some() {\n        debug_log();\n    }\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 7), &owner(read, "reason")).is_some()
        );
        // An arm of a nested match reads that match's scrutinee.
        let nested = "pub fn reason(x: Option<i32>, y: bool) -> i32 {\n    match y {\n        true => match x {\n            Some(v) => v,\n            None => 0,\n        },\n        false => 1,\n    }\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(nested, "reason"));
        assert_eq!(
            selector.map(|selector| selector.binding),
            Some(ScrutineeBinding::Parameter(0))
        );
    }

    #[test]
    fn a_match_the_call_may_skip_never_credits_the_arm() -> Result<(), String> {
        let skippable = [
            // Nested in another arm: `reason(None, false)` never enters it.
            (
                "pub fn reason(x: Option<i32>, y: bool) -> i32 {\n    match y {\n        true => match x {\n            Some(v) => v,\n            None => 0,\n        },\n        false => 7,\n    }\n}\n",
                5,
                "reason(None, false)",
            ),
            // After an early return that `None` takes.
            (
                "pub fn reason(x: Option<i32>) -> i32 {\n    if x.is_none() {\n        return 7;\n    }\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n",
                7,
                "reason(None)",
            ),
            // After a `?` that may return first.
            (
                "pub fn reason(x: Option<i32>) -> Option<i32> {\n    let y = check()?;\n    match x {\n        Some(v) => Some(v + y),\n        None => Some(0),\n    }\n}\n",
                5,
                "reason(None)",
            ),
        ];
        for (body, line, call) in skippable {
            let selector =
                ArmSelector::establish(&arm_probe("None => 0,", line), &owner(body, "reason"))
                    .ok_or_else(|| format!("premise: the None arm at line {line} is readable"))?;
            assert!(
                !selector.assertion_selects(&format!("assert_eq!({call}, 7);")),
                "a match the call may skip cannot credit its arm: {body}"
            );
            // Missing the arm's pattern still misses the arm.
            let observed = selector
                .observed_inputs(&test_with(&format!(
                    "fn t() {{\n    assert_eq!({}, 7);\n}}\n",
                    call.replace("None", "Some(1)")
                )))
                .ok_or_else(|| "premise: the call is readable".to_string())?;
            assert_eq!(observed.selection, ArmSelection::SelectsOther);
        }
        // Control: `let` statements with no control flow always run first.
        let straight = "pub fn reason(x: Option<i32>) -> i32 {\n    let base = 1;\n    let total = match x {\n        Some(v) => v + base,\n        None => 0,\n    };\n    total\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(straight, "reason"))
                .ok_or_else(|| "premise: the None arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(reason(None), 0);"));
        Ok(())
    }

    #[test]
    fn an_earlier_arm_that_may_match_blocks_selection() -> Result<(), String> {
        // An earlier guarded arm may take `2` first.
        let guarded = "fn width(n: u8) -> u8 {\n    match n {\n        v if v > 1 => {\n            v\n        }\n        2 => 20,\n        _ => 0,\n    }\n}\n";
        let selector = ArmSelector::establish(&arm_probe("2 => 20,", 6), &owner(guarded, "width"))
            .ok_or_else(|| "premise: the 2 arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(width(2), 20);"));
        // A non-matching input still provably misses the changed arm.
        assert_eq!(
            selector
                .observed_inputs(&test_with("fn t() {\n    assert_eq!(width(3), 3);\n}\n"))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        // Earlier arms that provably miss, including a block arm, let the
        // changed arm select.
        let plain = "fn width(n: u8) -> u8 {\n    match n {\n        1 => {\n            10\n        }\n        3 | 4 => 30,\n        2 => 20,\n        _ => 0,\n    }\n}\n";
        let selector = ArmSelector::establish(&arm_probe("2 => 20,", 7), &owner(plain, "width"))
            .ok_or_else(|| "premise: the 2 arm is readable".to_string())?;
        assert_eq!(selector.scrutinee(), "n");
        assert!(selector.assertion_selects("assert_eq!(width(2), 20);"));
        // The changed line must start an arm of the enclosing match.
        assert!(ArmSelector::establish(&arm_probe("10", 4), &owner(plain, "width")).is_none());
        Ok(())
    }

    #[test]
    fn non_ascii_receivers_and_mut_self_methods_are_read_safely() -> Result<(), String> {
        let body = "pub fn flip(&mut self, mode: Mode) -> u8 {\n    match mode {\n        Mode::Cold => 0,\n        Mode::Hot => 1,\n    }\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("Mode::Hot => 1,", 4), &owner(body, "flip"))
                .ok_or_else(|| "premise: the Hot arm reads its scrutinee from mode".to_string())?;
        // `&mut self` is not counted: `mode` is the call's first argument.
        assert_eq!(selector.binding, ScrutineeBinding::Parameter(0));
        assert!(selector.assertion_selects("assert_eq!(obj.flip(Mode::Hot), 1);"));
        // A non-ASCII receiver is read on character boundaries.
        assert!(selector.assertion_selects("assert_eq!(café.flip(Mode::Hot), 1);"));
        assert!(!selector.assertion_selects("assert_eq!(\"é\".flip(Mode::Cold), 0);"));
        assert_eq!(
            selector
                .observed_inputs(&test_with(
                    "fn t() {\n    let mut café = Thing;\n    assert_eq!(café.flip(Mode::Cold), 0);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        Ok(())
    }

    /// Review findings on #5638: each shape below once read as `Selects`
    /// for an input Rust routes to a different arm.
    #[test]
    fn review_false_credit_shapes_stay_unjudged() -> Result<(), String> {
        // `true`/`false` in a payload refute; they are not bindings.
        let bools = "fn f(x: Option<bool>) -> u8 {\n    match x {\n        Some(true) => 1,\n        Some(false) => 2,\n        None => 0,\n    }\n}\n";
        let some_true =
            ArmSelector::establish(&arm_probe("Some(true) => 1,", 3), &owner(bools, "f"))
                .ok_or_else(|| "premise: the Some(true) arm is readable".to_string())?;
        assert!(!some_true.assertion_selects("assert_eq!(f(Some(false)), 2);"));
        // A comma-less block-like body must not swallow the guard arm after it.
        let swallowed = "fn f(n: u8, flag: bool) -> u8 {\n    match n {\n        0 => match flag { true => 1, false => 2 }\n        x if x > 5 => 9,\n        7 => 70,\n        _ => 0,\n    }\n}\n";
        let seven = ArmSelector::establish(&arm_probe("7 => 70,", 5), &owner(swallowed, "f"));
        assert!(
            seven.is_none_or(|selector| !selector.assertion_selects("assert_eq!(f(7, true), 9);"))
        );
        // A `for` loop, a field-rename destructure, or a mutating method
        // rebinds or changes the parameter.
        for body in [
            "fn f(x: Option<u8>, xs: Vec<Option<u8>>) -> u8 {\n    for x in xs {\n        match x {\n            Some(v) => v,\n            None => 100,\n        };\n    }\n    1\n}\n",
            "fn f(x: Option<u8>, w: W) -> u8 {\n    let W { inner: x } = w;\n    match x {\n        Some(v) => v,\n        None => 100,\n    }\n}\n",
            "fn f(mut x: Option<u8>) -> u8 {\n    let _ = x.take();\n    match x {\n        Some(v) => v,\n        None => 100,\n    }\n}\n",
        ] {
            let line = body
                .lines()
                .position(|line| line.contains("None => 100"))
                .unwrap_or(0)
                + 1;
            assert!(
                ArmSelector::establish(&arm_probe("None => 100,", line), &owner(body, "f"))
                    .is_none(),
                "{body}"
            );
        }
        // A loop that reassigns the scrutinee after the match.
        let looped = "fn run(mut s: State) -> u8 {\n    loop {\n        match s {\n            Start => return 1,\n            Idle => s = Start,\n        }\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("Start => return 1,", 4), &owner(looped, "run"))
                .is_none()
        );
        // A `&mut self` method that assigns `*self` before the match.
        let reassigned = "fn f(&mut self) -> u8 {\n    *self = Mode::Off;\n    match self {\n        On => 1,\n        Off => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("On => 1,", 4), &owner(reassigned, "f")).is_none()
        );
        // Two arms on one line: which one changed is not known.
        assert!(ArmSelector::establish(&arm_probe("None => 0, Some(v) => v + 1,", 3), &owner("fn f(x: Option<u8>) -> u8 {\n    match x {\n        None => 0, Some(v) => v + 1,\n    }\n}\n", "f")).is_none());
        // Another enum's variant with the same name is not this one's.
        let typed = "fn f(level: Level) -> u8 {\n    match level {\n        Level::Low => 1,\n        Level::High => 2,\n    }\n}\n";
        let low = ArmSelector::establish(&arm_probe("Level::Low => 1,", 3), &owner(typed, "f"))
            .ok_or_else(|| "premise: the Low arm is readable".to_string())?;
        assert!(low.assertion_selects("assert_eq!(f(Level::Low), 1);"));
        assert!(!low.assertion_selects("assert_eq!(f(Priority::Low), 1);"));
        Ok(())
    }
}
