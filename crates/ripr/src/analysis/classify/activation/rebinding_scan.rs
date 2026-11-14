//! The one-pass rebinding scan (#11738) answers exactly what the
//! per-occurrence scans it replaced answered, and one call stays linear in
//! the owner body.
//!
//! `reference_owner_rebinds_operand` and `reference_in_match_arm_pattern` are
//! the replaced implementations, kept verbatim as the oracle: the rebinding
//! rule (#9197, RIPR-SPEC-0197) is unchanged, so every body, operand and
//! predicate line must read the same through both.

use super::*;
use crate::analysis::facts::FunctionSourceRole;
use std::path::PathBuf;

fn owner(body: &str) -> FunctionSummary {
    FunctionSummary {
        id: SymbolId("src/lib.rs::score".to_string()),
        name: "score".to_string(),
        file: PathBuf::from("src/lib.rs"),
        start_line: 7,
        end_line: 7 + body.lines().count(),
        body: body.into(),
        calls: crate::analysis::facts::SharedFactSlice::empty(),
        returns: Vec::new(),
        literals: crate::analysis::facts::SharedFactSlice::empty(),
        source_role: FunctionSourceRole::Production,
        attrs: crate::analysis::facts::SharedFactSlice::empty(),
        impl_attrs: Vec::new(),
        nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
        let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
        item: Default::default(),
        impl_context: Default::default(),
    }
}

fn reference_owner_rebinds_operand(
    owner: &FunctionSummary,
    operand: &str,
    predicate_line: usize,
) -> bool {
    if owner_declares_local(owner, operand, predicate_line) {
        return true;
    }
    let is_identifier = operand
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && operand
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !is_identifier {
        return false;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&owner.body);
    let through_predicate = masked
        .lines()
        .enumerate()
        .take_while(|(offset, _)| owner.start_line + offset <= predicate_line)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    let Some((signature, body)) = through_predicate.split_once('{') else {
        return false;
    };
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    // Only a `mut` parameter can be changed in place by a receiver call
    // (`x.add_assign(5)`); an immutable one cannot, so `x.abs()` stays a read.
    let mut_parameter = signature.match_indices("mut").any(|(start, _)| {
        !signature[..start].ends_with(is_word)
            && signature[start + 3..]
                .trim_start()
                .strip_prefix(operand)
                .is_some_and(|rest| rest.trim_start().starts_with(':'))
    });
    body.match_indices(operand).any(|(start, _)| {
        let before = &body[..start];
        let after = &body[start + operand.len()..];
        if before.chars().next_back().is_some_and(is_word)
            || after.chars().next().is_some_and(is_word)
            || before.ends_with('.')
            || before.ends_with("::")
        {
            return false;
        }
        let before_trimmed = before.trim_end();
        let after_trimmed = after.trim_start();
        let line_before = before.rsplit('\n').next().unwrap_or(before);
        let last_word = before_trimmed
            .rsplit(|ch: char| !is_word(ch))
            .next()
            .unwrap_or("");
        let assigned = (after_trimmed.starts_with('=')
            && !after_trimmed.starts_with("==")
            && !after_trimmed.starts_with("=>"))
            || ["+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<=", ">>="]
                .iter()
                .any(|assignment| after_trimmed.starts_with(assignment));
        let binding_keyword = matches!(last_word, "let" | "mut" | "ref");
        // `x.add_assign(5)` on a `mut` parameter: a receiver call (whitespace
        // tolerant) may mutate it in place, so it no longer holds the input.
        let receiver_call = mut_parameter
            && after_trimmed.strip_prefix('.').is_some_and(|rest| {
                let rest = rest.trim_start();
                let name_len = rest.chars().take_while(|ch| is_word(*ch)).count();
                name_len > 0 && rest[name_len..].trim_start().starts_with(['(', ':'])
            });
        // A pattern name sits before the arm's own `=>`: a guard use
        // (`n if 0 > n =>`), the scrutinee of a one-line match, or a use in an
        // earlier arm's result (`Some(v) => x, None =>`) is not a binding. An
        // arm that re-binds the scrutinee's own name is declined too (no
        // attempt to tie it to its match).
        // The one exemption is a bare first-arm binder of the scrutinee's own
        // name (`match n { n if 0 > n => .. }`): it binds the same value the
        // match reads, so it is not a different `n`. Any other arm pattern
        // naming it, or a scrutinee that is more than the bare name, is declined.
        let binds_scrutinee_itself = {
            let opening = before_trimmed.strip_suffix('{').map(str::trim_end);
            opening
                .and_then(|head| head.strip_suffix(operand))
                .is_some_and(|head| {
                    let head = head.trim_end();
                    head.strip_suffix("match")
                        .is_some_and(|rest| !rest.ends_with(is_word))
                })
                && (after_trimmed.starts_with("if ")
                    || after_trimmed.starts_with("=>")
                    || after_trimmed.starts_with(','))
        };
        let match_arm_pattern = !binds_scrutinee_itself && reference_in_match_arm_pattern(before);
        // A destructuring assignment `(x, y) = (y, x);` or `[x, ..] = ..;`:
        // the bracket group holding the name is followed by a bare `=`.
        let destructuring_assignment = {
            let mut depth = 0usize;
            let mut closed_at = None;
            for (offset, ch) in after.char_indices() {
                match ch {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' if depth == 0 => {
                        closed_at = Some(offset + 1);
                        break;
                    }
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
            }
            closed_at.is_some_and(|at| {
                let rest = after[at..].trim_start();
                rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
            })
        };
        let for_pattern = before
            .rfind("for ")
            .filter(|at| at == &0 || !before[..*at].ends_with(is_word))
            .is_some_and(|at| !contains_word(&before[at..], "in"));
        let let_pattern = before
            .rfind("let ")
            .filter(|at| at == &0 || !before[..*at].ends_with(is_word))
            .is_some_and(|at| !before[at..].contains(['=', ';']));
        let single_pipes = line_before
            .replace("||", "")
            .replace("|=", "")
            .matches('|')
            .count();
        assigned
            || receiver_call
            || destructuring_assignment
            || binding_keyword
            || match_arm_pattern
            || for_pattern
            || let_pattern
            || single_pipes % 2 == 1
            || line_before.contains("matches!")
    })
}

