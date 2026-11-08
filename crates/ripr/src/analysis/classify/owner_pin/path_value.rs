//! RIPR-SPEC-0197 / #9478: an owner passed as a path value to `.map` in a
//! bounded literal iterator chain compared with `assert_eq!`.

use super::closure_adaptor::literal_sequence;
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::indirect_owner::names_owner_as_value;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::extract::{IteratorChain, integer_element, parse_iterator_chain};
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
    } else if !names_owner_as_value(&mask_comments_and_strings(code), owner, test, index) {
        return false;
    }
    let masked = mask_comments_and_strings(code);
    let mentions_map = |text: &str| {
        masked.contains(&format!(".map({text})")) || masked.contains(&format!(".map( {text})"))
    };
    mentions_map(path)
        || (path.contains("::") && mentions_map(path.rsplit("::").next().unwrap_or(path)))
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
