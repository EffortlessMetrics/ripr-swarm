//! Exercises the inputs shipped with the runnable library examples.
#![cfg(feature = "lang-rust")]
use ripr::{CheckInput, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn packaged_library_example_has_valid_nonempty_analysis() -> Result<(), String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = check_workspace(CheckInput {
        root: manifest.join("examples/sample"),
        diff_file: Some(manifest.join("examples/library.diff")),
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let json = ripr::render_check(&output, &OutputFormat::Json)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let outcome = &value["analysis_outcome"];
    assert_eq!(outcome["analysis_complete"], false, "{json}");
    let limitations = outcome["outcome"]["limitations"]
        .as_array()
        .ok_or("missing limitations")?;
    assert_eq!(limitations.len(), 1, "{json}");
    assert!(
        limitations[0]["bounded_detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("no local Cargo.toml")),
        "{json}"
    );
    assert!(
        !output.findings.is_empty(),
        "sample must produce findings: {json}"
    );
    Ok(())
}

mod check_diff_example {
    include!("../examples/check_diff.rs");
    pub fn run() -> Result<(), String> {
        main()
    }
}
mod render_json_example {
    include!("../examples/render_json.rs");
    pub fn run() -> Result<(), String> {
        main()
    }
}
mod explain_example {
    include!("../examples/explain.rs");
    pub fn run() -> Result<(), String> {
        main()
    }
}

#[test]
fn packaged_library_example_entrypoints_run() -> Result<(), String> {
    check_diff_example::run()?;
    render_json_example::run()?;
    explain_example::run()?;
    Ok(())
}
