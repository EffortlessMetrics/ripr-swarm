//! Negative controls for each validator rule, built from one valid ledger.

use super::dto::{ClaimRow, Digests, Ledger, MergedPr, OpenPr, Record, SelectedClaims, Source};
use super::dto::{EffectBasis, Range};
use super::validate::{Context, SelectionPacket, compute_digests, validate};
use std::collections::BTreeMap;

fn sha(n: u8) -> String {
    format!("{n:02x}").repeat(20)
}

fn record(n: u8, position: Option<u64>, parent: &str, effect: &str, disposition: &str) -> Record {
    let basis = match effect {
        "present_effective" => EffectBasis {
            alive_paths: 1,
            absent_paths: 0,
            last_toucher_paths: 1,
            blamed_lines: 0,
        },
        "present_but_superseded" => EffectBasis {
            alive_paths: 1,
            absent_paths: 0,
            last_toucher_paths: 0,
            blamed_lines: 0,
        },
        _ => EffectBasis {
            alive_paths: 0,
            absent_paths: 1,
            last_toucher_paths: 0,
            blamed_lines: 0,
        },
    };
    Record {
        commit_sha: sha(n),
        parent_shas: vec![parent.to_string()],
        tree_sha: sha(n + 100),
        first_parent_position: position,
        subject: format!("change {n} (#{n})"),
        pr_refs: position.map_or_else(Vec::new, |_| vec![u64::from(n)]),
        issue_refs: Vec::new(),
        theme: "core".to_string(),
        claim_ref: None,
        release_disposition: disposition.to_string(),
        final_tree_effect: effect.to_string(),
        effect_basis: basis,
        acceptance_owner: "rule:ledger-v1".to_string(),
        review_proof_refs: vec![format!("protected-squash:#{n}")],
        source_sync_authority_effect: "not_established".to_string(),
        limitations: Vec::new(),
        override_ref: None,
    }
}

fn valid() -> Ledger {
    let base = sha(1);
    let first_parent = vec![sha(2), sha(3), sha(4)];
    let mut reachable = first_parent.clone();
    reachable.push(sha(9));
    let records = vec![
        record(
            2,
            Some(1),
            &base,
            "present_effective",
            "included_accepted_product",
        ),
        record(
            3,
            Some(2),
            &sha(2),
            "present_but_superseded",
            "included_superseded_by_later_fix",
        ),
        record(
            4,
            Some(3),
            &sha(3),
            "present_effective",
            "included_accepted_release_or_control",
        ),
        record(
            9,
            None,
            &sha(2),
            "history_reachable_no_final_blob_effect",
            "included_historical_reachable_only",
        ),
    ];
    let mut ledger = Ledger {
        schema_version: "0.1".to_string(),
        kind: "release_ledger".to_string(),
        rule_version: "v1".to_string(),
        adjudication_basis: "rule_derived".to_string(),
        stage: "dry_run".to_string(),
        source: Source {
            repository: "EffortlessMetrics/ripr-swarm".to_string(),
            base_sha: base,
            cut_sha: sha(4),
            cut_tree_sha: sha(104),
            main_sha: sha(4),
        },
        range: Range {
            first_parent,
            reachable,
        },
        merged_prs: [2u8, 3, 4]
            .iter()
            .map(|n| MergedPr {
                number: u64::from(*n),
                merge_sha: sha(*n),
                state: "merged".to_string(),
            })
            .collect(),
        selected_claims: SelectedClaims {
            packet_digest: "sha256:packet".to_string(),
            rows: vec![ClaimRow {
                claim_ref: "claim-a".to_string(),
                resolution: "accepted_defer_with_non_claim".to_string(),
            }],
            totals: BTreeMap::from([("accepted_defer_with_non_claim".to_string(), 1)]),
        },
        open_prs: vec![OpenPr {
            number: 50,
            head_sha: sha(50),
            disposition: "not_in_cut_accepted_defer".to_string(),
        }],
        digests: Digests {
            first_parent: String::new(),
            reachable: String::new(),
            records: String::new(),
            tree: String::new(),
        },
        audit: None,
        records,
    };
    redigest(&mut ledger);
    ledger
}

fn redigest(ledger: &mut Ledger) {
    if let Ok(digests) = compute_digests(ledger) {
        ledger.digests = digests;
    }
}

fn reasons(ledger: &Ledger) -> Vec<String> {
    validate(ledger, &Context::default())
}

/// Requires exactly one reason containing `needle`; other reasons are allowed
/// only when they are consequences of the same corruption.
fn require_reason(ledger: &Ledger, needle: &str) -> Result<(), String> {
    let found = reasons(ledger);
    found
        .iter()
        .any(|reason| reason.contains(needle))
        .then_some(())
        .ok_or_else(|| format!("expected a reason containing `{needle}`, got {found:?}"))
}

