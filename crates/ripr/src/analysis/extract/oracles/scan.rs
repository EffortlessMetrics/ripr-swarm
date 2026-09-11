use crate::analysis::facts::OracleFact;
use crate::domain::{OracleKind, OracleStrength};

use super::classify::classify_assertion;
use super::patterns::{
    contains_macro_invocation, contains_named_enum_variant, is_custom_assertion_helper,
    is_mock_expectation_line, is_side_effect_observer_assertion, is_snapshot_assertion,
    is_unwrap_err_bound_error_assertion,
};
use crate::analysis::extract::body_shadows_callee_at_line;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::extract::text::extract_identifier_tokens;

pub(crate) fn extract_assertions(body: &str, start_line: usize) -> Vec<OracleFact> {
    let bound_error_vars = unwrap_err_bound_variables(body);
    // #3709: the dedicated guarded-Result-match scanner owns every recognized
    // `match <direct-call> { Ok.. Err.. }` statement. Skipping its start lines
    // keeps the generic statement joiner from swallowing the whole match block
    // through the `expect_` name sniff, so both adapter paths attribute the
    // shape through one authority.
    let guarded = guarded_result_match_scan(body, start_line);
    let mut out = Vec::new();
    let mut lines = body.lines().enumerate().peekable();
    while let Some((offset, line)) = lines.next() {
        let mut trimmed = line.trim().to_string();
        if guarded.match_start_lines.contains(&(start_line + offset)) {
            continue;
        }
        if is_assertion_line(&trimmed) {
            collect_multiline_assertion(&mut trimmed, &mut lines);
            let mut classification = classify_assertion(&trimmed);
            // RIPR-SPEC-0106: upgrade exact assertions on unwrap_err-bound
            // variables to ExactErrorVariant so the ErrorVariant seam can credit
            // them as a kind-matching discriminator. Constructor-payload
            // equality is classified as WholeObjectEquality before this
            // binding-aware pass, so include it here.
            if matches!(
                classification.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            ) && is_unwrap_err_bound_error_assertion(&trimmed, &bound_error_vars)
            {
                classification.kind = OracleKind::ExactErrorVariant;
                classification.strength = OracleStrength::Strong;
            }
            let observed_tokens = extract_identifier_tokens(&trimmed);
            out.push(OracleFact {
                line: start_line + offset,
                text: trimmed,
                kind: classification.kind,
                strength: classification.strength,
                observed_tokens,
            });
        } else if trimmed.starts_with("if ") || trimmed.starts_with("if(") {
            // #3284: a terminal `if <cond> { return Err(...) }` guard is
            // the manual expansion of a message-carrying assertion. The
            // lexical path recognizes it WITHOUT consuming the block:
            // joining the whole if-body would swallow real assertions
            // inside it (review finding — a main regression in the first
            // draft), so only the condition line plus a one-line peek at
            // the first body statement participates.
            if let Some(oracle) =
                peeked_err_return_guard_oracle(&trimmed, lines.peek(), start_line + offset)
            {
                out.push(oracle);
            }
        }
    }
    out.extend(guarded.oracles);
    out.sort_by(|left, right| left.line.cmp(&right.line).then(left.text.cmp(&right.text)));
    out
}

/// Scan a function body for terminal Err-return guards and credit each
/// as its assertion twin (#3284). Used by the ra parser path, whose
/// assertion facts come from the AST — joining the guard block here
/// cannot swallow sibling assertions.
pub(crate) fn err_return_guard_oracles(body: &str, start_line: usize) -> Vec<OracleFact> {
    let mut out = Vec::new();
    let mut lines = body.lines().enumerate().peekable();
    while let Some((offset, line)) = lines.next() {
        let mut trimmed = line.trim().to_string();
        let guard_line = trimmed.starts_with("if ") || trimmed.starts_with("if(");
        if guard_line
            && let Some(oracle) =
                err_return_guard_oracle(&mut trimmed, &mut lines, start_line + offset)
        {
            out.push(oracle);
        }
    }
    out
}

/// Result of the guarded-Result-match scan (#3709).
#[derive(Debug, Default)]
pub(crate) struct GuardedResultMatchScan {
    /// Oracle facts for recognized matches whose Err guards carry a
    /// recognized discriminator and terminate. Strength carries the guard's
    /// precision: strong for an exact error-variant pin, medium for a
    /// concrete downcast type pin.
    pub(crate) oracles: Vec<OracleFact>,
    /// Start lines of every recognized `match <direct-call> { Ok(..) ..,
    /// Err(..) .. }` statement — including guards too weak to credit — so
    /// callers suppress the generic statement joiners on them and this
    /// scanner stays the single authority for the shape.
    pub(crate) match_start_lines: std::collections::BTreeSet<usize>,
}

/// Scan a test body for guarded Result matches over direct callee results
/// (#3709): `match <callee>(..) { Ok(..) => .., Err(e) => <guard> }`.
///
/// Recognition is bounded and fail-closed (#3709 supported grammar). The
/// scrutinee must be a plain path call (`path::to::callee(args)`) — no
/// method receivers, no trailing `?`, no macro, no chains — so the observed
/// result is the callee's own. The block must hold at least one `Err(` arm
/// and at least one other arm: an `Ok(` arm (classic guarded match) or a
/// catch-all arm (the guarded-routing form
/// `Err(e) if <pin> => {}, rest => <loud failure>` with no `Ok` arm, as in
/// the historical `expect_response` harness). An oracle fact is emitted
/// only when every Err arm carries a recognized discriminator AND is
/// terminal:
/// - a loud failure body (`panic!`, `assert!`, `bail!`, `return`,
///   re-raise, unwrap/expect), or
/// - a guarded accept body (`Err(e) if <pin> => {}`): the guard must carry
///   the pin, the body must be trivial, and every catch-all arm must fail
///   loudly, so a guard miss routes to an observed failure.
///
/// Recognized discriminators: an exact error-variant pin in binding
/// position (the arm pattern proper, a `matches!`/`assert_matches!` guard
/// or body pattern, or a guard equality `==`/`!=` against a variant path)
/// ranks strong; a bare concrete downcast pin ranks medium. Everything
/// else — wildcard arms, no-op arms, opaque or message-only guards, silent
/// catch-alls — emits no oracle and keeps the shape's existing weaker
/// meaning.
///
/// A same-named local `fn` or `let` binding in the test body defeats the
/// oracle (shared #3714 shadow authority): a shadowed name is not the
/// resolved callee, and crediting it would be exactly the token-coincidence
/// false-`exposed` family.
pub(crate) fn guarded_result_match_scan(body: &str, start_line: usize) -> GuardedResultMatchScan {
    // Comments and string contents are erased before scanning so a
    // commented-out match, or an arm body mentioning `panic!` inside a
    // diagnostic string, never becomes evidence. Masking preserves the byte
    // layout, so line attribution stays exact.
    let masked = mask_comments_and_strings(body);
    let mut scan = GuardedResultMatchScan::default();
    // Byte offsets of every line start in the masked body: the parse works
    // on the remainder of the body from a candidate line so a scrutinee and
    // its block that span lines are seen whole.
    let mut line_starts = vec![0usize];
    for (index, byte) in masked.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    for (offset, &line_start) in line_starts.iter().enumerate() {
        let remainder = &masked[line_start..];
        let indent = remainder.len() - remainder.trim_start().len();
        let Some(rest) = remainder[indent..].strip_prefix("match ") else {
            continue;
        };
        let line_number = start_line + offset;
        let Some(match_shape) = parse_guarded_result_match(rest) else {
            continue;
        };
        // Shape recognized: this scanner owns the statement either way.
        scan.match_start_lines.insert(line_number);
        let Some(oracle) = guarded_match_oracle_fact(&match_shape, line_number) else {
            continue;
        };
        // A shadowed callee name is not the resolved helper (#3714 authority,
        // shared via extract::shadow): without this defeat the oracle would
        // bind a local binding's result to the owner's seam. The masked
        // body keeps the check string/comment-safe; the match's own
        // body-relative line is the use site for the positional let rule.
        if body_shadows_callee_at_line(&masked, &match_shape.callee, offset) {
            continue;
        }
        scan.oracles.push(oracle);
    }
    scan
}

