//! Owner opt-outs read from the changed owner's attributes and the scopes
//! that enclose it (#13819, #13910).
//!
//! A cargo-mutants codebase marks code it deliberately does not
//! mutation-test with `#[mutants::skip]` (or
//! `#[cfg_attr(<predicate>, mutants::skip)]`). cargo-mutants honors the
//! attribute on the function itself, on an enclosing `impl` or `trait` block,
//! on an enclosing inline module and as an inner `#![mutants::skip]` at the
//! top of the file, and mutates nothing inside a skipped scope. The facts
//! normalizer owns the attribute recognition
//! ([`attributes_define_mutants_skip`]) and the parser owns the ancestry
//! ([`enclosing_scopes`]); this module only pairs each Rust finding with its
//! probe owner and reports which findings that owner, or a scope enclosing
//! it, opted out of.
//!
//! The fact is static and conservative. It changes no class, adds no credit
//! and makes no runtime claim: the app layer withholds the named findings
//! from actionable output and discloses the count, scope and reason.
//!
//! Only parser-backed attributes count. The lexical fallback leaves a
//! function's `attrs` empty and a file that does not parse cleanly yields no
//! enclosing scopes, so an owner read that way is never opted out and its
//! findings keep reporting (the safe direction for a disclosure). A
//! `#[mutants::skip] mod name;` declaration is not followed into the module's
//! own file (a documented bound; the miss keeps findings reporting).

use crate::analysis::facts::{FunctionFact, RustIndex, attributes_define_mutants_skip};
use crate::analysis::syntax::{EnclosingScopeKind, enclosing_scopes};
use crate::domain::Finding;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Why an owner's findings are withheld.
///
/// Serialized (as the same stable token [`Self::as_str`] returns) because a
/// repo seam carries its owner's opt-out through the classified seam caches
/// (#14139).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum OwnerOptOutReason {
    /// The owner, or a scope enclosing it, carries `#[mutants::skip]`,
    /// directly or through `cfg_attr`.
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

/// Which scope carried the opt-out attribute (#13910).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OwnerOptOutScope {
    /// The owner function itself, or a function whose body declares it.
    Fn,
    /// An enclosing `impl` block.
    Impl,
    /// An enclosing `trait` block.
    Trait,
    /// An enclosing inline module.
    Module,
    /// The owner's file, through an inner `#![mutants::skip]`.
    File,
}