fn reference_in_match_arm_pattern(before: &str) -> bool {
    let chars: Vec<char> = before.chars().collect();
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut i = chars.len();
    // After a `,` or a skipped `{}` block at this level the text read back is
    // the previous arm's result (or a tuple's earlier elements), which may be
    // any expression: it is skipped without classification, and the name is a
    // pattern if that read reaches the previous arm's `=>` or the match
    // block's `{` without leaving the level through an unmatched `(` or `[`
    // (a tuple or call comma, which resumes the strict read).
    let mut tolerant = false;
    while i > 0 {
        i -= 1;
        let ch = chars[i];
        if ch.is_whitespace() {
            continue;
        }
        if matches!(ch, ')' | ']' | '}') {
            let mut depth = 1usize;
            while i > 0 && depth > 0 {
                i -= 1;
                match chars[i] {
                    ')' | ']' | '}' => depth += 1,
                    '(' | '[' | '{' => depth -= 1,
                    _ => {}
                }
            }
            if depth > 0 {
                return false;
            }
            if ch == '}' {
                tolerant = true;
            }
            continue;
        }
        if tolerant {
            match ch {
                '(' | '[' => tolerant = false,
                '{' => {
                    let head: String = chars[..i].iter().collect();
                    let boundary = head.rfind([';', '{', '}']).map_or(0, |at| at + 1);
                    if contains_word(&head[boundary..], "match") {
                        return true;
                    }
                    tolerant = false;
                }
                ';' => return false,
                '>' if i > 0 && chars[i - 1] == '=' => return true,
                _ => {}
            }
            continue;
        }
        match ch {
            '(' | '[' => {}
            '{' => {
                let head: String = chars[..i].iter().collect();
                let boundary = head.rfind([';', '{', '}']).map_or(0, |at| at + 1);
                if contains_word(&head[boundary..], "match") {
                    return true;
                }
            }
            ',' => tolerant = true,
            '|' | '&' | ':' | '@' => {}
            '.' => {
                if i == 0 || chars[i - 1] != '.' {
                    return false;
                }
            }
            '=' => {
                if !(i > 0 && chars[i - 1] == '.') {
                    return false;
                }
            }
            c if is_word(c) => {
                let mut start = i;
                while start > 0 && is_word(chars[start - 1]) {
                    start -= 1;
                }
                let word: String = chars[start..=i].iter().collect();
                if matches!(
                    word.as_str(),
                    "if" | "match" | "while" | "let" | "for" | "in" | "return" | "else"
                ) {
                    return false;
                }
                i = start;
            }
            // `=>` directly before the name is a body use; any other
            // operator ends the pattern.
            _ => return false,
        }
    }
    false
}

