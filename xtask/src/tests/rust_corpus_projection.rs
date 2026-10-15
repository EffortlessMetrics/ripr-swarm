//! Independently authored composition proof over the real public Rust producer
//! and the shared xtask actionable-gaps projector. No handwritten raw finding,
//! language tag, gap identity, classification, or aggregate hash supplies a
//! positive control.

use crate::evidence_audit::{
    lane1_actionable_gap_packets_json, lane1_evidence_audit_from_repo_exposure,
    stamp_actionable_gaps_source_subject,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../../../crates/ripr/tests/common/fixture_git.rs"]
mod fixture_git;

const GAP: &str = "gap:54c07ec8eeadf203";
const SEAM: &str = "67f9714ba37b3967";
const SOURCE: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const PRICING: &str = "use ordinary_shop::discounted_total;\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n#[test]\nfn above_threshold_discounts() {\n    assert_eq!(discounted_total(1_000, 100), 990);\n}\n";
const EQUALITY: &str = "use ordinary_shop::discounted_total;\n#[test]\nfn boundary_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

type ProofResult<T = ()> = Result<T, String>;

struct OwnedCorpusRoot {
    path: PathBuf,
    retained_for_proof_owner: bool,
}

impl OwnedCorpusRoot {
    fn new(label: &str) -> ProofResult<Self> {
        // Optional explicit proof custody: a root-owned driver can select an
        // admitted export directory, then consume these genuine artifacts by
        // framed LSP. Ordinary unit runs remove their owned temporary root.
        let (parent, retained_for_proof_owner) =
            match std::env::var_os("RIPR_CORPUS_PROOF_EXPORT_DIR") {
                Some(path) => (PathBuf::from(path), true),
                None => (std::env::temp_dir(), false),
            };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = parent.join(format!("ripr-xtask-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self {
            path,
            retained_for_proof_owner,
        })
    }
}

impl Drop for OwnedCorpusRoot {
    fn drop(&mut self) {
        if !self.retained_for_proof_owner
            && let Err(error) = std::fs::remove_dir_all(&self.path)
        {
            eprintln!(
                "failed to remove owned corpus proof root {}: {error}",
                self.path.display()
            );
        }
    }
}

fn require(condition: bool, message: impl FnOnce() -> String) -> ProofResult {
    if condition { Ok(()) } else { Err(message()) }
}

fn write(path: &Path, contents: &str) -> ProofResult {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

fn pricing_files(root: &Path) -> ProofResult {
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"ordinary_shop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(&root.join("src/lib.rs"), SOURCE)?;
    write(&root.join("tests/pricing.rs"), PRICING)
}

fn pricing_repository(root: &Path) -> ProofResult {
    pricing_files(root)?;
    fixture_git::fixture_git_ok(root, &["init"])?;
    fixture_git::fixture_git_ok(root, &["config", "user.name", "RIPR proof"])?;
    fixture_git::fixture_git_ok(root, &["config", "user.email", "ripr@example.invalid"])?;
    write(&root.join(".git/info/exclude"), "/target/\n")?;
    fixture_git::fixture_git_ok(root, &["add", "Cargo.toml", "src", "tests"])?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "pricing corpus"])?;
    fixture_git::fixture_git_ok(root, &["branch", "-M", "main"])
}

