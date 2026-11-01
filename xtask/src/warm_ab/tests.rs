use super::judge::{Limits, Verdict, evidence_failure, judge, median};

#[test]
fn median_takes_the_middle_and_averages_an_even_pair() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[7]), Some(7));
    assert_eq!(median(&[9, 1, 5]), Some(5));
    assert_eq!(median(&[4, 10, 2, 8]), Some(6));
}

#[test]
fn a_doubled_candidate_fails_and_an_equal_one_passes() {
    let limits = Limits::default();
    // The #10489 shape: about 31 s became about 64 s on the same tree.
    let slow = judge(
        &[31_000, 30_400, 31_900],
        &[63_800, 61_700, 64_000],
        &limits,
    );
    assert_eq!(slow.verdict, Verdict::Fail, "{}", slow.detail);
    assert!(slow.ratio.is_some_and(|ratio| ratio > 1.9));
    let same = judge(
        &[31_000, 30_400, 31_900],
        &[31_500, 30_900, 32_100],
        &limits,
    );
    assert_eq!(same.verdict, Verdict::Pass, "{}", same.detail);
}

#[test]
fn a_big_ratio_under_the_noise_floor_passes() {
    let judged = judge(&[400, 410, 390], &[1_000, 1_100, 900], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Pass, "{}", judged.detail);
}

#[test]
fn the_ceiling_fails_even_when_the_baseline_is_as_slow() {
    let judged = judge(&[400_000], &[410_000], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.detail.contains("ceiling"), "{}", judged.detail);
}

#[test]
fn a_side_without_samples_fails() {
    let judged = judge(&[], &[1_000], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.ratio.is_none());
}

#[test]
fn one_sided_zero_findings_fail_on_either_side() {
    // 1/0: a fast empty candidate must not pass against a nonempty baseline.
    let cand_empty = evidence_failure(&[1, 1, 1], &[1, 0, 1]);
    assert!(cand_empty.is_some(), "candidate-side zero passed");
    assert!(
        cand_empty
            .as_deref()
            .unwrap_or_default()
            .contains("candidate"),
        "{cand_empty:?}"
    );
    // 0/1: the reciprocal one-sided zero fails too.
    let base_empty = evidence_failure(&[0, 1, 1], &[1, 1, 1]);
    assert!(base_empty.is_some(), "baseline-side zero passed");
    assert!(
        base_empty
            .as_deref()
            .unwrap_or_default()
            .contains("baseline"),
        "{base_empty:?}"
    );
}

#[test]
fn both_empty_and_healthy_finding_counts_are_judged_per_side() {
    assert!(evidence_failure(&[0], &[0]).is_some(), "both-empty passed");
    assert!(
        evidence_failure(&[], &[1]).is_some(),
        "no baseline counts passed"
    );
    assert!(
        evidence_failure(&[1], &[]).is_some(),
        "no candidate counts passed"
    );
    assert_eq!(evidence_failure(&[2, 1, 3], &[1, 1, 1]), None);
}

#[test]
fn a_missing_findings_field_is_rejected_not_counted_as_zero() {
    let healthy: serde_json::Value =
        serde_json::from_str("{\"findings\":[{\"id\":1}]}").unwrap_or_default();
    assert_eq!(super::findings_count(&healthy, "side"), Ok(1));
    let missing: serde_json::Value = serde_json::from_str("{\"other\":[]}").unwrap_or_default();
    assert_eq!(
        super::findings_count(&missing, "side"),
        Err("side report has no findings array".to_string())
    );
    let wrong_shape: serde_json::Value =
        serde_json::from_str("{\"findings\":3}").unwrap_or_default();
    assert_eq!(
        super::findings_count(&wrong_shape, "side"),
        Err("side report has no findings array".to_string())
    );
}

