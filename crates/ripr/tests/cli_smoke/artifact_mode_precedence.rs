use super::*;

/// #7873: artifact navigation must preserve CLI precedence even when the
/// explicit winner equals the built-in default. Replaying the printed argv
/// reloads repository policy, so losing Draft turns it back into Ready.
#[test]
fn context_artifact_navigation_preserves_mode_precedence() -> Result<(), String> {
    let fixture = SampleDiffFixture::new("artifact-mode-precedence")?;
    let nested = fixture.0.join("nested");
    std::fs::create_dir(&nested).map_err(|error| format!("create nested cwd: {error}"))?;
    std::fs::write(nested.join("ripr.toml"), "[analysis]\nmode = \"instant\"\n")
        .map_err(|error| format!("write unrelated nested config: {error}"))?;
    let cache = fixture.0.join("relocated-cache").display().to_string();
    let env = [
        ("RIPR_CACHE_DIR", cache.as_str()),
        ("RIPR_GIT_TIMEOUT", "0"),
    ];
    let run = |args: &[&str]| {
        run_command_with_env(env!("CARGO_BIN_EXE_ripr"), &nested, args, &env)
            .map_err(|error| format!("run ripr {args:?}: {error}"))
    };

    // Literal expectations come from CLI > selected-root config > default,
    // independently of the production navigation/config helpers.
    for (repo_mode, cli_mode, winner, printed_mode, exclude_unchanged) in [
        (Some("ready"), Some("draft"), "draft", Some("draft"), true),
        (Some("ready"), Some("deep"), "deep", Some("deep"), false),
        (Some("ready"), None, "ready", Some("ready"), false),
        (Some("draft"), None, "draft", None, false),
        (None, None, "draft", None, false),
    ] {
        if let Some(repo_mode) = repo_mode {
            std::fs::write(
                fixture.0.join("ripr.toml"),
                format!("[analysis]\nmode = \"{repo_mode}\"\n[reports]\nmax_related_tests = 1\n"),
            )
            .map_err(|error| format!("write selected-root config: {error}"))?;
        } else {
            std::fs::remove_file(fixture.0.join("ripr.toml"))
                .map_err(|error| format!("remove selected-root config: {error}"))?;
        }
        let mut overrides = Vec::new();
        if let Some(mode) = cli_mode {
            overrides.extend(["--mode", mode]);
        }
        if exclude_unchanged {
            overrides.push("--no-unchanged-tests");
        }
        let mut check_args = vec![
            "check",
            "--root",
            "..",
            "--diff",
            "../example.diff",
            "--json",
            "--write-artifact",
            "../accepted.json",
        ];
        check_args.extend_from_slice(&overrides);
        let check = run(&check_args)?;
        assert_success(&check);
        let artifact: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture.0.join("accepted.json"))
                .map_err(|error| format!("read artifact: {error}"))?,
        )
        .map_err(|error| format!("parse artifact: {error}"))?;
        if artifact["identity"]["mode"] != winner
            || artifact["identity"]["analysis_options"]["include_unchanged_tests"]
                != !exclude_unchanged
            || !artifact["findings"].as_array().is_some_and(|findings| {
                findings
                    .iter()
                    .any(|finding| finding["id"] == SAMPLE_DIFF_SELECTOR)
            })
        {
            return Err(format!(
                "artifact must contain selected finding and winner {winner}: {artifact}"
            ));
        }

        let mut context_args = vec![
            "context",
            "--root",
            "..",
            "--from",
            "../accepted.json",
            "--at",
            SAMPLE_DIFF_SELECTOR,
            "--max-related-tests",
            "0",
        ];
        context_args.extend_from_slice(&overrides);
        if !exclude_unchanged {
            let mut default_cap_args = context_args.clone();
            default_cap_args.drain(7..9);
            let default_cap = run(&default_cap_args)?;
            assert_success(&default_cap);
            let default_packet: serde_json::Value = serde_json::from_slice(&default_cap.stdout)
                .map_err(|error| format!("parse default-cap packet: {error}"))?;
            if default_packet["related_tests"].as_array().map(Vec::len) != Some(1) {
                return Err(format!(
                    "cap control needs a real related test: {default_packet}"
                ));
            }
        }
        let context = run(&context_args)?;
        assert_success(&context);
        let packet: serde_json::Value = serde_json::from_slice(&context.stdout)
            .map_err(|error| format!("parse context packet: {error}"))?;
        if packet["related_tests"] != serde_json::json!([]) {
            return Err(format!(
                "explicit render cap must override repo cap: {packet}"
            ));
        }
        let command = packet
            .pointer("/witness/explain_command")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("context must provide explain command: {packet}"))?;
        let words = shell_words::printed_words(command)?;
        let actual_mode = words
            .windows(2)
            .find(|pair| pair[0] == "--mode")
            .map(|pair| pair[1].as_str());
        if actual_mode != printed_mode {
            return Err(format!(
                "expected literal mode {printed_mode:?}, got {actual_mode:?}: {command}"
            ));
        }
        if words.iter().any(|word| word == "--no-unchanged-tests") != exclude_unchanged
            || words.iter().any(|word| word == "--max-related-tests")
            || !path_matches_printed(posix_flag_value(&words, "--root")?, &fixture.0)
            || !path_matches_printed(
                posix_flag_value(&words, "--from")?,
                &fixture.0.join("accepted.json"),
            )
        {
            return Err(format!(
                "navigation lost selected root, artifact or unrelated options: {command}"
            ));
        }
        let replay = replay_printed_ripr(command, &nested)?;
        assert_success(&replay);
        if !String::from_utf8_lossy(&replay.stdout).contains(SAMPLE_DIFF_SELECTOR) {
            return Err("explain replay lost the selected finding".to_string());
        }

        // Removal control: dropping the override must still reject the
        // artifact rather than silently consuming it under repository Ready.
        if cli_mode == Some("draft") {
            let mut wrong_words = words.clone();
            let mode_index = wrong_words
                .iter()
                .position(|word| word == "--mode")
                .ok_or("explicit Draft must appear before removal control")?;
            wrong_words.drain(mode_index..mode_index + 2);
            let wrong_args: Vec<&str> = wrong_words.iter().skip(1).map(String::as_str).collect();
            let wrong = run(&wrong_args)?;
            if wrong.status.code() != Some(2)
                || !String::from_utf8_lossy(&wrong.stderr).contains("identity mismatch on mode")
            {
                return Err(format!(
                    "removing explicit Draft must fail on mode identity: {wrong:?}"
                ));
            }
        }
    }
    if !Path::new(&cache).is_dir() || fixture.0.join("target/ripr/cache").exists() {
        return Err("cache relocation must remain independent of mode precedence".to_string());
    }
    Ok(())
}

