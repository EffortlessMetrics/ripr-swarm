//! RIPR-SPEC-0197 / #9478: an owner passed as a path value to `.map` in a
//! bounded literal iterator chain compared with `assert_eq!`.

use super::closure_adaptor::literal_sequence;
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::indirect_owner::names_owner_value;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::extract::{
    IteratorChain, integer_element, parse_chain_at, parse_iterator_chain,
};
use crate::analysis::facts::{FunctionFact, RustIndex, TestSummary};
use crate::analysis::test_grip_evidence::fold_owner_body_at;

/// When one `assert_eq!` operand is a bounded iterator chain naming the owner
/// as a path value and the other operand is the matching literal `Vec`, return
/// a representative owner call on the first input (`owner(first)`).
pub(in crate::analysis::classify) fn substitute_owner_call(
    test: &TestSummary,
    chain_operand: &str,
    expected_operand: &str,
    owner: &FunctionFact,
    index: &RustIndex,
) -> Option<String> {
    let chain = parse_iterator_chain(chain_operand)?;
    let expected = literal_sequence(expected_operand)?;
    if expected.is_empty() {
        return None;
    }
    if !value_path_names_owner(&test.body, &chain.owner_path, owner, test, index) {
        return None;
    }
    let outputs = folded_outputs(owner, &chain)?;
    if outputs != expected {
        return None;
    }
    let first = integer_element(chain.inputs.first()?)?;
    Some(format!("{}({first})", owner.name))
}

fn folded_outputs(owner: &FunctionFact, chain: &IteratorChain) -> Option<Vec<String>> {
    let mut out = Vec::with_capacity(chain.inputs.len());
    for element in &chain.inputs {
        let input = integer_element(element)?;
        let value = fold_owner_body_at(owner.body.as_str(), input)?;
        out.push(value.to_string());
    }
    Some(out)
}

fn value_path_names_owner(
    code: &str,
    path: &str,
    owner: &FunctionFact,
    test: &TestSummary,
    index: &RustIndex,
) -> bool {
    let path = path.trim();
    if path.rsplit("::").next() != Some(owner.name.as_str()) {
        return false;
    }
    if path.contains("::") {
        // A qualified path is credited only where the shared call identity
        // settles its qualifier on the owner; `crate::decoy::inc` is not the
        // owner merely because `crate` may name it.
        let segments: Vec<String> = path.split("::").map(String::from).collect();
        let Some((_, qualifier)) = segments.split_last() else {
            return false;
        };
        let resolves_to_owner = shared_owner_call_identity(index, owner).is_some_and(|identity| {
            identity.resolve_call(test, index, qualifier) == CallTarget::Owner
        });
        if !resolves_to_owner {
            return false;
        }
    } else if !names_owner_value(&mask_comments_and_strings(code), owner, test, index, |_| {
        false
    }) {
        return false;
    }
    let masked = mask_comments_and_strings(code);
    let mentions_map = |text: &str| {
        masked.contains(&format!(".map({text})")) || masked.contains(&format!(".map( {text})"))
    };
    mentions_map(path)
        || (path.contains("::") && mentions_map(path.rsplit("::").next().unwrap_or(path)))
}

/// Whether the owner name at `name_at` sits in a bounded `.map(owner)` chain
/// left to the owner pin (#9478), not an unresolved function value.
pub(in crate::analysis::classify) fn map_path_value_mention(
    code: &str,
    name_at: usize,
    owner: &FunctionFact,
    test: &TestSummary,
    index: &RustIndex,
) -> bool {
    let Some(open) = code[..name_at].rfind(".map(") else {
        return false;
    };
    let gap = &code[open + ".map(".len()..name_at];
    if !gap.chars().all(|c| c.is_whitespace()) {
        return false;
    }
    let Some(chain) = parse_chain_at(code, open) else {
        return false;
    };
    let Some(leaf) = chain
        .owner_path
        .rsplit("::")
        .next()
        .filter(|name| !name.is_empty())
    else {
        return false;
    };
    if leaf != owner.name {
        return false;
    }
    // Lift the function-value limit only where the owner pin can actually
    // credit this chain: the enclosing `assert_eq!` must pair it with a matching
    // literal `Vec`, so a rejected chain keeps the gap withheld.
    let Some((left, right)) = enclosing_assert_eq_operands(code, open) else {
        return false;
    };
    [(&left, &right), (&right, &left)]
        .into_iter()
        .any(|(chain_operand, expected)| {
            substitute_owner_call(test, chain_operand, expected, owner, index).is_some()
        })
}

/// The two operands of the `assert_eq!(..)` call that encloses `at`, split at
/// the one top-level comma. A call with a message or any other arity is not read.
fn enclosing_assert_eq_operands(code: &str, at: usize) -> Option<(String, String)> {
    let start = code[..at].rfind("assert_eq!(")?;
    let open = start + "assert_eq!".len();
    let mut depth = 0i32;
    let mut close = None;
    let mut commas = Vec::new();
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            ',' if depth == 1 => commas.push(open + offset),
            _ => {}
        }
    }
    let close = close.filter(|close| *close > at)?;
    let inner_start = open + 1;
    let (first, second) = match commas.as_slice() {
        [one] => (&code[inner_start..*one], &code[*one + 1..close]),
        [one, trailing] if code[*trailing + 1..close].trim().is_empty() => {
            (&code[inner_start..*one], &code[*one + 1..*trailing])
        }
        _ => return None,
    };
    Some((first.trim().to_string(), second.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_map_collect_chain() {
        let chain = parse_iterator_chain("vec![1, 2].into_iter().map(inc).collect::<Vec<_>>()");
        assert!(chain.is_some(), "bounded map/collect chain");
        let Some(chain) = chain else {
            return;
        };
        assert_eq!(chain.inputs, vec!["1", "2"]);
        assert_eq!(chain.owner_path, "inc");
    }

    #[test]
    fn fold_inc_body() {
        assert_eq!(
            fold_owner_body_at("fn inc(x: u32) -> u32 { x + 1 }", 1),
            Some(2)
        );
    }
}