impl OwnerOptOutScope {
    /// Stable machine token, shared by human, explain and JSON output.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Impl => "impl",
            Self::Trait => "trait",
            Self::Module => "module",
            Self::File => "file",
        }
    }

    fn from_enclosing(kind: EnclosingScopeKind) -> Self {
        match kind {
            EnclosingScopeKind::Fn => Self::Fn,
            EnclosingScopeKind::Impl => Self::Impl,
            EnclosingScopeKind::Trait => Self::Trait,
            EnclosingScopeKind::Module => Self::Module,
            EnclosingScopeKind::File => Self::File,
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
    /// The scope that carried the attribute.
    pub(crate) scope: OwnerOptOutScope,
    /// That scope's name as written: an enclosing function's, trait's or
    /// module's name, the impl's `Type` or `Trait for Type`; empty for the
    /// owner function itself and for the file.
    pub(crate) scope_label: String,
}

/// The opt-out one parser-indexed owner function carries, if any: the single
/// owner decision both the diff-scope findings ([`rust_owner_opt_outs`]) and
/// the repo-scope seam inventory (#14139) consume. A lexically read function
/// has empty `attrs`, so it is never opted out.
pub(crate) fn function_owner_opt_out(function: &FunctionFact) -> Option<OwnerOptOutReason> {
    attributes_define_mutants_skip(function.attrs.iter().map(String::as_str))
        .then_some(OwnerOptOutReason::MutantsSkip)
}

/// The opt-out decided for one owner function: its scope and that scope's
/// label, or `None` when nothing opts it out.
type Decision = Option<(OwnerOptOutScope, String)>;

/// Owner opt-outs for `findings`, in `findings` order. A finding with no
/// probe owner, or whose owner is not a parser-indexed function, is never
/// opted out. When several indexed functions share the owner's id, every one
/// of them must be opted out (fail closed on an ambiguous owner).
pub(crate) fn rust_owner_opt_outs(index: &RustIndex, findings: &[Finding]) -> Vec<OwnerOptOut> {
    let owner_ids: BTreeSet<&str> = findings
        .iter()
        .filter_map(|finding| finding.probe.owner.as_ref())
        .map(|owner| owner.0.as_str())
        .collect();
    if owner_ids.is_empty() {
        return Vec::new();
    }
    let owners: Vec<&FunctionFact> = index
        .functions()
        .iter()
        .filter(|function| owner_ids.contains(function.id.0.as_str()))
        .collect();
    let decisions = decide(index, &owners);
    let mut by_id: BTreeMap<&str, Option<(&str, &Decision)>> = BTreeMap::new();
    for (function, decision) in owners.iter().zip(&decisions) {
        let entry = by_id
            .entry(function.id.0.as_str())
            .or_insert(Some((function.name.as_str(), decision)));
        if decision.is_none() {
            *entry = None;
        }
    }
    findings
        .iter()
        .filter_map(|finding| {
            let owner = finding.probe.owner.as_ref()?;
            let (name, decision) = (*by_id.get(owner.0.as_str())?)?;
            let (scope, scope_label) = decision.clone()?;
            Some(OwnerOptOut {
                finding_id: finding.id.clone(),
                owner: name.to_string(),
                reason: OwnerOptOutReason::MutantsSkip,
                scope,
                scope_label,
            })
        })
        .collect()
}

/// One decision per owner, in `owners` order. The owner's own attributes
/// decide first; otherwise the innermost enclosing scope that carries a skip
/// decides. A file is parsed only when its text spells `mutants`.
fn decide(index: &RustIndex, owners: &[&FunctionFact]) -> Vec<Decision> {
    let mut decisions: Vec<Decision> = owners
        .iter()
        .map(|function| {
            attributes_define_mutants_skip(function.attrs.iter().map(String::as_str))
                .then(|| (OwnerOptOutScope::Fn, String::new()))
        })
        .collect();
    let mut by_file: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
    for (position, function) in owners.iter().enumerate() {
        if decisions[position].is_none() {
            by_file
                .entry(function.file.as_path())
                .or_default()
                .push(position);
        }
    }
    let files = index.files();
    for (file, positions) in by_file {
        let Some(data) = files.get(file) else {
            continue;
        };
        if data.used_lexical_fallback || !data.source.contains("mutants") {
            continue;
        }
        let wanted: Vec<(&str, usize)> = positions
            .iter()
            .map(|&position| (owners[position].name.as_str(), owners[position].start_line))
            .collect();
        let scopes = enclosing_scopes(&data.source, &wanted);
        for (&position, scopes) in positions.iter().zip(scopes) {
            decisions[position] = scopes.into_iter().flatten().find_map(|scope| {
                attributes_define_mutants_skip(scope.attrs.iter().map(String::as_str)).then(|| {
                    (
                        OwnerOptOutScope::from_enclosing(scope.kind),
                        scope.label.clone(),
                    )
                })
            });
        }
    }
    decisions
}

/// Seams a repo-scope format withheld because their owner opted out
/// (#14139). The count and the withheld seam ids are disclosed beside the
/// format's own counts, which no longer include those seams; the ids let a
/// snapshot comparison tell a withheld seam from a removed one. Nothing
/// withheld renders nothing, so a workspace without the attribute keeps
/// byte-identical repo output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepoOwnerOptOuts {
    /// Ids of the seams removed from the inventory the format renders,
    /// sorted and unique.
    pub(crate) seam_ids: Vec<String>,
    /// The withheld seams' owners (`RepoSeam::owner`, `file::name`), sorted
    /// and unique. A seam id hashes its byte offset, so adding the attribute
    /// line moves the owner's ids; a snapshot comparison matches the owner
    /// instead.
    pub(crate) owners: Vec<String>,
}

impl RepoOwnerOptOuts {
    /// How many seams were withheld.
    pub(crate) fn withheld(&self) -> usize {
        self.seam_ids.len()
    }

    /// Whether nothing was withheld (the format renders no disclosure).
    pub(crate) fn is_empty(&self) -> bool {
        self.seam_ids.is_empty()
    }

    /// The withholding reason. `MutantsSkip` is the only reason a seam
    /// carries today.
    pub(crate) fn reason(&self) -> OwnerOptOutReason {
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
    let mut seam_ids = std::collections::BTreeSet::new();
    let mut owners = std::collections::BTreeSet::new();
    entries.retain(|entry| {
        let seam = seam_of(entry);
        if seam.owner_opt_out().is_none() {
            return true;
        }
        seam_ids.insert(seam.id().as_str().to_string());
        owners.insert(seam.owner().to_string());
        false
    });
    RepoOwnerOptOuts {
        seam_ids: seam_ids.into_iter().collect(),
        owners: owners.into_iter().collect(),
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
