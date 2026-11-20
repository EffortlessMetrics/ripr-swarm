use super::{Fixture, PASS};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn check_producer_order(import_first: bool) -> Result<(), Box<dyn std::error::Error>> {
    let captures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/ripr/src/app/kani_import/fixtures/native-captures");
    let binding =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kani-alias-import.binding.json");
    let fixture = Fixture::new(&[("one.rs", "one")], PASS)?;
    fs::copy(captures.join("proof.rs.txt"), fixture.0.join("proof.rs"))?;
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
    let journal: serde_json::Value = serde_json::from_slice(&journal_json)?;
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
    let imported: serde_json::Value =
        serde_json::from_slice(&fs::read(reports.join("kani-import.json"))?)?;
    assert_eq!(imported["schema_version"], "ripr_kani_report.v1");
    assert_eq!(imported["provenance"], "external_unattested");
    assert_eq!(imported["binding"]["run_id"], "alias-test-import");
    let md = fs::read_to_string(reports.join("kani-import.md"))?;
    let embedded = md
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&embedded)?,
        imported
    );
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
