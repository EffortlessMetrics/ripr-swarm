//! Saved tracked-worktree producer custody (#7357): real temporary tracked Git
//! repositories through the production refresh bridge, with a controlled
//! barrier between the findings producer and the seam inventory.

use super::workspace::{
    CODE_STALE_SNAPSHOT, RefreshSource, SourceCustody, WorkspaceSession,
    after_worktree_findings_hook, refresh_document, run_check,
};
use crate::testing::fixture_git::{fixture_git_ok, remove_fixture_tree};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ROOT_IDENTITY: &str = "root:sha256:worktree-custody";
const OPEN_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const CLOSED_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const MOVED_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        amount - 15\n    } else {\n        amount\n    }\n}\n";
const PRICING_TESTS: &str = "use custody_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n\n#[test]\nfn exact_boundary_gets_the_discount() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = remove_fixture_tree(&self.root);
    }
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=ripr fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            message,
        ],
    )
}

/// `main` holds the open boundary; the checked-out `journey` branch closes it,
/// so the saved worktree diff against `main` yields a repair-ready boundary
/// item whose test is the fix site. The tree is clean.
fn install_fixture(tag: &str) -> Result<Fixture, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-worktree-custody-{tag}-{}-{stamp}",
        std::process::id()
    ));
    let _ = remove_fixture_tree(&root);
    let fixture = Fixture { root: root.clone() };
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"custody-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nname = \"custody_fixture\"\npath = \"src/lib.rs\"\n\n[workspace]\n",
    )?;
    write(&root, ".gitignore", "/target/\n/.ripr/\n")?;
    write(&root, "src/lib.rs", OPEN_LIB)?;
    write(&root, "tests/pricing.rs", PRICING_TESTS)?;
    fixture_git_ok(&root, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    commit(&root, "open boundary")?;
    fixture_git_ok(&root, &["checkout", "-q", "-b", "journey"])?;
    write(&root, "src/lib.rs", CLOSED_LIB)?;
    commit(&root, "closed boundary")?;
    Ok(fixture)
}

fn worktree_refresh(root: &Path) -> Result<super::workspace::Snapshot, String> {
    run_check(root, Some(ROOT_IDENTITY), None, RefreshSource::Worktree)
        .map_err(|failure| format!("{}: {}", failure.code, failure.detail))
}

fn session_over(snapshot: super::workspace::Snapshot) -> WorkspaceSession {
    WorkspaceSession {
        last_good: Some(Arc::new(snapshot)),
        desired_source: RefreshSource::Worktree,
        ..Default::default()
    }
}

fn commitment_of(snapshot: &super::workspace::Snapshot) -> Result<&str, String> {
    match &snapshot.custody {
        SourceCustody::Established { commitment } => Ok(commitment),
        other => Err(format!("custody must be established, got {other:?}")),
    }
}

fn ready_item(snapshot: &super::workspace::Snapshot) -> Result<String, String> {
    snapshot
        .items
        .iter()
        .find(|item| item.repair_readiness.ready)
        .map(|item| item.canonical_id.clone())
        .ok_or_else(|| {
            format!(
                "the control must yield a repair-ready item: {:?}",
                snapshot
                    .items
                    .iter()
                    .map(|item| (&item.canonical_id, item.repair_readiness.ineligibility))
                    .collect::<Vec<_>>()
            )
        })
}

fn assert_withheld(snapshot: &super::workspace::Snapshot, reason_part: &str) -> Result<(), String> {
    let SourceCustody::NotEstablished { reason } = &snapshot.custody else {
        return Err(format!("custody must be withheld: {:?}", snapshot.custody));
    };
    if !reason.contains(reason_part) {
        return Err(format!("withheld reason must name {reason_part}: {reason}"));
    }
    if snapshot.card_producers.is_some() {
        return Err("no card producers may bind without custody".to_string());
    }
    if snapshot.items.is_empty() {
        return Err("the findings themselves are still served".to_string());
    }
    for item in &snapshot.items {
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility
                != Some(crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE)
        {
            return Err(format!(
                "every item must carry the typed withheld reason: {} {:?}",
                item.canonical_id, item.repair_readiness
            ));
        }
    }
    Ok(())
}

