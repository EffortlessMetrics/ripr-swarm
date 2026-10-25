use super::{run_command_with_env, unique_temp_workspace};
use serde_json::{Value, json};
use std::path::Path;

fn report(root: &Path, name: &str, input: &Value) -> Result<(Value, String), String> {
    let input_path = root.join(format!("{name}-check.json"));
    let output_path = root.join(format!("{name}-report.json"));
    let markdown_path = output_path.with_extension("md");
    std::fs::write(
        &input_path,
        serde_json::to_vec(input).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let output = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        root,
        &[
            "reports",
            "ts-limitations",
            "--check-output",
            &input_path.to_string_lossy(),
            "--out",
            &output_path.to_string_lossy(),
            "--out-md",
            &markdown_path.to_string_lossy(),
        ],
        &[],
    )
    .map_err(|error| error.to_string())?;
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report =
        serde_json::from_slice(&std::fs::read(output_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let markdown = std::fs::read_to_string(markdown_path).map_err(|error| error.to_string())?;
    Ok((report, markdown))
}

fn assert_counts_and_disclosure(
    name: &str,
    report: &Value,
    markdown: &str,
    findings: usize,
    limits: usize,
    bounded: bool,
) -> Result<(), String> {
    assert_eq!(report["status"], "advisory", "{name}");
    assert_eq!(report["summary"]["findings_total"], findings, "{name}");
    assert_eq!(
        report["summary"]["typescript_family_findings_total"], findings,
        "{name}"
    );
    assert_eq!(report["summary"]["limitations_total"], limits, "{name}");
    let warnings = report["warnings"].as_array().ok_or("missing warnings")?;
    assert_eq!(warnings.len(), usize::from(bounded), "{name}: {warnings:?}");
    assert_eq!(
        markdown.contains("RIPR_CHECK_FINDINGS_BYTES=0"),
        bounded,
        "{name}"
    );
    if bounded {
        let warning = warnings[0].as_str().ok_or("warning must be text")?;
        assert!(
            warning.contains(&format!(
                "Counts cover only the {findings} supplied findings"
            )),
            "{name}: {warning}"
        );
        assert!(
            warning.contains("Omitted findings may change limitation counts and rankings"),
            "{name}: {warning}"
        );
        assert!(warning.contains("incomplete"), "{name}: {warning}");
        assert!(
            markdown.contains(warning),
            "{name}: Markdown must retain the warning"
        );
    }
    println!("REPORT {name}: supplied={findings} limitation_signals={limits} incomplete={bounded}");
    Ok(())
}

/// RIPR-SPEC-0085: byte-budget disclosure survives the producer-to-report
/// boundary; a budget crossed by the final record does not imply omission.
#[cfg(feature = "lang-typescript")]
#[test]
fn ts_limitations_preserves_zero_and_utf8_byte_boundary_contract() -> Result<(), String> {
    let root = unique_temp_workspace("ts-limitations-byte-boundary");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/typescript_static_limit_taxonomy");
        let fixture = root.join("fixture");
        for relative in [
            "ripr.toml",
            "src/decorated.tsx",
            "src/dispatch.ts",
            "src/proxy.js",
            "src/label.jsx",
            "tests/decorated.test.tsx",
            "tests/dispatch.test.ts",
            "tests/proxy.test.js",
            "tests/label.test.jsx",
        ] {
            let destination = fixture.join(relative.replace("src/decorated", "src/a_écran測🙂"));
            std::fs::create_dir_all(destination.parent().ok_or("fixture parent missing")?)
                .map_err(|error| error.to_string())?;
            let text = std::fs::read_to_string(source.join("input").join(relative))
                .map_err(|error| error.to_string())?;
            std::fs::write(
                destination,
                text.replace("src/decorated", "src/a_écran測🙂"),
            )
            .map_err(|error| error.to_string())?;
        }
        let diff = root.join("change.diff");
        std::fs::write(
            &diff,
            std::fs::read_to_string(source.join("diff.patch"))
                .map_err(|error| error.to_string())?
                .replace("src/decorated", "src/a_écran測🙂"),
        )
        .map_err(|error| error.to_string())?;
        let check = |budget: &str| -> Result<String, String> {
            let output = run_command_with_env(
                env!("CARGO_BIN_EXE_ripr"),
                &root,
                &[
                    "check",
                    "--root",
                    &fixture.to_string_lossy(),
                    "--diff",
                    &diff.to_string_lossy(),
                    "--format",
                    "json",
                ],
                &[("RIPR_CHECK_FINDINGS_BYTES", budget)],
            )
            .map_err(|error| error.to_string())?;
            assert!(
                output.status.success(),
                "check budget={budget}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).map_err(|error| error.to_string())
        };
        let full_raw = check("0")?;
        let full: Value = serde_json::from_str(&full_raw).map_err(|error| error.to_string())?;
        assert_eq!(full["findings"].as_array().map(Vec::len), Some(4));
        let marker = "  \"findings\": [\n";
        let start = full_raw.find(marker).ok_or("missing findings array")? + marker.len();
        let rest = &full_raw[start..];
        let first_end = rest
            .find("\n    },\n    {")
            .ok_or("missing record boundary")?
            + "\n    }".len();
        let first_raw = &rest[..first_end];
        let first: Value = serde_json::from_str(first_raw).map_err(|error| error.to_string())?;
        assert_eq!(
            first, full["findings"][0],
            "boundary must isolate a complete first finding"
        );
        assert!(first_raw.contains("a_écran測🙂"));
        assert!(
            first_raw.len() > first_raw.chars().count(),
            "fixture must distinguish UTF-8 bytes from characters"
        );
        let array_end = rest.find("\n  ]").ok_or("missing findings end")?;
        let all: Value = serde_json::from_str(&format!("[{}]", &rest[..array_end]))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            all, full["findings"],
            "array byte boundary must cover exactly four records"
        );
        let cases = [
            ("zero-opt-out", 0, 4, 12, false),
            ("one-byte", 1, 1, 3, true),
            ("first-over-boundary", first_end - 1, 1, 3, true),
            ("first-exact-boundary", first_end, 2, 6, true),
            ("first-under-boundary", first_end + 1, 2, 6, true),
            (
                "utf8-character-decoy",
                first_raw.chars().count(),
                1,
                3,
                true,
            ),
            ("all-exact-boundary", array_end, 4, 12, false),
            ("all-over-boundary", array_end - 1, 4, 12, false),
        ];
        for (name, budget, rendered, limits, bounded) in cases {
            let raw = if budget == 0 {
                full_raw.clone()
            } else {
                check(&budget.to_string())?
            };
            let input: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
            assert_eq!(
                input["findings"].as_array().map(Vec::len),
                Some(rendered),
                "{name}"
            );
            assert_eq!(
                input["summary"]["findings"], 4,
                "{name}: producer denominator remains full"
            );
            if bounded {
                assert_eq!(
                    input["analysis_outcome"]["document_complete"], false,
                    "{name}"
                );
                assert_eq!(
                    input["analysis_outcome"]["findings_rendered"], rendered,
                    "{name}"
                );
                assert_eq!(input["analysis_outcome"]["findings_total"], 4, "{name}");
                assert_eq!(
                    input["run_limitations"][0]["downstream_consumable"], false,
                    "{name}"
                );
                assert!(
                    input["run_limitations"][0]["message"].as_str().is_some_and(
                        |message| message.contains(&format!("{rendered} of 4 findings"))
                    ),
                    "{name}"
                );
            } else {
                assert!(input.get("run_limitations").is_none(), "{name}");
                assert!(
                    input["analysis_outcome"].get("document_complete").is_none(),
                    "{name}"
                );
            }
            assert_eq!(
                input["findings"][0], full["findings"][0],
                "{name}: a complete UTF-8 record survives"
            );
            let (report, markdown) = report(&root, name, &input)?;
            assert_counts_and_disclosure(name, &report, &markdown, rendered, limits, bounded)?;
            println!(
                "BOUNDARY {name}: budget={budget} first_bytes={first_end} first_chars={} rendered={rendered}/4",
                first_raw.chars().count()
            );
        }
        // Exceed the documented default cap so `0` mapped to the default
        // budget cannot pass merely because the first fixture was small.
        // This is a bounded output fixture, not a repository-scale scan.
        let template = std::fs::read_to_string(source.join("input/src/decorated.tsx"))
            .map_err(|error| error.to_string())?;
        let template_diff = std::fs::read_to_string(source.join("diff.patch"))
            .map_err(|error| error.to_string())?;
        let block = template_diff
            .split("diff --git ")
            .find(|block| block.starts_with("a/src/decorated.tsx "))
            .ok_or("missing decorated diff block")?;
        let mut large_diff = std::fs::read_to_string(&diff).map_err(|error| error.to_string())?;
        // Stay below the independent 200-file analysis partition budget.
        for index in 0..180 {
            let path = format!("src/zero-cap-{index:03}.tsx");
            std::fs::write(fixture.join(&path), &template).map_err(|error| error.to_string())?;
            large_diff.push_str("diff --git ");
            large_diff.push_str(&block.replace("src/decorated.tsx", &path));
        }
        std::fs::write(&diff, large_diff).map_err(|error| error.to_string())?;
        let large_raw = check("0")?;
        let large: Value = serde_json::from_str(&large_raw).map_err(|error| error.to_string())?;
        assert_eq!(
            large["findings"].as_array().map(Vec::len),
            Some(184),
            "zero must remove the default cap"
        );
        let start = large_raw
            .find(marker)
            .ok_or("missing large findings array")?
            + marker.len();
        let payload_bytes = large_raw[start..]
            .find("\n  ]")
            .ok_or("missing large findings end")?;
        assert!(
            payload_bytes > 1_000_000,
            "zero control must exceed the documented default byte budget"
        );
        let payload = &large_raw[start..start + payload_bytes];
        let penultimate_bytes = payload
            .rfind("\n    },\n    {")
            .ok_or("missing penultimate record boundary")?
            + "\n    }".len();
        assert!(
            penultimate_bytes > 1_000_000,
            "the default cap must be crossed before the last record, otherwise record-atomic overflow could retain the full set"
        );
        assert_eq!(
            large["analysis_outcome"]["analysis_complete"], true,
            "output-only control must not cross the separate analysis budget"
        );
        assert!(large.get("run_limitations").is_none());
        let (report, markdown) = report(&root, "zero-removes-default-cap", &large)?;
        assert_counts_and_disclosure(
            "zero-removes-default-cap",
            &report,
            &markdown,
            184,
            552,
            false,
        )?;
        println!(
            "ZERO above-default: payload_bytes={payload_bytes} penultimate_bytes={penultimate_bytes} rendered=184/184"
        );
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(root).map_err(|error| error.to_string());
    result.and(cleanup)
}

/// RIPR-SPEC-0085: the example cap is distinct from input truncation. Counts
/// and sources reconcile across records, and a bounded zero is not empty proof.
#[test]
fn ts_limitations_distinguishes_sample_cap_unicode_records_and_empty_input() -> Result<(), String> {
    let root = unique_temp_workspace("ts-limitations-record-boundary");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let records = (0..4)
            .map(|index| {
                let mut record = json!({
                    "id": format!("finding:é測🙂:{index}"), "language": "typescript",
                    "probe": {"file": format!("src/é測🙂-{index}.ts"), "line": index + 1},
                    "evidence": []
                });
                match index {
                    1 => {
                        record["evidence"] = json!([
                            "typescript_package_limitation: typescript_import_graph_unresolved"
                        ])
                    }
                    2 => record["static_limit_kind"] = json!("missing_import_graph"),
                    _ => {
                        record["evidence"] = json!([
                            "typescript_limitation: typescript_import_graph_unresolved",
                            "typescript_limitation: typescript_import_graph_unresolved"
                        ])
                    }
                }
                record
            })
            .collect::<Vec<_>>();
        for count in [2usize, 3, 4] {
            let name = format!("sample-cap-{count}");
            let input = json!({"findings": &records[..count]});
            let (report, markdown) = report(&root, &name, &input)?;
            assert_counts_and_disclosure(&name, &report, &markdown, count, count, false)?;
            assert_eq!(report["summary"]["distinct_limitations_total"], 1);
            let entry = &report["limitations"][0];
            assert_eq!(
                entry["count"], count,
                "one count per finding despite duplicate evidence"
            );
            assert_eq!(
                entry["sources"],
                if count == 2 {
                    json!(["typescript_limitation", "typescript_package_limitation"])
                } else {
                    json!([
                        "static_limit_kind",
                        "typescript_limitation",
                        "typescript_package_limitation"
                    ])
                },
                "sources must be unioned across distinct records, not replaced by the latest"
            );
            let samples = entry["samples"].as_array().ok_or("missing samples")?;
            assert_eq!(
                samples.len(),
                count.min(3),
                "retain every example below the cap and exactly three beyond it"
            );
            for (sample, source) in samples.iter().zip(&records) {
                assert_eq!(sample["finding_id"], source["id"]);
                assert_eq!(sample["file"], source["probe"]["file"]);
            }
            assert!(markdown.contains("src/é測🙂-0.ts:1 (finding:é測🙂:0)"));
        }
        let prefix = json!({
            "findings": [records[0]], "summary": {"findings": 4},
            "analysis_outcome": {"document_complete": false, "findings_rendered": 1, "findings_total": 4}
        });
        let (bounded_report, markdown) = report(&root, "input-prefix", &prefix)?;
        assert_counts_and_disclosure("input-prefix", &bounded_report, &markdown, 1, 1, true)?;
        for bounded in [false, true] {
            let name = if bounded {
                "bounded-zero"
            } else {
                "complete-empty"
            };
            let input = json!({
                "findings": [],
                "analysis_outcome": {"document_complete": !bounded, "findings_rendered": 0, "findings_total": if bounded { 4 } else { 0 }}
            });
            let (report, markdown) = report(&root, name, &input)?;
            assert_counts_and_disclosure(name, &report, &markdown, 0, 0, bounded)?;
            assert_eq!(report["limitations"], json!([]));
            assert!(markdown.contains(
                "No TypeScript-family limitation signals were found in the supplied check output"
            ));
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(root).map_err(|error| error.to_string());
    result.and(cleanup)
}