struct GuardedMatchShape {
    /// The scrutinee's call path (`helpers::expect_response`).
    path: String,
    /// The scrutinee's final path segment — the name the oracle binds to.
    callee: String,
    /// Whether an `Ok(`-headed arm exists (classic form vs pure routing).
    has_ok_arm: bool,
    /// Err-arm pattern slices, including a trailing `if <guard>` when the
    /// arm carries one.
    err_patterns: Vec<String>,
    /// Trimmed Err-arm bodies, one per Err arm.
    err_bodies: Vec<String>,
    /// Bodies of catch-all arms (neither `Ok(`- nor `Err(`-headed): the
    /// routing targets that make guarded accept arms terminal.
    catch_all_bodies: Vec<String>,
}

/// Parse the scrutinee and arms of a match statement whose source begins
/// `match ` (the prefix is already stripped). Returns `None` for every
/// shape the bounded grammar does not own.
fn parse_guarded_result_match(rest: &str) -> Option<GuardedMatchShape> {
    // Scrutinee: everything before the match block's opening brace at
    // paren/bracket depth zero.
    let mut depth = 0i32;
    let mut brace = None;
    for (index, character) in rest.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '{' if depth == 0 => {
                brace = Some(index);
                break;
            }
            _ => {}
        }
    }
    let brace = brace?;
    let scrutinee = rest[..brace].trim();
    let block = balanced_block(&rest[brace..])?;

    // The scrutinee must be a direct path call: `<path>(<args>)` with the
    // first call's closing paren the final character. Method receivers
    // (`x.callee(..)`), chains (`callee(..).method(..)`), bare `?`
    // propagation, and macros are all rejected — the observed result must be
    // the callee's own.
    let open = scrutinee.find('(')?;
    let path = scrutinee[..open].trim();
    let path_is_plain = !path.is_empty()
        && path.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == ':'
        })
        && !path.contains('!')
        && path.split("::").all(|segment| !segment.is_empty());
    if !path_is_plain {
        return None;
    }
    if !call_ends_at(scrutinee, open) {
        return None;
    }
    let callee = path.rsplit("::").next()?.to_string();

    // Arms: walk arm by arm. A block body (`=> { .. }`) ends at its
    // matching close brace — the trailing comma is optional — while an
    // expression body ends at the next depth-0 comma (string/comment
    // content is masked, and patterns, guards, and non-block bodies can
    // only contain commas inside delimiters). Walking per arm keeps every
    // body free of its neighbors' text, so a loud catch-all can never make
    // a silent Err arm look terminal.
    let bytes = block.as_bytes();
    let mut arms: Vec<(String, String)> = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b',')
        {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let Some(arrow) = top_level_arrow(block, cursor) else {
            break;
        };
        let pattern = block[cursor..arrow].trim();
        let body_start = arrow + 2;
        let body_start = body_start
            + block[body_start..]
                .len()
                .saturating_sub(block[body_start..].trim_start().len());
        let body_end = if bytes.get(body_start) == Some(&b'{') {
            balanced_block(&block[body_start..])
                .map(|inner| body_start + inner.len() + 2)
                .unwrap_or(bytes.len())
        } else {
            top_level_comma(block, body_start).unwrap_or(bytes.len())
        };
        if !pattern.is_empty() {
            arms.push((
                pattern.to_string(),
                block[body_start..body_end].trim().to_string(),
            ));
        }
        cursor = body_end;
    }
    let ok_arm_count = arms
        .iter()
        .filter(|(pattern, _)| pattern.starts_with("Ok("))
        .count();
    let err_arms: Vec<(String, String)> = arms
        .iter()
        .filter(|(pattern, _)| pattern.starts_with("Err("))
        .cloned()
        .collect();
    let catch_all_bodies: Vec<String> = arms
        .iter()
        .filter(|(pattern, _)| !pattern.starts_with("Ok(") && !pattern.starts_with("Err("))
        .map(|(_, body)| body.clone())
        .collect();
    if err_arms.is_empty() || (ok_arm_count == 0 && catch_all_bodies.is_empty()) {
        return None;
    }
    Some(GuardedMatchShape {
        path: path.to_string(),
        callee,
        has_ok_arm: ok_arm_count > 0,
        err_patterns: err_arms
            .iter()
            .map(|(pattern, _)| pattern.clone())
            .collect(),
        err_bodies: err_arms.iter().map(|(_, body)| body.clone()).collect(),
        catch_all_bodies,
    })
}

/// The first `=>` at delimiter depth zero at or after `from`, as a byte
/// offset into `text`.
fn top_level_arrow(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    for position in from..bytes.len() {
        match bytes[position] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 && bytes.get(position + 1) == Some(&b'>') => {
                return Some(position);
            }
            _ => {}
        }
    }
    None
}

