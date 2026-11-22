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
use std::collections::{BTreeMap, BTreeSet, HashSet};
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
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

/// One decision per owner, in `owners` order, batched per file through
/// [`decide_in_file`].
fn decide(index: &RustIndex, owners: &[&FunctionFact]) -> Vec<Decision> {
    let mut decisions: Vec<Decision> = vec![None; owners.len()];
    let mut by_file: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
    for (position, function) in owners.iter().enumerate() {
        by_file
            .entry(function.file.as_path())
            .or_default()
            .push(position);
    }
    let files = index.files();
    for (file, positions) in by_file {
        let in_file: Vec<&FunctionFact> = positions.iter().map(|&p| owners[p]).collect();
        let decided = match files.get(file) {
            Some(data) => decide_in_file(&data.source, data.used_lexical_fallback, &in_file),
            // No file facts: only the owners' own attributes can decide.
            None => decide_in_file("", true, &in_file),
        };
        for (position, decision) in positions.into_iter().zip(decided) {
            decisions[position] = decision;
        }
    }
    decisions
}

/// The one owner opt-out decision, for owners that all live in one file
/// whose text is `source`, in `owners` order. Both the diff-scope findings
/// ([`rust_owner_opt_outs`]) and the repo-scope seam inventory
/// ([`FileOwnerOptOuts`], #14139, #15000) consume it.
///
/// The owner's own attributes decide first; otherwise the innermost
/// enclosing scope that carries a skip decides. The file is parsed only when
/// it was read by the parser and its text spells `mutants`; a lexically read
/// owner has empty `attrs` and its file yields no scopes, so it is never
/// opted out.
fn decide_in_file(
    source: &str,
    used_lexical_fallback: bool,
    owners: &[&FunctionFact],
) -> Vec<Decision> {
    let mut decisions: Vec<Decision> = owners
        .iter()
        .map(|function| {
            attributes_define_mutants_skip(function.attrs.iter().map(String::as_str))
                .then(|| (OwnerOptOutScope::Fn, String::new()))
        })
        .collect();
    if used_lexical_fallback || !source.contains("mutants") {
        return decisions;
    }
    let positions: Vec<usize> = (0..owners.len())
        .filter(|&position| decisions[position].is_none())
        .collect();
    if positions.is_empty() {
        return decisions;
    }
    let wanted: Vec<(&str, usize)> = positions
        .iter()
        .map(|&position| (owners[position].name.as_str(), owners[position].start_line))
        .collect();
    let scopes = enclosing_scopes(source, &wanted);
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
    decisions
}

/// The repo-scope owner opt-outs of one file's functions (#14139, #15000):
/// the seam inventory builds one per production file and asks it for each
/// seam owner, so a repo seam is withheld exactly when [`decide_in_file`]
/// (the decision `ripr check` applies to diff findings) opts its owner out,
/// whether the owner itself or an enclosing `fn`, `impl`, `trait`, inline
/// module or file carries the skip. The file is parsed at most once.
#[derive(Debug, Default)]
pub(crate) struct FileOwnerOptOuts {
    /// The opted-out functions, by address: the facts are borrowed from the
    /// index for the whole inventory pass, and identity (not name or line)
    /// keeps two same-line functions apart.
    opted_out: HashSet<*const FunctionFact>,
}

