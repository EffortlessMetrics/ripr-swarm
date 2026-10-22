//! Disposition rule v1: a published, deterministic rule over final tree effect
//! and path class. Facts come from Git capture and the #2766 packet; nothing
//! here reads a commit's diff for meaning, so the ledger states
//! `adjudication_basis: rule_derived` and never claims per-commit review.
//!
//! Evaluation order (first match wins):
//!
//! 0. A first-parent commit with two parents, or any commit with an empty
//!    diff, is `blocking_unresolved`: the rule cannot judge it.
//! 1. A commit reached only through a merge (not first-parent) is
//!    `included_historical_reachable_only`; its tree effect stays a separate
//!    field.
//! 2. `present_but_superseded` is `included_superseded_by_later_fix`.
//! 3. `history_reachable_no_final_blob_effect` is
//!    `included_historical_reachable_only`.
//! 4. A subject naming the owner issue of a packet claim resolved
//!    `accepted_defer_with_non_claim` is `included_with_release_nonclaim`.
//! 5. Otherwise the strongest path class wins:
//!    product > unclassified (blocking) > control > automation > evidence > docs.
//!
//! The brief reads "only tests..." and "only docs..." for the lower classes; v1
//! takes the strongest class present so a tests-plus-docs commit is evidence
//! rather than unresolved. `blocking_unresolved` rows are resolved by the
//! reviewed overrides file, never by editing a row.

use serde::Deserialize;
use std::collections::BTreeMap;

use super::capture::{CommitFacts, GitFacts};
use super::dto::{
    ADJUDICATION_BASIS, AuditRef, BLOCKING, ClaimRow, DISPOSITIONS, HISTORY_NO_BLOB_EFFECT, KIND,
    Ledger, OpenPr, PRESENT_BUT_SUPERSEDED, Record, SCHEMA_VERSION, SelectedClaims,
};
use super::validate::{SelectionPacket, compute_digests};

pub(super) const RULE_VERSION: &str = "v1";
const RULE_OWNER: &str = "rule:ledger-v1";
pub(super) const OVERRIDES_KIND: &str = "release_ledger_overrides";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Overrides {
    pub(super) schema_version: String,
    pub(super) kind: String,
    pub(super) overrides: Vec<Override>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Override {
    pub(super) commit_sha: String,
    pub(super) disposition: String,
    pub(super) owner: String,
    pub(super) note: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum PathClass {
    Docs,
    Evidence,
    Automation,
    Control,
    Other,
    Product,
}

impl PathClass {
    fn theme(self) -> &'static str {
        match self {
            Self::Docs => "docs",
            Self::Evidence => "test_or_evidence",
            Self::Automation => "automation",
            Self::Control => "release_or_control",
            Self::Other => "unclassified",
            Self::Product => "product",
        }
    }
}

fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains("/__tests__/")
        || path.contains("/testdata/")
        || path.contains("/fixtures/")
        || path.contains("/test/")
        || path.contains("/test-fixtures/")
        || path.contains("/testing/")
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.ts")
        || name.starts_with("test_") && name.ends_with(".py")
}

pub(super) fn classify_path(path: &str) -> PathClass {
    if is_test_path(path)
        || path.starts_with("fixtures/")
        || path.starts_with("benchmarks/")
        || path.starts_with("metrics/")
        || path.starts_with("docs/specs/")
        || path.starts_with(".ripr/")
    {
        return PathClass::Evidence;
    }
    // Prose and lockfiles under a product prefix change no shipped behavior.
    if path.ends_with(".md") {
        return PathClass::Docs;
    }
    if path.ends_with("package-lock.json") {
        return PathClass::Control;
    }
    // `schemas/` is the published wire contract, so a change there is product.
    if path.starts_with("crates/ripr/src/")
        || path.starts_with("editors/")
        || path.starts_with("schemas/")
    {
        return PathClass::Product;
    }
    if path.starts_with("changelog.d/")
        || path.starts_with("docs/")
        || path.ends_with(".md")
        || matches!(path, "LICENSE" | "LICENSE-MIT" | "LICENSE-APACHE")
    {
        return PathClass::Docs;
    }
    let top = path.split('/').next().unwrap_or(path);
    // Unshipped repository automation: the same disposition as control, but its
    // own theme, so a release reader can tell it from packaging.
    if top == "xtask" {
        return PathClass::Automation;
    }
    let is_root_file = !path.contains('/');
    if matches!(
        top,
        ".github"
            | ".allow"
            | ".agents"
            | ".claude"
            | ".config"
            | ".cargo"
            | ".ci"
            | "policy"
            | "scripts"
            | "tools"
            | "packaging"
            | "npm"
            | "crates"
    ) || is_root_file
    {
        return PathClass::Control;
    }
    PathClass::Other
}