/// The first `,` at delimiter depth zero at or after `from`, as a byte
/// offset into `text`.
fn top_level_comma(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    for (position, &byte) in bytes[from..].iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => return Some(from + position),
            _ => {}
        }
    }
    None
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The closing paren of the call opening at `open` is the scrutinee's final
/// character (fail-closed against chains, `?`, and trailing operators).
fn call_ends_at(scrutinee: &str, open: usize) -> bool {
    let mut depth = 0i32;
    for (index, character) in scrutinee[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return open + index == scrutinee.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

/// The balanced `{..}` block starting at `text[0]`, or `None`.
fn balanced_block(text: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (index, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[1..index]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Build the oracle fact for a recognized guarded Result match, or `None`
/// when no Err arm carries a recognized, terminating discriminator.
fn guarded_match_oracle_fact(shape: &GuardedMatchShape, line: usize) -> Option<OracleFact> {
    // Every Err arm must fail loudly on its own text. A guarded accept arm
    // (`Err(e) if <pin> => {}`) is terminal only when it can route: it has
    // a guard, its body is trivial, and every catch-all arm fails loudly.
    // Bodies are sliced per arm segment, so a loud catch-all can never
    // make a silent Err arm look terminal.
    let routed = !shape.catch_all_bodies.is_empty()
        && shape
            .catch_all_bodies
            .iter()
            .all(|body| arm_terminates(body));
    let all_terminal =
        shape
            .err_patterns
            .iter()
            .zip(shape.err_bodies.iter())
            .all(|(pattern, body)| {
                arm_terminates(body)
                    || (split_pattern_guard(pattern).1.is_some()
                        && arm_body_is_trivial(body)
                        && routed)
            });
    if !all_terminal {
        return None;
    }
    // Exact variant pin, per Err arm in order, strongest binding first: the
    // arm pattern proper, a `matches!`/`assert_matches!` pattern in the
    // arm's body or its guard, or a guard equality (`==`/`!=`) against a
    // variant path. Arbitrary path tokens elsewhere in a guard never pin —
    // exactness is not inferred from names. EVERY Err arm must carry a
    // pin: one pinned arm beside an unpinned escape hatch is not an exact
    // result identity, and the match stays unrecognized (fail-closed).
    let mut variant_pin = None;
    let mut all_pinned = true;
    for (pattern_slice, body) in shape.err_patterns.iter().zip(shape.err_bodies.iter()) {
        let (pattern_proper, guard) = split_pattern_guard(pattern_slice);
        let mut pinned = false;
        if contains_named_enum_variant(pattern_proper) {
            if variant_pin.is_none() {
                variant_pin = Some(compact_whitespace(pattern_proper));
            }
            pinned = true;
        }
        let mut candidates = matches_guard_patterns(body);
        if let Some(guard_text) = guard {
            candidates.extend(matches_guard_patterns(guard_text));
        }
        for candidate in candidates {
            if contains_named_enum_variant(&candidate) {
                if variant_pin.is_none() {
                    variant_pin = Some(compact_whitespace(&candidate));
                }
                pinned = true;
                break;
            }
        }
        if let Some(guard) = guard
            && let Some(token) = guard_equality_variant_pin(guard)
        {
            if variant_pin.is_none() {
                variant_pin = Some(token);
            }
            pinned = true;
        }
        if downcast_invocation(body).is_some() {
            pinned = true;
        }
        if !pinned {
            all_pinned = false;
        }
    }
    if !all_pinned {
        return None;
    }
    // Concrete type pin: a downcast to a named error type without a variant.
    let downcast_pin = shape
        .err_bodies
        .iter()
        .find_map(|body| downcast_invocation(body));
    let (pin_text, strength) = if let Some(pin) = variant_pin {
        (pin, OracleStrength::Strong)
    } else if let Some(pin) = downcast_pin {
        (pin, OracleStrength::Medium)
    } else {
        // Wildcard arms, opaque predicates, and message-only diagnostics
        // stay unrecognized: exactness is never inferred from names or
        // payload text (#3709 fail-closed).
        return None;
    };
    let pin_text = truncate_chars(&pin_text, 120);
    let text = if shape.has_ok_arm {
        format!(
            "match {}(..) {{ Ok(..) => .., Err(..) => {pin_text} }}",
            shape.path
        )
    } else {
        // Guarded-routing form: the accept arm routes to a loud catch-all,
        // so the catch-all — not an `Ok` arm — closes the match.
        format!(
            "match {}(..) {{ Err(..) => {pin_text}, _ => .. }}",
            shape.path
        )
    };
    Some(OracleFact {
        line,
        observed_tokens: extract_identifier_tokens(&text),
        kind: OracleKind::GuardedResultMatch,
        strength,
        text,
    })
}

/// Split an arm's pattern slice into `(pattern proper, guard)`: the first
/// whole-word `if` at delimiter depth zero starts the guard (patterns
/// cannot contain `if`, and string/comment content is already masked).
fn split_pattern_guard(pattern_slice: &str) -> (&str, Option<&str>) {
    let bytes = pattern_slice.as_bytes();
    let mut depth = 0i32;
    for position in 0..bytes.len() {
        match bytes[position] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'i' if depth == 0
                && bytes.get(position + 1) == Some(&b'f')
                && (position == 0 || !is_ident_byte(bytes[position - 1]))
                && bytes
                    .get(position + 2)
                    .is_none_or(|next| !is_ident_byte(*next)) =>
            {
                return (
                    pattern_slice[..position].trim_end(),
                    Some(pattern_slice[position + 2..].trim_start()),
                );
            }
            _ => {}
        }
    }
    (pattern_slice, None)
}

/// A guard equality against an error-variant path: the `==`/`!=` operand at
/// delimiter depth zero whose right-hand side is a `Path::Variant` token
/// (`error.kind() == io::ErrorKind::InvalidData`). The variant path is the
/// pin; arbitrary operands and message strings never qualify.
fn guard_equality_variant_pin(guard: &str) -> Option<String> {
    let bytes = guard.as_bytes();
    let mut depth = 0i32;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' | b'!' if depth == 0 && bytes.get(index + 1) == Some(&b'=') => {
                let rhs = guard[index + 2..].trim_start();
                let token: String = rhs
                    .chars()
                    .take_while(|character| {
                        character.is_ascii_alphanumeric() || *character == '_' || *character == ':'
                    })
                    .collect();
                if is_variant_path(&token) {
                    return Some(token);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// A plain path token whose final segment names an enum variant
/// (`ParseError::InvalidData`, `io::ErrorKind::InvalidData`).
fn is_variant_path(token: &str) -> bool {
    !token.is_empty()
        && token.contains("::")
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == ':'
        })
        && token.split("::").all(|segment| !segment.is_empty())
        && token.rsplit("::").next().is_some_and(|segment| {
            segment
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
        })
}

/// Whether a guarded accept body is trivial (`{}`): an empty or unit arm
/// routes via its guard instead of acting.
fn arm_body_is_trivial(body: &str) -> bool {
    matches!(body.trim(), "" | "{}")
}

/// Whether a (masked) Err-arm body ends the failure loudly: a panic,
/// assertion, `bail!`, `return`, `Err(` re-raise, or unwrap/expect. An arm
/// that swallows the error (`Err(e) => {}`, logging only) is a no-op
/// failure arm and never credits (#3709).
fn arm_terminates(body: &str) -> bool {
    [
        "panic!(",
        "assert!",
        "unreachable!(",
        "todo!(",
        "unimplemented!(",
        "bail!(",
        ".unwrap(",
        ".expect(",
        "Err(",
    ]
    .iter()
    .any(|marker| body.contains(marker))
        || contains_whole_word(body, "return")
}

/// Whole-word containment for a keyword (identifier boundaries only).
fn contains_whole_word(text: &str, word: &str) -> bool {
    let mut search = 0usize;
    while let Some(relative) = text[search..].find(word) {
        let start = search + relative;
        let end = start + word.len();
        let before_ok = start == 0
            || !text[..start]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|character| !(character.is_ascii_alphanumeric() || character == '_'));
        if before_ok && after_ok {
            return true;
        }
        search = start + 1;
    }
    false
}

/// The pattern arguments of every `matches!`/`assert_matches!` guard in a
/// (masked) arm body: the slice after the first top-level comma.
fn matches_guard_patterns(body: &str) -> Vec<String> {
    let mut patterns = Vec::new();
    for prefix in ["matches!(", "assert_matches!("] {
        let mut from = 0usize;
        while let Some(relative) = body[from..].find(prefix) {
            let open = from + relative + prefix.len() - 1;
            if let Some(pattern) = slice_after_first_top_level_comma(&body[open..]) {
                patterns.push(pattern);
            }
            from = open + 1;
        }
    }
    patterns
}

/// Inside `text` (which starts at an opening paren), the slice after the
/// first comma at depth 1, up to the matching close paren.
fn slice_after_first_top_level_comma(text: &str) -> Option<String> {
    let mut depth = 0i32;
    let mut comma = None;
    let mut close = None;
    for (index, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            ',' if depth == 1 => comma = Some(index),
            _ => {}
        }
    }
    let comma = comma?;
    let close = close?;
    if comma >= close {
        return None;
    }
    Some(text[comma + 1..close].trim().to_string())
}

/// The `.downcast[_ref|_mut]::<Type>()` invocation text in a (masked) body,
/// through the turbofish's closing `>`.
fn downcast_invocation(body: &str) -> Option<String> {
    for prefix in [".downcast_ref::<", ".downcast_mut::<", ".downcast::<"] {
        if let Some(relative) = body.find(prefix) {
            let start = relative;
            let turbofish_open = start + prefix.len() - 1;
            let mut depth = 0i32;
            for (index, character) in body[turbofish_open..].char_indices() {
                match character {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(body[start..turbofish_open + index + 1].to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Collapse whitespace runs to single spaces (stable fact text for
/// multi-line guards).
fn compact_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(character);
    }
    out
}

/// Truncate to at most `max` characters without splitting one.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(max)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}...", &text[..cut])
}

/// The lexical-path guard oracle: recognize from the condition line plus
/// a single peeked body line, consuming nothing, so assertions inside the
/// guard body stay visible to the outer loop (review finding: the first
/// draft joined the whole block and swallowed them).
fn peeked_err_return_guard_oracle(
    line: &str,
    next: Option<&(usize, &str)>,
    line_number: usize,
) -> Option<OracleFact> {
    let brace = line.find('{')?;
    let head = line[..brace].trim();
    let condition = head.strip_prefix("if")?.trim();
    if condition.is_empty() {
        return None;
    }
    // Same fail-closed gate as the parser path: the Err return must be
    // the body's first statement, so a commented-out or string-embedded
    // `return Err(` never credits.
    let returns_on_line = line[brace..].replace(' ', "").starts_with("{returnErr(");
    let returns_on_next = next.is_some_and(|(_, next)| {
        let compact = next.replace(' ', "");
        compact.starts_with("returnErr(")
    });
    if !returns_on_line && !returns_on_next {
        return None;
    }
    let twin = err_return_guard_assertion(&format!("if {condition} {{ return Err(()) }}"))?;
    let classification = classify_assertion(&twin);
    let observed_tokens = extract_identifier_tokens(condition);
    Some(OracleFact {
        line: line_number,
        text: format!("if {condition} {{ return Err(..) }}"),
        kind: classification.kind,
        strength: classification.strength,
        observed_tokens,
    })
}

/// Join a guard line's continuation and recognize the assertion twin.
fn err_return_guard_oracle<'a, I>(
    trimmed: &mut String,
    lines: &mut std::iter::Peekable<I>,
    line: usize,
) -> Option<OracleFact>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    collect_multiline_assertion(trimmed, lines);
    let equivalent_assertion = err_return_guard_assertion(trimmed)?;
    let classification = classify_assertion(&equivalent_assertion);
    let observed_tokens = extract_identifier_tokens(trimmed);
    Some(OracleFact {
        line,
        text: trimmed.clone(),
        kind: classification.kind,
        strength: classification.strength,
        observed_tokens,
    })
}

/// The assertion twin of a terminal Err-return guard, when the guard's
/// condition can be structurally negated (#3284).
///
/// `if <lhs> != <rhs> { return Err(...) }` is equivalent to
/// `assert!(<lhs> == <rhs>, ...)`; `if !<expr> { ... }` is equivalent to
/// `assert!(<expr>)`; a top-level `==` condition negates to `!=`. Any
/// other condition returns `None` — exactness is never inferred from
/// messages or names.
///
/// Two fail-closed gates: the Err return must be the guard body's first
/// statement (`{returnErr(` after whitespace compaction — a commented-out
/// or string-embedded `return Err(` never credits), and the condition
/// must not carry a top-level `&&`/`||` (a compound's correct negation is
/// not a single assert twin, so it stays unrecognized rather than
/// mis-twinned).
fn err_return_guard_assertion(line: &str) -> Option<String> {
    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    if !compact.starts_with("if") || !compact.contains("{returnErr(") {
        return None;
    }
    let brace = compact.find('{')?;
    let condition = &compact[2..brace];
    if condition.is_empty() || has_top_level_boolean_operator(condition) {
        return None;
    }
    if let Some(inner) = condition
        .strip_prefix("!(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return Some(format!("assert!({inner})"));
    }
    if let Some(inner) = condition.strip_prefix('!') {
        if !inner.starts_with('=') {
            return Some(format!("assert!({inner})"));
        }
        return None;
    }
    // Top-level comparison: split on `==`/`!=` outside any nesting. The
    // condition has no braces here (they end the condition), so a flat
    // scan at depth zero suffices. char_indices keeps every slice on a
    // char boundary — a multibyte comparison operand (`✓`) must not
    // panic the byte-index arithmetic.
    let mut depth = 0usize;
    let mut split = None;
    for (index, character) in condition.char_indices() {
        let top_level_comparison = depth == 0
            && (condition[index..].starts_with("==") || condition[index..].starts_with("!="));
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if top_level_comparison => {
                split = Some(index);
                break;
            }
            _ => {}
        }
    }
    let at = split?;
    let (operator, rest) = if condition[at..].starts_with("!=") {
        ("!=", &condition[at + 2..])
    } else {
        ("==", &condition[at + 2..])
    };
    let lhs = &condition[..at];
    let rhs = rest;
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let negated = if operator == "!=" { "==" } else { "!=" };
    Some(format!("assert!({lhs} {negated} {rhs})"))
}

/// Whether a compacted condition carries a top-level `&&`/`||`: its
/// correct negation is not a single `assert!` twin (#3284 fail-closed).
fn has_top_level_boolean_operator(condition: &str) -> bool {
    let mut depth = 0usize;
    for (index, character) in condition.char_indices() {
        let top_level_operator = depth == 0
            && (condition[index..].starts_with("&&") || condition[index..].starts_with("||"));
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if top_level_operator => return true,
            _ => {}
        }
    }
    false
}

fn collect_multiline_assertion<'a, I>(statement: &mut String, lines: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = (usize, &'a str)>,
{
    while delimiter_depth(statement) > 0 {
        let Some((_, next_line)) = lines.peek() else {
            return;
        };
        statement.push('\n');
        statement.push_str(next_line.trim());
        let _ = lines.next();
    }
}