/// A clean saved worktree whose findings and inventory read the same bytes
/// binds the card producers: readiness is positive through the shared gates, the
/// card is served, and the producer commitment is part of the identity.
#[test]
fn agreeing_producers_establish_custody_and_serve_a_card() -> Result<(), String> {
    let fixture = install_fixture("agree")?;
    let snapshot = worktree_refresh(&fixture.root)?;
    let commitment = commitment_of(&snapshot)?.to_string();
    if !commitment.starts_with("rust-context:sha256:") {
        return Err(format!("unexpected commitment spelling: {commitment}"));
    }
    let canonical_id = ready_item(&snapshot)?;
    let Some(producers) = &snapshot.card_producers else {
        return Err("established custody must bind card producers".to_string());
    };
    if producers.bindings.is_empty() {
        return Err("the ready boundary item must bind a card seam".to_string());
    }
    let session = session_over(snapshot);
    let card = session
        .repair_card_document(&canonical_id, None, Some(&fixture.root))
        .map_err(|failure| format!("{}: {}", failure.code, failure.detail))?;
    let card_id = card
        .pointer("/card/repair_card_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    if card_id.is_none() {
        return Err(format!("the card must carry its identity: {card}"));
    }
    let disclosure = refresh_document(&session);
    if disclosure.pointer("/scope/consumed_source_commitment") != Some(&Value::from("established"))
        || disclosure.pointer("/scope/repair_cards") != Some(&Value::from("available"))
    {
        return Err(format!("scope must disclose the custody: {disclosure}"));
    }

    // Unchanged replay is deterministic, warm (the process-wide and persisted
    // result paths attach this invocation's observations).
    let replay = worktree_refresh(&fixture.root)?;
    if commitment_of(&replay)? != commitment {
        return Err("an unchanged replay must reproduce the commitment".to_string());
    }
    Ok(())
}

/// The saved source moves after the findings producer consumed it and before
/// the seam inventory reads it. No coherent card is returned; a fresh refresh
/// recovers on the new bytes. Removing the agreement check would bind cards
/// pairing the old findings with the new inventory and fail this control.
#[test]
fn source_movement_between_findings_and_inventory_withholds_the_card() -> Result<(), String> {
    let fixture = install_fixture("moved-source")?;
    let root = fixture.root.clone();
    after_worktree_findings_hook::install(move || {
        // The hook cannot return an error; a failed write surfaces below as an
        // unmoved (and therefore established) snapshot, which the control
        // rejects.
        let _ = write(&root, "src/lib.rs", MOVED_LIB);
    });
    let withheld = worktree_refresh(&fixture.root)?;
    assert_withheld(&withheld, "src/lib.rs")?;
    let readiness_detail = withheld
        .items
        .iter()
        .filter_map(|item| item.repair_readiness.seam_detail.as_deref())
        .next()
        .ok_or("a withheld item must explain itself")?;
    if !readiness_detail.contains("not shown to have consumed the same Rust source bytes") {
        return Err(format!("unexpected detail: {readiness_detail}"));
    }
    let canonical_id = withheld
        .items
        .first()
        .map(|item| item.canonical_id.clone())
        .ok_or("no item")?;
    let withheld_id = withheld.snapshot_id.clone();
    let session = session_over(withheld);
    let refusal = session
        .repair_card_document(&canonical_id, None, Some(&fixture.root))
        .err()
        .ok_or("a card must not be served over moved source")?;
    if refusal.code != super::repair_card::CODE_INCOMPLETE_INVENTORY {
        return Err(format!("unexpected refusal: {}", refusal.code));
    }
    let scope = refresh_document(&session);
    if scope.pointer("/scope/consumed_source_commitment") != Some(&Value::from("not_established"))
        || scope.pointer("/scope/repair_cards") != Some(&Value::from("withheld"))
    {
        return Err(format!("scope must withhold: {scope}"));
    }

    // A fresh analysis over the moved bytes establishes custody again.
    let recovered = worktree_refresh(&fixture.root)?;
    commitment_of(&recovered)?;
    if recovered.snapshot_id == withheld_id {
        return Err("recovery must be a different snapshot".to_string());
    }
    ready_item(&recovered)?;
    Ok(())
}

/// The same control for a consumed test file (the oracle side of the card).
#[test]
fn test_oracle_movement_between_findings_and_inventory_withholds_the_card() -> Result<(), String> {
    let fixture = install_fixture("moved-test")?;
    let root = fixture.root.clone();
    after_worktree_findings_hook::install(move || {
        let _ = write(
            &root,
            "tests/pricing.rs",
            &PRICING_TESTS.replace(
                "assert_eq!(discounted_total(100, 100), 90);",
                "let _ = discounted_total(100, 100);",
            ),
        );
    });
    let withheld = worktree_refresh(&fixture.root)?;
    assert_withheld(&withheld, "tests/pricing.rs")?;
    Ok(())
}

