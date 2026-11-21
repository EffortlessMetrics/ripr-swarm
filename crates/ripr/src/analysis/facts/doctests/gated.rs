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
//! This pass records, per doctested library root, the custom cfg names that
//! gate a refused runnable fenced example:
//!
//! - a `#[cfg_attr(P, doc = "..")]` line inside an open runnable fence (the
//!   shape that stubs the example out per configuration) contributes the
//!   names `not(P)` requires on: `not(x)` stubs the example when `x` is off
//!   and so runs it only when `x` is on, while `cfg_attr(x, ..)` runs it only
//!   when `x` is off and contributes nothing;
//! - an unprovable `#[cfg(P)]` after a runnable fence and before its item
//!   contributes the names `P` requires on, since the example is compiled
//!   only then.
//!
//! A name is required on when every combinator above it is a conjunction
//! once negations are pushed inward (`all(..)`, or `any(..)` under an odd
//! number of `not(..)`) and an even number of `not(..)` encloses it, so
//! `not(all(not(x), y))`, which is `x || not(y)`, requires nothing. Only a
//! document on a public production function counts, as for the admission
//! itself: rustdoc compiles no example inside a `#[cfg(test)]` module.
//!
//! Only custom names count, the ones a build script or `RUSTFLAGS` sets
//! (`error_generic_member_access`). `key = "value"` pairs (`feature`,
//! `target_os`) and built-in names (`unix`, `windows`, `debug_assertions`,
//! `target_*`, ..) gate ordinary code everywhere, so sharing one says nothing
//! about whether an example exercises the owner. A `cfg_attr` that adds no
//! doc text (`derive(Serialize)`, `doc(cfg(..))` under `docsrs`) does not
//! change what the example runs. A gate on an enclosing `mod` or `impl` of the
//! example is not collected here (that refusal keeps today's reading).

use super::{
    AttributeCfgFamily, attribute_cfg_family, attribute_path_and_optional_args,
    between_doc_and_item_cfg_refuses, is_between_doc_and_item, outer_doc_text, runnable_fence,
    split_top_level_operands,
};
use std::collections::BTreeSet;

/// Built-in cfg names rustc or Cargo set, and the build flags ripr models
/// itself. Every `target_*` name is built in too.
const BUILT_IN_NAMES: [&str; 13] = [
    "all",
    "any",
    "not",
    "test",
    "doctest",
    "docsrs",
    "unix",
    "windows",
    "debug_assertions",
    "miri",
    "proc_macro",
    "overflow_checks",
    "ub_checks",
];

/// The custom cfg names that gate each refused runnable fenced doc example in
/// `source`, keyed by the 1-based line of the item the document ends at. A
/// document is the `///` runs and the attribute, blank and comment lines
/// between them, up to its item.
pub(super) fn refused_example_gate_atoms(
    source: &str,
    defaults: Option<&BTreeSet<String>>,
) -> Vec<(usize, BTreeSet<String>)> {
    let mut found = Vec::new();
    if !source.contains("```") {
        return found;
    }
    let mut atoms = BTreeSet::new();
    // `open`: a fence is open, and `open_runnable`: that fence is runnable;
    // `seen_runnable`: a runnable fence opened anywhere in this document.
    let (mut in_document, mut open, mut open_runnable, mut seen_runnable) =
        (false, false, false, false);
    for (index, line) in source.lines().enumerate() {
        if let Some(text) = outer_doc_text(line) {
            in_document = true;
            if let Some(info) = text.trim_start().strip_prefix("```") {
                if open {
                    (open, open_runnable) = (false, false);
                } else {
                    open = true;
                    open_runnable = runnable_fence(info.trim());
                    seen_runnable |= open_runnable;
                }
            }
            continue;
        }
        if in_document && is_between_doc_and_item(line) {
            let trimmed = line.trim_start();
            if between_doc_and_item_cfg_refuses(trimmed, defaults) {
                atoms.extend(example_gate_atoms(trimmed, open_runnable, seen_runnable));
            }
            continue;
        }
        // The item line, or code outside any document, ends the document.
        if !atoms.is_empty() {
            found.push((index + 1, std::mem::take(&mut atoms)));
        }
        (in_document, open, open_runnable, seen_runnable) = (false, false, false, false);
    }
    found
}

