//! Render a check as the versioned JSON document machine consumers read.

use ripr::{OutputFormat, check_workspace};

#[path = "support/sample_workspace.rs"]
mod sample_workspace;

pub fn report() -> Result<String, String> {
    let workspace = sample_workspace::SampleWorkspace::create("render_json")?;
    let output = check_workspace(workspace.input())?;
    ripr::render_check(&output, &OutputFormat::Json)
}

pub fn main() -> Result<(), String> {
    println!("{}", report()?);
    Ok(())
}