#[test]
fn pins_parse_into_ids_and_commits_and_reject_unsafe_text() {
    use super::nightly::parse_pins;
    assert_eq!(
        parse_pins("xtask-tests=a58975de,product_rust=af8136f7"),
        Ok(vec![
            ("xtask-tests".to_string(), "a58975de".to_string()),
            ("product_rust".to_string(), "af8136f7".to_string())
        ])
    );
    for bad in [
        "",
        "noequals",
        "id=",
        "=abc",
        "id=--upload-pack=x",
        "id=abc;rm",
        "a/b=abc",
        "dup=abc,dup=def",
    ] {
        assert!(parse_pins(bad).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn external_pins_resolve_against_the_scoreboard_corpus_and_never_skip_silently() {
    use super::nightly::{parse_external_ids, resolve_external};
    use crate::reports::dx_scoreboard::parse_config;

    assert_eq!(
        parse_external_ids("rust-analyzer,tokio,servo"),
        Ok(vec![
            "rust-analyzer".to_string(),
            "tokio".to_string(),
            "servo".to_string()
        ])
    );
    for bad in ["", ",", "a/b", "id;rm", "tokio,tokio", "--flag x"] {
        assert!(
            parse_external_ids(bad).is_err(),
            "{bad:?} should be rejected"
        );
    }

    let config = parse_config(
        "schema_version = \"ripr-dx-scoreboards-v1\"\n\
         [[corpus]]\nid = \"tokio\"\nurl = \"https://github.com/tokio-rs/tokio\"\n\
         sha = \"6470b5e4b8e943ad014383a5bf6a93ba415678a3\"\n",
    );
    let Ok(config) = config else {
        assert_eq!(config.err(), None, "the fixture config must parse");
        return;
    };
    let resolved = resolve_external(&["tokio".to_string()], &config.corpus);
    assert!(resolved.is_ok_and(|pins| pins.len() == 1));
    // Negative control: an id the corpus does not declare is an error. A gate
    // that timed nothing would pass for the wrong reason.
    let missing = resolve_external(&["servo".to_string()], &config.corpus);
    assert!(missing.is_err_and(|error| error.contains("not in the scoreboard corpus")));
}

#[test]
fn external_pins_need_a_nightly_plan_and_cannot_mix_with_direct_arguments() {
    use super::parse_args;
    let args = |text: &str| -> Vec<String> { text.split(' ').map(str::to_string).collect() };
    assert!(
        parse_args(&args("--external-pins tokio,servo"))
            .is_ok_and(|options| options.nightly.is_some())
    );
    let mixed = parse_args(&args("--external-pins tokio --baseline /bin/true"));
    assert!(mixed.is_err_and(|error| error.contains("do not combine")));
    let bad = parse_args(&args("--external-pins a/b"));
    assert!(bad.is_err_and(|error| error.contains("external pin")));
    // Both lists write `warm-ab-<id>.*`, so a shared id would overwrite evidence.
    let shared = parse_args(&args("--pins tokio=a58975de --external-pins tokio"));
    assert!(shared.is_err_and(|error| error.contains("both --pins and --external-pins")));
}

#[cfg(unix)]
#[test]
fn a_stalled_remote_times_out_at_its_deadline() {
    use super::nightly::git_within;
    use std::time::{Duration, Instant};
    let dir = std::env::temp_dir().join(format!("ripr-warm-ab-stall-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(std::fs::create_dir_all(&dir).is_ok_and(|()| true));
    // The `ext` transport runs a command as the remote; `sleep` never speaks
    // the protocol, so the clone stalls until the deadline kills it.
    let started = Instant::now();
    let stalled = git_within(
        &[
            "-c",
            "protocol.ext.allow=always",
            "clone",
            "--quiet",
            "ext::sleep 60",
            "stalled",
        ],
        &dir,
        Duration::from_secs(2),
    );
    assert!(
        stalled.is_err_and(|error| error.contains("timed out")),
        "a stalled clone must fail as a timeout"
    );
    assert!(started.elapsed() < Duration::from_secs(30));
    // The primitive is not wedged by the kill. That the nightly loop moves on
    // to the next pin is the per-pin handler in `run_in`, which needs two
    // release builds and is not covered here.
    assert_eq!(
        git_within(&["--version"], &dir, Duration::from_secs(30)).map(|v| v.starts_with("git")),
        Ok(true)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
mod stub {
    use super::super::{build_packet, parse_args};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    type TestResult = Result<(), String>;

    fn stub(dir: &Path, name: &str, script: &str) -> Result<PathBuf, String> {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).map_err(|error| error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    const HEALTHY: &str = "printf '%s' '{\"findings\":[{\"id\":1}]}'\nexit 0";
    const EMPTY: &str = "printf '%s' '{\"findings\":[]}'\nexit 0";
    const MISSING: &str = "printf '%s' '{\"other\":[]}'\nexit 0";

    fn packet_for(
        name: &str,
        base_script: &str,
        cand_script: &str,
    ) -> Result<super::super::Packet, String> {
        let dir = std::env::temp_dir().join(format!("ripr-warm-ab-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let baseline = stub(&dir, "ripr-baseline", base_script)?;
        let candidate = stub(&dir, "ripr-candidate", cand_script)?;
        let tree = dir.join("tree");
        fs::create_dir_all(&tree).map_err(|error| error.to_string())?;
        let diff = dir.join("pin.diff");
        fs::write(&diff, "diff\n").map_err(|error| error.to_string())?;
        let args: Vec<String> = [
            "--baseline",
            &baseline.to_string_lossy(),
            "--candidate",
            &candidate.to_string_lossy(),
            "--tree",
            &tree.to_string_lossy(),
            "--diff",
            &diff.to_string_lossy(),
            "--pin",
            "stubpin",
            "--runs",
            "2",
            "--work",
            &dir.join("work").to_string_lossy(),
            "--out",
            &dir.join("out").to_string_lossy(),
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let options = parse_args(&args)?;
        let packet = build_packet(&options);
        let _ = fs::remove_dir_all(&dir);
        packet
    }

    #[test]
    fn a_fast_empty_candidate_cannot_pass_against_a_nonempty_baseline() -> TestResult {
        let packet = packet_for("cand-empty", HEALTHY, EMPTY)?;
        assert!(!packet.passed, "1/0 candidate passed: {}", packet.detail);
        Ok(())
    }

    #[test]
    fn a_fast_empty_baseline_cannot_pass_against_a_nonempty_candidate() -> TestResult {
        let packet = packet_for("base-empty", EMPTY, HEALTHY)?;
        assert!(!packet.passed, "0/1 baseline passed: {}", packet.detail);
        Ok(())
    }

    #[test]
    fn both_empty_reports_fail_and_healthy_reports_pass() -> TestResult {
        let both = packet_for("both-empty", EMPTY, EMPTY)?;
        assert!(!both.passed, "0/0 passed: {}", both.detail);
        let healthy = packet_for("healthy", HEALTHY, HEALTHY)?;
        assert!(healthy.passed, "healthy pair failed: {}", healthy.detail);
        Ok(())
    }

    #[test]
    fn a_candidate_report_missing_findings_is_rejected() {
        let error = packet_for("cand-missing", HEALTHY, MISSING)
            .err()
            .unwrap_or_default();
        assert!(
            error.contains("findings"),
            "missing candidate findings must be rejected naming findings: {error:?}"
        );
    }
}