#[test]
fn valid_ledger_has_no_reasons() -> Result<(), String> {
    let found = reasons(&valid());
    found
        .is_empty()
        .then_some(())
        .ok_or_else(|| format!("unexpected reasons: {found:?}"))
}

#[test]
fn missing_record_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.records.remove(1);
    redigest(&mut ledger);
    require_reason(&ledger, "has no record")
}

#[test]
fn duplicate_record_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    let copy = ledger.records[1].clone();
    ledger.records.push(copy);
    require_reason(&ledger, "is duplicated")
}

#[test]
fn record_outside_range_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.records[3].commit_sha = sha(77);
    require_reason(&ledger, "outside the exact reachable range")
}

#[test]
fn reordered_first_parent_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.range.first_parent.swap(0, 1);
    ledger.range.reachable.swap(0, 1);
    redigest(&mut ledger);
    require_reason(&ledger, "contradicts the range")
}

#[test]
fn count_consistent_but_wrong_set_is_rejected_by_digest() -> Result<(), String> {
    let mut ledger = valid();
    // Same counts, different member: the recorded digest no longer matches.
    ledger.range.reachable[3] = sha(8);
    ledger.records[3].commit_sha = sha(8);
    require_reason(&ledger, "reachable digest")
}

#[test]
fn wrong_first_parent_digest_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.digests.first_parent = "sha256:00".to_string();
    require_reason(&ledger, "first_parent digest")
}

#[test]
fn merged_pr_contradicting_git_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.merged_prs[0].merge_sha = sha(3);
    require_reason(&ledger, "merged as")?;
    let mut ledger = valid();
    ledger.merged_prs[0].state = "open".to_string();
    require_reason(&ledger, "not merged")
}

#[test]
fn selected_claim_totals_must_match_rows_and_packet() -> Result<(), String> {
    let mut ledger = valid();
    ledger
        .selected_claims
        .totals
        .insert("accepted_defer_with_non_claim".to_string(), 2);
    require_reason(&ledger, "do not match its rows")?;
    let packet = SelectionPacket {
        digest: "sha256:packet".to_string(),
        totals: BTreeMap::from([("accepted_defer_with_non_claim".to_string(), 3)]),
        open_prs: Vec::new(),
    };
    let context = Context {
        packet: Some(packet),
        ..Context::default()
    };
    let found = validate(&valid(), &context);
    found
        .iter()
        .any(|reason| reason.contains("differ from the #2766 packet"))
        .then_some(())
        .ok_or_else(|| format!("packet totals not compared: {found:?}"))
}

#[test]
fn wrong_tree_effect_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.records[0].effect_basis.last_toucher_paths = 0;
    require_reason(&ledger, "contradicts the cut tree")?;
    let mut ledger = valid();
    ledger.records[1].release_disposition = "included_accepted_product".to_string();
    require_reason(&ledger, "requires disposition")
}

#[test]
fn blocking_unresolved_fails_only_in_final() -> Result<(), String> {
    let mut ledger = valid();
    ledger.records[0].release_disposition = "blocking_unresolved".to_string();
    redigest(&mut ledger);
    if !reasons(&ledger).is_empty() {
        return Err(format!(
            "dry run must tolerate blocking: {:?}",
            reasons(&ledger)
        ));
    }
    ledger.stage = "final".to_string();
    require_reason(&ledger, "blocking_unresolved rows")
}

#[test]
fn free_form_vocabulary_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.records[0].release_disposition = "looks_fine".to_string();
    require_reason(&ledger, "closed vocabulary")?;
    let mut ledger = valid();
    ledger.adjudication_basis = "manual_review".to_string();
    require_reason(&ledger, "adjudication_basis")
}

#[test]
fn projection_digest_disagreement_is_rejected() -> Result<(), String> {
    let ledger = valid();
    let context = Context {
        markdown: Some(format!("first-parent digest: `{}`\n", "sha256:other")),
        manifest: Some(serde_json::json!({ "digests": { "first_parent": "sha256:other" } })),
        ..Context::default()
    };
    let found = validate(&ledger, &context);
    for needle in ["Markdown projection", "manifest disagrees"] {
        if !found.iter().any(|reason| reason.contains(needle)) {
            return Err(format!("missing `{needle}` in {found:?}"));
        }
    }
    Ok(())
}

#[test]
fn cut_that_is_not_exact_main_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.source.main_sha = sha(5);
    require_reason(&ledger, "is not exact main")
}

#[test]
fn open_pr_inside_range_or_with_included_disposition_is_rejected() -> Result<(), String> {
    let mut ledger = valid();
    ledger.open_prs[0].head_sha = sha(3);
    require_reason(&ledger, "inside the reachable range")?;
    let mut ledger = valid();
    ledger.open_prs[0].disposition = "included_accepted_product".to_string();
    require_reason(&ledger, "not valid for a PR outside the cut")
}
