use super::*;

fn recorded_run(stdout_bytes: usize, stderr_bytes: usize) -> Result<Report, String> {
    let root = std::env::temp_dir().join(format!(
        "journal-retention-boundary-{}-{}",
        std::process::id(),
        NEXT_RUN.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let file = root.join("proof.rs");
    fs::write(&file, "#[kani::proof]\nfn proof() {}\n").map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().into_owned();
    let harness = Harness {
        file: path.clone(),
        name: "proof".into(),
    };
    let mut report = Report::new("retention-boundary-synthetic".into());
    report.request(&super::super::parse_args(&[])?);
    report.availability(true);
    report.select(
        "ripr",
        &[(path, fs::read_to_string(&file).map_err(|e| e.to_string())?)],
        std::slice::from_ref(&harness),
    )?;
    report.capture(
        InvocationContext {
            package: "@preflight",
            argv: &[],
            budget: None,
            elapsed: Duration::ZERO,
            harnesses: &[],
        },
        CaptureOutcome {
            exit_code: Some(0),
            exit_success: Some(true),
            timed_out: false,
            stdout: Some("v"),
            stderr: Some(""),
            error: None,
        },
    );
    let pass = "Checking harness m::proof...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
    let stdout = format!(
        "{}\n{pass}",
        "x".repeat(
            stdout_bytes
                .checked_sub(pass.len() + 1)
                .ok_or("stdout fixture shorter than native verdict")?
        )
    );
    let stderr = "x".repeat(stderr_bytes);
    report.capture(
        InvocationContext {
            package: "ripr",
            argv: &[],
            budget: None,
            elapsed: Duration::ZERO,
            harnesses: std::slice::from_ref(&harness),
        },
        CaptureOutcome {
            exit_code: Some(0),
            exit_success: Some(true),
            timed_out: false,
            stdout: Some(&stdout),
            stderr: Some(&stderr),
            error: None,
        },
    );
    report.rows(
        "ripr",
        &super::super::results_from_output(&[harness], &stdout),
    );
    report.recheck_sources();
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    if report.selected_files_unchanged_after_run != Some(true)
        || report.results.len() != 1
        || report.invocations.len() != 2
        || !matches!(
            report.results[0].observation.status,
            super::super::HarnessStatus::Verified
        )
        || !report
            .invocations
            .iter()
            .all(|i| i.exit_success == Some(true) && i.unmatched_native_blocks.is_empty())
        || report.registered_claims != 0
        || report.claim_assurance != "not_established"
    {
        return Err("boundary fixture did not establish complete synthetic observations".into());
    }
    Ok(report)
}

#[test]
fn stream_overflow_refuses_otherwise_valid_lane() -> Result<(), String> {
    let mut positive = recorded_run(MAX_STREAM_BYTES, 0)?;
    positive.finish(Ok(()))?;
    if !positive.lane_success {
        return Err("exact stream cap did not pass".into());
    }
    let mut overflow = recorded_run(MAX_STREAM_BYTES + 1, 0)?;
    if overflow.invocations[1]
        .stdout
        .as_ref()
        .ok_or("missing stdout")?
        .text
        .is_some()
    {
        return Err("overflow stdout retained unexpectedly".into());
    }
    let error = match overflow.finish(Ok(())) {
        Err(error) => error,
        Ok(()) => return Err("omitted stream must refuse lane success".into()),
    };
    if !error.contains("per-stream limit") {
        return Err(format!("wrong stream refusal: {error}"));
    }
    if overflow.lane_success {
        return Err("omitted text admitted lane success".into());
    }
    Ok(())
}

#[test]
fn aggregate_overflow_refuses_otherwise_valid_lane() -> Result<(), String> {
    // One-byte preflight plus two individually admissible streams cross the
    // run limit by one byte. This isolates cumulative accounting from stream cap.
    if MAX_RUN_TEXT_BYTES != 2 * MAX_STREAM_BYTES {
        return Err("aggregate boundary fixture needs updated dimensions".into());
    }
    let mut positive = recorded_run(MAX_STREAM_BYTES, MAX_STREAM_BYTES - 1)?;
    positive.finish(Ok(()))?;
    if !positive.lane_success {
        return Err("exact aggregate cap did not pass".into());
    }
    let mut overflow = recorded_run(MAX_STREAM_BYTES, MAX_STREAM_BYTES)?;
    if overflow.invocations[1]
        .stdout
        .as_ref()
        .ok_or("missing stdout")?
        .text
        .is_some()
    {
        return Err("overflow stdout retained unexpectedly".into());
    }
    if overflow.invocations[1]
        .stderr
        .as_ref()
        .ok_or("missing stderr")?
        .text
        .is_some()
    {
        return Err("overflow stderr retained unexpectedly".into());
    }
    let error = match overflow.finish(Ok(())) {
        Err(error) => error,
        Ok(()) => return Err("omitted run text must refuse lane success".into()),
    };
    if !error.contains("aggregate run limit") {
        return Err(format!("wrong aggregate refusal: {error}"));
    }
    if overflow.lane_success {
        return Err("omitted text admitted lane success".into());
    }
    Ok(())
}
