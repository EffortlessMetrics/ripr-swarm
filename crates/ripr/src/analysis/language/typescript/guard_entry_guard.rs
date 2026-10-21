//! A changed line inside an `if` arm that no related test call enters (#8569).
//!
//! `if (coupon === "HALF") { total = total / 3; }` runs its body only when
//! the call's `coupon` is `"HALF"`. When every related test calls the owner
//! with literal inputs that provably leave the guard untaken, the changed
//! line never executes under the suite, so a strong oracle elsewhere in the
//! test passes on both versions: reach plus a strong oracle does not
//! discriminate the change. This is the TypeScript counterpart of the Rust
//! guard-entry rule (RIPR-SPEC-0263) and the Python guarded-arm gate.
//!
//! The rule only withdraws credit, and only on proof. A condition it cannot
//! read (member access, calls, a rebound or defaulted parameter, a computed
//! or non-literal argument) is `None`, which never excludes a call, so the
//! existing verdict stands. This module reads structure and evaluates
//! literals; which calls a test makes is the classifier's glue.

use super::{ts_mask_literals_and_comments, ts_parameter_is_stable};

/// One condition an enclosing block needs (`want == true`) or needs to be
/// false (`want == false`, an `else` arm) for the changed line to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuardCondition {
    pub(crate) text: String,
    pub(crate) want: bool,
}

impl GuardCondition {
    /// The condition as a reader would write the arm's guard.
    pub(crate) fn display(&self) -> String {
        if self.want {
            self.text.clone()
        } else {
            format!("!({})", self.text)
        }
    }
}

/// A literal value a test passes or a guard compares against.
#[derive(Clone, Debug, PartialEq)]
enum Lit {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Undef,
}

impl Lit {
    fn truthy(&self) -> bool {
        match self {
            Lit::Num(n) => *n != 0.0,
            Lit::Str(s) => !s.is_empty(),
            Lit::Bool(b) => *b,
            Lit::Null | Lit::Undef => false,
        }
    }

    fn nullish(&self) -> bool {
        matches!(self, Lit::Null | Lit::Undef)
    }

