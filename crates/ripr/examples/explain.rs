//! Explain the first finding of a check: its evidence and the next step.

use ripr::check_workspace;

#[path = "support/sample_workspace.rs"]
mod sample_workspace;

pub fn report() -> Result<String, String> {
    let workspace = sample_workspace::SampleWorkspace::create("explain")?;
    let input = workspace.input();
    let finding_id = check_workspace(input.clone())?
        .findings
        .first()
        .map(|finding| finding.id.clone())
        .ok_or_else(|| "sample diff produced no findings".to_string())?;
    ripr::explain_finding_with_input(input, &finding_id)
}

pub fn main() -> Result<(), String> {
    println!("{}", report()?);
    Ok(())
}
