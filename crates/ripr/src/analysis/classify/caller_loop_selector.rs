//! RIPR-SPEC-0240 / #5503: a match-arm selector fed by a bounded caller loop.
//!
//! A table-like owner (`fn max_scalar_value(n: usize) -> u32 { match n { 1 =>
//! .., 2 => .., 3 => .. } }`) is rarely called from a test. Its only caller
//! is a loop inside an `Iterator::next`, and the test collects the iterator
//! and compares the whole collection:
//!
//! ```text
//! for i in 1..MAX_UTF8_BYTES { let max = max_scalar_value(i); .. }   // next
//! let seqs = Utf8Sequences::new(..).collect::<Vec<_>>();           // test
//! assert_eq!(seqs, vec![..]);
//! ```
//!
//! The classifier sees only a name token shared with the test and reports a
//! certain `weakly_exposed` gap. This pass reads the finite selector domain
//! the loop feeds the owner (provenance: the loop's file and line) and judges
//! each arm against it. When the changed arm's selector is in the domain and
//! an exhausting, exact collect-and-compare test drives the iterator, the gap
//! claim is withheld as `rust_caller_loop_selector_unresolved`. ripr does not
//! prove the loop is entered for the test's input or that the arm's value
//! reaches the compared items, so nothing is credited. Like every
//! RIPR-SPEC-0240 rule it only withholds; the actionability flip stays
//! fail-closed.
//!
//! Refused (the gap is kept): an unresolved, empty or oversized domain
//! (bound: [`MAX_DOMAIN`]), a changed arm outside the domain, a loop variable
//! rebound in the loop, a call behind an earlier `continue`/`break`/`return`/
//! `?`, a caller that is not `Iterator::next`, a non-exhausting or adapted
//! consumer, a collection that is mutated or not the compared operand, and
//! more than one loop caller.

use super::gap_admission::{is_ignored, withhold_gap};
use crate::analysis::rust_index::{RustIndex, find_file_facts};
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustCallerLoopSelectorUnresolved;
const FIRST_UNRESOLVED_EDGE: &str = "whether the caller loop runs for the related test's input and its arm value reaches the compared items";

/// Largest selector domain read from a loop. A wider range is not enumerated.
pub(super) const MAX_DOMAIN: u64 = 64;

/// Consumer adapters that skip, reorder or end the iteration early, or drop
/// the items the owner shaped.
const NON_EXHAUSTING: [&str; 14] = [
    ".next(",
    ".nth(",
    ".take(",
    ".find(",
    ".any(",
    ".all(",
    ".position(",
    ".take_while(",
    ".step_by(",
    ".filter(",
    ".map(",
    ".skip(",
    ".last(",
    ".count(",
];

/// The selectors a bounded `for` loop feeds the owner, with where it is.
#[derive(Debug, PartialEq, Eq)]
struct LoopDomain {
    start: u64,
    /// One past the last selector.
    end: u64,
    line: usize,
}

impl LoopDomain {
    fn contains(&self, selector: u64) -> bool {
        self.start <= selector && selector < self.end
    }
}

