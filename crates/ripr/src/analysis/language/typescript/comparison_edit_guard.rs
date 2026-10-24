//! RIPR-SPEC-0246 rule 9: an edit to a ternary condition's comparison needs a
//! tested input the two comparisons treat differently.
//!
//! `return \`${n} item${n === 1 ? "" : "s"}\`` becoming `n <= 1` changes the
//! result only for the values where `n === 1` and `n <= 1` disagree (`0` and
//! below). A pair of exact assertions on `plural(1)` and `plural(3)` passes on
//! both versions, however exact the matcher is. So an `exposed` ladder result
//! whose change only swaps the operator or literal of one `param <op> number ?`
//! condition keeps its credit unless some observed owner call passes a value
//! where the two comparisons differ, or some call cannot be read.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    fn holds(self, value: f64, literal: f64) -> bool {
        use std::cmp::Ordering;
        let order = value.partial_cmp(&literal);
        match self {
            Self::Equal => order == Some(Ordering::Equal),
            Self::NotEqual => order != Some(Ordering::Equal),
            Self::Less => order == Some(Ordering::Less),
            Self::LessOrEqual => matches!(order, Some(Ordering::Less | Ordering::Equal)),
            Self::Greater => order == Some(Ordering::Greater),
            Self::GreaterOrEqual => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        }
    }
}

/// `param <op> literal` as written in a ternary condition.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Condition<'a> {
    parameter: &'a str,
    comparison: Comparison,
    literal: f64,
}

/// One changed ternary-condition comparison on a changed line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComparisonEdit {
    /// The owner parameter both comparisons read.
    pub(crate) parameter: String,
    /// The old and new comparison as written (`n === 1`, `n <= 1`).
    pub(crate) old_text: String,
    pub(crate) new_text: String,
    old: (Comparison, f64),
    new: (Comparison, f64),
}

/// What the observed owner calls show about a [`ComparisonEdit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComparisonWitness {
    /// Some observed call passes a value the two comparisons treat
    /// differently, or some call or relation cannot be read.
    Discriminates,
    /// Every observed call passes a value both comparisons treat alike.
    SameOutcome,
}