pub(super) fn strongest_class(paths: &[String]) -> Option<PathClass> {
    paths.iter().map(|path| classify_path(path)).max()
}

struct Decision {
    disposition: &'static str,
    theme: &'static str,
    claim_ref: Option<String>,
    limitations: Vec<String>,
}

fn decide(commit: &CommitFacts, packet: &SelectionPacket) -> Decision {
    let mut limitations = Vec::new();
    let first_parent = commit.first_parent_position.is_some();
    if first_parent && commit.pr_refs.is_empty() {
        limitations.push(
            "no trailing PR number in the subject: protected-squash proof is not established"
                .to_string(),
        );
    }
    let blocked = |theme: &'static str, reason: &str, mut limitations: Vec<String>| {
        limitations.push(reason.to_string());
        Decision {
            disposition: BLOCKING,
            theme,
            claim_ref: None,
            limitations,
        }
    };
    if first_parent && commit.parent_shas.len() > 1 {
        return blocked(
            "merge",
            "two-parent first-parent merge: needs an override",
            limitations,
        );
    }
    if commit.paths.is_empty() {
        return blocked("empty", "empty diff: needs an override", limitations);
    }
    let class = strongest_class(&commit.paths).unwrap_or(PathClass::Other);
    // Rule 0 looks at every path, not the strongest class: an unclassified path
    // beside a product path must not be hidden by the product class.
    if first_parent
        && commit
            .paths
            .iter()
            .any(|path| classify_path(path) == PathClass::Other)
    {
        return blocked(
            PathClass::Other.theme(),
            "path outside every v1 path class: needs an override",
            limitations,
        );
    }
    if !first_parent {
        limitations.push("reached through a merge, not first-parent".to_string());
        return decided(
            "included_historical_reachable_only",
            class,
            None,
            limitations,
        );
    }
    match commit.final_tree_effect.as_str() {
        PRESENT_BUT_SUPERSEDED => {
            return decided("included_superseded_by_later_fix", class, None, limitations);
        }
        HISTORY_NO_BLOB_EFFECT => {
            return decided(
                "included_historical_reachable_only",
                class,
                None,
                limitations,
            );
        }
        _ => {}
    }
    let mentioned: Vec<u64> = commit
        .pr_refs
        .iter()
        .chain(&commit.issue_refs)
        .copied()
        .collect();
    if let Some(claim) = packet.claims.iter().find(|claim| {
        claim.resolution == "accepted_defer_with_non_claim"
            && claim.owner_issue != 0
            && mentioned.contains(&claim.owner_issue)
    }) {
        return decided(
            "included_with_release_nonclaim",
            class,
            Some(claim.claim_id.clone()),
            limitations,
        );
    }
    let disposition = match class {
        PathClass::Product => "included_accepted_product",
        PathClass::Control | PathClass::Automation => "included_accepted_release_or_control",
        PathClass::Evidence => "included_test_or_evidence",
        PathClass::Docs => "included_structural_no_semantic_delta",
        PathClass::Other => {
            return blocked(
                class.theme(),
                "path outside every v1 path class: needs an override",
                limitations,
            );
        }
    };
    decided(disposition, class, None, limitations)
}

fn decided(
    disposition: &'static str,
    class: PathClass,
    claim_ref: Option<String>,
    limitations: Vec<String>,
) -> Decision {
    Decision {
        disposition,
        theme: class.theme(),
        claim_ref,
        limitations,
    }
}

pub(super) struct DeriveInput<'a> {
    pub(super) facts: &'a GitFacts,
    pub(super) packet: &'a SelectionPacket,
    pub(super) overrides: Option<&'a Overrides>,
    pub(super) stage: &'a str,
    pub(super) audit: Option<AuditRef>,
}