/// Deterministic generator (a 64-bit LCG), so a failure names a seed that
/// reproduces it.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap_or(0) % bound.max(1)
    }
}

/// Tokens that reach every branch of the rule: binders, assignments,
/// brackets of all three kinds (balanced or not), arm and closure syntax,
/// keywords the scans search for, masked text and non-ASCII chars
/// (a no-break space is whitespace to the arm scan).
const TOKENS: &[&str] = &[
    "x",
    "x",
    "x",
    "y",
    "n",
    "xs",
    "self.x",
    "m::x",
    "let",
    "let",
    "mut",
    "ref",
    "for",
    "in",
    "match",
    "match x {",
    "if",
    "else",
    "while",
    "return",
    "matches!(",
    "=>",
    "=",
    "=",
    "==",
    "+=",
    "<<=",
    ">=",
    "|",
    "|",
    "||",
    "|=",
    "&",
    "@",
    "..=",
    "..",
    ".",
    "::",
    ":",
    ",",
    ",",
    ";",
    ";",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "(x, y)",
    "[x, ..]",
    "Some(x)",
    "None",
    "0",
    "1",
    ".abs()",
    ".add_assign(5)",
    "\"x = 1\"",
    "/* let x */",
    "// x = 2",
    "\u{e9}",
    "\u{a0}",
    "\u{2192}",
    "_",
    " ",
    " ",
    "\n",
    "\n",
    "\n    ",
];

const SIGNATURES: &[&str] = &[
    "fn score(x: i32, y: i32) -> i32 {",
    "fn score(mut x: i32, n: usize) -> i32 {",
    "fn score(xs: &[i32], mut y: i32)\n    -> i32\n{",
    "fn score(x: i32) -> i32",
];

fn generated_body(seed: u64) -> String {
    let mut lcg = Lcg(seed);
    let mut body = SIGNATURES[lcg.below(SIGNATURES.len())].to_string();
    body.push('\n');
    for _ in 0..lcg.below(60) {
        body.push_str(TOKENS[lcg.below(TOKENS.len())]);
        if lcg.below(3) > 0 {
            body.push(' ');
        }
    }
    body.push_str("\n}\n");
    body
}