fn delimiter_depth(text: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*'
                && let Some('/') = chars.peek().copied()
            {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '/' => match chars.peek().copied() {
                Some('/') => {
                    let _ = chars.next();
                    in_line_comment = true;
                }
                Some('*') => {
                    let _ = chars.next();
                    in_block_comment = true;
                }
                _ => {}
            },
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Scan a test body for `let <var> = <expr>.unwrap_err()` and
/// `let <var> = <expr>.expect_err(...)` bindings.
///
/// Returns the set of variable names bound to unwrap_err results so
/// subsequent assertion lines can be recognized as error-variant oracles
/// (RIPR-SPEC-0106, Part A).
pub(crate) fn unwrap_err_bound_variables(body: &str) -> std::collections::BTreeSet<String> {
    let mut vars = std::collections::BTreeSet::new();
    // Split into statement-sized chunks on `;`/`{`/`}` so a
    // `let <ident> = <expr>.unwrap_err()` binding is recognized regardless of
    // source formatting — not only when `let` begins a trimmed source line.
    // Without this, a single-line test body (`fn t() { let e = f().unwrap_err();
    // assert_eq!(e, E::V); }`, e.g. un-rustfmt'd) hides the binding, the
    // assertion is never upgraded to ExactErrorVariant, and the seam carries a
    // contradictory `missing_discriminators` line despite being discriminated.
    for stmt in body.split([';', '{', '}']) {
        if let Some(binding) = let_binding_substring(stmt)
            && let Some(var) = extract_unwrap_err_binding(binding)
        {
            vars.insert(var);
        }
    }
    vars
}

/// Return the slice of `stmt` starting at a `let ` token boundary (preceded by
/// start-of-chunk or a non-identifier char so `let` inside a longer identifier
/// is skipped), or `None` when the chunk has no `let` binding.
fn let_binding_substring(stmt: &str) -> Option<&str> {
    let mut search_from = 0;
    while let Some(rel) = stmt[search_from..].find("let ") {
        let idx = search_from + rel;
        let prev_ok = idx == 0
            || !stmt[..idx]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if prev_ok {
            return Some(stmt[idx..].trim_start());
        }
        search_from = idx + 1;
    }
    None
}

/// If `line` is of the form `let <ident> = <expr>.unwrap_err()` or
/// `let <ident> = <expr>.expect_err(...)`, returns `Some(<ident>)`.
/// Otherwise returns `None`.
fn extract_unwrap_err_binding(line: &str) -> Option<String> {
    // Must start with `let `
    let rest = line.strip_prefix("let ")?.trim_start();
    // Grab the identifier (variable name)
    let ident_end = rest
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_alphanumeric() && *ch != '_')
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    if ident_end == 0 {
        return None;
    }
    let var = &rest[..ident_end];
    let after_var = rest[ident_end..].trim_start();
    // Allow optional type annotation: `let err: MyError = ...`
    let after_colon = if after_var.starts_with(':') {
        // Skip the type annotation up to the `=`
        after_var.find('=').map(|i| &after_var[i..])?
    } else {
        after_var
    };
    let after_eq = after_colon.strip_prefix('=')?.trim_start();
    // The expression must end in `.unwrap_err()` or `.expect_err(<anything>)`
    let expr = after_eq.trim_end_matches(';');
    if ends_with_unwrap_err(expr) || ends_with_expect_err(expr) {
        Some(var.to_string())
    } else {
        None
    }
}

fn ends_with_unwrap_err(expr: &str) -> bool {
    expr.trim_end().ends_with(".unwrap_err()")
}

fn ends_with_expect_err(expr: &str) -> bool {
    // .expect_err("...") — ends with `)` after some `.expect_err(`
    let expr = expr.trim_end();
    if !expr.ends_with(')') {
        return false;
    }
    // Find last `.expect_err(`
    expr.contains(".expect_err(")
}

pub(crate) fn extract_line_scanned_oracles(body: &str, start_line: usize) -> Vec<OracleFact> {
    let mut out = Vec::new();
    let mut lines = body.lines().enumerate().peekable();
    while let Some((offset, line)) = lines.next() {
        let mut statement = line.trim().to_string();
        if !is_line_scanned_oracle(&statement) {
            continue;
        }
        collect_multiline_assertion(&mut statement, &mut lines);
        let classification = classify_assertion(&statement);
        out.push(OracleFact {
            line: start_line + offset,
            text: statement.clone(),
            kind: classification.kind,
            strength: classification.strength,
            observed_tokens: extract_identifier_tokens(&statement),
        });
    }
    out
}

fn is_assertion_line(line: &str) -> bool {
    line.contains("assert!")
        || line.contains("assert_eq!")
        || line.contains("assert_ne!")
        || line.contains("assert_matches!")
        || line.contains("matches!")
        || is_snapshot_assertion(line)
        || is_custom_assertion_helper(line)
        || is_side_effect_observer_assertion(line)
        || line.contains("expect_")
        || line.contains(".expect(")
        || line.contains(".unwrap(")
        || line.contains("should_panic")
        || contains_macro_invocation(line, "ensure!")
}

fn is_line_scanned_oracle(line: &str) -> bool {
    !is_function_signature(line)
        && (is_custom_assertion_helper(line)
            || is_side_effect_observer_assertion(line)
            || is_mock_expectation_line(line)
            || contains_macro_invocation(line, "ensure!"))
}

fn is_function_signature(line: &str) -> bool {
    line.contains('(') && line.contains('{') && line.split_whitespace().any(|token| token == "fn")
}

#[cfg(test)]
mod spec_0106_scan_tests {
    use super::*;
    use crate::domain::OracleKind;

    #[test]
    fn line_scanner_collects_multiline_exact_fallible_oracle() -> Result<(), String> {
        let facts = extract_line_scanned_oracles(
            r#"
                ensure!(
                    state == TerminalState::Pass,
                    "terminal state must match"
                );
            "#,
            10,
        );
        let fact = facts
            .first()
            .ok_or_else(|| "expected multiline ensure oracle".to_string())?;
        if facts.len() != 1
            || fact.kind != OracleKind::ExactValue
            || fact.strength != crate::domain::OracleStrength::Strong
        {
            return Err(format!("unexpected multiline ensure oracle: {facts:?}"));
        }
        Ok(())
    }

    #[test]
    fn line_scanner_does_not_consume_expect_named_function_body() -> Result<(), String> {
        let facts = extract_line_scanned_oracles(
            r#"fn expect_metric_recorded() {
    ensure!(
        state == TerminalState::Pass,
        "terminal state must match"
    );
}"#,
            10,
        );
        let fact = facts
            .first()
            .ok_or_else(|| "expected nested ensure oracle".to_string())?;
        if facts.len() != 1
            || fact.line != 11
            || fact.kind != OracleKind::ExactValue
            || fact.strength != crate::domain::OracleStrength::Strong
            || fact.text.contains("fn expect_metric_recorded")
        {
            return Err(format!("unexpected expect-named scan result: {facts:?}"));
        }
        Ok(())
    }

    // Control 1 (POSITIVE): unwrap_err_bound_variables collects the variable name.
    #[test]
    fn unwrap_err_binding_recognized_and_variable_collected() {
        let body = r"
            let err = compute(-1).unwrap_err();
            assert_eq!(err, CalcError::Negative);
        ";
        let vars = unwrap_err_bound_variables(body);
        assert!(
            vars.contains("err"),
            "bound variable 'err' must be collected from unwrap_err() binding"
        );
    }

    #[test]
    fn expect_err_binding_also_recognized() {
        let body = r#"
            let err = compute(-1).expect_err("should fail");
            assert_eq!(err, CalcError::Negative);
        "#;
        let vars = unwrap_err_bound_variables(body);
        assert!(
            vars.contains("err"),
            "bound variable from expect_err() must also be collected"
        );
    }

    // Control 1 end-to-end: extract_assertions upgrades the oracle kind to ExactErrorVariant.
    #[test]
    fn extract_assertions_upgrades_unwrap_err_variant_to_exact_error_variant() {
        let body = r"
            let err = compute(-1).unwrap_err();
            assert_eq!(err, CalcError::Negative);
        ";
        let facts = extract_assertions(body, 1);
        let got = facts.iter().find(|f| f.text.contains("assert_eq!"));
        assert!(got.is_some(), "must find the assert_eq! fact");
        assert_eq!(
            got.map(|f| f.kind.clone()),
            Some(OracleKind::ExactErrorVariant),
            "assert_eq!(err, CalcError::Negative) on unwrap_err binding must be ExactErrorVariant"
        );
    }

    #[test]
    fn extract_assertions_upgrades_expect_err_constructor_payload_equality_to_exact_error_variant()
    -> Result<(), String> {
        let body = r#"
            let duplicate_id = "duplicate";
            let err = validate(duplicate_id)
                .expect_err("duplicate should fail");
            assert_eq!(
                err,
                CargoAllowError::new(format!("duplicate allow id `{}`", duplicate_id))
            );
        "#;
        let facts = extract_assertions(body, 1);
        let got = facts
            .iter()
            .find(|fact| fact.text.contains("CargoAllowError::new"))
            .ok_or_else(|| "must find constructor-payload equality assertion".to_string())?;
        if got.kind != OracleKind::ExactErrorVariant {
            return Err(format!(
                "constructor-payload equality on expect_err binding must be ExactErrorVariant, got {:?}",
                got.kind
            ));
        }
        Ok(())
    }

    #[test]
    fn extract_assertions_does_not_collect_after_line_comment_delimiter() -> Result<(), String> {
        let body = r#"
            assert!(first_observed); // (
            assert!(second_observed);
        "#;
        let facts = extract_assertions(body, 1);
        if facts.len() != 2 {
            return Err(format!(
                "line comment delimiter should not join assertions: {facts:?}"
            ));
        }
        if facts[0].text.contains("second_observed") {
            return Err(format!(
                "first assertion should not consume the second assertion: {:?}",
                facts[0]
            ));
        }
        Ok(())
    }

    // Formatting robustness: a single-line test body (`let` not at the start of
    // a source line, sitting mid-line after `{`) must still expose the binding.
    // Without statement-wise splitting the binding is hidden, the assertion is
    // never upgraded, and a discriminated error_path seam carries a
    // contradictory `missing_discriminators` line.
    #[test]
    fn unwrap_err_binding_detected_in_single_line_body() {
        let body =
            "fn t() { let err = compute(-1).unwrap_err(); assert_eq!(err, CalcError::Negative); }";
        let vars = unwrap_err_bound_variables(body);
        assert!(
            vars.contains("err"),
            "binding in a single-line body must be collected, not only `let`-at-line-start"
        );
    }

    // End-to-end (lexical path): a brace-prefixed binding (`let` sharing the
    // opening line of the fn body) is now detected, so the own-line assertion
    // upgrades to ExactErrorVariant. Isolates the binding-detection fix from the
    // separate line-classifier `{`-confusion that a fully single-line body trips.
    #[test]
    fn extract_assertions_upgrades_brace_prefixed_unwrap_err_variant() {
        let body = "fn t() { let err = compute(-1).unwrap_err();\n            assert_eq!(err, CalcError::Negative);\n}";
        let facts = extract_assertions(body, 1);
        let got = facts.iter().find(|f| f.text.contains("assert_eq!"));
        assert_eq!(
            got.map(|f| f.kind.clone()),
            Some(OracleKind::ExactErrorVariant),
            "brace-prefixed unwrap_err variant assertion must upgrade to ExactErrorVariant"
        );
    }

    #[test]
    fn multiple_bindings_separated_by_braces_all_collected() {
        let body = "fn t() { if c { let a = f().unwrap_err(); } let b = g().expect_err(\"x\"); }";
        let vars = unwrap_err_bound_variables(body);
        assert!(vars.contains("a") && vars.contains("b"), "got {vars:?}");
    }

    // Control 3 (GENERIC): generic assertion stays at its original classification.
    #[test]
    fn generic_assertion_on_unwrap_err_var_not_upgraded() {
        let body = r#"
            let err = compute(-1).unwrap_err();
            assert!(err.to_string().contains("error"));
        "#;
        let facts = extract_assertions(body, 1);
        let got = facts.iter().find(|f| f.text.contains("assert!"));
        assert!(got.is_some(), "must find the assert! fact");
        assert_ne!(
            got.map(|f| f.kind.clone()),
            Some(OracleKind::ExactErrorVariant),
            "generic assertion without variant token must not be upgraded to ExactErrorVariant"
        );
    }
}