impl FileOwnerOptOuts {
    /// Decide every function in `functions`, all from the file whose text is
    /// `source`.
    pub(crate) fn decide<'a>(
        source: &str,
        used_lexical_fallback: bool,
        functions: impl IntoIterator<Item = &'a FunctionFact>,
    ) -> Self {
        let owners: Vec<&FunctionFact> = functions.into_iter().collect();
        let decisions = decide_in_file(source, used_lexical_fallback, &owners);
        let opted_out = owners
            .into_iter()
            .zip(decisions)
            .filter(|(_, decision)| decision.is_some())
            .map(|(function, _)| std::ptr::from_ref(function))
            .collect();
        Self { opted_out }
    }

    /// The opt-out `owner` carries, if any. `owner` must be one of the
    /// functions this was decided over; any other function is not opted out.
    pub(crate) fn reason(&self, owner: &FunctionFact) -> Option<OwnerOptOutReason> {
        self.opted_out
            .contains(&std::ptr::from_ref(owner))
            .then_some(OwnerOptOutReason::MutantsSkip)
    }
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
/// the seam at extraction from [`FileOwnerOptOuts`]) and count them.
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
    use super::{FileOwnerOptOuts, OwnerOptOutReason, OwnerOptOutScope, decide_in_file};
    use crate::analysis::facts::{FileFacts, FunctionFact};
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

    /// The repo-scope decision ([`FileOwnerOptOuts`]) for the one function
    /// named `name`.
    fn decision(facts: &FileFacts, name: &str) -> Result<Option<OwnerOptOutReason>, String> {
        let decided =
            FileOwnerOptOuts::decide(&facts.source, facts.used_lexical_fallback, &facts.functions);
        facts
            .functions
            .iter()
            .find(|function| function.name == name)
            .map(|function| decided.reason(function))
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

    /// #15000: one source per enclosing kind cargo-mutants honors, each with
    /// an unannotated same-named sibling outside the skipped scope.
    const ENCLOSING: &str = "pub struct Cart;
pub struct Bag;

#[mutants::skip]
impl Cart {
    pub fn limit(&self, x: u32) -> bool {
        x > 3
    }
}

impl Bag {
    pub fn limit(&self, x: u32) -> bool {
        x > 3
    }
}

#[mutants::skip]
pub trait Capped {
    fn capped(&self, x: u32) -> bool {
        x > 3
    }
}

pub trait Open {
    fn capped(&self, x: u32) -> bool {
        x > 3
    }
}

#[mutants::skip]
pub mod skipped {
    pub mod inner {
        pub fn bound(x: u32) -> bool {
            x > 3
        }
    }
}

pub mod kept {
    pub fn bound(x: u32) -> bool {
        x > 3
    }
}

pub fn outer(x: u32) -> bool {
    #![mutants::skip]
    x > 3
}

pub fn plain(x: u32) -> bool {
    x > 3
}
";

    /// Every function of `facts` named `name`, in file order.
    fn named<'a>(facts: &'a FileFacts, name: &str) -> Vec<&'a FunctionFact> {
        facts
            .functions
            .iter()
            .filter(|function| function.name == name)
            .collect()
    }

    /// #15000: the repo-scope decision honors an enclosing `impl`, `trait`,
    /// inline module and the owner's own inner attribute, and leaves each
    /// same-named sibling outside the skipped scope reporting.
    #[test]
    fn repo_decision_honors_enclosing_scopes_but_not_siblings() -> Result<(), String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), ENCLOSING)?;
        assert!(!facts.used_lexical_fallback, "fixture must parse cleanly");
        let decided =
            FileOwnerOptOuts::decide(&facts.source, facts.used_lexical_fallback, &facts.functions);
        let skip = Some(OwnerOptOutReason::MutantsSkip);
        for (name, want) in [
            ("limit", [skip, None]),
            ("capped", [skip, None]),
            ("bound", [skip, None]),
        ] {
            let owners = named(&facts, name);
            assert_eq!(owners.len(), 2, "fixture must index two `{name}` owners");
            let got: Vec<_> = owners.iter().map(|owner| decided.reason(owner)).collect();
            assert_eq!(got, want, "{name}: the skipped scope's owner comes first");
        }
        assert_eq!(decided.reason(named(&facts, "outer")[0]), skip, "outer");
        assert_eq!(decided.reason(named(&facts, "plain")[0]), None, "plain");
        Ok(())
    }

    /// #15000: a file-level inner `#![mutants::skip]` opts out every owner in
    /// the file.
    #[test]
    fn repo_decision_honors_a_file_level_inner_skip() -> Result<(), String> {
        let source =
            format!("#![mutants::skip]\n{ENCLOSING}").replace("    #![mutants::skip]\n", "");
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        assert!(!facts.used_lexical_fallback, "fixture must parse cleanly");
        let decided =
            FileOwnerOptOuts::decide(&facts.source, facts.used_lexical_fallback, &facts.functions);
        assert_eq!(facts.functions.len(), 8, "fixture must index every owner");
        for function in &facts.functions {
            assert_eq!(
                decided.reason(function),
                Some(OwnerOptOutReason::MutantsSkip),
                "{}",
                function.name
            );
        }
        Ok(())
    }

    /// #15000: one decision owner. The repo-scope answer for every function
    /// equals the diff-scope decision ([`decide_in_file`], which
    /// `rust_owner_opt_outs` consumes), and the diff-scope decision names the
    /// scope that carried the skip.
    #[test]
    fn repo_and_diff_scope_decisions_agree() -> Result<(), String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), ENCLOSING)?;
        let owners: Vec<&FunctionFact> = facts.functions.iter().collect();
        let diff = decide_in_file(&facts.source, facts.used_lexical_fallback, &owners);
        let repo =
            FileOwnerOptOuts::decide(&facts.source, facts.used_lexical_fallback, &facts.functions);
        for (owner, decision) in owners.iter().zip(&diff) {
            assert_eq!(
                repo.reason(owner).is_some(),
                decision.is_some(),
                "{}",
                owner.name
            );
        }
        let scopes: Vec<(&str, Option<OwnerOptOutScope>)> = owners
            .iter()
            .zip(&diff)
            .map(|(owner, decision)| (owner.name.as_str(), decision.as_ref().map(|d| d.0)))
            .collect();
        assert_eq!(
            scopes,
            [
                ("limit", Some(OwnerOptOutScope::Impl)),
                ("limit", None),
                ("capped", Some(OwnerOptOutScope::Trait)),
                ("capped", None),
                ("bound", Some(OwnerOptOutScope::Module)),
                ("bound", None),
                ("outer", Some(OwnerOptOutScope::Fn)),
                ("plain", None),
            ]
        );
        Ok(())
    }

    /// A function the decision was not built over is never opted out, even
    /// when an equal fact was.
    #[test]
    fn repo_decision_answers_only_for_the_functions_it_decided() -> Result<(), String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), SOURCE)?;
        let decided =
            FileOwnerOptOuts::decide(&facts.source, facts.used_lexical_fallback, &facts.functions);
        let copy = named(&facts, "fn_level")[0].clone();
        assert_eq!(
            decided.reason(named(&facts, "fn_level")[0]),
            Some(OwnerOptOutReason::MutantsSkip)
        );
        assert_eq!(decided.reason(&copy), None);
        Ok(())
    }
}
