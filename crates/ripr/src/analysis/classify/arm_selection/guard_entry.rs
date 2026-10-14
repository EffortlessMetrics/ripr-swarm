//! Whether a related test's owner call enters the `if` that encloses a
//! changed side effect (RIPR-SPEC-0263, #7132).
//!
//! A guarded `v.push(x)` runs only for inputs that make its `if` condition
//! true. When every related test calls the owner with a literal that makes
//! the condition false, no mutant of the push executes under the tests, so
//! an assertion over the collection cannot reveal it, however exactly it
//! pins the collection.
//!
//! The reading is deliberately narrow and fails closed. The condition must
//! be one comparison between an immutable owner parameter and an integer
//! literal (`x > 10`, `10 <= x`), the `if` must be the innermost block
//! around the changed line, and nothing before it in the owner may rebind
//! or reassign the parameter or loop back to it. Each call must pass an
//! integer literal at the parameter's position. The owner must not name
//! itself past its signature (recursion can enter the guard with a value no
//! test passes), and the changed line must hold no brace of its own. Any
//! other shape is unknown, and unknown never names the guard.

use super::{
    function_parameters, integer_value, is_identifier, line_starts_of, owner_calls_in,
    parameter_declaration, parameter_name, whole_word_offsets,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, TestSummary};
use crate::domain::Probe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

impl Comparison {
    /// The comparison read with its operands swapped: `10 < x` is `x > 10`.
    fn flipped(self) -> Self {
        match self {
            Comparison::Less => Comparison::Greater,
            Comparison::LessEqual => Comparison::GreaterEqual,
            Comparison::Greater => Comparison::Less,
            Comparison::GreaterEqual => Comparison::LessEqual,
            Comparison::Equal | Comparison::NotEqual => self,
        }
    }

    fn holds(self, value: i128, bound: i128) -> bool {
        match self {
            Comparison::Less => value < bound,
            Comparison::LessEqual => value <= bound,
            Comparison::Greater => value > bound,
            Comparison::GreaterEqual => value >= bound,
            Comparison::Equal => value == bound,
            Comparison::NotEqual => value != bound,
        }
    }
}

/// The guard around a changed side effect, read as `<parameter> <op>
/// <bound>`, and how a related test's owner call passes that parameter.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct GuardEntry {
    owner: String,
    method: bool,
    local_roots: Vec<String>,
    parameter: String,
    index: usize,
    comparison: Comparison,
    bound: i128,
    condition: String,
}

impl GuardEntry {
    /// The guard of the probe's line, or `None` when the innermost block
    /// around it is not an `if` this module can read (see the module
    /// documentation for the accepted shape).
    pub(in crate::analysis) fn establish(probe: &Probe, owner: &FunctionSummary) -> Option<Self> {
        if owner.name.is_empty() {
            return None;
        }
        let relative = probe.location.line.checked_sub(owner.start_line)?;
        let masked = mask_comments_and_strings(&owner.body);
        let line_offset = masked
            .split_inclusive('\n')
            .take(relative)
            .map(str::len)
            .sum::<usize>();
        if line_offset == 0 || line_offset > masked.len() {
            return None;
        }
        // A brace on the changed line itself (`} else { small(v); }`,
        // `if y { v.push(x) }`) means the line opens or closes a block the
        // scan below does not see, so the enclosing `if` is not read.
        let probe_line = masked
            .get(line_offset..)?
            .lines()
            .next()
            .unwrap_or_default();
        if probe_line.contains(['{', '}']) {
            return None;
        }
        // An owner that names itself past its signature may call itself
        // (`walk(v, depth + 1)`), and a recursive call can enter the guard
        // with a value no test passes.
        if whole_word_offsets(&masked, &owner.name).len() > 1 {
            return None;
        }
        let before = masked.get(..line_offset)?;
        let brace = innermost_open_brace(before)?;
        let head = before.get(..brace)?;
        let keyword = *whole_word_offsets(head, "if").last()?;
        let condition = head.get(keyword + "if".len()..)?.trim();
        // The `if` must open this very block: text with a statement end,
        // a block, or an arm arrow between them belongs to something else.
        if condition.is_empty() || condition.contains(['{', '}', ';']) || condition.contains("=>") {
            return None;
        }
        let (parameter, comparison, bound) = parameter_comparison(condition)?;
        let parameters = function_parameters(owner);
        let index = parameters
            .iter()
            .position(|declared| parameter_name(declared) == parameter)?;
        let declared = parameter_declaration(owner, &parameter)?;
        let body_start = masked.find('\n')? + 1;
        let prelude = masked.get(body_start..keyword)?;
        if declared.mutable || may_rebind_before(prelude, &parameter) {
            return None;
        }
        let method = owner.item.has_self_param || super::owner_has_self_parameter(owner);
        let mut local_roots = ["crate", "self", "super", "Self"]
            .map(str::to_string)
            .to_vec();
        if let crate::analysis::facts::FunctionImplContext::Impl { self_type } = &owner.impl_context
            && let Some(type_name) = self_type
                .split('<')
                .next()
                .and_then(|path| path.rsplit("::").next())
                .map(str::trim)
                .filter(|name| !name.is_empty())
        {
            local_roots.push(type_name.to_string());
        }
        Some(Self {
            owner: owner.name.clone(),
            method,
            local_roots,
            parameter,
            index,
            comparison,
            bound,
            condition: condition.split_whitespace().collect::<Vec<_>>().join(" "),
        })
    }