/// `HEAD` moving between the producers is incoherent even when the saved
/// bytes agree: the card binds the head the inventory resolved.
#[test]
fn head_movement_between_findings_and_inventory_withholds_the_card() -> Result<(), String> {
    let fixture = install_fixture("moved-head")?;
    let root = fixture.root.clone();
    after_worktree_findings_hook::install(move || {
        let _ = fixture_git_ok(
            &root,
            &[
                "-c",
                "user.name=ripr fixture",
                "-c",
                "user.email=fixture@ripr.invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "moved head",
            ],
        );
    });
    let withheld = worktree_refresh(&fixture.root)?;
    assert_withheld(&withheld, "repository head")?;
    Ok(())
}

/// Disclosed inventory binding: an UNTRACKED Rust helper is outside the
/// tracked-worktree scope and never a finding, but the seam inventory walks
/// disk and reads it, so its bytes are bound into the commitment. This is a
/// stated residual, not tracked-scope proof.
#[test]
fn untracked_helper_is_bound_by_the_inventory_and_moves_identity() -> Result<(), String> {
    let fixture = install_fixture("equal-result")?;
    // An untracked helper is outside the worktree diff and the findings, but
    // the seam inventory reads the whole corpus.
    write(
        &fixture.root,
        "src/aux_helper.rs",
        "pub fn aux_value() -> i32 {\n    1\n}\n",
    )?;
    let first = worktree_refresh(&fixture.root)?;
    commitment_of(&first)?;
    let first_id = first.snapshot_id.clone();
    let first_items = first
        .items
        .iter()
        .map(|item| item.canonical_id.clone())
        .collect::<Vec<_>>();
    let first_digest = first.outcome.semantic_digest()?;

    write(
        &fixture.root,
        "src/aux_helper.rs",
        "pub fn aux_value() -> i32 {\n    2\n}\n",
    )?;
    let second = worktree_refresh(&fixture.root)?;
    commitment_of(&second)?;
    let second_items = second
        .items
        .iter()
        .map(|item| item.canonical_id.clone())
        .collect::<Vec<_>>();
    if first_items != second_items || first_digest != second.outcome.semantic_digest()? {
        return Err("the control needs equal findings and equal outcome identity".to_string());
    }
    if first.scope != second.scope {
        return Err("the control needs equal scope facts".to_string());
    }
    if first_id == second.snapshot_id {
        return Err("a changed consumed helper must change the snapshot identity".to_string());
    }
    let session = session_over(second);
    let stale = session
        .active_snapshot(Some(&first_id))
        .err()
        .ok_or("a pinned old snapshot must fail")?;
    if stale.code != CODE_STALE_SNAPSHOT {
        return Err(format!("expected stale_snapshot, got {}", stale.code));
    }
    Ok(())
}

/// Cold and warm refreshes, and a committed -> worktree -> committed switch,
/// keep source attribution: the committed result carries no worktree custody
/// and never shares the worktree identity.
#[test]
fn committed_and_worktree_refreshes_do_not_share_custody_or_identity() -> Result<(), String> {
    let fixture = install_fixture("switch")?;
    let committed = run_check(
        &fixture.root,
        Some(ROOT_IDENTITY),
        None,
        RefreshSource::Committed,
    )
    .map_err(|failure| format!("{}: {}", failure.code, failure.detail))?;
    let worktree = worktree_refresh(&fixture.root)?;
    let committed_again = run_check(
        &fixture.root,
        Some(ROOT_IDENTITY),
        None,
        RefreshSource::Committed,
    )
    .map_err(|failure| format!("{}: {}", failure.code, failure.detail))?;
    if committed.custody != SourceCustody::NotApplicable
        || committed_again.custody != SourceCustody::NotApplicable
    {
        return Err("committed snapshots make no worktree custody claim".to_string());
    }
    commitment_of(&worktree)?;
    if committed.snapshot_id == worktree.snapshot_id
        || committed.snapshot_id != committed_again.snapshot_id
    {
        return Err("source kind must stay load-bearing across the switch".to_string());
    }
    Ok(())
}

