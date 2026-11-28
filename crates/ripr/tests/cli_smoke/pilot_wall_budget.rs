//! #5322: a many-function repo must not outrun a hard pilot deadline by multiples
//! of the cancel grace after the budget is spent (hostile-input cancellation lag).
use super::{DeclaredHunkScratch, probe_command, run_owned_stdin_probe, unique_external_workspace};
use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const FUNCTION_COUNT: usize = 6_000;

#[test]
fn many_function_pilot_hard_timeout_exits_within_cancel_grace() -> TestResult {
    let sandbox = unique_external_workspace("pilot-wall-budget-5322")?;
    fs::create_dir_all(&sandbox)?;
    let _cleanup = DeclaredHunkScratch(sandbox.clone());
    let root = sandbox.join("many-fn-root");
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"many_fn\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"rust\"]\n",
    )?;
    let mut generated = String::from("pub fn anchor(a: u32) -> u32 { a + 1 }\n");
    for i in 0..FUNCTION_COUNT {
        generated.push_str(&format!("pub fn g{i}(a: u32) -> u32 {{ a + {i} }}\n"));
    }
    fs::write(root.join("src/lib.rs"), generated)?;
    let packet = root.join("target/ripr/pilot");
    let packet_arg = packet
        .to_str()
        .ok_or("packet path must be utf-8")?
        .to_string();

    let timeout_ms = 750_u64;
    let mut command = probe_command(env!("CARGO_BIN_EXE_ripr"));
    command
        .current_dir(&root)
        .args([
            "pilot",
            "--root",
            ".",
            "--out",
            &packet_arg,
            "--quiet",
            "--timeout-ms",
            &timeout_ms.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let started = Instant::now();
    let output = run_owned_stdin_probe(command, &[], Duration::from_secs(20))?;
    let elapsed = started.elapsed();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let summary = fs::read_to_string(packet.join("pilot-summary.json"))?;
    assert!(
        summary.contains(r#""reason": "timeout""#),
        "expected a timeout partial summary, got:\n{summary}"
    );
    // Hard limit: budget + the pilot cancel grace (5s) + startup slack.
    let ceiling = Duration::from_millis(timeout_ms + 6_500);
    assert!(
        elapsed <= ceiling,
        "pilot ran {elapsed:?} on {FUNCTION_COUNT} functions; expected <= {ceiling:?}\nstderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
