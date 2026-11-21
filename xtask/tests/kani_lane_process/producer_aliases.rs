use super::{Fixture, PASS};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn check_producer_order(import_first: bool) -> Result<(), Box<dyn std::error::Error>> {
    let captures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/ripr/src/app/kani_import/fixtures/native-captures");
    let fixture = Fixture::new(&[("one.rs", "one")], PASS)?;
    fs::copy(captures.join("proof.rs.txt"), fixture.0.join("proof.rs"))?;
    // Author a local replay binding against the retained capture's recorded identities.
    let provenance: Value = serde_json::from_slice(&fs::read(captures.join("provenance.json"))?)?;
    let success = provenance["runs"]
        .as_array()
        .ok_or("capture inventory")?
        .iter()
        .find(|run| run["id"] == "success")
        .ok_or("success capture")?;
    let source = &provenance["source"];
    let digest = source["inventory_digest"].as_str().ok_or("source digest")?;
    let binding = fixture.0.join("binding.json");
    fs::write(
        &binding,
        serde_json::to_vec(&json!({
            "schema_version": "ripr_kani_binding.v1",
            "repository": {"repository_id": "alias-test", "snapshot_id": format!("captured:{digest}"),
                "source_view": "captured_source_set", "source_digest": digest},
            "run_id": "alias-test-import", "artifact_sha256": success["native_sha256"],
            "source_files": [{"path": "proof.rs", "roles": ["implementation", "harness"], "sha256": source["sha256"]}],
            "context": {"target": provenance["producer"]["target"], "cargo_features": [], "default_features": false,
                "toolchain": provenance["producer"]["toolchain"],
                "environment_sha256": format!("sha256:{:x}", Sha256::digest(b"local importer replay")),
                "invocation": ["kani", "proof.rs", "--export-json", "success.native.json"]},
            "selected_harnesses": success["selected_harnesses"]
        }))?,
    )?;
    let reports = fixture.0.join("target/ripr/reports");
    let import = || {
        ripr::cli::run(vec![
            "ripr".into(),
            "reports".into(),
            "kani".into(),
            "--root".into(),
            fixture.0.display().to_string(),
            "--input".into(),
            captures.join("success.native.json").display().to_string(),
            "--binding".into(),
            binding.display().to_string(),
        ])
    };
    if import_first {
        import()?;
    }
    let observation = fixture.run("success", false)?;
    assert!(observation.output.status.success());
    let journal_json = fs::read(reports.join("kani.json"))?;
    if !import_first {
        import()?;
    }
    assert!(
        fs::read(reports.join("kani.json"))? == journal_json,
        "the import overwrote the journal JSON alias"
    );
    let journal: Value = serde_json::from_slice(&journal_json)?;
    assert_eq!(journal["report"]["schema"], "ripr.kani-observation.v1");
    let run_id = journal["report"]["run_id"]
        .as_str()
        .ok_or("journal run ID")?;
    let checkpoint = reports.join(journal["markdown_path"].as_str().ok_or("checkpoint path")?);
    assert_eq!(
        checkpoint.parent(),
        Some(reports.join("kani-runs").join(run_id).as_path())
    );
    assert_eq!(fs::read(checkpoint.with_extension("json"))?, journal_json);
    assert_eq!(fs::read_to_string(checkpoint)?, observation.report);
    assert_eq!(journal["markdown"], observation.report);
    assert_eq!(
        journal["markdown_sha256"],
        format!("sha256:{:x}", Sha256::digest(observation.report.as_bytes()))
    );
    assert_eq!(
        fs::read_to_string(reports.join("kani.md"))?,
        observation.report
    );
    let imported: Value = serde_json::from_slice(&fs::read(reports.join("kani-import.json"))?)?;
    assert_eq!(imported["schema_version"], "ripr_kani_report.v1");
    assert_eq!(imported["provenance"], "external_unattested");
    assert_eq!(imported["binding"]["run_id"], "alias-test-import");
    let md = fs::read_to_string(reports.join("kani-import.md"))?;
    let embedded = md
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(serde_json::from_str::<Value>(&embedded)?, imported);
    Ok(())
}

#[test]
fn both_kani_producers_journal_then_import() -> Result<(), Box<dyn std::error::Error>> {
    check_producer_order(false)
}

#[test]
fn both_kani_producers_import_then_journal() -> Result<(), Box<dyn std::error::Error>> {
    check_producer_order(true)
}
