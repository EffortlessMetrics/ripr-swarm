//! Doc examples left unread because of a cfg gate (#14935).
//!
//! The doctest admission refuses an example whose item carries a
//! `#[cfg(..)]` it cannot prove true or any `#[cfg_attr(..)]` between the
//! comment and the item. Such an example is still a test rustdoc may run:
//! anyhow wraps its nightly-only examples in
//! `#[cfg_attr(not(error_generic_member_access), doc = "# _ = stringify! {")]`
//! and runs them for real when that cfg is on. A changed function compiled
//! only under the same cfg is then checked by tests ripr never read, so a
//! "no test is seen calling this" reading of it is not a gap claim ripr can
//! make (`classify::gated_doctest`).
//!
//! This pass records, per doctested library root, the cfg atoms (bare names
//! and `key = "value"` pairs) of every refusing attribute that sits between a
//! runnable fenced doc example and its item: an unprovable `#[cfg(..)]`, or a
//! `#[cfg_attr(..)]` that adds a `doc = ".."` fragment (the shape that rewrites
//! the example per configuration). A `cfg_attr` that adds anything else
//! (`derive(Serialize)`, `doc(cfg(..))` under `docsrs`) does not change what the
//! example runs, so its atoms are not recorded. A gate on an enclosing `mod`
//! or `impl` is not collected here (that refusal keeps today's reading).

use super::{
    AttributeCfgFamily, attribute_cfg_family, attribute_path_and_optional_args,
    between_doc_and_item_cfg_refuses, is_between_doc_and_item, outer_doc_text, runnable_fence,
    split_top_level_operands,
};
use std::collections::BTreeSet;

/// Predicate combinators, never configuration atoms.
const COMBINATORS: [&str; 3] = ["all", "any", "not"];

/// Build flags ripr models itself: a gate on them is not an unknown
/// configuration of the owner.
const MODELED_FLAGS: [&str; 2] = ["test", "doctest"];

/// The cfg atoms of every attribute that refuses a runnable fenced doc
/// example in `source`. A document is the `///` runs and the attribute,
/// blank and comment lines between them, up to its item; an attribute counts
/// once a runnable fence (plain, `rust` or an edition) has opened in it.
pub(super) fn refused_example_gate_atoms(
    source: &str,
    defaults: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    if !source.contains("```") {
        return atoms;
    }
    let (mut in_document, mut fence_open, mut runnable) = (false, false, false);
    for line in source.lines() {
        if let Some(text) = outer_doc_text(line) {
            in_document = true;
            if let Some(info) = text.trim_start().strip_prefix("```") {
                if !fence_open {
                    runnable |= runnable_fence(info.trim());
                }
                fence_open = !fence_open;
            }
            continue;
        }
        if in_document && is_between_doc_and_item(line) {
            let trimmed = line.trim_start();
            if runnable && between_doc_and_item_cfg_refuses(trimmed, defaults) {
                atoms.extend(example_gate_atoms(trimmed));
            }
            continue;
        }
        // The item line, or code outside any document, ends the document.
        (in_document, fence_open, runnable) = (false, false, false);
    }
    atoms
}

/// The atoms of a refusing attribute on the example side, either polarity:
/// `cfg_attr(not(x), doc = "# _ = stringify! {")` stubs the example when `x`
/// is off and runs it when `x` is on.
fn example_gate_atoms(attribute: &str) -> BTreeSet<String> {
    let trimmed = attribute.trim();
    let Some((_, Some(args))) = attribute_path_and_optional_args(trimmed) else {
        return BTreeSet::new();
    };
    let predicate = match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::Cfg => Some(args),
        AttributeCfgFamily::CfgAttr => split_top_level_operands(args).and_then(|operands| {
            let adds_doc_text = operands.iter().skip(1).any(|operand| {
                let compact: String = operand.split_whitespace().collect();
                compact.starts_with("doc=")
            });
            adds_doc_text.then(|| operands.first().copied()).flatten()
        }),
        AttributeCfgFamily::Other => None,
    };
    predicate
        .map(|predicate| {
            predicate_atoms(predicate)
                .into_iter()
                .map(|(atom, _)| atom)
                .collect()
        })
        .unwrap_or_default()
}

/// The atoms a `#[cfg(P)]` gate on an owner requires on: each bare name
/// (`error_generic_member_access`) and `key = "value"` pair (`feature="std"`,
/// spaces removed) of `P` that is not under an odd number of `not(..)`. An
/// owner behind `#[cfg(not(x))]` is compiled when `x` is off, not in the
/// build that runs an example needing `x`, so `x` is not one of its atoms.
/// Any other attribute, or text that does not read, has none.
pub(crate) fn owner_gate_atoms(attribute: &str) -> BTreeSet<String> {
    let trimmed = attribute.trim();
    let Some((_, Some(args))) = attribute_path_and_optional_args(trimmed) else {
        return BTreeSet::new();
    };
    if !matches!(attribute_cfg_family(trimmed), AttributeCfgFamily::Cfg) {
        return BTreeSet::new();
    }
    predicate_atoms(args)
        .into_iter()
        .filter(|(_, negated)| !negated)
        .map(|(atom, _)| atom)
        .collect()
}

