//! The measured pilot, warm checks and editor must consume the private pin.

use super::*;

#[test]
fn measured_consumers_use_the_private_pin_and_preserve_the_cached_source() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let binary = scoreboard_test_ripr_binary()?;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let fixture = Fixture(crate::tests::temp_dir("dx-measured-consumer-root"));
    let corpus = fixture.0.join("corpus");
    let source = corpus.join("fixture");
    fs::create_dir_all(source.join("src")).map_err(|err| err.to_string())?;
    let git = |args: &[&str]| super::super::measure::git(Some(&source), args);
    git(&["init", "--quiet"])?;
    git(&["config", "core.autocrlf", "false"])?;
    git(&["config", "user.name", "t"])?;
    git(&["config", "user.email", "t@t"])?;
    git(&["config", "commit.gpgsign", "false"])?;
    let manifest = concat!(
        "[package]\n",
        "name = \"dx-consumer-root\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2024\"\n",
        "[workspace]\n",
    );
    let lockfile = concat!(
        "version = 4\n\n",
        "[[package]]\n",
        "name = \"dx-consumer-root\"\n",
        "version = \"0.1.0\"\n",
    );
    let config = "[languages]\nenabled = [\"rust\"]\n";
    let pinned = "pub fn n() -> u8 { 2 }\n";
    fs::write(source.join("Cargo.toml"), manifest).map_err(|err| err.to_string())?;
    fs::write(source.join("Cargo.lock"), lockfile).map_err(|err| err.to_string())?;
    fs::write(source.join(".gitignore"), "/ripr.toml\n/target/\n")
        .map_err(|err| err.to_string())?;
    fs::write(source.join("src/lib.rs"), "pub fn n() -> u8 { 1 }\n")
        .map_err(|err| err.to_string())?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "base"])?;
    let base = git(&["rev-parse", "HEAD"])?.trim().to_string();
    fs::write(source.join("src/lib.rs"), pinned).map_err(|err| err.to_string())?;
    fs::write(source.join("ripr.toml"), config).map_err(|err| err.to_string())?;
    git(&["add", "src/lib.rs"])?;
    git(&["add", "-f", "ripr.toml"])?;
    git(&["commit", "--quiet", "-m", "pin"])?;
    let pin = git(&["rev-parse", "HEAD"])?.trim().to_string();

    // The cached tip has no changed Rust file for the editor to open. A
    // wrong-root editor therefore yields NotMeasured instead of a Value.
    git(&["rm", "--quiet", "src/lib.rs", "ripr.toml"])?;
    fs::write(source.join("README.md"), "cached tip\n").map_err(|err| err.to_string())?;
    git(&["add", "README.md"])?;
    git(&["commit", "--quiet", "-m", "cached tip"])?;
    let cached_head = git(&["rev-parse", "HEAD"])?;
    let cached_changes = git(&["diff", "--name-only", "--diff-filter=AM", &base, "HEAD"])?;
    assert!(
        !cached_changes.lines().any(|path| path.ends_with(".rs")),
        "cached tip must not supply an editor subject: {cached_changes}"
    );
    let contaminated_config = "[languages\n";
    fs::write(source.join("ripr.toml"), contaminated_config).map_err(|err| err.to_string())?;
    fs::write(source.join("extra.rs"), "untracked\n").map_err(|err| err.to_string())?;
    fs::create_dir_all(source.join("target/ripr")).map_err(|err| err.to_string())?;
    fs::write(source.join("target/ripr/keep"), "cached artifact\n")
        .map_err(|err| err.to_string())?;
    let source_status = git(&["status", "--porcelain", "--untracked-files=all"])?;

    // Exercise the proposed wrong-root command against the same real binary.
    // Its failure must be the malformed cached config, not a setup/timeout.
    let wrong_args = warm_check_args(&source.display().to_string(), &base);
    let wrong_root = run_measured_check(&wrong_args, &fixture.0.join("wrong-root-cache"))?;
    assert!(!wrong_root.output.timed_out, "wrong-root control timed out");
    assert!(
        wrong_root.output.status.is_some_and(|status| !status.success()),
        "the contaminated cached root unexpectedly passed"
    );
    assert!(
        wrong_root.output.stderr.contains("ripr.toml"),
        "wrong-root refusal did not name the malformed config: {}",
        wrong_root.output.stderr
    );

    let options = parse_options(&[
        "--corpus-dir".to_string(),
        corpus.display().to_string(),
        "--samples".to_string(),
        "1".to_string(),
    ])?;
    let entry = CorpusEntry {
        id: "fixture".to_string(),
        url: String::new(),
        sha: pin.clone(),
        base_sha: Some(base.clone()),
        note: String::new(),
        heavy: false,
        language: None,
        ceilings: BTreeMap::new(),
        expected_partial: None,
        size_tier: None,
        memory_ratchet: None,
    };
    let scratch = fixture.0.join("run");
    fs::create_dir(&scratch).map_err(|err| err.to_string())?;
    let mut samples = Vec::new();
    let result = super::super::measure::measure_corpus_entry(
        &entry,
        &options,
        Path::new(&binary),
        &scratch,
        0,
        std::time::Duration::from_mins(2),
        &mut samples,
    );
    assert_eq!(result["status"], "measured");
    assert_eq!(samples.len(), 11, "{samples:?}");
    for metric in [
        "speed.cold_pilot_ms",
        "speed.warm_check_ms",
        "speed.warm_check_p50_ms",
        "speed.warm_check_p95_ms",
        "speed.editor_refresh_ms",
        "speed.editor_refresh_unanswered_progress_ms",
    ] {
        let matching: Vec<_> = samples
            .iter()
            .filter(|sample| sample.metric == metric)
            .collect();
        assert_eq!(matching.len(), 1, "{metric}: {samples:?}");
        assert!(
            matches!(matching[0].outcome, SampleOutcome::Value(value) if value.is_finite()),
            "{metric} did not complete against the private pin: {:?}",
            matching[0]
        );
    }
    let resilience = samples
        .iter()
        .find(|sample| sample.metric == "trust.editor_resilience_failures")
        .ok_or("missing editor resilience sample")?;
    assert!(
        matches!(resilience.outcome, SampleOutcome::Value(value) if value == 0.0),
        "{resilience:?}"
    );

    let checkout = fs::canonicalize(scratch.join("checkout-0")).map_err(|err| err.to_string())?;
    let pilot_path = scratch.join("pilot-fixture/pilot-summary.json");
    let pilot_bytes = fs::read(&pilot_path).map_err(|err| err.to_string())?;
    let pilot: Value = serde_json::from_slice(&pilot_bytes).map_err(|err| err.to_string())?;
    let pilot_root = pilot["root"].as_str().ok_or("pilot omitted its root")?;
    assert_eq!(
        fs::canonicalize(pilot_root).map_err(|err| err.to_string())?,
        checkout
    );
    assert_eq!(
        super::super::measure::git(Some(&checkout), &["rev-parse", "HEAD"])?.trim(),
        pin
    );
    assert_eq!(
        super::super::measure::git(Some(&checkout), &["rev-parse", "HEAD~1"])?.trim(),
        base
    );
    assert_eq!(
        fs::read_to_string(checkout.join("src/lib.rs")).map_err(|err| err.to_string())?,
        pinned,
        "editor sessions must restore the pinned file"
    );
    assert_eq!(
        fs::read_to_string(checkout.join("ripr.toml")).map_err(|err| err.to_string())?,
        config
    );
    assert!(!checkout.join("extra.rs").exists());
    assert!(!checkout.join("target/ripr/keep").exists());
    assert_eq!(git(&["rev-parse", "HEAD"])?, cached_head);
    assert_eq!(
        git(&["status", "--porcelain", "--untracked-files=all"])?,
        source_status
    );
    assert!(!source.join("src/lib.rs").exists());
    assert_eq!(
        fs::read_to_string(source.join("ripr.toml")).map_err(|err| err.to_string())?,
        contaminated_config
    );
    assert_eq!(
        fs::read_to_string(source.join("extra.rs")).map_err(|err| err.to_string())?,
        "untracked\n"
    );
    assert_eq!(
        fs::read_to_string(source.join("target/ripr/keep")).map_err(|err| err.to_string())?,
        "cached artifact\n"
    );
    Ok(())
}