/// The custom names a refusing attribute contributes on the example side (see
/// the module docs): a doc-text `cfg_attr` inside an open runnable fence gives
/// the names the negation of its predicate requires on; a `cfg` after a
/// runnable fence gives the names it requires on.
fn example_gate_atoms(
    attribute: &str,
    open_runnable: bool,
    seen_runnable: bool,
) -> BTreeSet<String> {
    let trimmed = attribute.trim();
    let Some((_, Some(args))) = attribute_path_and_optional_args(trimmed) else {
        return BTreeSet::new();
    };
    match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::Cfg if seen_runnable => required_names(args, false),
        AttributeCfgFamily::CfgAttr if open_runnable => split_top_level_operands(args)
            .filter(|operands| {
                operands.iter().skip(1).any(|operand| {
                    let compact: String = operand.split_whitespace().collect();
                    compact.starts_with("doc=")
                })
            })
            .and_then(|operands| operands.first().copied())
            .map(|predicate| required_names(predicate, true))
            .unwrap_or_default(),
        _ => BTreeSet::new(),
    }
}

/// The custom cfg names a `#[cfg(P)]` gate on an owner requires on
/// (`error_generic_member_access`), as the module docs define it. An owner behind `#[cfg(not(x))]` is compiled when `x`
/// is off, and one behind `#[cfg(any(x, y))]` with only `y` on, so neither is
/// compiled only in the build that runs an example needing `x`. Any other
/// attribute, or text that does not read, has none.
pub(crate) fn owner_gate_atoms(attribute: &str) -> BTreeSet<String> {
    let trimmed = attribute.trim();
    let Some((_, Some(args))) = attribute_path_and_optional_args(trimmed) else {
        return BTreeSet::new();
    };
    if !matches!(attribute_cfg_family(trimmed), AttributeCfgFamily::Cfg) {
        return BTreeSet::new();
    }
    required_names(args, false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Not,
    All,
    Any,
    Other,
}

/// The custom bare names `predicate` requires on, or `not(predicate)` when
/// `negated`: every enclosing combinator is a conjunction once negations are
/// pushed inward, and an even number of `not(..)` encloses the name. An
/// unknown combinator requires nothing. `key = "value"` pairs and built-in
/// names are skipped.
fn required_names(predicate: &str, negated: bool) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let chars: Vec<char> = predicate.chars().collect();
    let mut groups: Vec<Group> = Vec::new();
    let mut pending = Group::Other;
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        if ch == '"' {
            at = skip_literal(&chars, at);
            pending = Group::Other;
            continue;
        }
        if ch == '(' {
            groups.push(pending);
            pending = Group::Other;
            at += 1;
            continue;
        }
        if ch == ')' {
            groups.pop();
            at += 1;
            continue;
        }
        if !(ch.is_alphabetic() || ch == '_') {
            if !ch.is_whitespace() {
                pending = Group::Other;
            }
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
            at += 1;
        }
        let name: String = chars[start..at].iter().collect();
        pending = match name.as_str() {
            "not" => Group::Not,
            "all" => Group::All,
            "any" => Group::Any,
            _ => Group::Other,
        };
        let mut next = at;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        if chars.get(next) == Some(&'=') {
            // A `key = "value"` pair (`feature`, `target_os`): skip its value.
            continue;
        }
        if BUILT_IN_NAMES.contains(&name.as_str()) || name.starts_with("target_") {
            continue;
        }
        if required_through(&groups, negated) {
            names.insert(name);
        }
    }
    names
}

