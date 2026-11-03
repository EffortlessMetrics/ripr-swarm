//! Sink-alignment helpers for conservative H2 classification (#3221).

use super::super::boundary_input::BoundaryInput;
use super::super::*;

/// Evidence that a related test's oracle observes the changed sink (H2).
///
/// Returned by `sink_aligned_observation` when sink alignment is ESTABLISHED, so the
/// change can be classified `Exposed` (already-observed). Carries the
/// human-readable pieces for the `perl_already_discriminated:` evidence line.
#[derive(Clone, Debug)]
pub(crate) struct SinkAlignedObservation {
    pub(crate) test_name: String,
    pub(crate) observed_sink: String,
    pub(crate) oracle_shape: String,
    /// RIPR-SPEC-0235 rule 22: whether a changed ordering comparison has a
    /// literal threshold input in this oracle's owner call.
    pub(crate) boundary_input: BoundaryInput,
}

/// Whether this row's own evidence earned the finding's weak exposure (a
/// direct, reachable owner call whose linked oracle is strong, exact, in the
/// same test and targets the changed owner) while the shared sink-alignment
/// check establishes no alignment for it. Unequal sink text is only
/// unconfirmed, never proof that the oracle observes another sink (#5498).
pub(crate) fn perl_row_observation_unconfirmed(
    packet: &PerlFactPacket,
    change: &ChangeFact,
    ev: &PerlRelatedTestEvidence,
) -> bool {
    ev.relation_kind == RelationKind::DirectOwnerCall
        && ev.reachability_hint == ReachabilityHint::Reachable
        && ev.class == ExposureClass::WeaklyExposed
        && sink_aligned_observation(std::slice::from_ref(ev), change, packet).is_none()
}