const HAND_WRITTEN: &[&str] = &[
    "fn score(x: i32) -> i32 {\n    if x >= 10 { return 1; }\n    x\n}\n",
    "fn score(x: i32) -> i32 {\n    let x = x + 1;\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, y: i32) -> i32 {\n    (x, y) = (y, x);\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, y: i32) -> i32 {\n    [x, ..] =\n        [y];\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32) -> i32 {\n    foo(x\n    ) == 3;\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(mut x: i32) -> i32 {\n    x.add_assign(5);\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32) -> i32 {\n    x.abs();\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(n: i32) -> i32 {\n    match n {\n        n if 0 > n => 1,\n        _ => 0,\n    }\n}\n",
    "fn score(n: i32, o: Option<i32>) -> i32 {\n    match o {\n        Some(n) => n,\n        None => { if n > 3 { 1 } else { 0 } }\n    }\n}\n",
    "fn score(n: i32, o: Option<i32>) -> i32 {\n    match o {\n        Some(v) => { v }\n        n @ None => 0,\n    };\n    if n > 3 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, xs: &[i32]) -> i32 {\n    for x in xs { let _ = x; }\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, xs: &[i32]) -> i32 {\n    for y in xs.iter().map(|x| x + 1) { let _ = y; }\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, o: Option<i32>) -> i32 {\n    if let Some(x) = o { return x; }\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32, o: Option<i32>) -> i32 {\n    let ok = matches!(o, Some(v) if v > x);\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32) -> i32 {\n    // let x = 3;\n    let s = \"x = 4\";\n    if x >= 10 { 1 } else { 0 }\n}\n",
    "fn score(x: i32) -> i32 {\n    let r\u{e9}sum\u{e9} = 1;\n    if x\u{a0}>= 10 { 1 } else { r\u{e9}sum\u{e9} }\n}\n",
    "fn score(x: i32) -> i32 { if x >= 1 { return 1; } if x >= 2 { return 2; } x }\n",
];

fn corpus() -> Vec<String> {
    HAND_WRITTEN
        .iter()
        .map(|body| body.to_string())
        .chain((0..4_000).map(generated_body))
        .collect()
}

#[test]
fn rebinding_answer_equals_the_replaced_scan_on_every_body_operand_and_line() {
    let mut rebinding = 0usize;
    let mut plain = 0usize;
    for (case, body) in corpus().iter().enumerate() {
        let owner = owner(body);
        let lines = body.lines().count();
        for operand in ["x", "y", "n", "xs", "100"] {
            for predicate_line in owner.start_line.saturating_sub(1)..=owner.start_line + lines {
                let expected = reference_owner_rebinds_operand(&owner, operand, predicate_line);
                assert_eq!(
                    owner_rebinds_operand(&owner, operand, predicate_line),
                    expected,
                    "case {case}, operand {operand}, line {predicate_line}:\n{body}"
                );
                if expected {
                    rebinding += 1;
                } else {
                    plain += 1;
                }
            }
        }
    }
    // The corpus must decide both ways often, or equality proves little.
    assert!(rebinding > 5_000, "{rebinding} rebinding answers");
    assert!(plain > 5_000, "{plain} plain answers");
}

/// The four per-occurrence answers, each against the scan it replaced, at
/// every char boundary: the top-level answer ORs them with other rules, so
/// one wrong answer could hide behind another rule that also holds.
#[test]
fn each_precomputed_answer_equals_its_replaced_scan_at_every_offset() {
    let mut arm_patterns = 0usize;
    let mut closers = 0usize;
    let mut keyword_hits = 0usize;
    for (case, body) in corpus().iter().enumerate() {
        let masked = crate::analysis::language::mask_rust_comments_and_strings(body);
        let scan = RebindingScan::new(&masked);
        for (end, _) in masked.char_indices().chain([(masked.len(), ' ')]) {
            let before = &masked[..end];
            let chars: Vec<char> = before.chars().collect();
            assert_eq!(scan.chars_before(end), chars, "case {case} prefix {end}");
            let arm = reference_in_match_arm_pattern(before);
            assert_eq!(
                in_match_arm_pattern(&chars),
                arm,
                "case {case} arm pattern before {end}:\n{masked}"
            );
            arm_patterns += usize::from(arm);
            for (found, keyword) in [
                (scan.last_for_before(end), "for "),
                (scan.last_let_before(end), "let "),
            ] {
                assert_eq!(
                    found,
                    before.rfind(keyword),
                    "case {case} {keyword:?} {end}"
                );
                keyword_hits += usize::from(found.is_some());
            }
            let closer = reference_first_unmatched_closer(&masked[end..]).map(|at| end + at);
            assert_eq!(
                scan.first_unmatched_closer_from(end),
                closer,
                "case {case} closer from {end}:\n{masked}"
            );
            closers += usize::from(closer.is_some());
        }
        // A prefix shorter than the last one is counted afresh.
        assert_eq!(scan.chars_before(0), [] as [char; 0]);
    }
    assert!(arm_patterns > 500, "{arm_patterns} arm patterns");
    assert!(closers > 5_000, "{closers} closers");
    assert!(keyword_hits > 5_000, "{keyword_hits} keyword hits");
}

