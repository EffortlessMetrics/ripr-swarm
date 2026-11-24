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
const RETARGETED_BASE: &str = "\
pub fn doubled(x: u64) -> u64 { x * 2 }

pub fn checkout(amount: u64) -> u64 { loyalty_price(amount, 7) }

pub fn loyalty_price(amount: u64, years: u32) -> u64 {
    let threshold = 6;
    if years >= threshold { amount - 5 } else { amount }
}
";
const RETARGETED_GAP: &str =
    "gap:rust:src/lib.rs:loyalty_price:predicate_boundary:predicate:years==threshold";

struct Scenario<'a> {
    label: &'a str,
    base: &'a str,
    changed: &'a str,
    predicate: &'a str,
    gap: Option<&'a str>,
    startable: bool,
}

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
    let install = journey_roots("packaged-repair-no-path")?;
    let candidate = ordinary_candidate(&install)?;
    let cases = [
        Scenario {
            label: "uncalled",
            base: LOYALTY_BASE,
            changed: LOYALTY_NEW,
            predicate: "member_years >= 5",
            gap: Some(GAP),
            startable: true,
        },
        Scenario {
            label: "called",
            base: CALLED_BASE,
            changed: CALLED_NEW,
            predicate: "member_years >= 5",
            gap: Some(GAP),
            startable: false,
        },
        Scenario {
            label: "unrelated",
            base: CALLED_NEW,
            changed: &CALLED_NEW.replace("x * 2", "x * 3"),
            predicate: "member_years >= 5",
            gap: None,
            startable: true,
        },
        Scenario {
            label: "retargeted",
            base: RETARGETED_BASE,
            changed: &RETARGETED_BASE.replace("threshold = 6", "threshold = 5"),
            predicate: "years >= threshold",
            gap: Some(RETARGETED_GAP),
            startable: false,
        },
    ];
    let scenario_count = cases.len();
    for Scenario {
        label,
        base,
        changed,
        predicate,
        gap,
        startable,
    } in cases
    {
        let roots = &journey_roots(&format!("packaged-repair-{label}"))?;
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
                entry["kind"] == "predicate_boundary" && entry["expression"] == predicate
            });
        let entry = seams.next().ok_or("missing loyalty boundary seam")?;
        let seam = seam_id(entry)?;
        assert!(seams.next().is_none(), "ambiguous loyalty boundary seam");
        if label == "retargeted" {
            let diff =
                fixture_git_output(&roots.fixture, &["diff", "--unified=0", "main", "HEAD"])?;
            let hunks: Vec<_> = diff.lines().filter(|line| line.starts_with("@@")).collect();
            assert_eq!(hunks.len(), 1, "{diff}");
            assert!(hunks[0].starts_with("@@ -6 +6 @@"), "{diff}");
            assert_eq!(entry["line"], 7, "{entry}");
            let check = run(&[
                "check", "--root", &root_arg, "--base", "main", "--format", "json",
            ])?;
            require_success("retargeted diff check", &check)?;
            let check = parse_json(
                "retargeted diff check",
                &stdout_text("retargeted diff check", &check)?,
            )?;
            let findings = check["findings"]
                .as_array()
                .ok_or("diff check omitted findings")?;
            assert!(
                findings.iter().any(|finding| {
                    finding["canonical_gap"]["id"] == RETARGETED_GAP
                        && finding["classification"] == "no_static_path"
                }),
                "{check}"
            );
        }
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
        assert_eq!(card["subject"]["canonical_gap_id"], json!(gap), "{card}");
        assert_eq!(card["snapshot"]["currentness"], "current", "{card}");
        assert_eq!(card["selected_target"]["file"], TARGET, "{card}");
        if gap.is_none() {
            assert!(card["subject"]["finding_id"].is_null(), "{card}");
        } else if startable {
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
            require_success(label, &start)?;
            let next = printed_continue_command(&stdout_text(label, &start)?)?;
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
            assert_eq!(start.status.code(), Some(3), "{label} seam: {stderr}");
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
                "canonical_gap_id": gap,
                "seam_id": seam,
                "proposed_target": TARGET,
                "start": result,
            }),
        )?;
        roots.cleanup()?;
    }
    candidate.verify_final_identity()?;
    install.cleanup()?;
    candidate.record(
        "direct-id-start-complete.json",
        &json!({"scenarios": scenario_count, "cleanup": "pass"}),
    )
}
