use ripr::{CheckInput, OutputFormat, check_workspace};
use std::path::PathBuf;

fn sample_input() -> CheckInput {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("examples/sample");
    let diff_file = root.join("example.diff");

    CheckInput {
        root,
        diff_file: Some(diff_file),
        format: OutputFormat::Json,
        ..CheckInput::default()
    }
}

fn main() -> Result<(), String> {
    let output = check_workspace(sample_input())?;
    let rendered = ripr::app::render_check(&output, &OutputFormat::Json)?;
    println!("{rendered}");
    Ok(())
}
