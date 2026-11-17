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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OwnerOptOutReason {
    /// The owner, or a scope enclosing it, carries `#[mutants::skip]`,
    /// directly or through `cfg_attr`.
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