/// H2 (Campaign 31): determine whether a related test's oracle observes the
/// *specific changed sink*, not merely the same owner.
///
/// Returns `Some` when ALL of the following hold for at least one related
/// evidence:
/// - the relation is a `DirectOwnerCall` with `Reachable` reachability (advisory
///   relations can never prove observation);
/// - the linked oracle belongs to the relation's test, is strong-exact and
///   targets the changed owner;
/// - the oracle carries a non-empty `observed_sink`;
/// - the change carries a non-empty `changed_observable`;
/// - `observed_sink` and `changed_observable` refer to the same sink.
///
/// The last check is the discrimination gate. Token-substring
/// matching is the recurring false-`exposed` family, so this is conservative:
/// alignment is accepted when the two expressions are exactly equal, OR when
/// one is a recognized trivial aliasing of the other (e.g. the observable is
/// `return <expr>` and the sink is `<expr>`). Any uncertainty fails closed to
/// `WeaklyExposed` (caller's responsibility — this returns `None`).
pub(crate) fn sink_aligned_observation(
    related_evidence: &[PerlRelatedTestEvidence],
    change: &ChangeFact,
    packet: &PerlFactPacket,
) -> Option<SinkAlignedObservation> {
    // The change's observable — must be present and non-empty.
    let changed_observable = change.changed_observable.as_deref()?.trim();
    if changed_observable.is_empty() {
        return None;
    }
    // Normalize: strip a leading "return " so `return $x` aligns to `$x`.
    let normalized_observable = changed_observable
        .strip_prefix("return ")
        .unwrap_or(changed_observable)
        .trim();
    // RIPR-SPEC-0235 rule 19: an exact oracle on one input observes what a
    // pattern does with that input, not which inputs the pattern accepts, so
    // a changed pattern is never "already observed" by equal sink text.
    if matches_a_pattern(changed_observable) {
        return None;
    }

    // Rule 22: the first aligned oracle that is not an unestablished ordering
    // comparison wins; otherwise the first aligned oracle is kept so the caller
    // can tell "aligned but input unestablished" from "not aligned".
    let mut unestablished: Option<SinkAlignedObservation> = None;
    for ev in related_evidence {
        // Only a direct owner call with positive reachability can prove
        // observation. Advisory relations (file_proximity, package_reference,
        // ...) never can.
        if !ev.relation_kind.supports_positive_reachability()
            || ev.reachability_hint != ReachabilityHint::Reachable
        {
            continue;
        }
        let Some(oracle_id) = ev.oracle_id.as_deref() else {
            continue;
        };
        let oracle = packet.oracle(oracle_id)?;
        // RIPR-SPEC-0235 rule 5: the oracle must belong to the relation's
        // test, the same binding `classify_related_relation` uses.
        if oracle.test_id != ev.test_id {
            continue;
        }
        // Strong-exact oracle targeting the changed owner.
        if !oracle.is_strong_exact() {
            continue;
        }
        // RIPR-SPEC-0235 rule 16 matcher grading: a graded exception
        // assertion pins that an error is raised, not which boundary raises
        // it, so equal sink text never establishes observation.
        if packet.grades_oracle_matchers() && oracle.kind == OracleKind::ExceptionObserver {
            continue;
        }
        if oracle.target_owner_id.as_deref() != Some(change.owner_id.as_str()) {
            continue;
        }
        let Some(observed_sink) = oracle.observed_sink.as_deref() else {
            continue;
        };
        let observed_sink = observed_sink.trim();
        if observed_sink.is_empty() {
            continue;
        }
        // Sink alignment: the observed sink must refer to the same value as the
        // changed observable. Exact equality is the safe bar; the normalized
        // observable covers the common `return <expr>` form. Anything else
        // fails closed (returns None at the end of the loop).
        if sink_text_eq(observed_sink, changed_observable)
            || sink_text_eq(observed_sink, normalized_observable)
        {
            // RIPR-SPEC-0235 rule 23 (#7851): equal sink text is not
            // observation of a changed returned-list element. A scalar
            // `is` sees only the last value; a self-comparison pins nothing.
            if list_element_oracle_does_not_pin(oracle, changed_observable) {
                continue;
            }
            // Any strong exact oracle of the same test may supply the
            // threshold input: the producer links only some oracles to the
            // owner, and each is an exact-value assertion on that test's call.
            let boundary_input = boundary_input::boundary_input_for(
                changed_observable,
                &change.owner_id,
                packet
                    .oracles
                    .iter()
                    .filter(|candidate| {
                        candidate.test_id == ev.test_id && candidate.is_strong_exact()
                    })
                    .filter_map(|candidate| candidate.expression.as_deref()),
            );
            let aligned = SinkAlignedObservation {
                test_name: ev.test_name.clone(),
                observed_sink: observed_sink.to_string(),
                oracle_shape: ev
                    .oracle_shape
                    .clone()
                    .unwrap_or_else(|| change.behavior_hint.default_assertion_shape().to_string()),
                boundary_input,
            };
            if boundary_input == BoundaryInput::Unestablished {
                unestablished.get_or_insert(aligned);
            } else {
                return Some(aligned);
            }
        }
    }
    unestablished
}

