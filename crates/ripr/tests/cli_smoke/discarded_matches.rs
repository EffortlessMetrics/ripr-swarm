use super::{
    Digest, Sha256, run_matcher_calibration_with_deadline, unique_temp_workspace, workspace_root,
};

/// The independent contract is score(1) == 2. Original/wrong sources differ
/// only in the returned offset; their patches each describe the actual source.
/// Discarding a matcher accepts either value, while exact/guarded assertions
/// distinguish them. This exercises the current binary, not constructed facts.
#[test]
fn discarded_matcher_cli_controls_reject_false_credit_and_retain_consumers() -> Result<(), String> {
    let root = unique_temp_workspace("discarded-matcher-cli-5713");
    let retained = workspace_root()
        .join("target/ripr/reports/discarded-matcher-cli-5713")
        .join(
            root.file_name()
                .ok_or("missing control invocation identity")?,
        );
    let result = (|| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let run = |args: &[&str]| {
            let budget = deadline
                .saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_secs(10));
            if budget.is_zero() {
                return Err(
                    "current CLI control batch exceeded its 120s instrument budget".to_string(),
                );
            }
            run_matcher_calibration_with_deadline(args, budget).map_err(|error| error.to_string())
        };
        let mut binary =
            std::fs::File::open(env!("CARGO_BIN_EXE_ripr")).map_err(|error| error.to_string())?;
        let mut binary_digest = Sha256::new();
        let mut buffer = [0_u8; 65_536];
        loop {
            let count =
                std::io::Read::read(&mut binary, &mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            binary_digest.update(&buffer[..count]);
        }
        let binary_sha256 = format!("{:x}", binary_digest.finalize());
        std::fs::create_dir_all(&retained).map_err(|error| error.to_string())?;
        let cases = [
            ("bare-wildcard", "matches!(value, _);", "discarded"),
            ("bare-exact", "matches!(value, 2);", "discarded"),
            ("bare-guarded", "matches!(value, v if v == 2);", "discarded"),
            (
                "bound-exact",
                "let matched = matches!(value, 2);",
                "discarded",
            ),
            ("wrapped-wildcard", "assert!(matches!(value, _));", "weak"),
            ("wrapped-exact", "assert!(matches!(value, 2));", "strong"),
            (
                "wrapped-guarded",
                "assert!(matches!(value, v if v == 2));",
                "strong",
            ),
        ];
        let mut records = Vec::new();
        for (name, oracle, expected) in cases {
            for (variant, offset) in [("original", 1), ("wrong", 2)] {
                let id = format!("{name}-{variant}");
                let fixture = root.join(&id);
                std::fs::create_dir_all(fixture.join("src")).map_err(|error| error.to_string())?;
                std::fs::write(
                    fixture.join("Cargo.toml"),
                    "[package]\nname = \"discarded_matcher_control\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
                )
                .map_err(|error| error.to_string())?;
                let source = format!(
                    "pub fn score(value: i32) -> i32 {{\n    value + {offset}\n}}\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn observes_score() {{\n        let value = super::score(1);\n        {oracle}\n    }}\n}}\n"
                );
                let diff = format!(
                    "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn score(value: i32) -> i32 {{\n-    value + {}\n+    value + {offset}\n }}\n",
                    offset - 1
                );
                std::fs::write(fixture.join("src/lib.rs"), &source)
                    .map_err(|error| error.to_string())?;
                let patch = fixture.join("change.patch");
                std::fs::write(&patch, &diff).map_err(|error| error.to_string())?;
                let root_arg = fixture.to_string_lossy();
                let patch_arg = patch.to_string_lossy();
                let json = run(&[
                    "check", "--root", &root_arg, "--diff", &patch_arg, "--mode", "fast",
                    "--format", "json",
                ])?;
                let human = run(&[
                    "check", "--root", &root_arg, "--diff", &patch_arg, "--mode", "fast",
                    "--format", "human",
                ])?;
                let human_full = run(&[
                    "check",
                    "--root",
                    &root_arg,
                    "--diff",
                    &patch_arg,
                    "--mode",
                    "fast",
                    "--format",
                    "human-full",
                ])?;
                let output = retained.join(&id);
                std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                assert!(
                    json.stdout.len()
                        + json.stderr.len()
                        + human.stdout.len()
                        + human.stderr.len()
                        + human_full.stdout.len()
                        + human_full.stderr.len()
                        <= 262_144,
                    "{id}: the bounded control must retain at most 256KiB of CLI output"
                );
                for (file, bytes) in [
                    ("input.rs", source.as_bytes()),
                    ("diff.patch", diff.as_bytes()),
                    ("check.json", json.stdout.as_slice()),
                    ("human.txt", human.stdout.as_slice()),
                    ("human-full.txt", human_full.stdout.as_slice()),
                    ("json.stderr", json.stderr.as_slice()),
                    ("human.stderr", human.stderr.as_slice()),
                    ("human-full.stderr", human_full.stderr.as_slice()),
                ] {
                    std::fs::write(output.join(file), bytes).map_err(|error| error.to_string())?;
                }
                assert!(
                    json.status.success(),
                    "{id} JSON: {}",
                    String::from_utf8_lossy(&json.stderr)
                );
                assert!(
                    human.status.success(),
                    "{id} human: {}",
                    String::from_utf8_lossy(&human.stderr)
                );
                assert!(
                    human_full.status.success(),
                    "{id} full human: {}",
                    String::from_utf8_lossy(&human_full.stderr)
                );
                let report: serde_json::Value = serde_json::from_slice(&json.stdout)
                    .map_err(|error| format!("{id}: {error}"))?;
                let findings = report["findings"].as_array().ok_or("missing findings")?;
                assert_eq!(
                    findings.len(),
                    1,
                    "{id}: changed return seam must remain nonempty"
                );
                let finding = &findings[0];
                assert_eq!(finding["probe"]["family"], "return_value", "{id}");
                let kind = finding["oracle_kind"]
                    .as_str()
                    .ok_or("missing oracle kind")?;
                let strength = finding["oracle_strength"]
                    .as_str()
                    .ok_or("missing oracle strength")?;
                let related = finding["related_tests"]
                    .as_array()
                    .ok_or("missing related tests")?;
                assert_eq!(
                    related.len(),
                    1,
                    "{id}: intended consumer must remain reachable"
                );
                assert_eq!(related[0]["name"], "observes_score", "{id}");
                assert_eq!(related[0]["file"], "src/lib.rs", "{id}");
                assert_eq!(related[0]["line"], 8, "{id}");
                assert_eq!(related[0]["relation_reason"], "direct_owner_call", "{id}");
                assert_eq!(
                    related[0]["oracle_kind"], finding["oracle_kind"],
                    "{id}: kind projection"
                );
                assert_eq!(
                    related[0]["oracle_strength"], finding["oracle_strength"],
                    "{id}: strength projection"
                );
                let human_text =
                    std::str::from_utf8(&human.stdout).map_err(|error| error.to_string())?;
                let human_full_text =
                    std::str::from_utf8(&human_full.stdout).map_err(|error| error.to_string())?;
                assert!(
                    human_text.lines().any(|line| line
                        .trim()
                        .starts_with("Related test: src/lib.rs:8 observes_score")),
                    "{id}: concise output must retain the intended consumer"
                );
                let exposure = human_text
                    .lines()
                    .filter(|line| line.trim().starts_with("Static exposure:"))
                    .collect::<Vec<_>>();
                assert_eq!(
                    exposure.len(),
                    1,
                    "{id}: concise classification must be nonempty"
                );
                assert!(
                    exposure[0].contains(
                        finding["classification"]
                            .as_str()
                            .ok_or("missing classification")?
                    ),
                    "{id}: human/JSON classification agreement"
                );
                let projection = "- related test src/lib.rs:8 observes_score uses ";
                match expected {
                    "discarded" => {
                        assert_ne!(
                            finding["classification"], "exposed",
                            "{id}: computation cannot discriminate"
                        );
                        assert_ne!(
                            kind, "exact_value",
                            "{id}: discarded pattern cannot provide an oracle"
                        );
                        assert_ne!(strength, "strong", "{id}: unused boolean cannot fail");
                        for test in related {
                            assert_ne!(
                                test["oracle_strength"], "strong",
                                "{id}: related-test projection must agree"
                            );
                            assert_ne!(
                                test["oracle_kind"], "exact_value",
                                "{id}: related-test pattern must not leak"
                            );
                        }
                        for line in human_full_text.lines().map(str::trim) {
                            if line.starts_with(projection) {
                                assert!(
                                    !line.contains("uses strong "),
                                    "{id}: false strong human projection"
                                );
                                assert!(
                                    !line.contains(" exact value oracle:"),
                                    "{id}: discarded pattern in human projection"
                                );
                            }
                            assert!(
                                !line.starts_with("- discriminator yes: Strong oracle found:"),
                                "{id}: false strong human explanation"
                            );
                        }
                    }
                    "weak" => {
                        assert_eq!(finding["classification"], "weakly_exposed", "{id}");
                        assert_eq!(kind, "relational_check", "{id}");
                        assert_eq!(strength, "weak", "{id}");
                        assert!(
                            human_full_text
                                .lines()
                                .any(|line| line.trim().starts_with(&format!(
                                    "{projection}weak relational check oracle: {oracle}"
                                ))),
                            "{id}: full weak oracle projection"
                        );
                    }
                    "strong" => {
                        assert_eq!(finding["classification"], "exposed", "{id}");
                        assert_eq!(kind, "exact_value", "{id}");
                        assert_eq!(strength, "strong", "{id}");
                        assert!(
                            human_full_text
                                .lines()
                                .any(|line| line.trim().starts_with(&format!(
                                    "{projection}strong exact value oracle: {oracle}"
                                ))),
                            "{id}: full strong oracle projection"
                        );
                    }
                    _ => return Err(format!("unsupported expectation {expected}")),
                }
                records.push(serde_json::json!({
                    "id": id,
                    "expected": expected,
                    "source_sha256": format!("{:x}", Sha256::digest(source.as_bytes())),
                    "diff_sha256": format!("{:x}", Sha256::digest(diff.as_bytes())),
                    "json_sha256": format!("{:x}", Sha256::digest(&json.stdout)),
                    "human_sha256": format!("{:x}", Sha256::digest(&human.stdout)),
                    "human_full_sha256": format!("{:x}", Sha256::digest(&human_full.stdout)),
                    "classification": finding["classification"],
                    "oracle_kind": kind,
                    "oracle_strength": strength,
                }));
            }
        }
        assert_eq!(
            records.len(),
            14,
            "the authored control denominator is explicit"
        );
        let receipt = serde_json::json!({
            "kind": "discarded_matcher_current_cli_controls",
            "binary": env!("CARGO_BIN_EXE_ripr"),
            "binary_sha256": binary_sha256,
            "run_id": std::env::var("GITHUB_RUN_ID").ok(),
            "run_attempt": std::env::var("GITHUB_RUN_ATTEMPT").ok(),
            "command": "ripr check --root CASE --diff PATCH --mode fast --format json/human/human-full",
            "command_timeout_seconds": 10,
            "batch_deadline_seconds": 120,
            "post_capture_retained_output_limit_bytes_per_subject": 262144,
            "independent_contract": "score(1) == 2; original returns2, wrong returns3",
            "denominator": "14 authored static CLI subjects; not representative accuracy or fixture runtime execution",
            "records": records,
        });
        std::fs::write(
            retained.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}