#[cfg(test)]
mod guarded_result_match_tests {
    use super::guarded_result_match_scan;
    use crate::domain::{OracleKind, OracleStrength};

    const POSITIVE: &str = r#"
#[test]
fn validates_ready_response() {
    let mut cursor = std::io::Cursor::new("ready");
    match expect_response(&mut cursor, "ready") {
        Ok(response) => {
            assert_eq!(response.id, "ready");
            assert_eq!(response.body, "payload");
        }
        Err(error) => {
            if !matches!(
                error.downcast_ref::<ParseError>(),
                Some(ParseError::InvalidData { .. })
            ) {
                panic!("unexpected error variant: {error}");
            }
        }
    }
}
"#;

    #[test]
    fn positive_shape_credits_strong_variant_pin_with_callee_binding() {
        let scan = guarded_result_match_scan(POSITIVE, 1);
        assert_eq!(scan.oracles.len(), 1, "got {:?}", scan.oracles);
        let fact = &scan.oracles[0];
        assert_eq!(fact.kind, OracleKind::GuardedResultMatch);
        assert_eq!(fact.strength, OracleStrength::Strong);
        assert_eq!(fact.line, 5, "the match statement's own line");
        assert!(
            fact.text.contains("match expect_response(..)"),
            "the scrutinee callee is the binding token: {}",
            fact.text
        );
        assert!(
            fact.text.contains("Some(ParseError::InvalidData{..})")
                || fact.text.contains("Some(ParseError::InvalidData { .. })"),
            "the Err-arm variant pin is named: {}",
            fact.text
        );
        assert!(
            scan.match_start_lines.contains(&5),
            "the match start line is suppressed for the generic joiners"
        );
    }

