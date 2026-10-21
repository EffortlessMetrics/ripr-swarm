//! Execute the actual generated qualification traps, without installing a wheel.
#![cfg(target_os = "linux")]

use std::fs;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const WORKFLOW: &str = include_str!("../../../.github/workflows/python-wheel-qualification.yml");

#[test]
fn wheel_traps_preserve_literal_marker_paths_and_native_status() -> TestResult {
    let root = std::env::temp_dir().join(format!("wheel-trap-markers-{}", std::process::id()));
    fs::create_dir(&root)?;
    let result = exercise_traps(&root);
    let cleanup = fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}

fn exercise_traps(root: &std::path::Path) -> TestResult {
    for (variable, target, status, message) in [
        (
            "planted_marker",
            "${planted}/ripr",
            97,
            "planted PATH ripr was executed\n",
        ),
        (
            "project_marker",
            "${fixture}/.venv/bin/python",
            98,
            "project virtualenv was executed\n",
        ),
    ] {
        let start = format!("          cat > \"{target}\" <<SH\n");
        assert_eq!(WORKFLOW.matches(&start).count(), 1);
        let (_, tail) = WORKFLOW.split_once(&start).ok_or("trap start")?;
        let (body, _) = tail.split_once("          SH\n").ok_or("trap end")?;
        let source = format!("{start}{body}          SH\n");
        let source = source
            .lines()
            .map(|line| line.strip_prefix("          ").ok_or("indentation"))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        for (index, leaf) in [
            "plain space",
            "literal $CI_TRAP_INERT ; & [*] \" quote ' \\ ü",
            "line\nnext $CI_TRAP_INERT",
        ]
        .iter()
        .enumerate()
        {
            let case = root.join(format!("{variable}-{index}"));
            fs::create_dir_all(case.join(".venv/bin"))?;
            let marker = case.join(leaf);
            // Invoke the generated file directly through its declared Bash
            // interpreter. Its native status must escape the driver unchanged.
            let script = format!("{source}\n/bin/bash \"{target}\"\n");
            let output = Command::new("/usr/bin/timeout")
                .args([
                    "--kill-after=1s",
                    "5s",
                    "/bin/bash",
                    "-euo",
                    "pipefail",
                    "-c",
                ])
                .arg(script)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("planted", &case)
                .env("fixture", &case)
                .env(variable, &marker)
                .env("CI_TRAP_INERT", "EXPANDED_WRONG")
                .output()?;
            assert_eq!(output.status.code(), Some(status), "{output:?}");
            assert_eq!(fs::read_to_string(&marker)?, message);
            if leaf.contains("$CI_TRAP_INERT") {
                assert!(!case
                    .join(leaf.replace("$CI_TRAP_INERT", "EXPANDED_WRONG"))
                    .exists());
            }
        }
    }
    Ok(())
}
