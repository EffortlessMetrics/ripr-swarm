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

use crate::analysis::facts::{FunctionFact, RustIndex, attributes_define_mutants_skip};
use crate::domain::Finding;
use std::collections::BTreeMap;

/// Why an owner's findings are withheld.
///
/// Serialized (as the same stable token [`Self::as_str`] returns) because a
/// repo seam carries its owner's opt-out through the classified seam caches
/// (#14139).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum OwnerOptOutReason {
    /// The owner carries `#[mutants::skip]`, directly or through `cfg_attr`.
    #[serde(rename = "owner_mutants_skip")]
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

/// The opt-out one parser-indexed owner function carries, if any: the single
/// owner decision both the diff-scope findings ([`rust_owner_opt_outs`]) and
/// the repo-scope seam inventory (#14139) consume. A lexically read function
/// has empty `attrs`, so it is never opted out.
pub(crate) fn function_owner_opt_out(function: &FunctionFact) -> Option<OwnerOptOutReason> {
    attributes_define_mutants_skip(function.attrs.iter().map(String::as_str))
        .then_some(OwnerOptOutReason::MutantsSkip)
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
        .filter(|function| function_owner_opt_out(function).is_some())
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

/// Seams a repo-scope format withheld because their owner opted out
/// (#14139). The count is disclosed beside the format's own counts, which no
/// longer include those seams. `withheld == 0` renders nothing, so a
/// workspace without the attribute keeps byte-identical repo output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepoOwnerOptOuts {
    /// Seams removed from the inventory the format renders.
    pub(crate) withheld: usize,
}

impl RepoOwnerOptOuts {
    /// Whether nothing was withheld (the format renders no disclosure).
    pub(crate) fn is_empty(self) -> bool {
        self.withheld == 0
    }

    /// The withholding reason. `MutantsSkip` is the only reason a seam
    /// carries today.
    pub(crate) fn reason(self) -> OwnerOptOutReason {
        OwnerOptOutReason::MutantsSkip
    }
}

/// Remove the entries whose seam owner opted out (the decision recorded on
/// the seam at extraction from [`function_owner_opt_out`]) and count them.
/// The one repo-scope withholding call: every `repo-*` format runs its
/// owned inventory through it before rendering, so their counts agree.
pub(crate) fn withhold_repo_owner_opt_outs<T>(
    entries: &mut Vec<T>,
    seam_of: impl Fn(&T) -> &crate::analysis::seams::RepoSeam,
) -> RepoOwnerOptOuts {
    let before = entries.len();
    entries.retain(|entry| seam_of(entry).owner_opt_out().is_none());
    RepoOwnerOptOuts {
        withheld: before.saturating_sub(entries.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::{OwnerOptOutReason, function_owner_opt_out};
    use crate::analysis::facts::FileFacts;
    use crate::analysis::syntax::{
        LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter,
    };
    use std::path::Path;

    const SOURCE: &str = "#[mutants::skip]
pub fn fn_level(level: u32) -> bool {
    level > 3
}

#[doc = \"mutants::skip\"]
pub fn documented(level: u32) -> bool {
    level > 3
}

#[cfg_attr(test, mutants::skip)]
pub fn gated(level: u32) -> bool {
    level > 3
}

pub fn control(level: u32) -> bool {
    level > 3
}
";

    fn decision(facts: &FileFacts, name: &str) -> Result<Option<OwnerOptOutReason>, String> {
        facts
            .functions
            .iter()
            .find(|function| function.name == name)
            .map(function_owner_opt_out)
            .ok_or_else(|| format!("fixture must index `{name}`"))
    }

    /// #14139: the one owner decision the repo-scope seam extraction reads.
    /// Parser-backed attributes opt out `fn_level` and the `cfg_attr` form;
    /// a doc string naming the path and an unattributed owner do not.
    #[test]
    fn parser_backed_owner_decision_reads_only_a_real_mutants_skip() -> Result<(), String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), SOURCE)?;
        assert!(!facts.used_lexical_fallback, "fixture must parse cleanly");
        let skip = Some(OwnerOptOutReason::MutantsSkip);
        assert_eq!(decision(&facts, "fn_level")?, skip);
        assert_eq!(decision(&facts, "gated")?, skip);
        assert_eq!(decision(&facts, "documented")?, None);
        assert_eq!(decision(&facts, "control")?, None);
        Ok(())
    }

    /// A lexically read owner carries no parser-backed attributes, so the
    /// same source never opts it out.
    #[test]
    fn lexically_read_owner_is_never_opted_out() -> Result<(), String> {
        let facts = LexicalRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), SOURCE)?;
        assert!(
            facts.used_lexical_fallback,
            "fixture must exercise the lexical fallback"
        );
        for name in ["fn_level", "gated", "documented", "control"] {
            assert_eq!(decision(&facts, name)?, None, "{name}");
        }
        Ok(())
    }
}