    /// The guard's condition as written, whitespace collapsed.
    pub(in crate::analysis) fn condition(&self) -> &str {
        &self.condition
    }

    /// The guarded parameter's name.
    pub(in crate::analysis) fn parameter(&self) -> &str {
        &self.parameter
    }

    /// The literal inputs the test passes, when the test calls the owner
    /// and every call provably leaves the guard false. `None` when the test
    /// does not name the owner, some mention is not a readable direct call,
    /// some input is not an integer literal, or some input enters the guard.
    pub(in crate::analysis) fn unentered_inputs(&self, test: &TestSummary) -> Option<Vec<String>> {
        let masked = mask_comments_and_strings(&test.body);
        let mentions = whole_word_offsets(&masked, &self.owner);
        if mentions.is_empty() {
            return None;
        }
        let mut inputs = Vec::new();
        for line_start in line_starts_of(&test.body, &mentions) {
            let line_end = test.body[line_start..]
                .find('\n')
                .map_or(test.body.len(), |offset| line_start + offset);
            let line = &test.body[line_start..line_end];
            let calls = owner_calls_in(line, &self.owner, self.method, &self.local_roots);
            let line_mentions = mentions
                .iter()
                .filter(|offset| (line_start..line_end).contains(*offset))
                .count();
            if calls.len() != line_mentions {
                return None;
            }
            for call in calls {
                let input = call.arguments.get(self.index)?.trim();
                let value = integer_value(input)?;
                if self.comparison.holds(value, self.bound) {
                    return None;
                }
                inputs.push(input.to_string());
            }
        }
        Some(inputs)
    }
}