/// For each character of `line`, whether it is code: outside string literals,
/// template text and a trailing `//` comment, but inside a template's `${}`.
fn code_mask(line: &[char]) -> Vec<bool> {
    #[derive(Clone, Copy)]
    enum Mode {
        Code { depth: usize },
        Quoted(char),
        Template,
    }
    let mut mask = vec![false; line.len()];
    let mut stack = vec![Mode::Code { depth: 0 }];
    let mut at = 0;
    while at < line.len() {
        let c = line[at];
        let top = stack.len() - 1;
        match stack[top] {
            Mode::Code { depth } => match c {
                '\'' | '"' => stack.push(Mode::Quoted(c)),
                '`' => stack.push(Mode::Template),
                '/' if line.get(at + 1) == Some(&'/') => break,
                '{' => {
                    stack[top] = Mode::Code { depth: depth + 1 };
                    mask[at] = true;
                }
                '}' if depth == 0 && stack.len() > 1 => {
                    stack.pop();
                }
                '}' => {
                    stack[top] = Mode::Code {
                        depth: depth.saturating_sub(1),
                    };
                    mask[at] = true;
                }
                _ => mask[at] = true,
            },
            Mode::Quoted(quote) => {
                if c == '\\' {
                    at += 1;
                } else if c == quote {
                    stack.pop();
                }
            }
            Mode::Template => {
                if c == '\\' {
                    at += 1;
                } else if c == '`' {
                    stack.pop();
                } else if c == '$' && line.get(at + 1) == Some(&'{') {
                    stack.push(Mode::Code { depth: 0 });
                    at += 1;
                }
            }
        }
        at += 1;
    }
    mask
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// The comparison operator starting at `chars[at]`, longest first, with its
/// length. Arrow `=>`, shifts and assignments are not comparisons.
fn operator_at(chars: &[char], at: usize) -> Option<(Comparison, usize)> {
    let rest = |text: &str| {
        text.chars()
            .enumerate()
            .all(|(offset, expected)| chars.get(at + offset) == Some(&expected))
    };
    let after = |len: usize| chars.get(at + len).copied();
    for (text, comparison) in [
        ("===", Comparison::Equal),
        ("!==", Comparison::NotEqual),
        ("==", Comparison::Equal),
        ("!=", Comparison::NotEqual),
        ("<=", Comparison::LessOrEqual),
        (">=", Comparison::GreaterOrEqual),
    ] {
        if rest(text) {
            return Some((comparison, text.len()));
        }
    }
    let previous = at.checked_sub(1).map(|i| chars[i]);
    match chars.get(at) {
        // `<<`, `<<=`, `=>` never reach here as `<`; `<` followed by `<` is a shift.
        Some('<') if after(1) != Some('<') && previous != Some('<') => Some((Comparison::Less, 1)),
        Some('>')
            if after(1) != Some('>')
                && after(1) != Some('=')
                && !matches!(previous, Some('>' | '=' | '-')) =>
        {
            Some((Comparison::Greater, 1))
        }
        _ => None,
    }
}

/// A plain, optionally negated decimal numeric literal starting at `chars[at]`
/// and the index just after it.
fn number_at(chars: &[char], at: usize) -> Option<(f64, usize)> {
    let mut end = at;
    if chars.get(end) == Some(&'-') {
        end += 1;
        while chars.get(end) == Some(&' ') {
            end += 1;
        }
    }
    let digits_start = end;
    while chars
        .get(end)
        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
    {
        end += 1;
    }
    if end == digits_start || chars.get(end).copied().is_some_and(is_identifier_char) {
        return None;
    }
    let text: String = chars[at..end].iter().filter(|c| **c != ' ').collect();
    Some((text.parse::<f64>().ok()?, end))
}

/// Every `param <op> number ?` condition of `line` that is code, with the
/// character range it covers, in order.
fn conditions(line: &str) -> Vec<(usize, usize, Condition<'_>)> {
    let chars: Vec<char> = line.chars().collect();
    let mask = code_mask(&chars);
    let byte_of: Vec<usize> = line
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(line.len()))
        .collect();
    let mut found = Vec::new();
    for at in 0..chars.len() {
        let Some((comparison, length)) = operator_at(&chars, at) else {
            continue;
        };
        // `param` immediately before the operator.
        let mut ident_end = at;
        while ident_end > 0 && chars[ident_end - 1] == ' ' {
            ident_end -= 1;
        }
        let mut ident_start = ident_end;
        while ident_start > 0 && is_identifier_char(chars[ident_start - 1]) {
            ident_start -= 1;
        }
        if ident_start == ident_end || chars[ident_start].is_ascii_digit() {
            continue;
        }
        // What precedes the identifier must start a fresh operand, so that
        // the comparison is the whole condition: `(`, `{`, `,`, `:`, `?`, a
        // lone `=`, `return`, or the start of the line.
        let mut before = ident_start;
        while before > 0 && chars[before - 1] == ' ' {
            before -= 1;
        }
        let starts_operand = match before.checked_sub(1).map(|i| chars[i]) {
            None => true,
            Some('(' | '{' | ',' | ':') => true,
            Some('?') => before >= 2 && !matches!(chars[before - 2], '?' | '.'),
            Some('=') => before < 2 || !matches!(chars[before - 2], '=' | '!' | '<' | '>'),
            Some(c) if is_identifier_char(c) => {
                let mut word_start = before;
                while word_start > 0 && is_identifier_char(chars[word_start - 1]) {
                    word_start -= 1;
                }
                chars[word_start..before].iter().collect::<String>() == "return"
            }
            Some(_) => false,
        };
        if !starts_operand {
            continue;
        }
        let mut number_start = at + length;
        while chars.get(number_start) == Some(&' ') {
            number_start += 1;
        }
        let Some((literal, number_end)) = number_at(&chars, number_start) else {
            continue;
        };
        // The comparison ends the condition: `?`, not `??` or `?.`.
        let mut after = number_end;
        while chars.get(after) == Some(&' ') {
            after += 1;
        }
        if chars.get(after) != Some(&'?') || matches!(chars.get(after + 1), Some('?' | '.')) {
            continue;
        }
        if !mask[ident_start..number_end].iter().all(|code| *code) {
            continue;
        }
        found.push((
            ident_start,
            number_end,
            Condition {
                parameter: &line[byte_of[ident_start]..byte_of[ident_end]],
                comparison,
                literal,
            },
        ));
    }
    found
}

