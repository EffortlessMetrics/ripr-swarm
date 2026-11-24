//! The CLI counterpart to #2335's two committed editor Start cases.

use super::{agent_status, attempt_id_from_continue, printed_continue_command, seam_id};
use crate::boundary_gap::{
    JourneyRoots, commit_all, decoy_was_invoked, init_git, journey_roots, ordinary_candidate,
    parse_json, path_arg, plant_path_decoy, require_success, run_installed, stdout_text,
};
use crate::common::fixture_git::fixture_git_output;
use serde_json::{Value, json};

#[path = "../common/repair_start_fixture.rs"]
mod repair_start_fixture;

use repair_start_fixture::{CALLED_BASE, CALLED_NEW, LOYALTY_BASE, LOYALTY_NEW, LOYALTY_TEST};

const GAP: &str = "gap:rust:src/lib.rs:loyalty_price:predicate_boundary:predicate:member_years==5";
const TARGET: &str = "tests/loyalty_price.rs";

fn commit_fixture(roots: &JourneyRoots, base: &str, changed: &str) -> Result<Value, String> {
    let root = &roots.fixture;
    for directory in ["src", "tests"] {
        std::fs::create_dir(root.join(directory)).map_err(|error| error.to_string())?;
    }
    for (path, contents) in [
        (
            "Cargo.toml",
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (".gitignore", "/target/\n"),
        ("src/lib.rs", base),
        ("tests/t.rs", LOYALTY_TEST),
    ] {
        std::fs::write(root.join(path), contents).map_err(|error| format!("{path}: {error}"))?;
    }
    init_git(root)?;
    let base = commit_all(root, "base")?;
    fixture_git_output(root, &["checkout", "-qb", "feature"])?;
    std::fs::write(root.join("src/lib.rs"), changed).map_err(|error| error.to_string())?;
    let head = commit_all(root, "change")?;
    Ok(json!({
        "root": root,
        "base": base,
        "base_tree": fixture_git_output(root, &["rev-parse", "main^{tree}"])?.trim(),
        "head": head,
        "head_tree": fixture_git_output(root, &["rev-parse", "HEAD^{tree}"])?.trim(),
    }))
}

#[test]
fn direct_id_start_requires_more_than_a_proposed_target() -> Result<(), String> {
    let called = journey_roots("packaged-repair-called")?;
    let uncalled = journey_roots("packaged-repair-uncalled")?;
    let candidate = ordinary_candidate(&called)?;
    for (label, roots, base, changed, startable) in [
        ("uncalled", &uncalled, LOYALTY_BASE, LOYALTY_NEW, true),
        ("called", &called, CALLED_BASE, CALLED_NEW, false),
    ] {
        let marker = plant_path_decoy(&roots.decoy)?;
        let fixture = commit_fixture(roots, base, changed)?;
        candidate.record(&format!("{label}-fixture.json"), &fixture)?;
        let root_arg = path_arg(&roots.fixture);
        let run = |args: &[&str]| run_installed(&candidate, &roots.decoy, &roots.decoy, args);

        let report = run(&[
            "check",
            "--root",
            &root_arg,
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ])?;
        require_success("repo exposure", &report)?;
        let report = parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let mut seams = report["seams"]
            .as_array()
            .ok_or("repo exposure omitted seams")?
            .iter()
            .filter(|entry| {
                entry["kind"] == "predicate_boundary" && entry["expression"] == "member_years >= 5"
            });
        let seam = seam_id(seams.next().ok_or("missing loyalty boundary seam")?)?;
        assert!(seams.next().is_none(), "ambiguous loyalty boundary seam");
        let card = run(&[
            "agent",
            "card",
            "--root",
            &root_arg,
            "--seam-id",
            seam,
            "--json",
        ])?;
        require_success("agent card", &card)?;
        let card = parse_json("agent card", &stdout_text("agent card", &card)?)?;
        assert_eq!(card["subject"]["seam_id"], seam, "{card}");
        assert_eq!(card["subject"]["canonical_gap_id"], GAP, "{card}");
        assert_eq!(card["snapshot"]["currentness"], "current", "{card}");
        assert_eq!(card["selected_target"]["file"], TARGET, "{card}");
        if startable {
            assert_eq!(
                card["canonical_next_action"]["action_class"], "run_command",
                "{card}"
            );
        } else {
            assert_eq!(card["instruction"]["state"], "static_limitation", "{card}");
            assert!(card["next_action"].is_null(), "{card}");
        }

        let start = run(&[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            seam,
            "--phase",
            "before",
        ])?;
        let result = if startable {
            require_success("uncalled direct Start", &start)?;
            let next = printed_continue_command(&stdout_text("uncalled direct Start", &start)?)?;
            let attempt = attempt_id_from_continue(&next)?;
            let status = agent_status(&candidate, roots, &attempt)?;
            assert_eq!(status["attempt"]["attempt_id"], attempt, "{status}");
            assert_eq!(status["attempt"]["seam_id"], seam, "{status}");
            assert_eq!(status["attempt"]["state"], "awaiting_edit", "{status}");
            assert_eq!(status["attempt"]["currentness"], "current", "{status}");
            assert!(
                roots
                    .fixture
                    .join("target/ripr/workflow/before.repo-exposure.json")
                    .is_file()
            );
            json!({ "attempt_id": attempt, "continuation": next })
        } else {
            let stderr = String::from_utf8_lossy(&start.stderr);
            assert_eq!(start.status.code(), Some(3), "called seam: {stderr}");
            assert!(
                start.stdout.is_empty(),
                "called seam emitted a success document"
            );
            assert!(stderr.contains("no test that reaches"), "{stderr}");
            assert!(
                stderr.contains("code=seam_not_repair_eligible disposition=not_started"),
                "{stderr}"
            );
            let facade = run(&["repair", seam, "--root", &root_arg, "--json"])?;
            assert_eq!(
                facade.status.code(),
                Some(3),
                "{:?}",
                String::from_utf8_lossy(&facade.stderr)
            );
            let refusal = parse_json("repair refusal", &stdout_text("repair refusal", &facade)?)?;
            assert_eq!(refusal["status"], "refused", "{refusal}");
            assert_eq!(
                refusal["error"]["code"], "seam_not_repair_eligible",
                "{refusal}"
            );
            assert_eq!(refusal["error"]["disposition"], "not_started", "{refusal}");
            assert!(refusal["error"]["next"].is_null(), "{refusal}");
            let workflow = roots.fixture.join("target/ripr/workflow");
            for artifact in [
                "workflow.json",
                "commands.md",
                "agent-brief.json",
                "agent-packet.json",
                "before.repo-exposure.json",
            ] {
                assert!(!workflow.join(artifact).exists(), "unexpected {artifact}");
            }
            let manifests = roots.fixture.join("target/ripr/repair-attempts");
            if manifests.exists() {
                let entries = std::fs::read_dir(manifests).map_err(|error| error.to_string())?;
                for entry in entries {
                    let entry = entry.map_err(|error| error.to_string())?;
                    assert!(!entry.path().join("attempt.json").exists());
                }
            }
            json!({ "attempt_id": null, "exit_code": start.status.code() })
        };
        assert!(!roots.fixture.join(TARGET).exists());
        assert!(fixture_git_output(&roots.fixture, &["status", "--porcelain"])?.is_empty());
        assert!(!decoy_was_invoked(&marker));
        candidate.record(
            &format!("{label}-result.json"),
            &json!({
                "canonical_gap_id": GAP,
                "seam_id": seam,
                "proposed_target": TARGET,
                "start": result,
            }),
        )?;
    }
    candidate.verify_final_identity()?;
    called.cleanup()?;
    uncalled.cleanup()?;
    candidate.record(
        "direct-id-start-complete.json",
        &json!({"scenarios": 2, "cleanup": "pass"}),
    )
}
