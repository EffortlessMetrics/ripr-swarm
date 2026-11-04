use ripr::{CheckInput, OutputFormat, check_workspace};
use std::path::PathBuf;

fn sample_input() -> CheckInput {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let diff_file = manifest_dir.join("examples/library.diff");

    CheckInput {
        root: manifest_dir.join("examples/sample"),
        diff_file: Some(diff_file),
        format: OutputFormat::Json,
        ..CheckInput::default()
    }
}

fn main() -> Result<(), String> {
    let output = check_workspace(sample_input())?;

    let json = ripr::render_check(&output, &OutputFormat::Json)?;
    let report: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    eprintln!("analysis outcome: {}", report["analysis_outcome"]);

    println!("schema: {}", output.schema_version);
    println!("findings: {}", output.findings.len());
    for finding in &output.findings {
        println!("- {}: {}", finding.id, finding.class.as_str());
    }

    Ok(())
}