/// `Some` when the two lines differ only in the operator or literal of one
/// ternary-condition comparison on the same identifier.
pub(crate) fn ts_comparison_edit(old_line: &str, new_line: &str) -> Option<ComparisonEdit> {
    let (old_line, new_line) = (old_line.trim(), new_line.trim());
    let old = conditions(old_line);
    let new = conditions(new_line);
    if old.len() != new.len() || old.is_empty() {
        return None;
    }
    let normalize = |line: &str, found: &[(usize, usize, Condition<'_>)]| {
        let mut chars: Vec<char> = line.chars().collect();
        for (start, end, _) in found.iter().rev() {
            drop(chars.splice(*start..*end, ['@']));
        }
        chars.into_iter().collect::<String>()
    };
    if normalize(old_line, &old) != normalize(new_line, &new) {
        return None;
    }
    let mut differing = old.iter().zip(&new).filter(|((_, _, a), (_, _, b))| a != b);
    let ((old_start, old_end, before), (new_start, new_end, after)) = differing.next()?;
    if differing.next().is_some() || before.parameter != after.parameter {
        return None;
    }
    let text = |line: &str, start: usize, end: usize| {
        line.chars()
            .skip(start)
            .take(end - start)
            .collect::<String>()
    };
    Some(ComparisonEdit {
        parameter: before.parameter.to_string(),
        old_text: text(old_line, *old_start, *old_end),
        new_text: text(new_line, *new_start, *new_end),
        old: (before.comparison, before.literal),
        new: (after.comparison, after.literal),
    })
}

impl ComparisonEdit {
    /// Whether the two comparisons give different answers for `value`.
    fn differs_at(&self, value: f64) -> bool {
        self.old.0.holds(value, self.old.1) != self.new.0.holds(value, self.new.1)
    }

    /// A whole number where the two comparisons disagree, for the message.
    pub(crate) fn differing_value(&self) -> Option<i64> {
        let center = self.new.1.round();
        (0..=3i64)
            .flat_map(|step| [step, -step])
            .map(|step| center as i64 + step)
            .find(|candidate| self.differs_at(*candidate as f64))
    }
}

/// Check every observed owner call. Any piece that cannot be read, from an
/// unmapped relation to a non-literal argument or a call that leaves the
/// compared parameter unbound, reads `Discriminates`, which keeps the ladder
/// verdict.
pub(crate) fn ts_comparison_witness(
    edit: &ComparisonEdit,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    all_tests: &[TypeScriptTest],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> ComparisonWitness {
    witness_or_keep(
        edit,
        owner,
        candidates,
        all_tests,
        alias_map,
        workspace_root,
    )
    .unwrap_or(ComparisonWitness::Discriminates)
}

fn witness_or_keep(
    edit: &ComparisonEdit,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    all_tests: &[TypeScriptTest],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> Option<ComparisonWitness> {
    if !owner.params.contains(&edit.parameter) {
        return None;
    }
    for bindings in observed_numeric_calls(owner, candidates, all_tests, alias_map, workspace_root)?
    {
        let (_, value) = bindings.iter().find(|(name, _)| *name == edit.parameter)?;
        if !value.is_finite() || edit.differs_at(*value) {
            return Some(ComparisonWitness::Discriminates);
        }
    }
    Some(ComparisonWitness::SameOutcome)
}