pub(in crate::analysis) fn withhold_caller_loop_selector_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || finding.probe.family != ProbeFamily::MatchArm
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let owner_code = masked(&owner.body);
    let Some(selector) = owner_code
        .lines()
        .nth(changed_line - owner.start_line)
        .and_then(arm_selector)
    else {
        return false;
    };
    if !selector_param(&owner_code, &owner.name) {
        return false;
    }

    // Every non-test function that calls the owner, with the loop reading.
    let mut loops = Vec::new();
    for caller in index.functions() {
        if caller.id == owner.id || !calls_name(&masked(&caller.body), &owner.name) {
            continue;
        }
        let Some(facts) = find_file_facts(index, &caller.file) else {
            return false;
        };
        match caller_loop(
            &masked(&caller.body),
            &owner.name,
            &facts.source,
            caller.start_line,
        ) {
            // Another call outside a loop reaches the owner by a path this
            // pass does not read.
            CallerLoop::NotLooped => return false,
            CallerLoop::Refused => return false,
            CallerLoop::Domain(domain) => {
                let Some(iterator) = iterator_type(&facts.source, caller) else {
                    return false;
                };
                loops.push((domain, iterator, caller));
            }
        }
    }
    // A test that calls the owner itself is judged by the ordinary reach rules.
    if index
        .tests()
        .iter()
        .any(|test| calls_name(&masked(&test.body), &owner.name))
    {
        return false;
    }
    let [(domain, iterator, caller)] = loops.as_slice() else {
        return false;
    };
    let Some(facts) = find_file_facts(index, &caller.file) else {
        return false;
    };
    let impl_source = masked(&facts.source);
    if !domain.contains(selector) {
        return false;
    }

    let mut labels = Vec::new();
    for related in &finding.related_tests {
        let Some(test) = index.tests().iter().find(|test| {
            test.name == related.name
                && test.file == related.file
                && test.start_line == related.line
        }) else {
            continue;
        };
        if is_ignored(test) || !collects_and_compares(&masked(&test.body), iterator, &impl_source) {
            continue;
        }
        labels.push(format!(
            "`{}` ({}:{})",
            test.name,
            test.file.display().to_string().replace('\\', "/"),
            test.start_line
        ));
    }
    if labels.is_empty() {
        return false;
    }
    let loop_file = caller.file.display().to_string().replace('\\', "/");
    let next_step = format!(
        "{} collects every item of `{iterator}` and compares the collection exactly. `{}` feeds `{}` the selectors {}..{} from a loop at {loop_file}:{}, and arm {selector} is one of them, but ripr does not prove the loop is entered for the test's input or that the arm's value reaches the compared items, so ripr does not report a gap. Check that the collection differs between the old and the new arm value; add a test only if it does not.",
        labels.join(", "),
        caller.name,
        owner.name,
        domain.start,
        domain.end - 1,
        domain.line
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    finding.evidence.push(format!(
        "caller_loop_selector_domain: `{iterator}::{}` feeds `{}` the selectors {}..{} from {loop_file}:{}",
        caller.name,
        owner.name,
        domain.start,
        domain.end - 1,
        domain.line
    ));
    true
}

fn masked(text: &str) -> String {
    crate::analysis::extract::mask_comments_and_strings(text)
}

