use super::*;
use super::field_snapshot::{populate, read_json};

const PANIC: &str = "rust_panic_outcome_unresolved";
const FRAMEWORK: &str = "rust_framework_oracle_unresolved";

#[test]
fn an_ordinary_panic_token_test_name_keeps_pilot_and_brief_recommendations() -> Result<(), String> {
    for name in ["ordinary_weak_assertion", PANIC] {
        let fixture = Fixture::dir(&format!("facade-panic-marker-name-{name}"))?;
        let root = &fixture.root;
        populate(root, "assert!(record.items > 0);", name)?;
        let root_arg = root.to_string_lossy().into_owned();
        let pilot = run_ripr(root, &["pilot", "--root", &root_arg, "--quiet"])?;
        assert_exit(&pilot, 0, "pilot on the ordinary weak assertion");
        let summary_path = root.join("target/ripr/pilot/pilot-summary.json");
        let exposure_path = root.join("target/ripr/pilot/repo-exposure.json");
        let summary = read_json(&summary_path)?;
        let exposure = read_json(&exposure_path)?;
        assert_eq!(summary["status"], "complete");
        assert_eq!(summary["actionable_seams_total"], 3, "{summary}");
        assert_eq!(summary["withheld_static_limitations_total"], 0, "{summary}");
        let seams = exposure["seams"].as_array().ok_or("missing repo seams")?;
        assert_eq!(seams.len(), 3, "one return and two field seams");
        for seam in seams {
            let record = &seam["evidence_record"];
            assert_eq!(record["grip_class"], "weakly_gripped", "{record}");
            for stage in ["reach", "activate", "propagate", "observe"] {
                assert_eq!(record["evidence_path"][stage]["state"], "yes", "{record}");
            }
            assert_eq!(record["evidence_path"]["discriminate"]["state"], "weak");
            let limits = record["static_limitations"].as_array().ok_or("missing limits")?;
            assert!(
                limits.iter().all(|limit| {
                    limit["category"] != PANIC && limit["category"] != FRAMEWORK
                }),
                "{record}"
            );
        }
        for (line, seam_id) in [(8, "9690b8968d3b2c8e"), (9, "969e35968d468b51")] {
            let fields: Vec<_> = seams.iter().filter(|seam| seam["seam_id"] == seam_id).collect();
            assert_eq!(fields.len(), 1, "one exact field seam");
            assert_eq!(fields[0]["kind"], "field_construction");
            assert_eq!(fields[0]["line"], line);
            let brief = run_ripr(
                root,
                &["agent", "brief", "--root", &root_arg, "--seam-id", seam_id, "--json"],
            )?;
            assert_exit(&brief, 0, "brief on the ordinary weak field");
            let brief: serde_json::Value = serde_json::from_slice(&brief.stdout)
                .map_err(|error| error.to_string())?;
            let top = brief["top_seams"].as_array().ok_or("missing brief top_seams")?;
            assert_eq!(top.len(), 1, "{brief}");
            assert_eq!(top[0]["seam_id"], seam_id);
            assert!(
                brief["warnings"].as_array().is_some_and(|warnings| {
                    warnings.iter().all(|warning| {
                        warning.as_str().is_some_and(|warning| !warning.contains("expected panic"))
                    })
                }),
                "{brief}"
            );
        }
        let warm = run_ripr_env(
            root,
            &["pilot", "--root", &root_arg, "--quiet"],
            &[("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")],
        )?;
        assert_exit(&warm, 0, "warm pilot on the ordinary weak assertion");
        assert!(stderr_text(&warm).contains("phase=cache_load status=hit"));
        assert_eq!(read_json(&exposure_path)?["seams"], exposure["seams"]);
        assert_eq!(read_json(&summary_path)?["actionable_seams_total"], 3);
        assert_eq!(read_json(&summary_path)?["withheld_static_limitations_total"], 0);
    }
    Ok(())
}

#[test]
fn a_field_snapshot_cannot_replace_an_established_panic_explanation() -> Result<(), String> {
    let fixture = Fixture::dir("facade-panic-snapshot-coexist")?;
    let root = &fixture.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML)
        .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        include_str!("../../../../fixtures/evidence-promotion-honesty-corpus/repo-exposure-subjects/panic-marker-coexist/src/lib.rs"),
    )
    .map_err(|error| error.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src"])?;
    git_commit(root, "panic and snapshot coexistence")?;
    let root_arg = root.to_string_lossy().into_owned();
    let pilot = run_ripr(root, &["pilot", "--root", &root_arg, "--quiet"])?;
    assert_exit(&pilot, 0, "pilot on genuine panic and field snapshot uncertainty");
    let summary = read_json(&root.join("target/ripr/pilot/pilot-summary.json"))?;
    let exposure = read_json(&root.join("target/ripr/pilot/repo-exposure.json"))?;
    assert_eq!(summary["status"], "complete");
    assert_eq!(summary["actionable_seams_total"], 0);
    assert_eq!(summary["withheld_static_limitations_total"], 2);
    let seams = exposure["seams"].as_array().ok_or("missing repo seams")?;
    assert_eq!(seams.len(), 2);
    let fields: Vec<_> = seams.iter().filter(|seam| seam["kind"] == "field_construction").collect();
    assert_eq!(fields.len(), 1);
    let field = fields[0];
    assert_eq!(field["seam_id"], "96862f968d31ef3e");
    assert_eq!(field["grip_class"], "reachable_unrevealed");
    let limits = field["evidence_record"]["static_limitations"]
        .as_array().ok_or("missing static limitations")?;
    let named: Vec<_> = limits.iter().filter(|limit| limit["category"] == PANIC).collect();
    assert_eq!(named.len(), 1);
    assert_eq!(named[0]["stage"], "discriminate");
    assert_eq!(named[0]["state"], "no");
    assert!(named[0]["reason"].as_str().is_some_and(|reason| {
        reason.starts_with(&format!("{PANIC}: ")) && reason.contains("snapshot_panicking_record")
    }));
    assert!(limits.iter().all(|limit| limit["category"] != FRAMEWORK));
    let brief = run_ripr(
        root,
        &["agent", "brief", "--root", &root_arg, "--seam-id", "96862f968d31ef3e", "--json"],
    )?;
    assert_exit(&brief, 0, "brief preserves the genuine panic explanation");
    let brief: serde_json::Value = serde_json::from_slice(&brief.stdout)
        .map_err(|error| error.to_string())?;
    assert_eq!(brief["top_seams"].as_array().ok_or("missing top_seams")?.len(), 0);
    assert!(brief["warnings"].as_array().is_some_and(|warnings| {
        warnings.iter().any(|warning| {
            warning.as_str().is_some_and(|warning| warning.contains("expected panic"))
        }) && warnings.iter().all(|warning| {
            warning.as_str().is_some_and(|warning| !warning.contains("framework oracle"))
        })
    }));
    Ok(())
}