/// Whether `text` applies a pattern: a `=~` or `!~` binding, or a `qr`, `m`,
/// `s`, `tr`, `y` or `split` word that opens a delimited pattern. It errs
/// toward true (a hash key `s => 1` is not skipped specially beyond `=>`):
/// the only effect of a false positive is that a sink stays `weakly_exposed`
/// instead of `exposed`.
fn matches_a_pattern(text: &str) -> bool {
    if text.contains("=~") || text.contains("!~") {
        return true;
    }
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        if !is_word(chars[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        let word: String = chars[start..end].iter().collect();
        // A sigil or `->` makes the word a variable or method, not an operator.
        let named = start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&' | '>');
        if !named && matches!(word.as_str(), "qr" | "m" | "s" | "tr" | "y" | "split") {
            let mut next = end;
            if word == "split" {
                while next < chars.len() && chars[next].is_whitespace() {
                    next += 1;
                }
            }
            let fat_comma = chars[next..].iter().collect::<String>().starts_with("=>");
            if !fat_comma
                && chars.get(next).is_some_and(|c| {
                    matches!(
                        c,
                        '/' | '{' | '[' | '(' | '<' | '|' | '!' | '#' | '\'' | '"' | ','
                    )
                })
            {
                return true;
            }
        }
        start = end;
    }
    false
}

/// Whether `changed_observable` names a numbered capture such as `$1` or
/// `$2`. #7851 false promotions pin list-element mutations to those captures;
/// scalar-return observables such as `$result` keep scalar `is` alignment.
fn changed_observable_is_returned_list_element(text: &str) -> bool {
    let Some(digits) = text.trim().strip_prefix('$') else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// RIPR-SPEC-0235 rule 23 (#7851): whether this oracle cannot observe a
/// changed returned-list element even when `observed_sink` equals
/// `changed_observable`. The producer often copies the whole changed
/// expression onto every strong oracle (equal-fact promotion). A Test::More
/// `is` whose got operand starts with `scalar` inspects only the last value
/// of an explicit list; an `is_deeply` (or `is`) whose first two operands
/// are the same text is a tautology. Both stay unaligned so a literal-list
/// `is_deeply` remains the discriminator. A sink whose text itself names
/// `scalar` or `wantarray` still aligns: that is the scalar arm of a
/// context-sensitive return (`perl-idiom-wantarray-exact`), not a list
/// element. This is a bounded lexical read of packet strings, like rule 17;
/// it does not parse Perl.
fn list_element_oracle_does_not_pin(oracle: &OracleFact, changed_observable: &str) -> bool {
    if !changed_observable_is_returned_list_element(changed_observable) {
        return false;
    }
    let Some(expression) = oracle.expression.as_deref() else {
        return false;
    };
    match assertion_form(expression) {
        Some(AssertionForm::ExactIs) => {
            is_self_comparison_expression(expression)
                || (got_starts_with_scalar(expression)
                    && !sink_names_scalar_or_wantarray(
                        oracle.observed_sink.as_deref().unwrap_or(""),
                    ))
        }
        Some(AssertionForm::IsDeeply) => is_self_comparison_expression(expression),
        None => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssertionForm {
    ExactIs,
    IsDeeply,
}

fn strip_test_qualifiers(expression: &str) -> &str {
    let call = expression.trim_start();
    call.strip_prefix("Test::More::")
        .or_else(|| call.strip_prefix("Test2::V0::"))
        .or_else(|| call.strip_prefix("Test2::V1::"))
        .unwrap_or(call)
}

fn ident_at(text: &str, name: &str) -> bool {
    text.starts_with(name)
        && text.get(name.len()..).is_none_or(|rest| {
            rest.is_empty()
                || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
        })
}

fn assertion_form(expression: &str) -> Option<AssertionForm> {
    let call = strip_test_qualifiers(expression);
    if ident_at(call, "is_deeply") {
        Some(AssertionForm::IsDeeply)
    } else if ident_at(call, "is") {
        Some(AssertionForm::ExactIs)
    } else {
        None
    }
}

fn leading_ident_len(text: &str) -> usize {
    text.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .count()
}

fn call_args_inside(expression: &str) -> Option<&str> {
    let call = strip_test_qualifiers(expression);
    let name_len = leading_ident_len(call);
    if name_len == 0 {
        return None;
    }
    let rest = call.get(name_len..)?.trim_start();
    let rest = rest.strip_prefix('(')?;
    let chars: Vec<char> = rest.chars().collect();
    let mut depth = 1i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..char_index_to_byte(rest, i)]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn char_index_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn got_starts_with_scalar(expression: &str) -> bool {
    let Some(inside) = call_args_inside(expression) else {
        return false;
    };
    ident_at(inside.trim_start(), "scalar")
}

/// Whether `text` names `scalar` or `wantarray` as a code word, not a
/// variable, method or longer identifier. Used so a scalar `is` still
/// aligns when the equal sink is the scalar arm of a `wantarray` return.
fn sink_names_scalar_or_wantarray(text: &str) -> bool {
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        if !is_word(chars[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        let word: String = chars[start..end].iter().collect();
        let named = start > 0 && matches!(chars[start - 1], '$' | '@' | '%' | '&' | '>');
        if !named && matches!(word.as_str(), "scalar" | "wantarray") {
            return true;
        }
        start = end;
    }
    false
}

fn is_self_comparison_expression(expression: &str) -> bool {
    let Some(inside) = call_args_inside(expression) else {
        return false;
    };
    let Some((got, expected)) = first_two_args(inside) else {
        return false;
    };
    !got.is_empty() && sink_text_eq(&got, &expected)
}

fn first_two_args(inside: &str) -> Option<(String, String)> {
    let chars: Vec<char> = inside.chars().collect();
    let mut args: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' => depth_paren += 1,
            ')' => depth_paren = depth_paren.saturating_sub(1),
            '[' => depth_brack += 1,
            ']' => depth_brack = depth_brack.saturating_sub(1),
            '{' => depth_brace += 1,
            '}' => depth_brace = depth_brace.saturating_sub(1),
            ',' if depth_paren == 0 && depth_brack == 0 && depth_brace == 0 => {
                args.push(
                    chars[start..i]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string(),
                );
                start = i + 1;
                if args.len() == 2 {
                    return Some((args[0].clone(), args[1].clone()));
                }
            }
            _ => {}
        }
        i += 1;
    }
    if args.len() == 1 {
        let second = chars[start..].iter().collect::<String>().trim().to_string();
        if !second.is_empty() {
            return Some((args[0].clone(), second));
        }
    }
    None
}

/// RIPR-SPEC-0235 rule 9: sink text equality that ignores whitespace which
/// cannot change what the expression means. It fails closed: text with a
/// construct whose spacing can carry meaning (`/`, `~`, `#`, a backtick, `$"`,
/// `$'`, or a `q`, `qq`, `qw`, `qr`, `m`, `s`, `tr` or `y` word) compares
/// exactly. Otherwise whitespace inside a quoted literal is kept, and outside
/// one it is dropped except between two identifier characters
/// (`$a lt $b` is not `$altb`), between two characters that can fuse into one
/// operator (`- -1` is not `--1`), and next to a `.` (`1 . 5` is not `1.5`).
/// A sigil ends an identifier, so `scalar @a` equals `scalar@a`.
pub(crate) fn sink_text_eq(left: &str, right: &str) -> bool {
    left == right
        || canonical_sink_text(left)
            .zip(canonical_sink_text(right))
            .is_some_and(|(left, right)| left == right)
}

fn canonical_sink_text(text: &str) -> Option<String> {
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    let fuses = |c: char| matches!(c, '<' | '>' | '=' | '!' | '&' | '|' | '*' | '+' | '-' | ':');
    if text.contains(['/', '~', '`', '#']) || text.contains("$\"") || text.contains("$'") {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut pending_space = false;
    let mut word = String::new();
    for c in text.trim().chars() {
        if let Some(open) = quote {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        if is_word(c) {
            word.push(c);
        } else {
            if matches!(
                word.as_str(),
                "q" | "qq" | "qw" | "qr" | "m" | "s" | "tr" | "y"
            ) {
                return None;
            }
            word.clear();
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            if let Some(prev) = out.chars().next_back()
                && ((is_word(prev) && is_word(c))
                    || (fuses(prev) && fuses(c))
                    || prev == '.'
                    || c == '.')
            {
                out.push(' ');
            }
            pending_space = false;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
        }
        out.push(c);
    }
    if matches!(
        word.as_str(),
        "q" | "qq" | "qw" | "qr" | "m" | "s" | "tr" | "y"
    ) {
        return None;
    }
    Some(out)
}