fn boundary(ch: char) -> bool {
    !(ch.is_alphanumeric() || ch == '_')
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// Offsets where `name` appears as a whole identifier.
fn word_offsets(text: &str, name: &str) -> Vec<usize> {
    text.match_indices(name)
        .map(|(at, _)| at)
        .filter(|&at| {
            text[..at].chars().next_back().is_none_or(boundary)
                && text[at + name.len()..].chars().next().is_none_or(boundary)
        })
        .collect()
}

/// Whether `text` binds `name` as a local, loop variable, parameter or
/// closure parameter (`let name`, `let mut name`, `for name`, `name:`, `|name|`).
fn binds(text: &str, name: &str) -> bool {
    word_offsets(text, name).into_iter().any(|at| {
        let before = text[..at].trim_end();
        let after = text[at + name.len()..].trim_start();
        ["let", "mut", "for", "|", ","]
            .iter()
            .any(|k| before.ends_with(k))
            || (after.starts_with(':') && !after.starts_with("::"))
            || after.starts_with("= ")
    })
}

/// Offsets of free-function calls `name(`: a whole identifier that is not a
/// method (`.name(`) or a path tail (`x::name(`).
fn call_offsets(text: &str, name: &str) -> Vec<usize> {
    word_offsets(text, name)
        .into_iter()
        .filter(|&at| {
            text[at + name.len()..].starts_with('(') && !text[..at].trim_end().ends_with(['.', ':'])
        })
        .collect()
}

fn calls_name(text: &str, name: &str) -> bool {
    !call_offsets(text, name).is_empty()
}

/// An unsigned integer literal: decimal or hex, `_` separators, optional
/// integer suffix.
fn int_literal(text: &str) -> Option<u64> {
    let text = text.trim().replace('_', "");
    let (digits, radix) = match text.strip_prefix("0x") {
        Some(hex) => (hex.to_string(), 16),
        None => (text, 10),
    };
    let end = digits
        .find(|ch: char| {
            radix == 16 && !ch.is_ascii_hexdigit() || radix == 10 && !ch.is_ascii_digit()
        })
        .unwrap_or(digits.len());
    let (number, suffix) = digits.split_at(end);
    let suffix_ok = suffix.is_empty()
        || matches!(
            suffix,
            "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
        );
    if number.is_empty() || !suffix_ok {
        return None;
    }
    u64::from_str_radix(number, radix).ok()
}

/// The integer literal an arm line selects on: `2 => 0x07FF,`. Alternatives,
/// ranges, guards and bindings are not a single selector.
fn arm_selector(line: &str) -> Option<u64> {
    if line.matches("=>").count() != 1 {
        return None;
    }
    let (pattern, _) = line.split_once("=>")?;
    int_literal(pattern)
}

/// The owner has one parameter and its body matches on exactly that name.
fn selector_param(owner_code: &str, owner_name: &str) -> bool {
    let Some(head) = owner_code.split_once('{').map(|(head, _)| head) else {
        return false;
    };
    let Some((_, params)) = head.split_once(&format!("fn {owner_name}(")) else {
        return false;
    };
    let Some((params, _)) = params.split_once(')') else {
        return false;
    };
    if params.contains(',') {
        return false;
    }
    let Some((name, _)) = params.split_once(':') else {
        return false;
    };
    let name = name.trim();
    // One `match`, on the parameter: the changed arm cannot belong to another.
    is_ident(name)
        && owner_code.contains(&format!("match {name} {{"))
        && word_offsets(owner_code, "match").len() == 1
}

enum CallerLoop {
    /// The owner is not called inside a `for` loop here.
    NotLooped,
    /// A loop call this pass cannot read soundly.
    Refused,
    Domain(LoopDomain),
}

/// Read the one `for VAR in A..B { .. owner(VAR) .. }` loop in `code`.
fn caller_loop(code: &str, owner: &str, file_source: &str, start_line: usize) -> CallerLoop {
    let mut found = None;
    for (at, _) in code.match_indices("for ") {
        if !code[..at].chars().next_back().is_none_or(boundary) {
            continue;
        }
        let Some((header, _)) = code[at + 4..].split_once('{') else {
            continue;
        };
        let Some((var, range)) = header.split_once(" in ") else {
            continue;
        };
        let var = var.trim();
        let body_start = at + 4 + header.len() + 1;
        let Some(body) = block(&code[body_start..]) else {
            continue;
        };
        if !calls_name(body, owner) {
            continue;
        }
        if found.is_some() || !is_ident(var) {
            return CallerLoop::Refused;
        }
        let call = format!("{owner}({var})");
        let calls = call_offsets(body, owner).len();
        // The call is the loop's own statement: top level of the body, once,
        // with the loop variable as its whole argument, and no exit before it.
        let Some(call_at) = body.find(&call) else {
            return CallerLoop::Refused;
        };
        let exits_first = ["continue", "break", "return", "?"]
            .iter()
            .any(|exit| body[..call_at].contains(exit));
        let rebound = word_offsets(body, var).iter().any(|&use_at| {
            let before = body[..use_at].trim_end();
            let after = body[use_at + var.len()..].trim_start();
            before.ends_with("let") || before.ends_with("let mut") || after.starts_with("= ")
        });
        if calls != 1 || depth_at(body, call_at) != 0 || exits_first || rebound {
            return CallerLoop::Refused;
        }
        // A caller-local binding of a bound's name shadows the file-level const.
        let shadowed = range
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .filter(|name| is_ident(name))
            .any(|name| binds(code, name));
        if shadowed {
            return CallerLoop::Refused;
        }
        let Some((start, end)) = range_bounds(range.trim(), file_source) else {
            return CallerLoop::Refused;
        };
        if start >= end || end - start > MAX_DOMAIN {
            return CallerLoop::Refused;
        }
        let line = start_line + code[..at].matches('\n').count();
        found = Some(LoopDomain { start, end, line });
    }
    match found {
        Some(domain) => CallerLoop::Domain(domain),
        None => CallerLoop::NotLooped,
    }
}

/// `A..B` or `A..=B` where each side is a literal or a unique `const`.
/// Returns the half-open bounds.
fn range_bounds(range: &str, file_source: &str) -> Option<(u64, u64)> {
    let (low, high, inclusive) = match range.split_once("..=") {
        Some((low, high)) => (low, high, true),
        None => {
            let (low, high) = range.split_once("..")?;
            (low, high, false)
        }
    };
    let low = bound(low.trim(), file_source)?;
    let high = bound(high.trim(), file_source)?;
    Some((
        low,
        if inclusive {
            high.checked_add(1)?
        } else {
            high
        },
    ))
}

fn bound(text: &str, file_source: &str) -> Option<u64> {
    if let Some(value) = int_literal(text) {
        return Some(value);
    }
    if !is_ident(text) {
        return None;
    }
    let code = masked(file_source);
    // A second definition anywhere in the file (another module, a shadowing
    // const, a `static`) is ambiguous.
    let declaration = format!("const {text}");
    let defined: Vec<usize> = word_offsets(&code, &declaration);
    let [at] = defined.as_slice() else {
        return None;
    };
    let rest = code[at + declaration.len()..]
        .trim_start()
        .strip_prefix(':')?;
    let (_, value) = rest.split_once('=')?;
    int_literal(value.split(';').next()?)
}

/// The text between `{` (already consumed) and its matching `}`.
fn block(after_open: &str) -> Option<&str> {
    let mut depth = 1usize;
    for (at, ch) in after_open.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after_open[..at]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Brace depth of `text` at byte offset `at`.
fn depth_at(text: &str, at: usize) -> usize {
    let opened = text[..at].matches('{').count();
    let closed = text[..at].matches('}').count();
    opened.saturating_sub(closed)
}

/// `Iterator for TYPE` when `caller` is the `next` method of that impl.
fn iterator_type(
    file_source: &str,
    caller: &crate::analysis::facts::FunctionFact,
) -> Option<String> {
    if caller.name != "next" {
        return None;
    }
    let before: Vec<&str> = file_source
        .lines()
        .take(caller.start_line.saturating_sub(1))
        .collect();
    let head = before
        .iter()
        .rev()
        .find(|line| line.trim_start().starts_with("impl"))?;
    let (before_trait, after) = head.split_once("Iterator for ")?;
    // `DoubleEndedIterator for` is another trait.
    if !before_trait
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_whitespace() || ch == '>' || ch == ':')
    {
        return None;
    }
    let name: String = after
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    is_ident(&name).then_some(name)
}

/// A test that builds `ITERATOR::ctor(..)`, collects it directly and compares
/// the collection, as one whole operand, in an `assert_eq!`.
fn collects_and_compares(test_code: &str, iterator: &str, impl_source: &str) -> bool {
    let Some(binding) = collected_binding(test_code, iterator, impl_source) else {
        return false;
    };
    let uses = word_offsets(test_code, &binding);
    let operands = assert_operand_hits(test_code, &binding);
    // The `let` plus every other mention must be an exact assert operand: a
    // later push, pop, retain, reassignment or other read breaks the claim.
    operands >= 1 && uses.len() == operands + 1
}

/// `let [mut] NAME = ITERATOR::ctor(..).collect()` (turbofish allowed).
fn collected_binding(test_code: &str, iterator: &str, impl_source: &str) -> Option<String> {
    let needle = format!("{iterator}::");
    let mut bindings = Vec::new();
    for statement in test_code.split(';') {
        // The first statement of a body follows the `fn .. {` header.
        let statement = statement.rsplit(['{', '}']).next().unwrap_or("");
        let Some(rest) = statement.trim_start().strip_prefix("let ") else {
            continue;
        };
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("mut ").unwrap_or(rest);
        let Some((name, rhs)) = rest.split_once('=') else {
            continue;
        };
        let name = name.split(':').next().unwrap_or("").trim();
        let rhs = rhs.trim();
        if !is_ident(name) || !rhs.starts_with(&needle) {
            continue;
        }
        let ctor = rhs[needle.len()..].split('(').next().unwrap_or("").trim();
        let Some(tail) = after_call(&rhs[needle.len()..]) else {
            continue;
        };
        let Some(collect) = tail.trim_start().strip_prefix(".collect") else {
            continue;
        };
        let collect = match collect.strip_prefix("::<") {
            Some(generic) => generic.split_once(">()").map(|(_, after)| after),
            None => collect.strip_prefix("()"),
        };
        let Some(collect) = collect else {
            continue;
        };
        if collect.trim().is_empty()
            && !NON_EXHAUSTING.iter().any(|op| rhs.contains(op))
            && ctor_builds_iterator(impl_source, iterator, ctor)
        {
            bindings.push(name.to_string());
        }
    }
    let [only] = bindings.as_slice() else {
        return None;
    };
    Some(only.clone())
}

/// `fn ctor` is defined once in the impl blocks of `iterator` and returns
/// `Self` or the iterator type, so the collected value is the iterator whose
/// `next` the loop is in.
fn ctor_builds_iterator(impl_source: &str, iterator: &str, ctor: &str) -> bool {
    let declaration = format!("fn {ctor}");
    let mut signatures = Vec::new();
    for at in word_offsets(impl_source, "impl") {
        let Some((header, rest)) = impl_source[at..].split_once('{') else {
            continue;
        };
        let target = header.rsplit(" for ").next().unwrap_or(header).trim();
        let target = target.split('<').next().unwrap_or(target).trim();
        if !header.contains(iterator) || target.split_whitespace().last() != Some(iterator) {
            continue;
        }
        let Some(block) = block(rest) else {
            continue;
        };
        for at in word_offsets(block, &declaration) {
            signatures.push(block[at..].split(['{', ';']).next().unwrap_or(""));
        }
    }
    let [signature] = signatures.as_slice() else {
        return false;
    };
    let Some((_, ret)) = signature.split_once("->") else {
        return false;
    };
    let ret = ret.trim();
    ret == "Self" || ret == iterator
}

/// The text after `ident(..)`, with the call's parentheses balanced.
fn after_call(text: &str) -> Option<&str> {
    let open = text.find('(')?;
    if !is_ident(&text[..open]) {
        return None;
    }
    let mut depth = 0usize;
    for (at, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + at + 1..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// How many `assert_eq!` calls have exactly one of their first two arguments
/// exactly `name` (optionally borrowed).
fn assert_operand_hits(test_code: &str, name: &str) -> usize {
    let mut hits = 0;
    // Only equality is an exact comparison; `assert_ne!` rules out one value.
    for macro_name in ["assert_eq!"] {
        for at in word_offsets(test_code, macro_name.trim_end_matches('!'))
            .into_iter()
            .filter(|&at| test_code[at + macro_name.len() - 1..].starts_with('!'))
        {
            let Some(args) = test_code[at + macro_name.len()..]
                .trim_start()
                .strip_prefix('(')
            else {
                continue;
            };
            let Some(args) = paren_args(args) else {
                continue;
            };
            let named = args
                .iter()
                .take(2)
                .filter(|arg| arg.trim().trim_start_matches('&').trim() == name)
                .count();
            // `assert_eq!(x, x)` compares the collection with itself.
            if named == 1 {
                hits += 1;
            }
        }
    }
    hits
}

/// Top-level comma-separated arguments up to the matching `)`.
fn paren_args(after_open: &str) -> Option<Vec<&str>> {
    let mut depth = 0usize;
    let mut args = Vec::new();
    let mut start = 0;
    for (at, ch) in after_open.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                args.push(&after_open[start..at]);
                return Some(args);
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&after_open[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONSTS: &str = "const MAX_BYTES: usize = 4;\n";

    #[test]
    fn integer_literals_read_decimal_hex_separators_and_suffixes() {
        assert_eq!(int_literal("2"), Some(2));
        assert_eq!(int_literal(" 0x07FF "), Some(0x07FF));
        assert_eq!(int_literal("0x0010_FFFF"), Some(0x0010_FFFF));
        assert_eq!(int_literal("4usize"), Some(4));
        assert_eq!(int_literal("n"), None);
        assert_eq!(int_literal("4.0"), None);
    }

    #[test]
    fn an_arm_selector_is_one_integer_literal() {
        assert_eq!(arm_selector("        2 => 0x07FF,"), Some(2));
        assert_eq!(arm_selector("        1 | 2 => 0,"), None);
        assert_eq!(arm_selector("        1..=2 => 0,"), None);
        assert_eq!(arm_selector("        n if n > 2 => 0,"), None);
        assert_eq!(arm_selector("        _ => unreachable!(),"), None);
    }

    #[test]
    fn the_selector_param_is_the_one_matched_name() {
        let code = "fn m(n: usize) -> u32 { match n { 1 => 1, _ => 0 } }";
        assert!(selector_param(code, "m"));
        let two = "fn m(n: usize, k: u8) -> u32 { match n { 1 => 1, _ => 0 } }";
        assert!(!selector_param(two, "m"));
        let other = "fn m(n: usize) -> u32 { match other { 1 => 1, _ => 0 } }";
        assert!(!selector_param(other, "m"));
    }

    fn domain_of(code: &str) -> Option<(u64, u64)> {
        match caller_loop(code, "m", CONSTS, 1) {
            CallerLoop::Domain(domain) => Some((domain.start, domain.end)),
            _ => None,
        }
    }

    #[test]
    fn a_const_and_literal_range_is_enumerated() {
        let body = "{ for i in 1..MAX_BYTES { let x = m(i); use_it(x); } }";
        assert_eq!(domain_of(body), Some((1, 4)));
        let inclusive = "{ for i in 1..=3 { let x = m(i); } }";
        assert_eq!(domain_of(inclusive), Some((1, 4)));
    }

    #[test]
    fn the_domain_bound_is_exact() {
        let at_bound = format!("{{ for i in 0..{MAX_DOMAIN} {{ let x = m(i); }} }}");
        assert_eq!(domain_of(&at_bound), Some((0, MAX_DOMAIN)));
        let over = format!("{{ for i in 0..{} {{ let x = m(i); }} }}", MAX_DOMAIN + 1);
        assert_eq!(domain_of(&over), None);
    }

    #[test]
    fn unreadable_loops_are_refused() {
        // Unresolved bound, empty range, a call behind an exit, a rebound
        // variable, an expression argument, and a conditional call.
        for body in [
            "{ for i in 1..LIMIT { let x = m(i); } }",
            "{ for i in 3..3 { let x = m(i); } }",
            "{ for i in 1..4 { if skip() { continue; } let x = m(i); } }",
            "{ for i in 1..4 { let i = 2; let x = m(i); } }",
            "{ for i in 1..4 { let x = m(i + 1); } }",
            "{ for i in 1..4 { if ok() { let x = m(i); } } }",
            "{ for i in 1..4 { let x = m(i); let y = m(i); } }",
        ] {
            assert!(
                matches!(caller_loop(body, "m", CONSTS, 1), CallerLoop::Refused),
                "{body}"
            );
        }
        assert!(matches!(
            caller_loop("{ let x = m(2); }", "m", CONSTS, 1),
            CallerLoop::NotLooped
        ));
    }

    #[test]
    fn only_free_calls_of_the_whole_name_count() {
        assert!(calls_name("let x = m(i);", "m"));
        for text in ["foo_m(i)", "self.m(i)", "x::m(i)", "m (i)", "let m = 1;"] {
            assert!(!calls_name(text, "m"), "{text}");
        }
    }

    #[test]
    fn a_line_with_two_arms_is_not_one_selector() {
        assert_eq!(arm_selector("1 => a, 2 => b,"), None);
    }

    #[test]
    fn a_second_match_in_the_owner_is_refused() {
        let code =
            "fn m(n: usize) -> u32 { match n { 1 => 1, _ => 0 } match kind { 2 => 2, _ => 0 } }";
        assert!(!selector_param(code, "m"));
    }

    #[test]
    fn a_named_bound_the_caller_rebinds_is_refused() {
        for body in [
            "{ let MAX_BYTES = 2; for i in 1..MAX_BYTES { let x = m(i); } }",
            "{ let mut MAX_BYTES = 2; for i in 1..MAX_BYTES { let x = m(i); } }",
        ] {
            assert!(matches!(
                caller_loop(body, "m", CONSTS, 1),
                CallerLoop::Refused
            ));
        }
        // Plain uses of the const elsewhere in the caller are not bindings.
        let used = "{ for i in 1..MAX_BYTES { let x = m(i); } let a = [0; MAX_BYTES]; }";
        assert_eq!(domain_of(used), Some((1, 4)));
    }

    #[test]
    fn a_second_const_definition_is_ambiguous() {
        let source = "const MAX_BYTES: usize = 4;\nmod other { const MAX_BYTES: usize = 9; }\n";
        assert_eq!(bound("MAX_BYTES", source), None);
        assert_eq!(bound("MAX_BYTES", CONSTS), Some(4));
    }

    #[test]
    fn only_a_direct_collect_compared_whole_is_a_consumer() {
        let imp: &str = "impl Seq { fn new(a: u32, b: u32) -> Self { Seq } }\nimpl Other { fn new(a: u32) -> Vec<u32> { vec![] } }";
        let good = "let seqs = Seq::new(1, 2).collect::<Vec<_>>(); assert_eq!(seqs, vec![1]);";
        assert!(collects_and_compares(good, "Seq", imp));
        let plain = "let seqs = Seq::new(1, 2).collect(); assert_eq!(vec![1], seqs);";
        assert!(collects_and_compares(plain, "Seq", imp));
        // An earlier unrelated constructor call does not hide the later one.
        let earlier = "let first = Seq::new(1, 2).next(); let seqs = Seq::new(1, 2).collect(); assert_eq!(seqs, vec![1]);";
        assert!(collects_and_compares(earlier, "Seq", imp));
        for bad in [
            "let seqs = Seq::new(1, 2).collect::<Vec<_>>(); assert_eq!(seqs, seqs);",
            "let seqs = Seq::new(1, 2).collect::<Vec<_>>(); assert_ne!(seqs, vec![]);",
            "let seqs = Seq::new(1, 2).take(1).collect::<Vec<_>>(); assert_eq!(seqs, vec![1]);",
            "let seqs = Seq::new(1, 2).map(f).collect::<Vec<_>>(); assert_eq!(seqs, vec![1]);",
            "let seqs = Seq::new(1, 2).collect::<Vec<_>>(); assert_eq!(seqs.len(), 1);",
            "let mut seqs = Seq::new(1, 2).collect::<Vec<_>>(); seqs.pop(); assert_eq!(seqs, vec![1]);",
            "let seqs = Seq::new(1, 2).collect::<Vec<_>>();",
            "let seqs = Other::new(1, 2).collect::<Vec<_>>(); assert_eq!(seqs, vec![1]);",
            "let seqs = Seq::new(1, 2).next(); assert_eq!(seqs, Some(1));",
        ] {
            assert!(!collects_and_compares(bad, "Seq", imp), "{bad}");
        }
        // A constructor that returns another type is not the iterator.
        let other =
            "impl Seq { fn new(a: u32, b: u32) -> std::iter::Empty<u32> { std::iter::empty() } }";
        assert!(!collects_and_compares(good, "Seq", other));
        // Two definitions of the constructor are ambiguous.
        let twice = "impl Seq { fn new() -> Self { Seq } fn new() -> Self { Seq } }";
        assert!(!collects_and_compares(good, "Seq", twice));
    }
}