/// An inventory failure after the findings producer ran must not drop the
/// findings: the snapshot is served with custody withheld and the cause named.
#[test]
fn inventory_failure_degrades_to_withheld_custody_and_keeps_findings() -> Result<(), String> {
    let fixture = install_fixture("inventory-failure")?;
    let root = fixture.root.clone();
    after_worktree_findings_hook::install(move || {
        // Deleting the checked-out branch ref makes the inventory's own head
        // resolution fail after the findings producer already ran.
        let _ = fixture_git_ok(&root, &["update-ref", "-d", "HEAD"]);
    });
    let withheld = worktree_refresh(&fixture.root)?;
    assert_withheld(&withheld, "seam inventory could not be gathered")?;
    Ok(())
}

/// A deleted tracked Rust file in the findings scope is fail-closed: the
/// findings producer cannot capture its bytes, so custody is withheld (or the
/// file is simply not consumed). Either way no card is served for an
/// unobserved path.
#[test]
fn deleted_tracked_rust_file_keeps_custody_and_card_producers_consistent() -> Result<(), String> {
    let fixture = install_fixture("deleted-file")?;
    write(
        &fixture.root,
        "src/extra.rs",
        "pub fn extra() -> i32 {\n    3\n}\n",
    )?;
    commit(&fixture.root, "add extra")?;
    std::fs::remove_file(fixture.root.join("src/extra.rs")).map_err(|e| e.to_string())?;
    let snapshot = worktree_refresh(&fixture.root)?;
    match &snapshot.custody {
        SourceCustody::Established { .. } => {
            if snapshot.card_producers.is_none() {
                return Err("established custody must bind card producers".to_string());
            }
        }
        SourceCustody::NotEstablished { .. } => {
            if snapshot.card_producers.is_some() {
                return Err("withheld custody must not bind card producers".to_string());
            }
        }
        SourceCustody::NotApplicable => return Err("a worktree snapshot has custody".to_string()),
    }
    Ok(())
}

/// Two saved edits to TRACKED, findings-consumed Rust files with equal
/// findings: the commitment and snapshot id change, and a request pinned to the
/// earlier snapshot fails stale.
fn tracked_equal_result_control(
    tag: &str,
    relative: &str,
    edited: &str,
    stage: bool,
) -> Result<(), String> {
    let fixture = install_fixture(tag)?;
    let first = worktree_refresh(&fixture.root)?;
    let first_commitment = commitment_of(&first)?.to_string();
    let first_id = first.snapshot_id.clone();
    let first_items = first
        .items
        .iter()
        .map(|item| item.canonical_id.clone())
        .collect::<Vec<_>>();
    write(&fixture.root, relative, edited)?;
    if stage {
        fixture_git_ok(&fixture.root, &["add", relative])?;
    }
    let second = worktree_refresh(&fixture.root)?;
    let second_commitment = commitment_of(&second)?.to_string();
    let second_items = second
        .items
        .iter()
        .map(|item| item.canonical_id.clone())
        .collect::<Vec<_>>();
    if first_items.is_empty() || first_items != second_items {
        return Err("the control needs equal, non-empty findings".to_string());
    }
    if first_commitment == second_commitment {
        return Err(format!(
            "{relative}: an equal-result edit must move the commitment"
        ));
    }
    if first_id == second.snapshot_id {
        return Err(format!(
            "{relative}: an equal-result edit must move the snapshot id"
        ));
    }
    let session = session_over(second);
    let stale = session
        .active_snapshot(Some(&first_id))
        .err()
        .ok_or("a pinned old snapshot must fail")?;
    if stale.code != CODE_STALE_SNAPSHOT {
        return Err(format!("expected stale_snapshot, got {}", stale.code));
    }
    Ok(())
}

#[test]
fn tracked_test_file_equal_result_edit_moves_commitment_and_stales_pinned_ids() -> Result<(), String>
{
    let edited = format!("// saved comment-only edit\n{PRICING_TESTS}");
    tracked_equal_result_control("tracked-test", "tests/pricing.rs", &edited, false)
}

#[test]
fn tracked_source_file_equal_result_staged_edit_moves_commitment_and_stales_pinned_ids()
-> Result<(), String> {
    let edited = format!("// staged comment-only edit\n{CLOSED_LIB}");
    tracked_equal_result_control("tracked-src", "src/lib.rs", &edited, true)
}
