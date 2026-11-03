//! A hop whose call is spelled through another workspace crate (#11401).
//!
//! `app::checkout` calls `wscore::discount(price)`: the call is a path
//! qualified invocation, and neither the bare name nor the spelled path alone
//! is callee identity. The hop binds here only when the manifests say that
//! the caller's package depends, in its `[dependencies]`, on the package that
//! holds the unique callee, under the callee crate's own name.
//!
//! Fail-closed admit, all of:
//!
//! - the callee is the one function of its name in the workspace (the chain
//!   resolver checks this first), a body-carrying free `pub fn` declared at
//!   the top level of its package's library root file, so the path
//!   `crate_name::callee` names it;
//! - the manifests give exactly one name for the callee's library in the
//!   caller's package: a `[dependencies]` path entry (never a
//!   `[dev-dependencies]` or `[build-dependencies]` one, which library code
//!   cannot see), whose key is the callee's own library identifier (a renamed
//!   dependency is not resolved here), between two different packages;
//! - the caller mentions the callee name exactly once, in one call fact, as
//!   `<crate_name>::<callee>(..)` with exactly that one qualifier segment;
//! - nothing in the caller's package can rebind `<crate_name>`: no import,
//!   alias, glob, block-local item or module of that name, and no item that
//!   may be spliced in by a macro or an `include`;
//! - the caller invokes no macro that could hide an early return or a
//!   rebinding.
//!
//! The hop carries reach and argument bindings exactly like a same-crate
//! direct call hop; every later rule that decides whether a result is
//! forwarded still reads the caller body.

use super::{HelperHop, split_call_arguments_at_paren};
use crate::analysis::classify::{call_identity, related_tests};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex};

pub(super) fn cross_crate_free_hop(
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
        || callee.file == caller.file
        || !index.member_crates.has_manifest_root()
    {
        return None;
    }
    if related_tests::path_is_absolute_form(&callee.file)
        || related_tests::path_is_absolute_form(&caller.file)
        || call_identity::under_tests_dir(&callee.file)
        || call_identity::under_tests_dir(&caller.file)
        || caller.source_role.registers_executable_test()
    {
        return None;
    }
    let source = index.files().get(&caller.file)?.source.to_string();
    if !is_unrestricted_root_pub_fn(index, callee) {
        return None;
    }
    let crate_name = index
        .member_crates
        .library_dependency_name(&caller.file, &callee.file)?;

    let mut mentions = caller.calls.iter().filter(|call| call.name == callee_name);
    let call = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&call.text);
    let paren = sole_crate_qualified_call(&masked, &crate_name, callee_name)?;
    if super::caller_invokes_opaque_macro(&caller.body, true) {
        return None;
    }
    if super::qualified_hop::namespace_may_rebind(&source, &crate_name)
        || package_declares_name(index, caller, &crate_name)
    {
        return None;
    }
    let arguments = split_call_arguments_at_paren(&call.text, paren)?;
    Some(HelperHop {
        caller: caller.clone(),
        call_text: call.text.to_string(),
        arguments,
        receiver: None,
        through_macro: false,
        module_qualified: true,
        crate_qualifier: Some(crate_name),
        transparent_macros_rebound: super::transparent_macro_name_rebound(&caller.body, index),
        diverging_handlers: Vec::new(),
    })
}

/// Whether `callee` is declared `pub` (not `pub(..)`) at the top level of its
/// file, outside every module block and every impl, so that with the file as
/// its package's library root the path `crate_name::callee` names it.
fn is_unrestricted_root_pub_fn(index: &RustIndex, callee: &FunctionSummary) -> bool {
    let Some(facts) = index.files().get(&callee.file) else {
        return false;
    };
    let masked = crate::analysis::extract::mask_comments_and_strings(&facts.source);
    let depth_before = masked
        .lines()
        .take(callee.start_line.saturating_sub(1))
        .flat_map(str::chars)
        .fold(0usize, |depth, ch| match ch {
            '{' => depth + 1,
            '}' => depth.saturating_sub(1),
            _ => depth,
        });
    if depth_before != 0 {
        return false;
    }
    let declaration = masked
        .lines()
        .skip(callee.start_line.saturating_sub(1))
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("#["));
    let Some(rest) = declaration.and_then(|line| line.strip_prefix("pub")) else {
        return false;
    };
    rest.starts_with(char::is_whitespace) && {
        let rest = rest.trim_start();
        !rest.starts_with('(') && rest.starts_with("fn ")
    }
}