    fn same_kind(&self, other: &Lit) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// The `if` / `else if` / `else` conditions enclosing the changed line, from
/// the owner's own source, or `None` when the line is not exactly located or
/// sits under no readable conditional arm.
pub(crate) fn enclosing_conditions(
    owner_source: &str,
    start_line: usize,
    changed_line: usize,
    line_text: &str,
) -> Option<Vec<GuardCondition>> {
    let wanted = line_text.trim();
    // A line that opens with `}` closes a block the scan would count as
    // still open, and CRLF text shifts the byte offsets the scan relies on.
    if wanted.is_empty() || wanted.starts_with('}') || owner_source.contains('\r') {
        return None;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    // Located by position only: a deleted line's text is gone from the new
    // side, and another identical line elsewhere is not the changed one.
    let index = changed_line
        .checked_sub(start_line)
        .filter(|at| lines.get(*at).is_some_and(|line| line.trim() == wanted))?;
    let line_start: usize = lines[..index].iter().map(|line| line.len() + 1).sum();

    let masked = ts_mask_literals_and_comments(owner_source);
    let original: Vec<char> = owner_source.chars().collect();
    let masked_chars: Vec<char> = masked.chars().collect();
    if original.len() != masked_chars.len() {
        return None;
    }
    // Character offset of the changed line's start. Lines were split on
    // `\n` (a `\r` stays in the line), so byte math matches char math only
    // for ASCII; any other text is refused.
    if !owner_source.is_ascii() {
        return None;
    }
    let target = line_start.min(original.len());

    struct Block {
        conditions: Vec<GuardCondition>,
    }
    let mut stack: Vec<Block> = Vec::new();
    // Conditions a following `else` arm inherits from the block just closed.
    let mut chain: Option<Vec<GuardCondition>> = None;
    let mut closing_conditions: Vec<Vec<GuardCondition>> = Vec::new();
    let mut segment_start = 0usize;
    let mut parens = 0usize;
    for (at, c) in masked_chars.iter().enumerate().take(target) {
        match c {
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            ';' if parens == 0 => {
                chain = None;
                segment_start = at + 1;
            }
            '{' if parens == 0 => {
                let header: String = masked_chars[segment_start..at].iter().collect();
                let header_original: String = original[segment_start..at].iter().collect();
                let (conditions, next_chain) =
                    block_conditions(&header, &header_original, chain.take().unwrap_or_default());
                closing_conditions.push(next_chain);
                stack.push(Block { conditions });
                segment_start = at + 1;
            }
            '}' if parens == 0 => {
                stack.pop()?;
                chain = closing_conditions.pop().filter(|next| !next.is_empty());
                segment_start = at + 1;
            }
            _ => {}
        }
    }
    // A segment still open at the line start means the line continues a
    // statement or sits in an unbalanced scan; refuse rather than guess.
    if parens != 0 || stack.is_empty() {
        return None;
    }
    let conditions: Vec<GuardCondition> = stack
        .into_iter()
        .flat_map(|block| block.conditions)
        .collect();
    (!conditions.is_empty()).then_some(conditions)
}

/// The conditions a block with `header` needs, and the chain a directly
/// following `else` inherits. `masked` and `original` are the same span
/// (equal length, ASCII), so offsets read in one index the other.
fn block_conditions(
    masked: &str,
    original: &str,
    inherited: Vec<GuardCondition>,
) -> (Vec<GuardCondition>, Vec<GuardCondition>) {
    let none = (Vec::new(), Vec::new());
    let lead = masked.len() - masked.trim_start().len();
    let body_masked = masked[lead..].trim_end();
    let body_original = &original[lead..lead + body_masked.len()];
    // Drop `keyword` and following whitespace from both spans.
    let mut m = body_masked;
    let mut o = body_original;
    let mut is_else = false;
    if let Some(rest) = m.strip_prefix("else")
        && !rest.starts_with(is_identifier_char)
    {
        is_else = true;
        let cut = m.len() - rest.trim_start().len();
        m = &m[cut..];
        o = &o[cut..];
        if m.is_empty() {
            return (inherited, Vec::new());
        }
    }
    let Some(after) = m.strip_prefix("if") else {
        return none;
    };
    if after.starts_with(is_identifier_char) {
        return none;
    }
    let cut = m.len() - after.trim_start().len();
    let m = &m[cut..];
    let o = &o[cut..];
    if !m.starts_with('(') {
        return none;
    }
    let mut depth = 0usize;
    let mut close = None;
    for (at, c) in m.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close.filter(|close| m[close + 1..].trim().is_empty()) else {
        return none;
    };
    let condition = o[1..close].trim().to_string();
    if condition.is_empty() {
        return none;
    }
    let mut needs = if is_else { inherited } else { Vec::new() };
    let mut next = needs.clone();
    needs.push(GuardCondition {
        text: condition.clone(),
        want: true,
    });
    next.push(GuardCondition {
        text: condition,
        want: false,
    });
    (needs, next)
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// What the owner's parameter list shows: per parameter, whether it has a
/// default. `None` when the list cannot be read against `params`.
pub(crate) fn parameter_defaults(owner_source: &str, params: &[String]) -> Option<Vec<bool>> {
    let masked = ts_mask_literals_and_comments(owner_source);
    let open = masked.find('(')?;
    let mut depth = 0usize;
    let mut end = None;
    for (at, c) in masked[open..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + at);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let list = &masked[open + 1..end];
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (at, c) in list.char_indices() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    if !list[start..].trim().is_empty() {
        parts.push(&list[start..]);
    }
    if parts.len() != params.len() {
        return None;
    }
    let mut defaults = Vec::new();
    for (part, name) in parts.iter().zip(params) {
        let part = part.trim();
        let ident: String = part
            .chars()
            .take_while(|c| is_identifier_char(*c))
            .collect();
        if ident != *name {
            return None;
        }
        // A default `= v` after the optional annotation; `=>` inside a type
        // is not one.
        let has_default = part.match_indices('=').any(|(at, _)| {
            let after = &part[at + 1..];
            !after.starts_with(['>', '=']) && !part[..at].ends_with(['=', '!'])
        });
        defaults.push(has_default);
    }
    Some(defaults)
}

/// One owner call's argument bindings, read from literal arguments.
pub(crate) struct CallInputs<'a> {
    pub(crate) owner_source: &'a str,
    pub(crate) params: &'a [String],
    pub(crate) defaults: &'a [bool],
    pub(crate) arguments: &'a [String],
}

impl CallInputs<'_> {
    fn lookup(&self, name: &str) -> Option<Lit> {
        let index = self.params.iter().position(|param| param == name)?;
        if !ts_parameter_is_stable(self.owner_source, name) {
            return None;
        }
        match self.arguments.get(index) {
            // An explicit `undefined` applies a parameter's default.
            Some(argument) => parse_literal(argument).filter(|literal| {
                *literal != Lit::Undef || !self.defaults.get(index).copied().unwrap_or(true)
            }),
            None if !self.defaults.get(index).copied().unwrap_or(true) => Some(Lit::Undef),
            None => None,
        }
    }

    /// `Some(false)` when `condition` is provably false for this call and
    /// `Some(true)` when provably true; `None` when unreadable.
    fn evaluate(&self, condition: &str) -> Option<bool> {
        self.evaluate_at(condition, 0)
    }

    fn evaluate_at(&self, condition: &str, depth: usize) -> Option<bool> {
        if depth > MAX_DEPTH {
            return None;
        }
        let condition = condition.trim();
        // A ternary (or optional chain) changes precedence.
        if condition.contains('?') {
            return None;
        }
        if let Some(parts) = split_top_level(condition, "||") {
            return or_of(parts.iter().map(|part| self.evaluate_at(part, depth + 1)));
        }
        if let Some(parts) = split_top_level(condition, "&&") {
            return and_of(parts.iter().map(|part| self.evaluate_at(part, depth + 1)));
        }
        if let Some(inner) = strip_outer_parens(condition) {
            return self.evaluate_at(inner, depth + 1);
        }
        if let Some(rest) = condition.strip_prefix('!')
            && !rest.starts_with('=')
        {
            // `!x === y` negates `x` first, not the comparison.
            if has_top_level_comparison(rest) {
                return None;
            }
            return self.evaluate_at(rest, depth + 1).map(|value| !value);
        }
        for operator in ["===", "!==", "==", "!=", "<=", ">=", "<", ">"] {
            if let Some((left, right)) = split_once_top_level(condition, operator) {
                let left = self.operand(left, 0)?;
                let right = self.operand(right, 0)?;
                return compare(&left, operator, &right);
            }
        }
        self.operand(condition, 0).map(|value| value.truthy())
    }

    fn operand(&self, text: &str, depth: usize) -> Option<Lit> {
        if depth > MAX_DEPTH {
            return None;
        }
        let text = text.trim();
        if let Some(inner) = strip_outer_parens(text) {
            return self.operand(inner, depth + 1);
        }
        if let Some(literal) = parse_literal(text) {
            return Some(literal);
        }
        if !text.is_empty()
            && text.chars().all(is_identifier_char)
            && !text.starts_with(|c: char| c.is_ascii_digit())
        {
            return self.lookup(text);
        }
        None
    }
}

/// Whether `inputs` provably keep one arm closed: some needed condition is
/// provably the opposite of what the arm needs.
pub(crate) fn call_excludes_arm(inputs: &CallInputs<'_>, conditions: &[GuardCondition]) -> bool {
    // A spread argument shifts every later position.
    if inputs
        .arguments
        .iter()
        .any(|argument| argument.trim_start().starts_with("..."))
    {
        return false;
    }
    conditions
        .iter()
        .any(|condition| inputs.evaluate(&condition.text) == Some(!condition.want))
}

fn has_top_level_comparison(text: &str) -> bool {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' | '<' | '>' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// Whether every call-shaped identifier in the (masked) test text is one of
/// `allowed`, and every member call hangs off an owner namespace receiver,
/// a test-framework object, or an already-checked call result. Any other
/// call could be a file-local helper that reaches the owner with an input
/// the test text does not show.
pub(crate) fn calls_only_allowed(test_text: &str, allowed: &[&str], receivers: &[String]) -> bool {
    let masked = ts_mask_literals_and_comments(test_text);
    let chars: Vec<char> = masked.chars().collect();
    let mut at = 0usize;
    while at < chars.len() {
        if is_identifier_char(chars[at]) && (at == 0 || !is_identifier_char(chars[at - 1])) {
            let mut end = at;
            while end < chars.len() && is_identifier_char(chars[end]) {
                end += 1;
            }
            let word: String = chars[at..end].iter().collect();
            let mut next = end;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            let calls = chars.get(next) == Some(&'(');
            let before = chars[..at].iter().rev().find(|c| !c.is_whitespace());
            let is_member = before == Some(&'.');
            if calls && is_member && !member_receiver_is_readable(&chars, at, receivers) {
                return false;
            }
            let keyword = matches!(
                word.as_str(),
                "if" | "for"
                    | "while"
                    | "switch"
                    | "catch"
                    | "function"
                    | "return"
                    | "await"
                    | "async"
                    | "typeof"
                    | "void"
                    | "new"
                    | "of"
                    | "in"
            );
            if calls && !is_member && !keyword && !allowed.contains(&word.as_str()) {
                return false;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    true
}

/// Harmless `jest` / `vi` members: they build or configure mocks and timers
/// but never load a module or run its code (`requireActual`, `requireMock`
/// and `createMockFromModule` do).
/// Deepest nesting the condition reader follows before it gives up.
const MAX_DEPTH: usize = 32;

const HARMLESS_MOCK_MEMBERS: [&str; 16] = [
    "fn",
    "spyOn",
    "mock",
    "unmock",
    "doMock",
    "useFakeTimers",
    "useRealTimers",
    "setSystemTime",
    "advanceTimersByTime",
    "runAllTimers",
    "clearAllMocks",
    "resetAllMocks",
    "restoreAllMocks",
    "clearAllTimers",
    "isMockFunction",
    "mocked",
];

/// Longest backward scan over a call's arguments before the reader gives up.
const MAX_RECEIVER_SCAN: usize = 4000;

/// Whether the member call whose name starts at `at` hangs off a receiver the
/// reader can account for: an owner namespace (no call hop), a test-runner
/// object, `expect(..)` / `assert(..)` and their chains, or a harmless
/// `jest` / `vi` member. A chain off any other call result, an index or a
/// parenthesized expression is unreadable.
fn member_receiver_is_readable(chars: &[char], at: usize, receivers: &[String]) -> bool {
    let mut end = at;
    let mut via_call = false;
    // Member names from the call back toward the root (`jest.fn().x` has
    // `x`, then `fn`).
    let mut members: Vec<String> = Vec::new();
    {
        let mut k = at;
        while k < chars.len() && is_identifier_char(chars[k]) {
            k += 1;
        }
        members.push(chars[at..k].iter().collect());
    }
    loop {
        // `end` is just after a `.`; step to the receiver's last token.
        let mut i = end;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i == 0 || chars[i - 1] != '.' {
            return false;
        }
        i -= 1;
        // Optional chaining `?.`.
        if i > 0 && chars[i - 1] == '?' {
            i -= 1;
        }
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i == 0 {
            return false;
        }
        if chars[i - 1] == ']' {
            return false;
        }
        if chars[i - 1] == ')' {
            via_call = true;
            // Step to the call's callee: `expect(x).toBe(` roots at `expect`.
            let mut depth = 0usize;
            let mut open = None;
            for (scanned, k) in (0..i).rev().enumerate() {
                if scanned > MAX_RECEIVER_SCAN {
                    return false;
                }
                match chars[k] {
                    ')' => depth += 1,
                    '(' => {
                        depth -= 1;
                        if depth == 0 {
                            open = Some(k);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(open) = open else {
                return false;
            };
            let mut k = open;
            while k > 0 && chars[k - 1].is_whitespace() {
                k -= 1;
            }
            i = k;
        }
        let stop = i;
        while i > 0 && is_identifier_char(chars[i - 1]) {
            i -= 1;
        }
        if i == stop {
            return false;
        }
        let word: String = chars[i..stop].iter().collect();
        let mut j = i;
        while j > 0 && chars[j - 1].is_whitespace() {
            j -= 1;
        }
        if j > 0 && chars[j - 1] == '.' {
            members.push(word);
            end = i;
            continue;
        }
        return match word.as_str() {
            "expect" | "assert" => true,
            "test" | "it" | "describe" => !via_call,
            "jest" | "vi" => members
                .last()
                .is_some_and(|member| HARMLESS_MOCK_MEMBERS.contains(&member.as_str())),
            _ => !via_call && receivers.contains(&word),
        };
    }
}

fn or_of(values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut all_false = true;
    for value in values {
        match value {
            Some(true) => return Some(true),
            Some(false) => {}
            None => all_false = false,
        }
    }
    all_false.then_some(false)
}

fn and_of(values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut all_true = true;
    for value in values {
        match value {
            Some(false) => return Some(false),
            Some(true) => {}
            None => all_true = false,
        }
    }
    all_true.then_some(true)
}

fn compare(left: &Lit, operator: &str, right: &Lit) -> Option<bool> {
    match operator {
        "===" | "!==" => {
            let equal = left.same_kind(right) && left == right;
            Some(equal == (operator == "==="))
        }
        "==" | "!=" => {
            let equal = if left.same_kind(right) {
                left == right
            } else if left.nullish() && right.nullish() {
                true
            } else if left.nullish() || right.nullish() {
                false
            } else {
                // Loose coercion between number, string and boolean.
                return None;
            };
            Some(equal == (operator == "=="))
        }
        _ => match (left, right) {
            (Lit::Num(a), Lit::Num(b)) => Some(match operator {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            }),
            _ => None,
        },
    }
}

/// A literal the way a test writes it, or `None` for anything computed.
fn parse_literal(text: &str) -> Option<Lit> {
    let text = text.trim();
    match text {
        "true" => return Some(Lit::Bool(true)),
        "false" => return Some(Lit::Bool(false)),
        "null" => return Some(Lit::Null),
        "undefined" => return Some(Lit::Undef),
        _ => {}
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if first == last && matches!(first, b'\'' | b'"' | b'`') {
            let inner = &text[1..text.len() - 1];
            let interpolates = first == b'`' && inner.contains("${");
            let plain = !(inner.contains('\\') || inner.contains(first as char) || interpolates);
            return plain.then(|| Lit::Str(inner.to_string()));
        }
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    // Legacy octal (`010`) is not decimal.
    if digits.len() > 1 && digits.starts_with('0') && !digits.starts_with("0.") {
        return None;
    }
    let well_formed = !digits.is_empty()
        && digits.chars().next().is_some_and(|c| c.is_ascii_digit())
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() <= 1
        && !digits.ends_with('.');
    if well_formed {
        return text.parse::<f64>().ok().map(Lit::Num);
    }
    None
}

fn strip_outer_parens(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0usize;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    (depth == 0).then_some(inner)
}

/// Split at every top-level `operator`, or `None` when it does not occur.
fn split_top_level<'a>(text: &'a str, operator: &str) -> Option<Vec<&'a str>> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    let mut quote: Option<char> = None;
    let mut at = 0usize;
    while at < text.len() {
        let rest = &text[at..];
        let c = rest.chars().next()?;
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
        } else if matches!(c, '\'' | '"' | '`') {
            quote = Some(c);
        } else if matches!(c, '(' | '[' | '{') {
            depth += 1;
        } else if matches!(c, ')' | ']' | '}') {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && rest.starts_with(operator) {
            parts.push(&text[start..at]);
            start = at + operator.len();
            at = start;
            continue;
        }
        at += c.len_utf8();
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(&text[start..]);
    Some(parts)
}

/// Split at the single top-level comparison `operator`, refusing text that
/// holds a longer operator containing it or a second comparison.
fn split_once_top_level<'a>(text: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut found = None;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (at, c) in text.char_indices() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && text[at..].starts_with(operator) => {
                let before = text[..at].chars().next_back();
                let after = text[at + operator.len()..].chars().next();
                // `==` inside `===`, `<` inside `<=`, and so on.
                if matches!(before, Some('=' | '!' | '<' | '>'))
                    || matches!(after, Some('='))
                    || (operator.len() == 1 && matches!(after, Some('<' | '>')))
                {
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                found = Some(at);
            }
            _ => {}
        }
    }
    let at = found?;
    Some((&text[..at], &text[at + operator.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "export function price(qty: number, coupon?: string) {\n  let total = qty * 10;\n  if (coupon === 'HALF') {\n    total = total / 3;\n  } else if (qty > 100) {\n    total = 0;\n  } else {\n    total += 1;\n  }\n  return total;\n}\n";

    fn at(line: usize, text: &str) -> Option<Vec<GuardCondition>> {
        enclosing_conditions(SOURCE, 1, line, text)
    }

    #[test]
    fn reads_then_else_if_and_else_arms() {
        let then = at(4, "total = total / 3;")
            .map(|c| c.iter().map(GuardCondition::display).collect::<Vec<_>>());
        assert_eq!(then, Some(vec!["coupon === 'HALF'".to_string()]));
        let else_if =
            at(6, "total = 0;").map(|c| c.iter().map(GuardCondition::display).collect::<Vec<_>>());
        assert_eq!(
            else_if,
            Some(vec![
                "!(coupon === 'HALF')".to_string(),
                "qty > 100".to_string()
            ])
        );
        let else_arm =
            at(8, "total += 1;").map(|c| c.iter().map(GuardCondition::display).collect::<Vec<_>>());
        assert_eq!(
            else_arm,
            Some(vec![
                "!(coupon === 'HALF')".to_string(),
                "!(qty > 100)".to_string()
            ])
        );
        // Unguarded lines have no conditions.
        assert_eq!(at(2, "let total = qty * 10;"), None);
        assert_eq!(at(10, "return total;"), None);
    }

    fn inputs<'a>(
        arguments: &'a [String],
        params: &'a [String],
        defaults: &'a [bool],
    ) -> CallInputs<'a> {
        CallInputs {
            owner_source: SOURCE,
            params,
            defaults,
            arguments,
        }
    }

    #[test]
    fn evaluates_literal_calls_against_guards() {
        let params = vec!["qty".to_string(), "coupon".to_string()];
        let defaults = parameter_defaults(SOURCE, &params).unwrap_or_default();
        assert_eq!(defaults, vec![false, false]);
        let then = at(4, "total = total / 3;").unwrap_or_default();
        let missing = vec!["2".to_string()];
        assert!(call_excludes_arm(
            &inputs(&missing, &params, &defaults),
            &then
        ));
        let wrong = vec!["2".to_string(), "'NONE'".to_string()];
        assert!(call_excludes_arm(
            &inputs(&wrong, &params, &defaults),
            &then
        ));
        let entering = vec!["2".to_string(), "'HALF'".to_string()];
        assert!(!call_excludes_arm(
            &inputs(&entering, &params, &defaults),
            &then
        ));
        let computed = vec!["2".to_string(), "pick()".to_string()];
        assert!(!call_excludes_arm(
            &inputs(&computed, &params, &defaults),
            &then
        ));
        // The else arm is excluded by the entering call and not the other.
        let else_arm = at(8, "total += 1;").unwrap_or_default();
        assert!(call_excludes_arm(
            &inputs(&entering, &params, &defaults),
            &else_arm
        ));
        assert!(!call_excludes_arm(
            &inputs(&wrong, &params, &defaults),
            &else_arm
        ));
    }

    #[test]
    fn a_defaulted_parameter_is_not_assumed_undefined() {
        let source = "function f(a: number, b = 'X') {\n  if (b === 'Y') {\n    a = 1;\n  }\n  return a;\n}\n";
        let params = vec!["a".to_string(), "b".to_string()];
        let defaults = parameter_defaults(source, &params).unwrap_or_default();
        assert_eq!(defaults, vec![false, true]);
        let arms = enclosing_conditions(source, 1, 3, "a = 1;").unwrap_or_default();
        let call = vec!["1".to_string()];
        let inputs = CallInputs {
            owner_source: source,
            params: &params,
            defaults: &defaults,
            arguments: &call,
        };
        assert!(!call_excludes_arm(&inputs, &arms));
    }

    #[test]
    fn loose_coercion_and_unreadable_syntax_never_exclude() {
        let params = vec!["x".to_string()];
        let defaults = vec![false];
        for (guard, argument) in [
            ("x == 1", "'1'"),
            ("x.length > 2", "'a'"),
            ("check(x)", "1"),
            ("x === other", "1"),
            ("x > 'a'", "1"),
            ("x === 1 || x.y", "2"),
        ] {
            let source = "function f(x) {\n  return x;\n}\n";
            let arguments = vec![argument.to_string()];
            let call = CallInputs {
                owner_source: source,
                params: &params,
                defaults: &defaults,
                arguments: &arguments,
            };
            let arms = vec![GuardCondition {
                text: guard.to_string(),
                want: true,
            }];
            assert!(!call_excludes_arm(&call, &arms), "{guard} with {argument}");
        }
        // Definite exclusions.
        for (guard, argument) in [
            ("x === 1", "2"),
            ("x > 5", "3"),
            ("!x", "1"),
            ("x === 'a' && y", "'b'"),
        ] {
            let source = "function f(x) {\n  return x;\n}\n";
            let arguments = vec![argument.to_string()];
            let call = CallInputs {
                owner_source: source,
                params: &params,
                defaults: &defaults,
                arguments: &arguments,
            };
            let arms = vec![GuardCondition {
                text: guard.to_string(),
                want: true,
            }];
            assert!(call_excludes_arm(&call, &arms), "{guard} with {argument}");
        }
    }

    #[test]
    fn spread_undefined_default_precedence_and_crlf_never_exclude() {
        let params = vec!["qty".to_string(), "coupon".to_string()];
        let defaults = parameter_defaults(SOURCE, &params).unwrap_or_default();
        let then = at(4, "total = total / 3;").unwrap_or_default();
        // A spread shifts positions: `price(...a)` may carry the coupon.
        let spread = vec!["...a".to_string()];
        assert!(!call_excludes_arm(
            &inputs(&spread, &params, &defaults),
            &then
        ));
        // An explicit `undefined` applies a default.
        let source = "function price(qty, coupon = 'HALF') {\n  if (coupon === 'HALF') {\n    qty = 1;\n  }\n}\n";
        let defaulted = parameter_defaults(source, &params).unwrap_or_default();
        assert_eq!(defaulted, vec![false, true]);
        let arms = enclosing_conditions(source, 1, 3, "qty = 1;").unwrap_or_default();
        let explicit = vec!["2".to_string(), "undefined".to_string()];
        let call = CallInputs {
            owner_source: source,
            params: &params,
            defaults: &defaulted,
            arguments: &explicit,
        };
        assert!(!call_excludes_arm(&call, &arms));
        // Precedence and ternaries are unreadable.
        for guard in [
            "!x === false",
            "x === 1 && y ? 1 : 2",
            "x === 1 ? true : false",
        ] {
            let arguments = vec!["2".to_string()];
            let p = vec!["x".to_string()];
            let d = vec![false];
            let call = CallInputs {
                owner_source: "function f(x) {\n  return x;\n}\n",
                params: &p,
                defaults: &d,
                arguments: &arguments,
            };
            assert!(
                !call_excludes_arm(
                    &call,
                    &[GuardCondition {
                        text: guard.to_string(),
                        want: true
                    }]
                ),
                "{guard}"
            );
        }
        // CRLF text and a line that opens with `}` are refused.
        let crlf = SOURCE.replace('\n', "\r\n");
        assert_eq!(
            enclosing_conditions(&crlf, 1, 4, "total = total / 3;"),
            None
        );
        assert_eq!(
            enclosing_conditions(SOURCE, 1, 5, "} else if (qty > 100) {"),
            None
        );
        // `Array<string>= []` is a default.
        let generic = vec!["a".to_string()];
        assert_eq!(
            parameter_defaults("function f(a: Array<string>= []) {}", &generic),
            Some(vec![true])
        );
    }

    #[test]
    fn braces_and_ifs_in_strings_and_comments_do_not_open_blocks() {
        let source = "function f(x: number) {\n  const s = \"if (x === 1) {\"; // }\n  /* if (x === 2) { */\n  total = 1;\n  if (x === 3) {\n    total = 2;\n  }\n}\n";
        assert_eq!(enclosing_conditions(source, 1, 4, "total = 1;"), None);
        let arms = enclosing_conditions(source, 1, 6, "total = 2;").unwrap_or_default();
        assert_eq!(arms.len(), 1);
        assert_eq!(arms[0].text, "x === 3");
    }

    #[test]
    fn only_framework_and_owner_calls_are_readable() {
        let allowed = ["price", "expect"];
        let ns = vec!["pricing".to_string()];
        assert!(calls_only_allowed(
            "expect(price(2)).not.toBe(20); await price(3, 'A'); pricing.price(1);",
            &allowed,
            &ns
        ));
        for refused in [
            "expect(price(2)).toBe(20); expect(check('HALF')).toBe(1);",
            "run(price);",
            "expect(price(2)).toBe(20); h.check('HALF');",
            "cart.checkout('HALF');",
            "a.b.checkout('HALF');",
        ] {
            assert!(!calls_only_allowed(refused, &allowed, &ns), "{refused}");
        }
    }

    #[test]
    fn comparison_boundaries_precedence_nullish_and_octal() {
        let params = vec!["a".to_string()];
        let defaults = vec![false];
        let src = "function f(a) {\n  return a;\n}\n";
        let excludes = |guard: &str, argument: &str| {
            let arguments = vec![argument.to_string()];
            let call = CallInputs {
                owner_source: src,
                params: &params,
                defaults: &defaults,
                arguments: &arguments,
            };
            call_excludes_arm(
                &call,
                &[GuardCondition {
                    text: guard.to_string(),
                    want: true,
                }],
            )
        };
        assert!(!excludes("a >= 5", "5"));
        assert!(excludes("a > 5", "5"));
        assert!(!excludes("a <= 5", "5"));
        assert!(excludes("a < 5", "5"));
        // `&&` binds tighter than `||`: `a===1 || (a===2 && a===3)` holds for 1.
        assert!(!excludes("a === 1 || a === 2 && a === 3", "1"));
        assert!(excludes("a === 1 || a === 2 && a === 3", "2"));
        // `??` is unreadable.
        assert!(!excludes("a ?? 1", "undefined"));
        assert!(!excludes("a === 10", "010"));
    }

    #[test]
    fn indexed_and_parenthesized_receivers_are_not_readable() {
        let allowed = ["price", "expect"];
        let ns: Vec<String> = Vec::new();
        for refused in [
            "helpers[0].run('HALF');",
            "(cart).checkout('HALF');",
            "other(1).run('HALF');",
        ] {
            assert!(!calls_only_allowed(refused, &allowed, &ns), "{refused}");
        }
        assert!(calls_only_allowed(
            "expect(price(2)).resolves.toBe(1);",
            &allowed,
            &ns
        ));
    }

    #[test]
    fn comments_before_if_do_not_shift_the_condition_text() {
        let cond = |source: &str, line: usize, text: &str| {
            enclosing_conditions(source, 1, line, text)
                .map(|c| c.iter().map(GuardCondition::display).collect::<Vec<_>>())
        };
        for lead in [
            "// 0",
            "// note: 0",
            "// legacy note",
            "/* 0 */",
            "/* a\n b */",
        ] {
            let source = format!(
                "function f(x) {{\n  {lead}\n  if (x === 'HALF') {{\n    x = 2;\n  }}\n}}\n"
            );
            let line = source
                .lines()
                .position(|l| l.trim() == "x = 2;")
                .map_or(0, |p| p + 1);
            assert_eq!(
                cond(&source, line, "x = 2;"),
                Some(vec!["x === 'HALF'".to_string()]),
                "{lead}"
            );
            let chain = format!(
                "function f(x) {{\n  if (x === 1) {{\n    x = 5;\n  }} {lead}\n  else if (x === 'B') {{\n    x = 2;\n  }}\n}}\n"
            );
            let line = chain
                .lines()
                .position(|l| l.trim() == "x = 2;")
                .map_or(0, |p| p + 1);
            assert_eq!(
                cond(&chain, line, "x = 2;"),
                Some(vec!["!(x === 1)".to_string(), "x === 'B'".to_string()]),
                "{lead}"
            );
        }
    }

    #[test]
    fn mock_module_loaders_and_call_result_chains_are_unreadable() {
        let allowed = ["price", "expect"];
        let ns: Vec<String> = Vec::new();
        for refused in [
            "expect(jest.requireActual('../src/checkout').half()).toBe(15);",
            "expect(vi.requireMock('x').half()).toBe(1);",
            "jest.createMockFromModule('x').run();",
            "jest.requireActual('x').run('HALF');",
            "price(2).toFixed(2);",
        ] {
            assert!(!calls_only_allowed(refused, &allowed, &ns), "{refused}");
        }
        for readable in [
            "jest.fn().mockReturnValue(1);",
            "jest.spyOn(console, 'log');",
            "vi.useFakeTimers();",
            "expect(price(2)).toBe(20);",
        ] {
            assert!(calls_only_allowed(readable, &allowed, &ns), "{readable}");
        }
    }

    #[test]
    fn deeply_nested_conditions_give_up() {
        let params = vec!["a".to_string()];
        let defaults = vec![false];
        let src = "function f(a) {\n  return a;\n}\n";
        let nested = format!("{}a === 2{}", "(".repeat(200), ")".repeat(200));
        let arguments = vec!["1".to_string()];
        let call = CallInputs {
            owner_source: src,
            params: &params,
            defaults: &defaults,
            arguments: &arguments,
        };
        assert!(!call_excludes_arm(
            &call,
            &[GuardCondition {
                text: nested,
                want: true
            }]
        ));
    }
}