/// The compact context option changes presentation only for live and artifact
/// packets, structured failures and plain selector misses (#13356).
#[test]
fn context_compact_preserves_packets_and_refusals() -> Result<(), String> {
    let fixture = SampleDiffFixture::new("context-compact-雪")?;
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("example.diff").display().to_string();
    let artifact = fixture.0.join("compact-check.json").display().to_string();
    let check = run_ripr(&[
        "check", "--root", &root, "--diff", &diff, "--json", "--write-artifact", &artifact,
    ]);
    assert_success(&check);
    let check_json: serde_json::Value =
        serde_json::from_slice(&check.stdout).map_err(|error| error.to_string())?;
    assert!(check_json["findings"].as_array().is_some_and(|findings| {
        findings.iter().any(|finding| finding["id"] == SAMPLE_DIFF_SELECTOR)
    }));
    for source in [["--diff", diff.as_str()], ["--from", artifact.as_str()]] {
        let args = [
            "context", "--root", &root, source[0], source[1], "--at", SAMPLE_DIFF_SELECTOR,
        ];
        let pretty = run_ripr(&args);
        let mut explicit = args.to_vec();
        explicit.push("--json");
        let explicit = run_ripr(&explicit);
        let mut compact_args = args.to_vec();
        compact_args.extend(["--format", "json-compact"]);
        let compact = run_ripr(&compact_args);
        assert_success(&pretty);
        assert_success(&explicit);
        assert_success(&compact);
        assert!(String::from_utf8_lossy(&pretty.stdout).contains('雪'));
        assert_eq!(
            pretty.stdout, explicit.stdout,
            "default bytes must remain stable"
        );
        assert_context_compaction(&pretty.stdout, &compact.stdout)?;
    }

    let missing_root = fixture.0.join("missing-root").display().to_string();
    let args = ["context", "--root", &missing_root, "--at", SAMPLE_DIFF_SELECTOR];
    let pretty = run_ripr(&args);
    let mut compact_args = args.to_vec();
    compact_args.extend(["--format", "json-compact"]);
    let compact = run_ripr(&compact_args);
    assert_eq!(pretty.status.code(), Some(2));
    assert_eq!(compact.status.code(), pretty.status.code());
    assert_context_compaction(&pretty.stdout, &compact.stdout)?;

    for flags in [
        vec!["--format", "invalid"],
        vec!["--json", "--format", "json-compact"],
    ] {
        let mut args = vec!["context", "--root", &root, "--at", SAMPLE_DIFF_SELECTOR];
        args.extend(flags);
        let refused = run_ripr(&args);
        assert_eq!(refused.status.code(), Some(2));
        assert!(refused.stdout.is_empty());
    }

    let missing = fixture.0.join("missing-artifact.json").display().to_string();
    let args = [
        "context", "--root", &root, "--from", &missing, "--at", SAMPLE_DIFF_SELECTOR,
    ];
    let pretty = run_ripr(&args);
    let mut compact_args = args.to_vec();
    compact_args.extend(["--format", "json-compact"]);
    let compact = run_ripr(&compact_args);
    assert_eq!(pretty.status.code(), Some(2));
    assert_eq!(compact.status.code(), pretty.status.code());
    assert_context_compaction(&pretty.stdout, &compact.stdout)?;
    let refusal: serde_json::Value =
        serde_json::from_slice(&compact.stdout).map_err(|error| error.to_string())?;
    assert_eq!(refusal["run_limitations"][0]["category"], "check_artifact_unusable");

    let args = [
        "context", "--root", &root, "--diff", &diff, "--at", "probe:missing",
    ];
    let pretty = run_ripr(&args);
    let mut compact_args = args.to_vec();
    compact_args.extend(["--format", "json-compact"]);
    let compact = run_ripr(&compact_args);
    assert_eq!(pretty.status.code(), Some(2));
    assert_eq!(compact.status.code(), pretty.status.code());
    assert!(pretty.stdout.is_empty() && compact.stdout.is_empty());
    assert_eq!(pretty.stderr, compact.stderr);
    Ok(())
}

fn assert_context_compaction(pretty: &[u8], compact: &[u8]) -> Result<(), String> {
    let pretty_value: serde_json::Value =
        serde_json::from_slice(pretty).map_err(|error| error.to_string())?;
    let compact_value: serde_json::Value =
        serde_json::from_slice(compact).map_err(|error| error.to_string())?;
    assert_eq!(pretty_value, compact_value);
    assert!(compact.len() < pretty.len());
    // Outside strings remove only JSON whitespace. Preserve original member
    // order, escapes and numeric bytes, including UTF-8 inside strings.
    let mut expected = Vec::new();
    let (mut string, mut escaped) = (false, false);
    for &byte in pretty {
        if string {
            expected.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else if byte == b'"' {
            string = true;
            expected.push(byte);
        } else if !matches!(byte, b' ' | b'\n' | b'\r' | b'\t') {
            expected.push(byte);
        }
    }
    expected.push(b'\n');
    assert_eq!(compact, expected);
    Ok(())
}