/// The byte index of the argument-list paren of the only mention of `callee`
/// in `masked`, when that mention is exactly `<crate_name>::<callee>(` with
/// nothing before the crate name that makes it a path continuation, a method
/// call or a generic argument.
fn sole_crate_qualified_call(masked: &str, crate_name: &str, callee: &str) -> Option<usize> {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii();
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
    if !masked[at + callee.len()..].starts_with('(') {
        return None;
    }
    let head = masked[..at].strip_suffix("::")?;
    let before_name = head.strip_suffix(crate_name)?;
    if before_name.chars().next_back().is_some_and(is_word)
        || before_name.ends_with([':', '.', '>', '<', '#'])
    {
        return None;
    }
    Some(at + callee.len())
}

/// Whether any file of the caller's package declares a module, function or
/// other item that could give `name` a different meaning than the extern
/// crate: a conservative scan of the masked sources for `mod <name>`.
fn package_declares_name(index: &RustIndex, caller: &FunctionSummary, name: &str) -> bool {
    let package = related_tests::package_prefix(&caller.file);
    index.files().iter().any(|(path, facts)| {
        if related_tests::package_prefix(path) != package {
            return false;
        }
        let masked = crate::analysis::extract::mask_comments_and_strings(&facts.source);
        masked
            .match_indices(name)
            .any(|(at, _)| names_a_module(&masked, at, name.len()))
    })
}

/// Whether the identifier at `[at, at + len)` follows the `mod` keyword.
fn names_a_module(masked: &str, at: usize, len: usize) -> bool {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii();
    if masked[at + len..].chars().next().is_some_and(is_word) {
        return false;
    }
    let before = masked[..at].trim_end();
    before
        .strip_suffix("mod")
        .is_some_and(|rest| !rest.chars().next_back().is_some_and(is_word))
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
        index
    }

    fn declared_pub_at_root(source: &str) -> Result<bool, String> {
        let index = index_of(&[("wscore/src/lib.rs", source)]);
        let callee = index
            .functions()
            .iter()
            .find(|function| function.name == "discount")
            .ok_or("fixture must define `discount`")?;
        Ok(is_unrestricted_root_pub_fn(&index, callee))
    }

    #[test]
    fn only_an_unrestricted_pub_fn_at_the_file_root_is_nameable_by_the_crate_path()
    -> Result<(), String> {
        let body = "(p: u32) -> u32 {\n    p\n}\n";
        assert!(declared_pub_at_root(&format!("pub fn discount{body}"))?);
        assert!(declared_pub_at_root(&format!(
            "#[inline]\n/// Docs.\npub fn discount{body}"
        ))?);
        for (label, source) in [
            ("private", format!("fn discount{body}")),
            ("crate-visible", format!("pub(crate) fn discount{body}")),
            ("restricted", format!("pub(super) fn discount{body}")),
            (
                "in a module",
                format!("pub mod pricing {{\n    pub fn discount{body}}}\n"),
            ),
            (
                "in a private module",
                format!("mod pricing {{\n    pub fn discount{body}}}\n"),
            ),
        ] {
            assert!(!declared_pub_at_root(&source)?, "{label}");
        }
        Ok(())
    }

    #[test]
    fn only_a_sole_call_spelled_through_the_crate_name_binds() {
        let call = |text: &str| sole_crate_qualified_call(text, "wscore", "discount");
        assert_eq!(call("wscore::discount(price)"), Some(16));
        assert_eq!(call("    wscore::discount(price)"), Some(20));
        for (label, text) in [
            ("bare", "discount(price)"),
            ("method", "x.discount(price)"),
            ("other crate", "legacy::discount(price)"),
            ("longer path", "app::wscore::discount(price)"),
            ("leading colons", "::wscore::discount(price)"),
            ("prefixed name", "xwscore::discount(price)"),
            ("generic argument", "Vec<wscore::discount(price)"),
            ("two calls", "wscore::discount(a) + wscore::discount(b)"),
            ("not called", "wscore::discount"),
            ("longer callee", "wscore::discounted(price)"),
        ] {
            assert_eq!(call(text), None, "{label}");
        }
    }

    #[test]
    fn a_module_named_like_the_crate_is_seen_but_a_longer_name_is_not() {
        let named = |source: &str| {
            source
                .match_indices("wscore")
                .any(|(at, _)| names_a_module(source, at, "wscore".len()))
        };
        assert!(named("mod wscore {}"));
        assert!(named("pub mod wscore;"));
        assert!(!named("mod wscore_ext {}"));
        assert!(!named("let wscore = 1;"));
        assert!(!named("module wscore"));
    }
}
