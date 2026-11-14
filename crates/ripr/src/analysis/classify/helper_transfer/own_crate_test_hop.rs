//! A hop from an integration-test helper spelled through the package's own
//! library name (#8387).
//!
//! `tests/translate.rs` holds `fn t(p: &str) -> Hir { regex_syntax::hir::translate(p).unwrap() }`
//! and every test calls `t(..)`. The helper's call is a path-qualified
//! invocation, so the bare-call route declines it; the helper is not an item
//! of the library, so the own-module route ([`super::qualified_hop`]) declines
//! it; and helper and callee share one package, so the cross-crate route
//! ([`super::cross_crate_hop`]) declines it. A test that wrote the same path
//! in its own body is already related (`own_crate_qualified_entry_path`,
//! #8999), so the test the diff adds stayed a name-token neighbour only
//! because it went through its file's helper.
//!
//! Fail-closed admit, all of:
//!
//! - the callee is the one function of its name in the workspace, a
//!   body-carrying free function of the root package's library (`src/lib.rs`
//!   or a `src/` module composed under it, no `#[path]` remap);
//! - the caller is a free function, not itself a registered test, in a
//!   test-crate root of the root package (`tests/<name>.rs` or
//!   `tests/<name>/main.rs`);
//! - the caller mentions the callee name exactly once, in one call fact, as
//!   exactly `<crate>::<module path of the callee>::<callee>(..)`, with
//!   `<crate>` the root manifest's library identifier;
//! - nothing in the caller's file or body can give `<crate>` another meaning
//!   (an import ending in or renamed to it, a glob, a declaration, a binding
//!   or a spliced item), by the same authority that reads a test's own
//!   crate-qualified call;
//! - the caller invokes no macro that could hide an early return or a
//!   rebinding.
//!
//! The hop is `module_qualified` with no `crate_qualifier`: it establishes
//! reach only. It binds no result forwarding, so a test related through it
//! can lead the related-test list but cannot become `exposed` on this hop.

use super::{HelperHop, split_call_arguments_at_paren};
use crate::analysis::classify::{call_identity, related_tests};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn own_crate_test_helper_hop(
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
        || caller.item.container != FunctionContainer::Free
        || caller.source_role.registers_executable_test()
    {
        return None;
    }
    // The callee is an item of the root package's library; the caller sits
    // under that package's `tests/` directory.
    if related_tests::path_is_absolute_form(&callee.file)
        || related_tests::path_is_absolute_form(&caller.file)
        || related_tests::package_prefix(&callee.file).is_some()
        || related_tests::package_prefix(&caller.file).is_some()
        || call_identity::under_tests_dir(&callee.file)
        || !call_identity::under_tests_dir(&caller.file)
        || super::super::owner_pin::target_root(&callee.file, index, &BTreeSet::new())
            .is_none_or(|root| root != Path::new("src/lib.rs"))
    {
        return None;
    }
    let module = call_identity::root_library_item_module_path(index, callee)?;
    let crate_name = index.root_library_name.as_deref()?;
    let qualifier = std::iter::once(crate_name)
        .chain(module.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::");

    let mut mentions = caller.calls.iter().filter(|call| call.name == callee_name);
    let call = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&call.text);
    let paren =
        super::cross_crate_hop::sole_crate_qualified_call(&masked, &qualifier, callee_name)?;
    if super::caller_invokes_opaque_macro(&caller.body, true) {
        return None;
    }
    // An ordinary import from the library (`use <crate>::Type;`) leaves the
    // crate name the library, so the file is read by the test-entry
    // authority, not by the stricter any-mention scan of the library routes.
    if !related_tests::own_crate_name_unshadowed_in_test_crate_root(
        index,
        &caller.file,
        &caller.body,
        crate_name,
    ) {
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
        crate_qualifier: None,
        transparent_macros_rebound: super::transparent_macro_name_rebound(&caller.body, index),
        diverging_handlers: Vec::new(),
    })
}