/// The forward scan the destructuring rule ran from each occurrence: the
/// offset of the first closing bracket met at depth zero.
fn reference_first_unmatched_closer(after: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in after.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return Some(offset),
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn many_comparisons(count: usize) -> String {
    let mut body = String::from("fn score(x: i32, limit: i32) -> i32 {\n");
    for step in 0..count {
        body.push_str(&format!("    if x >= {step} {{ return {step}; }}\n"));
    }
    body.push_str("    limit\n}\n");
    body
}

/// #11738: one changed comparison per line of a long function asks once per
/// line. The replaced scan read the whole prefix and the whole rest of the
/// body once per occurrence of the operand, so one ask was quadratic in the
/// body and the asks together cubic. The bound is relative to the replaced
/// scan on the same owner, so it holds on a slow or busy runner: an ask near
/// the end of this body must cost under a tenth of a replaced ask there
/// (measured: about a sixtieth in a debug build, and the gap grows with the
/// body). A per-occurrence whole-body read coming back fails it.
#[test]
fn an_ask_on_a_long_owner_costs_a_fraction_of_a_replaced_ask() {
    const COMPARISONS: usize = 800;
    const ASKS: usize = 100;
    const REFERENCE_ASKS: usize = 2;
    let owner = owner(&many_comparisons(COMPARISONS));
    let line = |step: usize| owner.start_line + 1 + step;
    let started = std::time::Instant::now();
    let reference_rebound = (COMPARISONS - REFERENCE_ASKS..COMPARISONS)
        .filter(|step| reference_owner_rebinds_operand(&owner, "x", line(*step)))
        .count();
    let reference = started.elapsed();
    let started = std::time::Instant::now();
    let rebound = (COMPARISONS - ASKS..COMPARISONS)
        .filter(|step| owner_rebinds_operand(&owner, "x", line(*step)))
        .count();
    let asked = started.elapsed();
    assert_eq!(reference_rebound, 0, "no comparison line rebinds `x`");
    assert_eq!(rebound, 0, "no comparison line rebinds `x`");
    // ASKS / REFERENCE_ASKS / 5 = 10: each ask at least ten times cheaper.
    assert!(
        asked < reference * 5,
        "{ASKS} asks took {asked:?}; the replaced scan took {reference:?} for {REFERENCE_ASKS}"
    );
}

/// The same owner still reads a rebinding wherever it sits: the bound above
/// must not come from skipping the body.
#[test]
fn a_rebinding_among_many_comparisons_is_read_from_its_line_on() {
    let body = many_comparisons(400).replace(
        "    if x >= 250 { return 250; }\n",
        "    if x >= 250 { return 250; }\n    (x, _) = (limit, 0);\n",
    );
    let owner = owner(&body);
    let rebinding_line = owner.start_line + 252;
    for (line, expected) in [
        (rebinding_line - 1, false),
        (rebinding_line, true),
        (rebinding_line + 100, true),
    ] {
        assert_eq!(
            owner_rebinds_operand(&owner, "x", line),
            expected,
            "line {line}"
        );
        assert_eq!(
            reference_owner_rebinds_operand(&owner, "x", line),
            expected,
            "reference, line {line}"
        );
    }
}
