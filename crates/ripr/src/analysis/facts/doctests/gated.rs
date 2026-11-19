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
//! fenced doc example and its item. It reads only what the admission refuses
//! for a cfg reason; a gate on an enclosing `mod` or `impl` is not collected
//! here (that refusal keeps today's reading).

use super::{
    AttributeCfgFamily, attribute_cfg_family, attribute_path_and_optional_args,
    between_doc_and_item_cfg_refuses, is_between_doc_and_item, outer_doc_text,
    split_top_level_operands,
};
use std::collections::BTreeSet;

/// Predicate combinators, never configuration atoms.
const COMBINATORS: [&str; 3] = ["all", "any", "not"];

/// Build flags ripr models itself: a gate on them is not an unknown
/// configuration of the owner.
const MODELED_FLAGS: [&str; 2] = ["test", "doctest"];

/// The cfg atoms of every attribute that refuses a fenced doc example in
/// `source`: each `///` run that holds a fence line, followed by the
/// attribute, blank and comment lines up to its item.
pub(super) fn refused_example_gate_atoms(
    source: &str,
    defaults: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    if !source.contains("```") {
        return atoms;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        if outer_doc_text(lines[index]).is_none() {
            index += 1;
            continue;
        }
        let mut fenced = false;
        while index < lines.len()
            && let Some(text) = outer_doc_text(lines[index])
        {
            fenced |= text.trim_start().starts_with("```");
            index += 1;
        }
        if !fenced {
            continue;
        }
        for line in lines[index..]
            .iter()
            .take_while(|line| is_between_doc_and_item(line))
        {
            let trimmed = line.trim_start();
            if between_doc_and_item_cfg_refuses(trimmed, defaults) {
                atoms.extend(attribute_gate_atoms(trimmed));
            }
        }
    }
    atoms
}

/// The configuration atoms of a `#[cfg(P)]` or `#[cfg_attr(P, ..)]`
/// attribute's predicate `P`: each bare name (`error_generic_member_access`)
/// and each `key = "value"` pair (`feature="std"`, spaces removed). Any other
/// attribute, or text that does not read, has none.
pub(crate) fn attribute_gate_atoms(attribute: &str) -> BTreeSet<String> {
    let trimmed = attribute.trim();
    let Some((_, Some(args))) = attribute_path_and_optional_args(trimmed) else {
        return BTreeSet::new();
    };
    let predicate = match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::Cfg => Some(args),
        AttributeCfgFamily::CfgAttr => {
            split_top_level_operands(args).and_then(|operands| operands.first().copied())
        }
        AttributeCfgFamily::Other => None,
    };
    predicate.map(predicate_atoms).unwrap_or_default()
}

fn predicate_atoms(predicate: &str) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    let chars: Vec<char> = predicate.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        if ch == '"' {
            // A literal outside `key = ".."` belongs to no atom; skip it.
            at = skip_literal(&chars, at);
            continue;
        }
        if !(ch.is_alphabetic() || ch == '_') {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
            at += 1;
        }
        let name: String = chars[start..at].iter().collect();
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
                atoms.insert(format!("{name}={literal}"));
                at = end;
                continue;
            }
        }
        if !COMBINATORS.contains(&name.as_str()) && !MODELED_FLAGS.contains(&name.as_str()) {
            atoms.insert(name);
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
    fn gate_atoms_read_names_and_key_value_pairs_of_the_predicate_only() {
        assert_eq!(
            attribute_gate_atoms("#[cfg(error_generic_member_access)]"),
            atoms(&["error_generic_member_access"])
        );
        assert_eq!(
            attribute_gate_atoms(
                "#[cfg_attr(not(error_generic_member_access), doc = \"# _ = stringify! {\")]"
            ),
            atoms(&["error_generic_member_access"]),
            "only the cfg_attr predicate is read, not the attribute it adds"
        );
        assert_eq!(
            attribute_gate_atoms("#[cfg(any(feature = \"std\", not(anyhow_no_core_error)))]"),
            atoms(&["feature=\"std\"", "anyhow_no_core_error"])
        );
        assert_eq!(
            attribute_gate_atoms("#[cfg(all(test, doctest, unix))]"),
            atoms(&["unix"])
        );
        assert!(attribute_gate_atoms("#[must_use]").is_empty());
        assert!(attribute_gate_atoms("#[doc(hidden)]").is_empty());
        assert!(attribute_gate_atoms("#[cfg(unbalanced").is_empty());
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
    fn an_unfenced_doc_run_or_an_admitted_gate_records_nothing() {
        // No fence: a doc comment with a gate is not an example.
        let plain = "/// Plain docs.\n#[cfg(custom_gate)]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(plain, None).is_empty());
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
