//! RIPR-SPEC-0197 rule 1 extension: a test-local helper that polls one
//! `Future` to completion and returns its output.
//!
//! `assert_eq!(block_on(owner(..)), e)` observes the `async` owner's return
//! value exactly as `owner(..).await` does, but only when `block_on` is a
//! helper that does nothing else. The helper is read from its own source
//! against a closed set of spellings; a name alone (`block_on`), a helper that
//! transforms the output, or one with a timeout or a second branch is not one,
//! and the call keeps the `rust_indirect_owner_call_unresolved` limit.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionFact;
use crate::analysis::rust_index::RustIndex;
use std::collections::{BTreeMap, BTreeSet};

/// The `if let` spelling of the poll loop.
const IF_LET: &str = "fn PH_name < PH_f : Future > ( PH_p : PH_f ) -> PH_f :: Output { \
    let mut PH_fut = pin ! ( PH_p ) ; \
    let mut PH_cx = Context :: from_waker ( Waker :: noop ( ) ) ; \
    loop { if let Poll :: Ready ( PH_v ) = PH_fut . as_mut ( ) . poll ( & mut PH_cx ) { return PH_v ; } } }";

/// The `match` spelling of the same loop.
const MATCH: &str = "fn PH_name < PH_f : Future > ( PH_p : PH_f ) -> PH_f :: Output { \
    let mut PH_fut = pin ! ( PH_p ) ; \
    let mut PH_cx = Context :: from_waker ( Waker :: noop ( ) ) ; \
    loop { match PH_fut . as_mut ( ) . poll ( & mut PH_cx ) { \
    Poll :: Ready ( PH_v ) => return PH_v , Poll :: Pending => { } } } }";

/// Names every definition of which is a poll-to-completion helper. A name
/// with any other definition in the workspace is left out, since the call's
/// callee could be that one.
pub(super) fn verified_helpers(index: &RustIndex) -> BTreeSet<String> {
    let mut by_name: BTreeMap<&str, bool> = BTreeMap::new();
    for function in index.functions() {
        let verdict = polls_to_completion(function);
        by_name
            .entry(function.name.as_str())
            .and_modify(|all| *all &= verdict)
            .or_insert(verdict);
    }
    by_name
        .into_iter()
        .filter(|(_, all)| *all)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// `helper(inner)` with a verified helper, as the future expression `inner`.
pub(super) fn strip_helper_call<'a>(
    operand: &'a str,
    helpers: &BTreeSet<String>,
) -> Option<&'a str> {
    if helpers.is_empty() {
        return None;
    }
    let operand = operand.trim();
    let masked = mask_comments_and_strings(operand);
    let name_end = masked.find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))?;
    if !helpers.contains(&operand[..name_end]) {
        return None;
    }
    let rest = &masked[name_end..];
    let open = name_end + (rest.len() - rest.trim_start().len());
    if masked.as_bytes().get(open) != Some(&b'(') || !masked.ends_with(')') {
        return None;
    }
    // The first parenthesis closes at the end: the call has no tail.
    let mut depth = 0usize;
    for (at, ch) in masked.char_indices().skip_while(|(at, _)| *at < open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && at + 1 != masked.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(operand[open + 1..operand.len() - 1].trim())
}

fn polls_to_completion(function: &FunctionFact) -> bool {
    if !function.attrs.is_empty() || !function.impl_attrs.is_empty() {
        return false;
    }
    let body: &str = &function.body;
    if !body.contains("Poll") {
        return false;
    }
    let tokens = drop_std_prefixes(tokenize(&mask_comments_and_strings(body)));
    [IF_LET, MATCH]
        .iter()
        .any(|template| matches_template(&tokens, &tokenize(template)))
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !ch.is_whitespace() {
            tokens.push(ch.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

/// `std::pin::pin!` and `core::task::Waker` read as `pin!` and `Waker`.
fn drop_std_prefixes(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut at = 0;
    while at < tokens.len() {
        if matches!(tokens[at].as_str(), "std" | "core")
            && tokens.get(at + 1).is_some_and(|t| t == ":")
            && tokens.get(at + 2).is_some_and(|t| t == ":")
            && tokens
                .get(at + 3)
                .is_some_and(|t| matches!(t.as_str(), "future" | "pin" | "task"))
            && tokens.get(at + 4).is_some_and(|t| t == ":")
            && tokens.get(at + 5).is_some_and(|t| t == ":")
        {
            at += 6;
            continue;
        }
        out.push(tokens[at].clone());
        at += 1;
    }
    out
}

/// Token equality where each `PH_` word binds one identifier, the same one
/// at every use.
fn matches_template(tokens: &[String], template: &[String]) -> bool {
    if tokens.len() != template.len() {
        return false;
    }
    let mut bound: BTreeMap<&str, &str> = BTreeMap::new();
    for (token, expected) in tokens.iter().zip(template) {
        if expected.starts_with("PH_") {
            if token.starts_with(|ch: char| ch.is_ascii_digit())
                || !token.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            {
                return false;
            }
            if *bound.entry(expected).or_insert(token) != token {
                return false;
            }
        } else if token != expected {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polls(source: &str) -> bool {
        let tokens = drop_std_prefixes(tokenize(&mask_comments_and_strings(source)));
        [IF_LET, MATCH]
            .iter()
            .any(|template| matches_template(&tokens, &tokenize(template)))
    }

    const HELPER: &str = "fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }";

    #[test]
    fn the_polling_loop_is_a_helper() {
        assert!(polls(HELPER));
    }

    #[test]
    fn the_match_spelling_and_std_paths_are_a_helper() {
        assert!(polls(
            "fn run<T: Future>(f: T) -> T::Output {
                let mut g = std::pin::pin!(f);
                let mut c = std::task::Context::from_waker(std::task::Waker::noop());
                loop {
                    match g.as_mut().poll(&mut c) {
                        Poll::Ready(out) => return out,
                        Poll::Pending => {}
                    }
                }
            }"
        ));
    }

    #[test]
    fn a_helper_that_changes_the_output_is_not_one() {
        assert!(!polls(&HELPER.replace("return v;", "return v + 1;")));
    }

    #[test]
    fn a_helper_with_a_second_exit_is_not_one() {
        assert!(!polls(
            &HELPER.replace("loop {", "if false { return todo!(); } loop {")
        ));
    }

    #[test]
    fn polling_a_different_binding_is_not_a_helper() {
        assert!(!polls(&HELPER.replace("fut.as_mut()", "other.as_mut()")));
    }

    fn strip(operand: &str) -> Option<&str> {
        let helpers = BTreeSet::from(["block_on".to_string()]);
        strip_helper_call(operand, &helpers)
    }

    #[test]
    fn a_helper_call_reads_as_its_argument() {
        assert_eq!(strip("block_on(add_tax(100))"), Some("add_tax(100)"));
        assert_eq!(strip(" block_on (f(&[1, 2]))"), Some("f(&[1, 2])"));
    }

    #[test]
    fn only_a_complete_call_of_a_verified_name_is_stripped() {
        assert_eq!(strip("block_on(a(1)).unwrap()"), None);
        assert_eq!(strip("block_on(a(1)) + 1"), None);
        assert_eq!(strip("other(a(1))"), None);
        assert_eq!(strip("rt.block_on(a(1))"), None);
        assert_eq!(strip("block_on"), None);
    }
}
