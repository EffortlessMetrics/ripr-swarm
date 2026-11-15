//! Owner opt-outs read from the changed owner's own attributes (#13819).
//!
//! A cargo-mutants codebase marks a function it deliberately does not
//! mutation-test with `#[mutants::skip]` (or
//! `#[cfg_attr(<predicate>, mutants::skip)]`). The facts normalizer owns the
//! attribute recognition ([`attributes_define_mutants_skip`]); this module
//! only pairs each Rust finding with its probe owner and reports which
//! findings that owner opted out of.
//!
//! The fact is static and conservative. It changes no class, adds no credit
//! and makes no runtime claim: the app layer withholds the named findings
//! from actionable output and discloses the count and reason.
//!
//! Only parser-backed attributes count. The lexical fallback leaves a
//! function's `attrs` empty, so an owner read that way is never opted out and
//! its findings keep reporting (the safe direction for a disclosure).

use crate::analysis::facts::{RustIndex, attributes_define_mutants_skip};
use crate::domain::Finding;
use std::collections::BTreeMap;

/// Why an owner's findings are withheld.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OwnerOptOutReason {
    /// The owner carries `#[mutants::skip]`, directly or through `cfg_attr`.
    MutantsSkip,
}

impl OwnerOptOutReason {
    /// Stable machine token, shared by human and JSON output.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MutantsSkip => "owner_mutants_skip",
        }
    }
}

/// One finding withheld because its owner opted out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnerOptOut {
    /// The withheld finding's id.
    pub(crate) finding_id: String,
    /// The owner function's name as written.
    pub(crate) owner: String,
    pub(crate) reason: OwnerOptOutReason,
}

/// Owner opt-outs for `findings`, in `findings` order. A finding with no
/// probe owner, or whose owner is not a parser-indexed function, is never
/// opted out.
pub(crate) fn rust_owner_opt_outs(index: &RustIndex, findings: &[Finding]) -> Vec<OwnerOptOut> {
    if findings.is_empty() {
        return Vec::new();
    }
    let skipped_owners: BTreeMap<&str, &str> = index
        .functions()
        .iter()
        .filter(|function| {
            attributes_define_mutants_skip(function.attrs.iter().map(String::as_str))
        })
        .map(|function| (function.id.0.as_str(), function.name.as_str()))
        .collect();
    if skipped_owners.is_empty() {
        return Vec::new();
    }
    findings
        .iter()
        .filter_map(|finding| {
            let owner = finding.probe.owner.as_ref()?;
            let name = skipped_owners.get(owner.0.as_str())?;
            Some(OwnerOptOut {
                finding_id: finding.id.clone(),
                owner: (*name).to_string(),
                reason: OwnerOptOutReason::MutantsSkip,
            })
        })
        .collect()
}
