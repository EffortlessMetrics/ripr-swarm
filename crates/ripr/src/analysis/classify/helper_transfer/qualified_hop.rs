//! A hop whose call is spelled through an own-crate module path (#8999).
//!
//! `VersionReq::matches` calls `eval::matches_req(..)`: the call is a path
//! qualified invocation, and name identity alone is not callee identity
//! (#3296 review B1). The hop binds here only when the spelled path resolves,
//! from the caller's own module, to the module the unique free-function
//! callee is defined in. Every other shape declines and keeps the
//! pre-existing "not statically splittable" stop.
//!
//! Fail-closed admit, all of:
//!
//! - the callee is the one function of its name, a module-level `fn` with a
//!   body, and caller and callee are items of the root package's library
//!   (both compose under `src/lib.rs`, no `#[path]` remap, every module
//!   segment declared exactly once);
//! - the caller mentions the callee name exactly once in one call fact, as
//!   `<qualifier>::<callee>(..)` with a plain-identifier qualifier;
//! - the qualifier is `crate::..`, `self::..`, `super::..` or a child module
//!   of the caller's module, and the file does not `use`-import the first
//!   named segment (an import may bind it to another module);
//! - the resolved module path equals the callee's module path.

use super::{HelperHop, split_call_arguments_at_paren};
use crate::analysis::classify::{call_identity, related_tests};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn qualified_free_hop(
    caller: &FunctionSummary,
    callee_name: &str,
    index: &RustIndex,
) -> Option<HelperHop> {
    let mut candidates = index
        .functions()
        .iter()
        .filter(|function| function.name == callee_name);
    let callee = candidates.next()?;
    if candidates.next().is_some()
        || callee.item.container != FunctionContainer::Free
        || !callee.item.has_body
    {
        return None;
    }
    if !in_root_library(index, caller) || !in_root_library(index, callee) {
        return None;
    }
    let caller_module = call_identity::root_library_member_module_path(index, caller)?;
    let callee_module = call_identity::root_library_item_module_path(index, callee)?;

    let mut mentions = caller.calls.iter().filter(|call| call.name == callee_name);
    let call = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&call.text);
    let (qualifier, paren) = sole_qualified_call(&masked, callee_name)?;
    let source = index.files().get(&caller.file)?.source.to_string();
    let resolved = resolve_qualifier(&qualifier, &caller_module, &source)?;
    if resolved != callee_module {
        return None;
    }
    let arguments = split_call_arguments_at_paren(&call.text, paren)?;
    Some(HelperHop {
        caller: caller.clone(),
        call_text: call.text.to_string(),
        arguments,
        receiver: None,
        through_macro: false,
    })
}

/// Whether `function` is an item of the root package's library: a `src/`
/// file of the root package composing under `src/lib.rs`.
fn in_root_library(index: &RustIndex, function: &FunctionSummary) -> bool {
    !related_tests::path_is_absolute_form(&function.file)
        && !call_identity::under_tests_dir(&function.file)
        && related_tests::package_prefix(&function.file).is_none()
        && super::super::owner_pin::target_root(&function.file, index, &BTreeSet::new())
            .is_some_and(|root| root == Path::new("src/lib.rs"))
}

