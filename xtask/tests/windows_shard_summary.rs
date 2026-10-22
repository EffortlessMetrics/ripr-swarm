//! #9272: the sharded Windows workflow's failure summary must still name the
//! failing tests when nextest colors its status rows. Runs the checked-in
//! `Summarize failures` step against a colored log.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn summarize_step() -> Result<String, String> {
    let workflow = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.github/workflows/windows-advisory-shards.yml");
    let text = fs::read_to_string(workflow).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = text
        .lines()
        .skip_while(|line| !line.contains("- name: Summarize failures"))
        .skip_while(|line| line.trim() != "run: |")
        .skip(1)
        .take_while(|line| line.starts_with("          "))
        .map(|line| line.trim_start())
        .collect();
    if lines.is_empty() {
        return Err("Summarize failures step not found".to_string());
    }
    Ok(lines.join("\n").replace("${{ matrix.shard }}", "1"))
}

#[test]
fn colored_nextest_rows_still_reach_the_summary() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("ripr-shard-summary-{}", std::process::id()));
    fs::create_dir_all(dir.join("target")).map_err(|e| e.to_string())?;
    let log = "\u{1b}[31;1m        FAIL\u{1b}[0m [   0.1s] \u{1b}[35mxtask\u{1b}[0m some::failing_test\n\
               \u{1b}[31;1m       LEAK\u{1b}[0m [   0.1s] \u{1b}[35mxtask\u{1b}[0m some::leaky_test\n\
               \u{1b}[31;1m     Summary\u{1b}[0m [  30.0s] 10 tests run: 8 passed, 1 failed\n";
    fs::write(dir.join("target/shard-1.log"), log).map_err(|e| e.to_string())?;
    fs::write(dir.join("target/shard-1.status"), "100\n").map_err(|e| e.to_string())?;
    let summary = dir.join("summary.md");
    let script = dir.join("step.sh");
    fs::write(&script, summarize_step()?).map_err(|e| e.to_string())?;
    let status = Command::new("bash")
        .arg(&script)
        .current_dir(&dir)
        .env("GITHUB_STEP_SUMMARY", &summary)
        .status()
        .map_err(|e| e.to_string())?;
    let out = fs::read_to_string(&summary).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);
    if !status.success() {
        return Err(format!("summary step failed: {status}"));
    }
    for needle in [
        "some::failing_test",
        "some::leaky_test",
        "Summary",
        "status: 100",
    ] {
        if !out.contains(needle) {
            return Err(format!("summary lost `{needle}`:\n{out}"));
        }
    }
    Ok(())
}