/// The offset of the innermost `{` still open at the end of `before`, or
/// `None` when the innermost open delimiter is a `(` or `[`.
fn innermost_open_brace(before: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in before.bytes().enumerate().rev() {
        match byte {
            b'}' | b')' | b']' => depth += 1,
            b'{' | b'(' | b'[' => {
                if depth == 0 {
                    return (byte == b'{').then_some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// `x > 10` or `10 < x` (optionally parenthesized) as the parameter, the
/// comparison with the parameter on the left, and the integer bound.
fn parameter_comparison(condition: &str) -> Option<(String, Comparison, i128)> {
    let mut condition = condition.trim();
    while let Some(inner) = condition
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    {
        condition = inner.trim();
    }
    if condition.contains(['(', ')', '&', '|', '!']) && !condition.contains("!=") {
        return None;
    }
    let (at, width, comparison) = single_comparison(condition)?;
    let left = condition.get(..at)?.trim();
    let right = condition.get(at + width..)?.trim();
    match (integer_value(left), integer_value(right)) {
        (None, Some(bound)) if is_identifier(left) => Some((left.to_string(), comparison, bound)),
        (Some(bound), None) if is_identifier(right) => {
            Some((right.to_string(), comparison.flipped(), bound))
        }
        _ => None,
    }
}

/// The one comparison operator in `condition`: its offset, width and kind.
/// `None` when there is none, more than one, or another operator such as
/// `&&`, `||`, `!` or a shift.
fn single_comparison(condition: &str) -> Option<(usize, usize, Comparison)> {
    let bytes = condition.as_bytes();
    let mut found = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let pair = bytes.get(index..index + 2);
        let (width, comparison) = match (bytes[index], pair) {
            (_, Some(b"<=")) => (2, Comparison::LessEqual),
            (_, Some(b">=")) => (2, Comparison::GreaterEqual),
            (_, Some(b"==")) => (2, Comparison::Equal),
            (_, Some(b"!=")) => (2, Comparison::NotEqual),
            (_, Some(b"<<" | b">>")) => return None,
            (b'<', _) => (1, Comparison::Less),
            (b'>', _) => (1, Comparison::Greater),
            (b'=' | b'!', _) => return None,
            _ => {
                index += 1;
                continue;
            }
        };
        if found.is_some() {
            return None;
        }
        found = Some((index, width, comparison));
        index += width;
    }
    found
}

/// Whether the owner text before the guard may give the parameter another
/// value by the time the guard reads it: any loop (its body may run the
/// guard again after a reassignment), or any mention of the parameter other
/// than a plain read in an expression. Telling a call argument (`f(x)`)
/// from a pattern position (`Some(x) =`) needs a parser, so both refuse.
fn may_rebind_before(prelude: &str, parameter: &str) -> bool {
    if ["loop", "while", "for"]
        .iter()
        .any(|keyword| !whole_word_offsets(prelude, keyword).is_empty())
    {
        return true;
    }
    whole_word_offsets(prelude, parameter)
        .into_iter()
        .any(|offset| {
            let prefix = prelude[..offset].trim_end();
            let suffix = prelude[offset + parameter.len()..].trim_start();
            ["let", "mut", "ref", "|", "(", ",", "{", "@", "[", "&"]
                .iter()
                .any(|marker| prefix.ends_with(marker))
                || (prefix.ends_with(':') && !prefix.ends_with("::"))
                || (suffix.starts_with('=') && !suffix.starts_with("=="))
                || [
                    "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>=", "@", ":",
                ]
                .iter()
                .any(|marker| suffix.starts_with(marker))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_comparison_reads_one_literal_bound_either_way_round() {
        assert_eq!(
            parameter_comparison("x > 10"),
            Some(("x".to_string(), Comparison::Greater, 10))
        );
        assert_eq!(
            parameter_comparison("(10 <= x)"),
            Some(("x".to_string(), Comparison::GreaterEqual, 10))
        );
        assert_eq!(
            parameter_comparison("x != 0"),
            Some(("x".to_string(), Comparison::NotEqual, 0))
        );
        for unread in [
            "x > y",
            "x > 10 && y",
            "x + 1 > 10",
            "!x",
            "x.len() > 10",
            "x >> 2 > 1",
            "let Some(v) = x",
            "x > LIMIT",
        ] {
            assert_eq!(parameter_comparison(unread), None, "{unread}");
        }
    }

    #[test]
    fn comparison_holds_like_rust() {
        assert!(!Comparison::Greater.holds(4, 10));
        assert!(Comparison::Greater.holds(11, 10));
        assert!(!Comparison::Greater.holds(10, 10));
        assert!(Comparison::GreaterEqual.holds(10, 10));
        assert!(Comparison::Less.flipped() == Comparison::Greater);
    }

    #[test]
    fn a_loop_or_rebinding_before_the_guard_refuses() {
        assert!(!may_rebind_before("    let y = x + 1;\n    ", "x"));
        assert!(may_rebind_before("    let x = x + 1;\n    ", "x"));
        assert!(may_rebind_before("    x = 3;\n    ", "x"));
        assert!(may_rebind_before("    x += 3;\n    ", "x"));
        assert!(may_rebind_before("    for i in 0..3 {\n    ", "x"));
        assert!(may_rebind_before(
            "    let Some(x) = other else { return };\n",
            "x"
        ));
    }
}