/// The qualifier segments and the byte index of the argument-list paren of
/// the only mention of `callee` in `masked`, when that mention is a path
/// qualified call (`a::b::callee(`). `None` for a bare, method, generic or
/// repeated mention.
fn sole_qualified_call(masked: &str, callee: &str) -> Option<(Vec<String>, usize)> {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut found = None;
    for (at, _) in masked.match_indices(callee) {
        if masked[..at].chars().next_back().is_some_and(is_word)
            || masked[at + callee.len()..]
                .chars()
                .next()
                .is_some_and(is_word)
        {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(at);
    }
    let at = found?;
    let after = &masked[at + callee.len()..];
    if !after.starts_with('(') {
        return None;
    }
    let paren = at + callee.len();
    let mut rest = &masked[..at];
    let mut segments = Vec::new();
    while let Some(head) = rest.strip_suffix("::") {
        let start = head
            .rfind(|ch: char| !is_word(ch))
            .map_or(0, |index| index + 1);
        let segment = &head[start..];
        if segment.is_empty() || segment.starts_with(|ch: char| ch.is_ascii_digit()) {
            return None;
        }
        segments.push(segment.to_string());
        rest = &head[..start];
    }
    // A leading `::`, a generic argument list or a receiver before the path
    // is not a plain own-crate module path.
    if segments.is_empty() || rest.ends_with([':', '.', '>', '<']) {
        return None;
    }
    segments.reverse();
    Some((segments, paren))
}

/// The module path `qualifier` names when written in `caller_module`, or
/// `None` when it cannot be read (fails closed).
fn resolve_qualifier(
    qualifier: &[String],
    caller_module: &[String],
    caller_source: &str,
) -> Option<Vec<String>> {
    let mut segments = qualifier.iter().map(String::as_str).peekable();
    let mut base: Vec<String> = caller_module.to_vec();
    match segments.peek().copied()? {
        "crate" => {
            segments.next();
            base.clear();
        }
        "self" => {
            segments.next();
        }
        "super" => {
            while segments.peek() == Some(&"super") {
                segments.next();
                base.pop()?;
            }
        }
        first => {
            // A bare first segment names a child module of this module only
            // when no import may bind the name to something else.
            if first.starts_with("r#")
                || super::super::activation::name_imported_in_file(caller_source, first)
            {
                return None;
            }
        }
    }
    for segment in segments {
        if matches!(segment, "crate" | "self" | "super") || segment.starts_with("r#") {
            return None;
        }
        base.push(segment.to_string());
    }
    Some(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;
    use std::path::PathBuf;

    fn index_of(files: &[(&str, &str)]) -> RustIndex {
        let mut index = RustIndex::default();
        for (path, text) in files {
            let facts = summarize_file(PathBuf::from(path), (*text).to_string());
            index.extend_functions(facts.functions.iter().cloned());
            index.extend_tests(facts.tests.iter().cloned());
            index.insert_file_only(PathBuf::from(path), facts);
        }
        crate::analysis::facts::compose_fixture_source_roles(&mut index, Path::new(""));
        index
    }

    const EVAL: &str = "pub(crate) fn matches_req(r: &u64, v: u64) -> bool {\n    v >= *r\n}\n";

    fn hop_names(files: &[(&str, &str)]) -> Option<Vec<String>> {
        let index = index_of(files);
        let caller = index
            .functions()
            .iter()
            .find(|function| function.name == "matches")?;
        let hop = qualified_free_hop(caller, "matches_req", &index)?;
        Some(hop.arguments)
    }

    fn lib(call: &str, prelude: &str) -> String {
        format!(
            "mod eval;\n{prelude}pub struct Req(u64);\nimpl Req {{\n    pub fn matches(&self, v: u64) -> bool {{\n        {call}\n    }}\n}}\n"
        )
    }

    #[test]
    fn a_child_module_path_to_the_unique_callee_binds_the_hop() {
        let source = lib("eval::matches_req(&self.0, v)", "");
        assert_eq!(
            hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]),
            Some(vec!["&self.0".to_string(), "v".to_string()])
        );
        let source = lib("crate::eval::matches_req(&self.0, v)", "");
        assert!(hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]).is_some());
        let source = lib("self::eval::matches_req(&self.0, v)", "");
        assert!(hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]).is_some());
    }

    #[test]
    fn a_path_naming_another_module_declines() {
        let source = lib("other::matches_req(&self.0, v)", "mod other;\n");
        assert_eq!(
            hop_names(&[
                ("src/lib.rs", &source),
                ("src/eval.rs", EVAL),
                ("src/other.rs", "pub fn unrelated() {}\n"),
            ]),
            None
        );
        // `super::` from the crate root has no parent.
        let source = lib("super::eval::matches_req(&self.0, v)", "");
        assert_eq!(
            hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]),
            None
        );
    }

    #[test]
    fn an_import_of_the_first_segment_declines() {
        let source = lib("eval::matches_req(&self.0, v)", "use other::eval;\n");
        assert_eq!(
            hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]),
            None
        );
    }

    #[test]
    fn a_bare_method_generic_or_repeated_mention_declines() {
        for call in [
            "matches_req(&self.0, v)",
            "self.0.matches_req(v)",
            "<Req>::matches_req(&self.0, v)",
            "eval::matches_req(&self.0, v) && eval::matches_req(&self.0, v)",
            "std::eval::matches_req(&self.0, v)",
        ] {
            let source = lib(call, "");
            assert_eq!(
                hop_names(&[("src/lib.rs", &source), ("src/eval.rs", EVAL)]),
                None,
                "{call}"
            );
        }
    }

    #[test]
    fn a_same_named_callee_in_a_second_module_declines() {
        let source = lib("eval::matches_req(&self.0, v)", "mod twin;\n");
        assert_eq!(
            hop_names(&[
                ("src/lib.rs", &source),
                ("src/eval.rs", EVAL),
                ("src/twin.rs", EVAL),
            ]),
            None
        );
    }
}