fn genuine_rust_capture(root: &Path) -> ProofResult<Value> {
    let output = ripr::check_workspace(ripr::CheckInput {
        root: root.to_path_buf(),
        base: Some("HEAD".into()),
        mode: ripr::Mode::Instant,
        format: ripr::OutputFormat::RepoExposureJson,
        git_timeout: Some(std::time::Duration::from_secs(30)),
        ..ripr::CheckInput::default()
    })?;
    let capture = ripr::render_check(&output, &ripr::OutputFormat::RepoExposureJson)?;
    let value: Value = serde_json::from_str(&capture).map_err(|error| error.to_string())?;
    let seams = value["seams"]
        .as_array()
        .ok_or("real capture omitted seams")?;
    require(seams.len() == 1, || {
        format!("producer must retain one actual pricing subject: {value}")
    })?;
    let producer_root = value
        .pointer("/artifact/repository/root")
        .and_then(Value::as_str)
        .ok_or("actual capture omitted its producer frame")?;
    require(
        Path::new(producer_root).is_absolute()
            && std::fs::canonicalize(producer_root).map_err(|error| error.to_string())?
                == std::fs::canonicalize(root).map_err(|error| error.to_string())?,
        || format!("actual producer frame must name the selected physical root: {value}"),
    )?;
    let corpus = &value["source_subject"]["corpus"];
    require(
        corpus["scope"] == "rust_workspace"
            && corpus["cache_schema_version"] == "1.73"
            && corpus["files_content_hash"].as_str().is_some_and(|hash| {
                hash.len() == 16
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            && value["source_subject"]["files"]
                .as_array()
                .is_some_and(|files| !files.is_empty()),
        || {
            format!(
                "producer must carry its actual existing typed content authority and nonempty named subjects: {value}"
            )
        },
    )?;
    Ok(value)
}

fn actionable_projection(root: &Path, captured: &Value) -> ProofResult<Value> {
    let root_text = root.to_str().ok_or("fixture root is not UTF-8")?;
    let report = lane1_evidence_audit_from_repo_exposure(root_text, &captured.to_string())?;
    let projected = lane1_actionable_gap_packets_json(&report)?;
    let producer_root = captured
        .pointer("/artifact/repository/root")
        .and_then(Value::as_str)
        .ok_or("producer capture has no root")?;
    let stamped = stamp_actionable_gaps_source_subject(
        &projected,
        captured.get("source_subject"),
        Path::new(producer_root),
        root,
    )?;
    let actionable: Value = serde_json::from_str(&stamped).map_err(|error| error.to_string())?;
    require(
        actionable["source_subject"]["corpus"] == captured["source_subject"]["corpus"],
        || {
            format!(
                "actual xtask derivation must copy captured aggregate authority without reminting: {captured}; {actionable}"
            )
        },
    )?;
    let named = actionable["source_subject"]["files"]
        .as_array()
        .ok_or("projection omitted named stamp")?;
    let input_files = captured["source_subject"]["files"]
        .as_array()
        .ok_or("capture omitted named stamp")?;
    require(named.iter().all(|file| input_files.contains(file)), || {
        format!(
            "projection must copy every named-file digest from actual capture: {captured}; {actionable}"
        )
    })?;
    write(
        &root.join("target/ripr/reports/actionable-gaps.json"),
        &stamped,
    )?;
    write(
        &root.join("target/corpus-proof/actual-rust-capture.json"),
        &captured.to_string(),
    )?;
    Ok(actionable)
}

#[test]
fn genuine_bare_root_producer_preserves_raw_language_corpus_and_unbounded_verify_exclusion()
-> ProofResult {
    let fixture = OwnedCorpusRoot::new("rust-corpus-actionable-projection")?;
    let root = &fixture.path;
    pricing_repository(root)?;
    let captured = genuine_rust_capture(root)?;
    require(
        captured["seams"][0]["seam_id"] == SEAM
            && captured["seams"][0]["file"] == "src/lib.rs"
            && captured["seams"][0]["grip_class"] == "weakly_gripped"
            && captured["seams"][0]["evidence_record"]["canonical_gap_id"] == GAP,
        || format!("positive must be the real exact nonempty weak pricing gap: {captured}"),
    )?;
    let raw = &captured["seams"][0]["evidence_record"]["raw_findings"];
    let findings = raw.as_array().ok_or("real producer omitted raw findings")?;
    require(
        !findings.is_empty()
            && findings.iter().all(|finding| {
                finding["language"] == "rust"
                    && finding["language_status"] == "stable"
                    && finding["file"] == "src/lib.rs"
                    && finding["line"] == 2
            }),
        || {
            format!(
                "the Rust typed factory must explicitly supply every actual raw language; extension/identity inference is insufficient: {captured}"
            )
        },
    )?;
    let actionable = actionable_projection(root, &captured)?;
    let packets = actionable["packets"]
        .as_array()
        .ok_or("actual xtask projection omitted packets")?;
    require(
        packets.len() == 1
            && packets[0]["canonical_gap_id"] == GAP
            && packets[0]["gap_state"] == "actionable"
            && packets[0]["primary_anchor"]["file"] == "src/lib.rs"
            && packets[0]["primary_anchor"]["line"] == 2
            && packets[0]["public_projection_eligible"] == false
            && packets[0]["projection_exclusion_reasons"] == json!(["unbounded_verify_command"])
            && actionable["summary"]["public_projection_eligible_packets"] == 0
            && actionable["summary"]["public_projection_excluded_packets"] == 1
            && actionable["summary"]["projection_exclusion_reasons"]
                == json!([{"count": 1, "label": "unbounded_verify_command"}])
            && packets[0]["verify_command"]
                == captured["seams"][0]["evidence_record"]["canonical_item"]["verify_command"]
            && packets[0]["verify_command"]
                .as_str()
                .is_some_and(|command| !command.is_empty())
            && packets[0]["receipt_command"]
                .as_str()
                .is_some_and(|command| !command.is_empty())
            && packets[0]["raw_findings"]
                == captured["seams"][0]["evidence_record"]["raw_findings"]
            && actionable["source_subject"]["files"]
                .as_array()
                .is_some_and(|files| !files.is_empty()),
        || {
            format!(
                "shared xtask projector must retain the exact bare-root packet, inherited unbounded-verify exclusion and copied provenance: {captured}; {actionable}"
            )
        },
    )
}

#[test]
fn genuine_completed_no_action_corpus_produces_an_empty_queue_without_invented_language()
-> ProofResult {
    let fixture = OwnedCorpusRoot::new("rust-corpus-empty-projection")?;
    let root = &fixture.path;
    pricing_repository(root)?;
    write(&root.join("tests/new_boundary.rs"), EQUALITY)?;
    let captured = genuine_rust_capture(root)?;
    require(
        captured["seams"][0]["seam_id"] == SEAM
            && captured["seams"][0]["grip_class"] == "strongly_gripped"
            && captured["seams"][0]["evidence_record"]["canonical_gap_id"].is_null()
            && captured["seams"][0]["evidence_record"]["canonical_item"]["gap_state"]
                == "already_observed",
        || {
            format!(
                "empty queue must come from a genuinely completed still-nonempty exact subject: {captured}"
            )
        },
    )?;
    let actionable = actionable_projection(root, &captured)?;
    require(
        actionable["packets"] == json!([])
            && actionable["summary"]["packets_emitted"] == 0
            && actionable["summary"]["already_observed"] == 1
            && actionable["source_subject"]["files"] == json!([])
            && actionable.get("language").is_none(),
        || {
            format!(
                "actual completed queue must remain empty and unqualified; no vacuous Rust language may be minted: {actionable}"
            )
        },
    )
}

#[test]
fn genuine_workspace_corpus_cannot_be_rebased_into_a_member_corpus_with_legal_named_subjects()
-> ProofResult {
    let fixture = OwnedCorpusRoot::new("rust-corpus-member-rebase")?;
    let workspace = &fixture.path;
    let member = workspace.join("shop");
    pricing_files(&member)?;
    write(
        &workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"shop\"]\nresolver = \"3\"\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["init"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.name", "RIPR proof"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.email", "ripr@example.invalid"])?;
    write(
        &workspace.join(".git/info/exclude"),
        "/target/\n/shop/target/\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["add", "Cargo.toml", "shop"])?;
    fixture_git::fixture_git_ok(workspace, &["commit", "-m", "workspace pricing corpus"])?;
    let captured = genuine_rust_capture(workspace)?;
    require(
        captured["seams"][0]["file"] == "shop/src/lib.rs"
            && captured["seams"][0]["owner"] == "shop/src/lib.rs::discounted_total"
            && captured["seams"][0]["grip_class"] == "weakly_gripped",
        || {
            format!(
                "subroot control must originate in a real nonempty workspace-member producer: {captured}"
            )
        },
    )?;
    let required = BTreeSet::from(["src/lib.rs".to_string(), "tests/pricing.rs".to_string()]);
    let derived = crate::gap_source_subject_shared::derive_source_subject(
        captured.get("source_subject"), workspace, &member, &required,
    ).map_err(|reason| format!("legally rebased named subjects must be available to distinguish corpus root refusal: {reason}; {captured}"))?;
    let derived = serde_json::to_value(derived).map_err(|error| error.to_string())?;
    let copied = derived["files"]
        .as_array()
        .ok_or("member derivation omitted named subjects")?;
    require(
        copied.len() == 2
            && required
                .iter()
                .all(|path| copied.iter().any(|file| file["path"] == *path))
            && copied.iter().all(|file| {
                captured["source_subject"]["files"]
                    .as_array()
                    .is_some_and(|input| {
                        input.iter().any(|original| {
                            original["path"]
                                == format!("shop/{}", file["path"].as_str().unwrap_or_default())
                                && original["digest"] == file["digest"]
                        })
                    })
            })
            && derived["corpus"]
                == json!({"unavailable": "input_source_subject_corpus_root_mismatch"}),
        || {
            format!(
                "legal per-file subroot rebasing must copy only actual named digests and withhold the parent aggregate: {captured}; {derived}"
            )
        },
    )
}

#[test]
fn genuine_workspace_producer_preserves_current_corpus_and_native_bounded_projection() -> ProofResult
{
    let fixture = OwnedCorpusRoot::new("rust-corpus-workspace-projection")?;
    let workspace = &fixture.path;
    let member = workspace.join("crates/ordinary_shop");
    pricing_files(&member)?;
    write(
        &workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/ordinary_shop\"]\nresolver = \"3\"\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["init"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.name", "RIPR proof"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.email", "ripr@example.invalid"])?;
    write(
        &workspace.join(".git/info/exclude"),
        "/target/\n/crates/ordinary_shop/target/\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["add", "Cargo.toml", "crates"])?;
    fixture_git::fixture_git_ok(
        workspace,
        &["commit", "-m", "workspace bounded projection corpus"],
    )?;
    fixture_git::fixture_git_ok(workspace, &["branch", "-M", "main"])?;
    let captured = genuine_rust_capture(workspace)?;
    // Observed by the parent's frozen official62c CLI in the exact SOURCE /
    // PRICING workspace shape, native-workspace-calibration-capture.json.
    let seam_id = "49bdd89bd71dd5b9";
    let gap_id = "gap:ba9f4264c3029d1c";
    require(
        captured["seams"][0]["seam_id"] == seam_id
            && captured["seams"][0]["file"] == "crates/ordinary_shop/src/lib.rs"
            && captured["seams"][0]["line"] == 2
            && captured["seams"][0]["owner"] == "crates/ordinary_shop/src/lib.rs::discounted_total"
            && captured["seams"][0]["grip_class"] == "weakly_gripped"
            && captured["seams"][0]["evidence_record"]["canonical_gap_id"] == gap_id,
        || {
            format!(
                "workspace positive must retain the exact genuinely observed nonempty product subject: {captured}"
            )
        },
    )?;
    let raw = captured["seams"][0]["evidence_record"]["raw_findings"]
        .as_array()
        .ok_or("workspace native producer omitted raw findings")?;
    require(
        !raw.is_empty()
            && raw.iter().all(|finding| {
                finding["language"] == "rust"
                    && finding["language_status"] == "stable"
                    && finding["file"] == "crates/ordinary_shop/src/lib.rs"
                    && finding["line"] == 2
            }),
        || {
            format!(
                "workspace typed factory must explicitly tag the actual raw population: {captured}"
            )
        },
    )?;
    let actionable = actionable_projection(workspace, &captured)?;
    let packets = actionable["packets"]
        .as_array()
        .ok_or("workspace projector omitted packets")?;
    require(
        packets.len() == 1
            && packets[0]["canonical_gap_id"] == gap_id
            && packets[0]["primary_anchor"]["file"] == "crates/ordinary_shop/src/lib.rs"
            && packets[0]["primary_anchor"]["line"] == 2
            && packets[0]["gap_state"] == "actionable"
            && packets[0]["public_projection_eligible"] == true
            && packets[0]["projection_exclusion_reasons"] == json!([])
            && packets[0]["verify_command"]
                == "cargo test -p ordinary_shop above_threshold_discounts"
            && packets[0]["verify_command_source"] == "related_test_or_observer.name"
            && packets[0]["receipt_command"]
                == captured["seams"][0]["evidence_record"]["canonical_item"]["receipt_command"]
            && packets[0]["raw_findings"]
                == captured["seams"][0]["evidence_record"]["raw_findings"]
            && actionable["summary"]["public_projection_eligible_packets"] == 1
            && actionable["summary"]["public_projection_excluded_packets"] == 0,
        || {
            format!(
                "workspace projector must select its actual bounded related-target fallback, retain exact native identity/provenance and declare eligibility honestly: {captured}; {actionable}"
            )
        },
    )
    // This proves genuine generation. Framed consumer qualification reads
    // these exact fields and bytes; source/corpus currentness and full LSP
    // admission remain independently checked at the shared consumer boundary.
}

#[test]
fn genuine_workspace_projection_regeneration_retains_nonempty_fallback_and_corpus_authority()
-> ProofResult {
    let fixture = OwnedCorpusRoot::new("related-provenance-regeneration")?;
    let workspace = &fixture.path;
    pricing_files(&workspace.join("crates/ordinary_shop"))?;
    write(
        &workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/ordinary_shop\"]\nresolver = \"3\"\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["init"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.name", "RIPR proof"])?;
    fixture_git::fixture_git_ok(workspace, &["config", "user.email", "ripr@example.invalid"])?;
    write(
        &workspace.join(".git/info/exclude"),
        "/target/\n/crates/ordinary_shop/target/\n",
    )?;
    fixture_git::fixture_git_ok(workspace, &["add", "Cargo.toml", "crates"])?;
    fixture_git::fixture_git_ok(
        workspace,
        &["commit", "-m", "workspace related-target provenance"],
    )?;
    fixture_git::fixture_git_ok(workspace, &["branch", "-M", "main"])?;
    let original_head = fixture_git::fixture_git_output(workspace, &["rev-parse", "HEAD"])?;
    let captured = genuine_rust_capture(workspace)?;
    let original = actionable_projection(workspace, &captured)?;
    let seam = &captured["seams"][0];
    let packets = original["packets"]
        .as_array()
        .ok_or("original queue omitted packets")?;
    require(
        seam["seam_id"] == "49bdd89bd71dd5b9"
            && seam["grip_class"] == "weakly_gripped"
            && packets.len() == 1
            && packets[0]["canonical_gap_id"] == "gap:ba9f4264c3029d1c"
            && packets[0]["primary_anchor"]["file"] == "crates/ordinary_shop/src/lib.rs"
            && packets[0]["primary_anchor"]["line"] == 2
            && packets[0]["public_projection_eligible"] == true
            && packets[0]["projection_exclusion_reasons"] == json!([])
            && packets[0]["verify_command_source"] == "related_test_or_observer.name"
            && packets[0]["verify_command"]
                == "cargo test -p ordinary_shop above_threshold_discounts",
        || {
            format!(
                "regeneration needs the exact genuine nonempty bounded fallback before stimulus: {captured}; {original}"
            )
        },
    )?;
    let saved = workspace.join("target/provenance-proof");
    write(
        &saved.join("original.repo-exposure.json"),
        &captured.to_string(),
    )?;
    let installed = workspace.join("target/ripr/reports/actionable-gaps.json");
    let original_bytes = std::fs::read(&installed).map_err(|error| error.to_string())?;
    std::fs::write(saved.join("original.actionable-gaps.json"), &original_bytes)
        .map_err(|error| error.to_string())?;
    let named = original["source_subject"]["files"]
        .as_array()
        .ok_or("original named subject missing")?;
    let mut named_bytes = Vec::new();
    for file in named {
        let path = file["path"].as_str().ok_or("named subject path missing")?;
        named_bytes.push((
            path.to_string(),
            std::fs::read(workspace.join(path)).map_err(|error| error.to_string())?,
        ));
    }
    let added_relative = "crates/ordinary_shop/tests/unclaimed_control.rs";
    let added = workspace.join(added_relative);
    let added_bytes = "#[test]\nfn unrelated_control() {\n    assert_eq!(2 + 2, 4);\n}\n";
    require(
        !added.exists() && named.iter().all(|file| file["path"] != added_relative),
        || "stimulus must add an actual unclaimed test, not alter an old named subject".into(),
    )?;
    write(&added, added_bytes)?;
    require(
        fixture_git::fixture_git_output(workspace, &["rev-parse", "HEAD"])? == original_head,
        || "unclaimed test addition must leave HEAD unchanged".into(),
    )?;
    for (path, bytes) in &named_bytes {
        require(
            std::fs::read(workspace.join(path)).map_err(|error| error.to_string())? == *bytes,
            || format!("unclaimed test stimulus changed original named bytes: {path}"),
        )?;
    }
    let recaptured = genuine_rust_capture(workspace)?;
    let regenerated = actionable_projection(workspace, &recaptured)?;
    let regenerated_packets = regenerated["packets"]
        .as_array()
        .ok_or("regenerated queue omitted packets")?;
    require(
        recaptured["seams"][0]["seam_id"] == "49bdd89bd71dd5b9"
            && recaptured["seams"][0]["grip_class"] == "weakly_gripped"
            && regenerated_packets.len() == 1
            && regenerated_packets[0] == packets[0]
            && regenerated["source_subject"]["files"] == original["source_subject"]["files"]
            && regenerated["source_subject"]["corpus"] == recaptured["source_subject"]["corpus"]
            && regenerated["source_subject"]["corpus"]["files_content_hash"]
                != original["source_subject"]["corpus"]["files_content_hash"],
        || {
            format!(
                "real regeneration must restore the same meaningful packet while recapturing actual corpus authority: {recaptured}; {regenerated}"
            )
        },
    )?;
    write(
        &saved.join("regenerated.repo-exposure.json"),
        &recaptured.to_string(),
    )?;
    let regenerated_bytes = std::fs::read(&installed).map_err(|error| error.to_string())?;
    std::fs::write(
        saved.join("regenerated.actionable-gaps.json"),
        regenerated_bytes,
    )
    .map_err(|error| error.to_string())?;
    write(&saved.join("unclaimed_control.rs"), added_bytes)?;
    write(
        &saved.join("producer-projection-plan.json"),
        &json!({
            "fixture_root": workspace,
            "head": original_head.trim(),
            "seam_id": "49bdd89bd71dd5b9",
            "canonical_gap_id": "gap:ba9f4264c3029d1c",
            "stimulus_path": added_relative,
            "stimulus_file": "target/provenance-proof/unclaimed_control.rs",
            "original_capture": "target/provenance-proof/original.repo-exposure.json",
            "original_queue": "target/provenance-proof/original.actionable-gaps.json",
            "regenerated_capture": "target/provenance-proof/regenerated.repo-exposure.json",
            "regenerated_queue": "target/provenance-proof/regenerated.actionable-gaps.json"
        })
        .to_string(),
    )?;
    // The optional framed consumer replays exactly this saved corpus change.
    // Both queues above are genuine unchanged projector outputs, not consumer-
    // authored stamps. Their capture chronology remains in this test.
    std::fs::remove_file(&added).map_err(|error| error.to_string())?;
    std::fs::write(&installed, original_bytes).map_err(|error| error.to_string())?;
    write(
        &workspace.join("target/corpus-proof/actual-rust-capture.json"),
        &captured.to_string(),
    )?;
    Ok(())
}