/// Whether a name nested in `groups` (outermost first) is required on, as
/// `required_names` defines it.
fn required_through(groups: &[Group], negated: bool) -> bool {
    let mut odd = negated;
    for group in groups {
        match (group, odd) {
            (Group::Not, _) => odd = !odd,
            (Group::All, false) | (Group::Any, true) => {}
            _ => return false,
        }
    }
    !odd
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
    fn owner_gate_atoms_read_the_custom_names_a_cfg_requires_on() {
        assert_eq!(
            owner_gate_atoms("#[cfg(error_generic_member_access)]"),
            atoms(&["error_generic_member_access"])
        );
        assert_eq!(
            owner_gate_atoms("#[cfg(all(nightly_api, not(other_api)))]"),
            atoms(&["nightly_api"]),
            "a name under `not(..)` is required off"
        );
        assert_eq!(
            owner_gate_atoms("#[cfg(not(not(nightly_api)))]"),
            atoms(&["nightly_api"])
        );
        assert!(owner_gate_atoms("#[cfg(not(nightly_api))]").is_empty());
        assert!(
            owner_gate_atoms("#[cfg(not(all(not(nightly_api), other_api)))]").is_empty(),
            "this is `nightly_api || not(other_api)`, so neither name is required"
        );
        assert_eq!(
            owner_gate_atoms("#[cfg(not(any(not(nightly_api), other_api)))]"),
            atoms(&["nightly_api"]),
            "this is `nightly_api && not(other_api)`"
        );
        assert!(
            owner_gate_atoms("#[cfg(any(nightly_api, feature = \"serde\"))]").is_empty(),
            "under `any(..)` no single name is required"
        );
        assert!(
            owner_gate_atoms("#[cfg(feature = \"std\")]").is_empty(),
            "a key-value pair is not a custom name"
        );
        assert!(
            owner_gate_atoms("#[cfg(all(unix, target_os = \"linux\", target_has_atomic))]")
                .is_empty()
        );
        assert!(owner_gate_atoms("#[cfg(all(test, doctest, docsrs))]").is_empty());
        assert!(owner_gate_atoms("#[cfg_attr(nightly_api, inline)]").is_empty());
        assert!(owner_gate_atoms("#[must_use]").is_empty());
        assert!(owner_gate_atoms("#[cfg(unbalanced").is_empty());
    }

    #[test]
    fn example_atoms_need_doc_text_inside_a_fence_or_a_required_cfg_after_one() {
        let stub = "#[cfg_attr(not(error_generic_member_access), doc = \"# _ = stringify! {\")]";
        assert_eq!(
            example_gate_atoms(stub, true, true),
            atoms(&["error_generic_member_access"]),
            "a stub under not(x) runs the example only with x on"
        );
        let inverted = "#[cfg_attr(nightly_api, doc = \"# _ = stringify! {\")]";
        assert!(
            example_gate_atoms(inverted, true, true).is_empty(),
            "a stub under x runs the example only with x off"
        );
        assert!(
            example_gate_atoms(stub, false, true).is_empty(),
            "outside an open runnable fence it rewrites no example code"
        );
        assert!(
            example_gate_atoms("#[cfg_attr(nightly_api, derive(Debug))]", true, true).is_empty(),
            "a cfg_attr that adds no doc text does not change the example"
        );
        assert!(
            example_gate_atoms("#[cfg_attr(docsrs, doc(cfg(nightly_api)))]", true, true).is_empty()
        );
        assert_eq!(
            example_gate_atoms("#[cfg(custom_gate)]", false, true),
            atoms(&["custom_gate"])
        );
        assert!(
            example_gate_atoms("#[cfg(not(custom_gate))]", false, true).is_empty(),
            "an item behind cfg(not(x)) runs its example only with x off"
        );
        assert!(example_gate_atoms("#[cfg(custom_gate)]", false, false).is_empty());
        assert!(example_gate_atoms("#[cfg(unix)]", false, true).is_empty());
    }

    #[test]
    fn refused_examples_record_the_names_of_the_refusing_attribute() {
        let source = "/// ```\n\
#[cfg_attr(not(nightly_api), doc = \"# _ = stringify! {\")]\n\
/// assert!(lib::f());\n\
#[cfg_attr(not(nightly_api), doc = \"# };\")]\n\
/// ```\n\
#[must_use]\n\
pub fn f() -> bool { true }\n";
        assert_eq!(
            refused_example_gate_atoms(source, None),
            vec![(7, atoms(&["nightly_api"]))],
            "keyed by the item line the document ends at"
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
        // A doc-text cfg_attr inside a `text` block after a runnable one.
        let later = "/// ```\n/// lib::f();\n/// ```\n/// ```text\n#[cfg_attr(custom_gate, doc = \"x\")]\n/// ```\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(later, None).is_empty());
        // A gate after the item line belongs to the next item, not the example.
        let next =
            "/// ```\n/// lib::f();\n/// ```\npub fn f() {}\n#[cfg(custom_gate)]\npub fn g() {}\n";
        assert!(refused_example_gate_atoms(next, None).is_empty());
        // `cfg(doctest)` admits the example, so nothing was left unread.
        let doctest = "/// ```\n/// lib::f();\n/// ```\n#[cfg(doctest)]\npub fn f() {}\n";
        assert!(refused_example_gate_atoms(doctest, None).is_empty());
        let custom = "/// ```\n/// lib::f();\n/// ```\n#[cfg(custom_gate)]\npub fn f() {}\n";
        assert_eq!(
            refused_example_gate_atoms(custom, None),
            vec![(5, atoms(&["custom_gate"]))]
        );
    }
}