pub(super) fn derive(input: &DeriveInput<'_>) -> Result<Ledger, String> {
    let facts = input.facts;
    let overrides = index_overrides(input.overrides, facts)?;
    let mut records = Vec::with_capacity(facts.commits.len());
    for commit in &facts.commits {
        let mut decision = decide(commit, input.packet);
        let mut disposition = decision.disposition.to_string();
        let mut owner = RULE_OWNER.to_string();
        let mut override_ref = None;
        if let Some(entry) = overrides.get(commit.commit_sha.as_str()) {
            if decision.disposition != BLOCKING {
                return Err(format!(
                    "override for {} replaces rule disposition {}: overrides resolve only blocking_unresolved rows",
                    commit.commit_sha, decision.disposition
                ));
            }
            disposition = entry.disposition.clone();
            owner = entry.owner.clone();
            override_ref = Some(format!(
                "override:{}",
                commit.commit_sha.get(..12).unwrap_or(&commit.commit_sha)
            ));
            decision.limitations.push(format!(
                "rule disposition {} overridden: {}",
                decision.disposition, entry.note
            ));
        }
        records.push(Record {
            commit_sha: commit.commit_sha.clone(),
            parent_shas: commit.parent_shas.clone(),
            tree_sha: commit.tree_sha.clone(),
            first_parent_position: commit.first_parent_position,
            subject: commit.subject.clone(),
            pr_refs: commit.pr_refs.clone(),
            issue_refs: commit.issue_refs.clone(),
            theme: decision.theme.to_string(),
            claim_ref: decision.claim_ref,
            release_disposition: disposition,
            final_tree_effect: commit.final_tree_effect.clone(),
            effect_basis: commit.effect_basis.clone(),
            acceptance_owner: owner,
            review_proof_refs: commit
                .pr_refs
                .last()
                .map(|number| format!("protected-squash:#{number}"))
                .into_iter()
                .collect(),
            // The source-parent path inventory (J) is not an input of v1.
            source_sync_authority_effect: "not_established".to_string(),
            limitations: decision.limitations,
            override_ref,
        });
    }
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for claim in &input.packet.claims {
        *totals.entry(claim.resolution.clone()).or_default() += 1;
    }
    let mut ledger = Ledger {
        schema_version: SCHEMA_VERSION.to_string(),
        kind: KIND.to_string(),
        rule_version: RULE_VERSION.to_string(),
        adjudication_basis: ADJUDICATION_BASIS.to_string(),
        stage: input.stage.to_string(),
        source: facts.source.clone(),
        range: facts.range.clone(),
        merged_prs: facts.merged_prs.clone(),
        selected_claims: SelectedClaims {
            packet_digest: input.packet.digest.clone(),
            rows: input
                .packet
                .claims
                .iter()
                .map(|claim| ClaimRow {
                    claim_ref: claim.claim_id.clone(),
                    resolution: claim.resolution.clone(),
                })
                .collect(),
            totals,
        },
        open_prs: input
            .packet
            .open_prs
            .rows
            .iter()
            .map(|pr| OpenPr {
                number: pr.pr,
                head_sha: pr.head.clone(),
                disposition: pr.disposition.clone(),
            })
            .collect(),
        digests: super::dto::Digests {
            first_parent: String::new(),
            reachable: String::new(),
            records: String::new(),
            tree: String::new(),
        },
        audit: input.audit.clone(),
        records,
    };
    ledger.digests = compute_digests(&ledger)?;
    Ok(ledger)
}

fn index_overrides<'a>(
    overrides: Option<&'a Overrides>,
    facts: &GitFacts,
) -> Result<BTreeMap<&'a str, &'a Override>, String> {
    let mut indexed = BTreeMap::new();
    let Some(overrides) = overrides else {
        return Ok(indexed);
    };
    if overrides.kind != OVERRIDES_KIND || overrides.schema_version != "1" {
        return Err(format!(
            "overrides must be kind {OVERRIDES_KIND} schema_version 1"
        ));
    }
    for entry in &overrides.overrides {
        if !facts
            .commits
            .iter()
            .any(|commit| commit.commit_sha == entry.commit_sha)
        {
            return Err(format!(
                "override names {} outside the range",
                entry.commit_sha
            ));
        }
        if !DISPOSITIONS.contains(&entry.disposition.as_str()) || entry.disposition == BLOCKING {
            return Err(format!(
                "override for {} must resolve to a closed non-blocking disposition",
                entry.commit_sha
            ));
        }
        if entry.owner.trim().is_empty() || entry.note.trim().is_empty() {
            return Err(format!(
                "override for {} needs an owner and a note",
                entry.commit_sha
            ));
        }
        if indexed.insert(entry.commit_sha.as_str(), entry).is_some() {
            return Err(format!("override for {} is duplicated", entry.commit_sha));
        }
    }
    Ok(indexed)
}