/// Each atom of `predicate` with whether an odd number of `not(..)` encloses
/// it.
fn predicate_atoms(predicate: &str) -> Vec<(String, bool)> {
    let mut atoms = Vec::new();
    let chars: Vec<char> = predicate.chars().collect();
    // One entry per open parenthesis: whether it opened a `not(`.
    let mut parens: Vec<bool> = Vec::new();
    let mut after_not = false;
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        if ch == '"' {
            // A literal outside `key = ".."` belongs to no atom; skip it.
            at = skip_literal(&chars, at);
            after_not = false;
            continue;
        }
        if ch == '(' {
            parens.push(after_not);
            after_not = false;
            at += 1;
            continue;
        }
        if ch == ')' {
            parens.pop();
            at += 1;
            continue;
        }
        if !(ch.is_alphabetic() || ch == '_') {
            if !ch.is_whitespace() {
                after_not = false;
            }
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
            at += 1;
        }
        let name: String = chars[start..at].iter().collect();
        after_not = name == "not";
        let negated = parens.iter().filter(|opened_not| **opened_not).count() % 2 == 1;
        let mut next = at;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        if chars.get(next) == Some(&'=') {
            let mut value = next + 1;
            while value < chars.len() && chars[value].is_whitespace() {
                value += 1;
            }
            if chars.get(value) == Some(&'"') {
                let end = skip_literal(&chars, value);
                let literal: String = chars[value..end].iter().collect();
                atoms.push((format!("{name}={literal}"), negated));
                at = end;
                continue;
            }
        }
        if !COMBINATORS.contains(&name.as_str()) && !MODELED_FLAGS.contains(&name.as_str()) {
            atoms.push((name, negated));
        }
    }
    atoms
}

/// The index just past the string literal opening at `open`.
fn skip_literal(chars: &[char], open: usize) -> usize {
    let mut at = open + 1;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 2,
            '"' => return at + 1,
            _ => at += 1,
        }
    }
    chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atoms(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn owner_gate_atoms_read_the_positive_names_and_pairs_of_a_cfg() {
        assert_eq!(
            owner_gate_atoms("#[cfg(error_generic_member_access)]"),
            atoms(&["error_generic_member_access"])
        );
        assert_eq!(
            owner_gate_atoms("#[cfg(any(feature = \"std\", not(anyhow_no_core_error)))]"),
            atoms(&["feature=\"std\""]),
            "an atom under `not(..)` is compiled in the other build"
        );
        assert_eq!(
            owner_gate_atoms("#[cfg(not(not(nightly_api)))]"),
            atoms(&["nightly_api"])
        );
        assert!(owner_gate_atoms("#[cfg(not(nightly_api))]").is_empty());
        assert_eq!(
            owner_gate_atoms("#[cfg(all(test, doctest, unix))]"),
            atoms(&["unix"])
        );
        assert!(owner_gate_atoms("#[cfg_attr(nightly_api, inline)]").is_empty());
        assert!(owner_gate_atoms("#[must_use]").is_empty());
        assert!(owner_gate_atoms("#[doc(hidden)]").is_empty());
        assert!(owner_gate_atoms("#[cfg(unbalanced").is_empty());
    }

    #[test]
    fn example_atoms_come_from_doc_text_cfg_attrs_and_unprovable_cfgs() {
        assert_eq!(
            example_gate_atoms(
                "#[cfg_attr(not(error_generic_member_access), doc = \"# _ = stringify! {\")]"
            ),
            atoms(&["error_generic_member_access"]),
            "only the predicate is read, either polarity"
        );
        assert!(
            example_gate_atoms("#[cfg_attr(feature = \"serde\", derive(Serialize))]").is_empty(),
            "a cfg_attr that adds no doc text does not change the example"
        );
        assert!(example_gate_atoms("#[cfg_attr(docsrs, doc(cfg(feature = \"x\")))]").is_empty());
        assert_eq!(
            example_gate_atoms("#[cfg(custom_gate)]"),
            atoms(&["custom_gate"])
        );
    }

    #[test]
    fn refused_examples_record_the_atoms_of_the_refusing_attribute() {
        let source = "/// ```\n\
#[cfg_attr(not(nightly_api), doc = \"# _ = stringify! {\")]\n\
/// assert!(lib::f());\n\
#[cfg_attr(not(nightly_api), doc = \"# };\")]\n\
/// ```\n\
#[must_use]\n\
pub fn f() -> bool { true }\n";
        assert_eq!(
            refused_example_gate_atoms(source, None),
            atoms(&["nightly_api"])
        );
    }

    #[test]
    fn a_non_example_document_or_an_admitted_gate_records_nothing() {
        // No fence: a doc comment with a gate is not an example.
        let plain = "/// Plain docs.\n#[cfg(custom_gate)]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(plain, None).is_empty());
        // A `text` block is not run, and its closing fence opens nothing.
        let text = "/// ```text\n/// f()\n/// ```\n#[cfg(custom_gate)]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(text, None).is_empty());
        // A gate after the item line belongs to the next item, not the example.
        let next =
            "/// ```\n/// lib::f();\n/// ```\npub fn f() {}\n#[cfg(custom_gate)]\npub fn g() {}\n";
        assert!(refused_example_gate_atoms(next, None).is_empty());
        // `cfg(doctest)` admits the example, so nothing was left unread.
        let doctest = "/// ```\n/// lib::f();\n/// ```\n#[cfg(doctest)]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(doctest, None).is_empty());
        // A default feature is provably on, so that gate does not refuse.
        let defaults = BTreeSet::from(["std".to_string()]);
        let on = "/// ```\n/// lib::f();\n/// ```\n#[cfg(feature = \"std\")]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(on, Some(&defaults)).is_empty());
        assert_eq!(
            refused_example_gate_atoms(on, None),
            atoms(&["feature=\"std\""]),
            "without the manifest's defaults the same gate refuses"
        );
    }
}