    #[test]
    fn downcast_without_variant_credits_medium_type_pin() {
        let body = r#"
#[test]
fn checks_error_type() {
    match parse(input) {
        Ok(value) => { assert_eq!(value, 1); }
        Err(error) => {
            if error.downcast_ref::<ParseError>().is_none() {
                panic!("wrong error type: {error}");
            }
        }
    }
}
"#;
        let scan = guarded_result_match_scan(body, 1);
        assert_eq!(scan.oracles.len(), 1, "got {:?}", scan.oracles);
        assert_eq!(scan.oracles[0].kind, OracleKind::GuardedResultMatch);
        assert_eq!(scan.oracles[0].strength, OracleStrength::Medium);
        assert!(scan.oracles[0].text.contains(".downcast_ref::<ParseError>"));
    }

    #[test]
    fn err_pattern_pin_credits_without_matches_macro() {
        let body = r#"
#[test]
fn pattern_pin() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(ParseError::InvalidData { .. }) => panic!("invalid data"),
    }
}
"#;
        let scan = guarded_result_match_scan(body, 1);
        assert_eq!(scan.oracles.len(), 1, "got {:?}", scan.oracles);
        assert_eq!(scan.oracles[0].strength, OracleStrength::Strong);
    }

    #[test]
    fn wildcard_or_noop_err_arms_never_credit() {
        let wildcard = r#"
#[test]
fn swallows_wildcard() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(_) => panic!("failed"),
    }
}
"#;
        let scan = guarded_result_match_scan(wildcard, 1);
        assert!(
            scan.oracles.is_empty(),
            "a wildcard Err arm has no discriminator: {:?}",
            scan.oracles
        );
        let noop = r#"
