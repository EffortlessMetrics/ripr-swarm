//! The Evidence legend is a first-run aid: later runs keep one pointer line.
use super::{run_command, unique_external_workspace};
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const LEGEND: &str = "Stages (static evidence)";
const POINTER: &str = "Terms: `ripr check --legend`";

fn workspace(label: &str) -> TestResult<std::path::PathBuf> {
    let project = unique_external_workspace(label)?;
    fs::create_dir_all(project.join("src"))?;
    fs::create_dir_all(project.join("tests"))?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(project.join("src/lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
    fs::write(
        project.join("tests/price.rs"),
        "use pricing::price;\n#[test]\nfn exact_price() { assert_eq!(price(), 2); }\n",
    )?;
    fs::write(
        project.join("change.diff"),
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn price() -> u32 { 1 }\n+pub fn price() -> u32 { 2 }\n",
    )?;
    Ok(project)
}

fn check(project: &Path, extra: &[&str]) -> TestResult<String> {
    let mut args = vec!["check", "--diff", "change.diff"];
    args.extend_from_slice(extra);
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(project), &args)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn legend_prints_on_the_first_run_then_collapses_to_a_pointer() -> TestResult {
    let project = workspace("legend-first-run")?;
    let marker = project.join("target/ripr/legend-seen");

    let first = check(&project, &[])?;
    assert!(first.contains(LEGEND), "{first}");
    assert!(!first.contains(POINTER), "{first}");
    assert!(marker.is_file(), "first run must record the legend as seen");

    let second = check(&project, &[])?;
    assert!(!second.contains(LEGEND), "{second}");
    assert!(second.contains(POINTER), "{second}");
    // Only the ten legend lines became one line; the finding is unchanged.
    assert_eq!(
        first.lines().count() - 9,
        second.lines().count(),
        "legend must collapse by exactly nine lines"
    );

    let forced = check(&project, &["--legend"])?;
    assert!(forced.contains(LEGEND), "{forced}");
    assert!(!forced.contains(POINTER), "{forced}");
    Ok(())
}

#[test]
fn machine_formats_and_a_run_without_a_legend_do_not_consume_the_first_run() -> TestResult {
    let project = workspace("legend-machine")?;
    let marker = project.join("target/ripr/legend-seen");

    check(&project, &["--format", "json"])?;
    assert!(!marker.exists(), "json must not record the legend as seen");

    let first = check(&project, &[])?;
    assert!(first.contains(LEGEND), "{first}");
    assert!(marker.is_file());
    Ok(())
}