#[test]
fn swallows_error() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => {
            let _typed = error.downcast_ref::<ParseError>();
        }
    }
}
"#;
        let scan = guarded_result_match_scan(noop, 1);
        assert!(
            scan.oracles.is_empty(),
            "a non-terminal Err arm never credits: {:?}",
            scan.oracles
        );
        // The shape is still owned by the scanner (suppressed), even without a fact.
        assert!(!scan.match_start_lines.is_empty());
    }

    #[test]
    fn wrong_error_predicate_and_message_only_diagnostics_do_not_pin() {
        let opaque = r#"
#[test]
fn opaque_predicate() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => {
            if !error.to_string().contains("ParseError::InvalidData") {
                panic!("bad error");
            }
        }
    }
}
"#;
        let scan = guarded_result_match_scan(opaque, 1);
        assert!(
            scan.oracles.is_empty(),
            "a message-only variant mention never pins: {:?}",
            scan.oracles
        );
    }

    #[test]
    fn non_direct_scrutinees_are_not_guarded_result_matches() {
        let let_binding = r#"
#[test]
fn different_result_binding() {
    let result = expect_response(&mut cursor, "ready");
    match result {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("bad error"),
    }
}
"#;
        let scan = guarded_result_match_scan(let_binding, 1);
        assert!(
            scan.oracles.is_empty() && scan.match_start_lines.is_empty(),
            "a variable scrutinee is not a direct callee result: {:?}",
            scan.oracles
        );
        let method_call = r#"
#[test]
fn method_scrutinee() {
    match client.expect_response(&mut cursor) {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("bad error"),
    }
}
"#;
        let scan = guarded_result_match_scan(method_call, 1);
        assert!(scan.oracles.is_empty(), "{:?}", scan.oracles);
        let bare_question = r#"
#[test]
fn propagation_scrutinee() -> Result<(), Box<dyn std::error::Error>> {
    match expect_response(&mut cursor, "ready")? {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("bad error"),
    }
}
"#;
        let scan = guarded_result_match_scan(bare_question, 1);
        assert!(scan.oracles.is_empty(), "{:?}", scan.oracles);
        let chained = r#"
#[test]
fn chained_scrutinee() {
    match expect_response(&mut cursor).map(into_response) {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("bad error"),
    }
}
"#;
        let scan = guarded_result_match_scan(chained, 1);
        assert!(scan.oracles.is_empty(), "{:?}", scan.oracles);
    }

    #[test]
    fn shadowed_callee_does_not_credit() {
        let body = r#"
#[test]
fn local_shadow() {
    fn expect_response(reader: &mut Cursor, id: &str) -> Result<u32, ParseError> {
        Ok(1)
    }
    match expect_response(&mut cursor, "ready") {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("{}", error),
    }
}
"#;
        let scan = guarded_result_match_scan(body, 1);
        assert!(
            scan.oracles.is_empty(),
            "a local fn shadowing the callee must not credit: {:?}",
            scan.oracles
        );
        let let_shadow = r#"
#[test]
fn let_shadow() {
    let expect_response = read_only_expect_response;
    match expect_response(&mut cursor, "ready") {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => panic!("{}", error),
    }
}
"#;
        let scan = guarded_result_match_scan(let_shadow, 1);
        assert!(
            scan.oracles.is_empty(),
            "a let binding shadowing the callee must not credit: {:?}",
            scan.oracles
        );
    }

    #[test]
    fn option_and_single_arm_matches_are_not_guarded_result_matches() {
        let option_match = r#"
#[test]
fn option_match() {
    match parse(input) {
        Some(value) => assert_eq!(value, 1),
        None => panic!("none"),
    }
}
"#;
        let scan = guarded_result_match_scan(option_match, 1);
        assert!(scan.oracles.is_empty() && scan.match_start_lines.is_empty());
    }

    #[test]
    fn commented_out_match_and_string_mentions_never_credit() {
        let commented = r#"
#[test]
fn commented_out() {
    // match expect_response(&mut cursor) {
    //     Ok(v) => assert_eq!(v, 1),
    //     Err(e) => panic!("{}", e),
    // }
    let note = "match expect_response(x) { Ok(..) => .., Err(..) => panic!(x) }";
    assert_eq!(parse(input), 1);
}
"#;
        let scan = guarded_result_match_scan(commented, 1);
        assert!(
            scan.oracles.is_empty(),
            "comment/string mentions never become evidence: {:?}",
            scan.oracles
        );
    }

    #[test]
    fn qualified_path_scrutinee_binds_the_final_segment() {
        let body = r#"
#[test]
fn qualified_path() {
    match helpers::expect_response(&mut cursor, "ready") {
        Ok(value) => assert_eq!(value, 1),
        Err(ParseError::InvalidData) => panic!("invalid data"),
    }
}
"#;
        let scan = guarded_result_match_scan(body, 1);
        assert_eq!(scan.oracles.len(), 1, "{:?}", scan.oracles);
        assert!(
            scan.oracles[0]
                .text
                .contains("match helpers::expect_response(..)")
        );
    }

    // The historical `expect_response` harness shape (#13162/#3709): a
    // guarded accept arm routes to a loud catch-all; there is no `Ok` arm.
    #[test]
    fn guarded_routing_form_without_ok_arm_credits_strong() {
        let body = r#"
#[test]
fn fixture_requires_exact_response_identity() {
    match expect_response(&invalid, &id, "workspace/configuration") {
        Err(error)
            if error.kind() == io::ErrorKind::InvalidData
                && error.to_string().contains("workspace/configuration") => {}
        result => bail!(
            "fixture accepted an invalid response or lost method context: {result:?}"
        ),
    }
}
"#;
        let scan = guarded_result_match_scan(body, 1);
        assert_eq!(scan.oracles.len(), 1, "{:?}", scan.oracles);
        let fact = &scan.oracles[0];
        assert_eq!(fact.kind, OracleKind::GuardedResultMatch);
        assert_eq!(fact.strength, OracleStrength::Strong);
        assert!(
            fact.text.contains("io::ErrorKind::InvalidData"),
            "the guard equality pin is named: {}",
            fact.text
        );
        assert!(
            fact.text.contains("_ => .."),
            "the routing form is disclosed without a fabricated Ok arm: {}",
            fact.text
        );
        assert!(
            scan.match_start_lines.contains(&4),
            "the routing match start line is suppressed for the generic joiners"
        );
    }

    #[test]
    fn routing_form_falsifiers_stay_non_crediting() {
        // A silent catch-all cannot observe a guard miss: routing to it
        // swallows the failure.
        let silent_catch_all = r#"
#[test]
fn routes_to_silence() {
    match expect_response(&invalid, &id, "m") {
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {}
        result => {
            eprintln!("ignored: {result:?}");
        }
    }
}
"#;
        let scan = guarded_result_match_scan(silent_catch_all, 1);
        assert!(
            scan.oracles.is_empty(),
            "a silent catch-all routing target never credits: {:?}",
            scan.oracles
        );
        // A trivial Err body without a guard does not route (the arm ends
        // the match) — the swallowed error is never observed.
        let unguarded_trivial = r#"
#[test]
fn unguarded_trivial() {
    match expect_response(&invalid, &id, "m") {
        Err(error) => {}
        result => bail!("unexpected result: {result:?}"),
    }
}
"#;
        let scan = guarded_result_match_scan(unguarded_trivial, 1);
        assert!(
            scan.oracles.is_empty(),
            "an unguarded trivial Err arm never credits: {:?}",
            scan.oracles
        );
        // A guard equality against a message token (masked) or a plain
        // binding never pins, even when the shape is otherwise loud.
        let message_equality = r#"
#[test]
fn message_only_equality() {
    match expect_response(&invalid, &id, "m") {
        Err(error) if error.to_string() == "ParseError::InvalidData" => {}
        result => bail!("unexpected result: {result:?}"),
    }
}
"#;
        let scan = guarded_result_match_scan(message_equality, 1);
        assert!(
            scan.oracles.is_empty(),
            "an equality against a string literal never pins: {:?}",
            scan.oracles
        );
    }

    #[test]
    fn loud_body_err_arm_with_neighbor_pollution_stays_honest() {
        // The loud catch-all text belongs to the catch-all arm only: a
        // silent Err arm must not inherit its `bail!` as a failure action.
        let silent_err_loud_neighbor = r#"
#[test]
fn silent_err_loud_neighbor() {
    match expect_response(&invalid, &id, "m") {
        Err(error) => {
            log_failure(error);
        }
        result => bail!("unexpected result: {result:?}"),
    }
}
"#;
        let scan = guarded_result_match_scan(silent_err_loud_neighbor, 1);
        assert!(
            scan.oracles.is_empty(),
            "a silent Err arm next to a loud catch-all never credits: {:?}",
            scan.oracles
        );
    }

    #[test]
    fn multi_err_arm_requires_every_arm_pinned_and_terminal() {
        let mixed = r#"
#[test]
fn one_pin_one_wildcard() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(ParseError::InvalidData) => panic!("invalid data"),
        Err(other) => panic!("other: {other}"),
    }
}
"#;
        // The wildcard arm is loud but unpinned: the shape stays
        // unrecognized (every Err arm must pin), fail-closed.
        let scan = guarded_result_match_scan(mixed, 1);
        assert!(
            scan.oracles.is_empty(),
            "an unpinned (wildcard-headed) Err arm blocks the oracle: {:?}",
            scan.oracles
        );
    }
}

#[cfg(test)]
mod err_guard_parity_tests {
    use super::extract_assertions;
    use crate::domain::{OracleKind, OracleStrength};

    #[test]
    fn err_return_guard_is_an_oracle_equal_to_its_assert_twin() {
        // #3284: `if actual != expected { return Err(...) }` in a test body
        // is the manual expansion of a message-carrying assertion; leaving
        // it unrecognized made equivalent harness forms change the
        // credited oracle and, through it, the production gap accounting.
        let guard = extract_assertions(
            "if actual != expected {\n    return Err(anyhow!(\"actual={actual:?}\"));\n}\nOk(())\n",
            3,
        );
        assert!(
            guard
                .iter()
                .any(|fact| fact.kind == OracleKind::RelationalCheck),
            "the Err-return guard must be recognized: {guard:?}"
        );
        let twin = extract_assertions("assert!(actual == expected, \"actual={actual:?}\");\n", 3);
        let guard_best = guard
            .iter()
            .find(|fact| fact.kind == OracleKind::RelationalCheck)
            .map(|fact| (fact.kind.clone(), fact.strength.clone()));
        let twin_best = twin
            .iter()
            .find(|fact| fact.kind == OracleKind::RelationalCheck)
            .map(|fact| (fact.kind.clone(), fact.strength.clone()));
        assert_eq!(
            guard_best, twin_best,
            "the guard and its assert! twin must carry identical oracle meaning"
        );
        assert_eq!(
            guard_best.map(|(_, strength)| strength),
            Some(OracleStrength::Weak)
        );
    }

    #[test]
    fn comment_only_or_compound_guards_never_credit() {
        // #3284 fail-closed gates: an Err return that is not the body's
        // first statement (commented out or in a string), and compound
        // conditions whose negation is not a single assert twin, produce
        // no oracle.
        let commented = extract_assertions(
            "if actual != expected { /* previously: return Err(\"m\") */ eprintln!(\"m\"); }
",
            3,
        );
        assert!(
            commented.is_empty(),
            "commented-out Err must not credit: {commented:?}"
        );
        let compound = extract_assertions(
            "if a != b || c != d { return Err(\"x\"); }
",
            3,
        );
        assert!(
            compound.is_empty(),
            "compound guard must stay unrecognized: {compound:?}"
        );
    }

    #[test]
    fn guard_conditions_without_structural_equivalence_stay_unrecognized() {
        // No inference from messages or opaque conditions: a guard whose
        // condition cannot be structurally negated into an assertion
        // contributes no oracle fact.
        let opaque = extract_assertions(
            "if !matches!(result, Expected::Good(_)) {\n    return Err(anyhow!(\"bad\"));\n}\n",
            3,
        );
        assert!(
            !opaque
                .iter()
                .any(|fact| fact.kind == OracleKind::RelationalCheck
                    || fact.kind == OracleKind::ExactValue),
            "opaque guard conditions must not be guessed into oracles: {opaque:?}"
        );
    }
}

#[cfg(test)]
mod multibyte_guard_tests {
    use super::extract_assertions;
    use crate::domain::OracleKind;

    #[test]
    fn multibyte_comparison_operands_do_not_panic() {
        // #3284: the Linux CI panic — a guard comparing a multibyte
        // operand must classify (or reject) without byte-slicing inside
        // a character boundary.
        let guard = extract_assertions(
            "if actual != \"\u{2713}\" {\n    return Err(\"mismatch\");\n}\n",
            3,
        );
        assert!(
            guard
                .iter()
                .any(|fact| fact.kind == OracleKind::RelationalCheck),
            "multibyte guard must credit: {guard:?}"
        );
    }
}
