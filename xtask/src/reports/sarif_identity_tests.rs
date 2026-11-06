//! Producer-to-policy controls: these consume validated packets, real source
//! bytes and parsed diffs through the public analysis and rendering paths.

use super::*;
use ripr::{CheckInput, CheckOutput, OutputFormat};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new(case: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-sarif-identity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        let input = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/perl_packet_contract_migration/sarif_identity")
            .join(case);
        for (from, to) in [
            ("source.pm", "lib/App/Discount.pm"),
            ("packet.json", "packet.json"),
            ("diff.patch", "diff.patch"),
        ] {
            let destination = fixture.0.join(to);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::copy(input.join(from), destination).map_err(|error| error.to_string())?;
        }
        Ok(fixture)
    }

    fn check(&self, suppression: Option<&str>) -> Result<(CheckOutput, Value), String> {
        let mut config = ripr::config::RiprConfig::default();
        config.languages.enabled = vec![ripr::domain::LanguageId::Perl];
        let suppression_policy = if let Some(id) = suppression {
            let directory = self.0.join(".ripr");
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let path = directory.join("suppressions.toml");
            std::fs::write(
                &path,
                format!(
                    "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"{id}\"\nreason = \"reviewed return behavior\"\nowner = \"identity-control\"\n"
                ),
            )
            .map_err(|error| error.to_string())?;
            Some(path)
        } else {
            None
        };
        let output = ripr::app::check_workspace_with_config(
            CheckInput {
                root: self.0.clone(),
                diff_file: Some(self.0.join("diff.patch")),
                perl_facts_path: Some(self.0.join("packet.json")),
                suppression_policy,
                ..CheckInput::default()
            },
            &config,
        )?;
        assert!(
            !output.findings.is_empty(),
            "producer path must contain subjects"
        );
        assert!(output.findings.iter().all(|finding| {
            finding.source_currentness == ripr::domain::SourceCurrentness::CandidateCurrent
        }));
        let rendered = ripr::render_check(&output, &OutputFormat::Sarif)?;
        let sarif: Value = serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
        let results = results(&sarif)?;
        assert_eq!(results.len(), output.findings.len());
        for result in results {
            assert_eq!(result["fingerprints"], result["partialFingerprints"]);
        }
        Ok((output, sarif))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn results(sarif: &Value) -> Result<&[Value], String> {
    sarif
        .pointer("/runs/0/results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "expected rendered SARIF results".to_string())
}

fn policy_rows(sarif: &Value) -> Result<Vec<SarifPolicyResult>, String> {
    let text = serde_json::to_string(sarif).map_err(|error| error.to_string())?;
    parse_sarif_policy_results(&text, "producer-rendered SARIF")
}

fn compare(current: &[SarifPolicyResult], baseline: &[SarifPolicyResult]) -> SarifPolicyReport {
    build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Note,
        "current".to_string(),
        Some("baseline".to_string()),
        current,
        Some(baseline),
        false,
    )
}

#[test]
fn sarif_identity_preserves_perl_changes_and_supported_line_shifts() -> Result<(), String> {
    let (_, baseline) = Fixture::new("baseline_one")?.check(None)?;
    let (current_output, current) = Fixture::new("current_two")?.check(None)?;
    assert_eq!(current_output.findings.len(), 2);
    assert_eq!(
        current_output.findings[0].class,
        current_output.findings[1].class
    );
    let baseline = policy_rows(&baseline)?;
    let current = policy_rows(&current)?;
    assert_eq!(baseline.len(), 1);
    assert_eq!(
        current.len(),
        2,
        "separate changed behaviors must survive policy dedup"
    );
    assert!(current.iter().any(|row| row.key == baseline[0].key));
    let added = current
        .iter()
        .find(|row| row.line == Some(7))
        .ok_or("missing second change")?;
    let report = compare(&current, &baseline);
    assert_eq!(report.new_results.len(), 1);
    assert_eq!(report.new_results[0].key, added.key);
    assert_eq!(report.current_compared_results, 2);

    let keys = current.iter().map(|row| &row.key).collect::<BTreeSet<_>>();
    for case in ["shifted_owner", "shifted_body"] {
        let (_, moved) = Fixture::new(case)?.check(None)?;
        let moved = policy_rows(&moved)?;
        assert_eq!(
            moved.iter().map(|row| &row.key).collect::<BTreeSet<_>>(),
            keys
        );
        assert!(compare(&moved, &current).new_results.is_empty());
        assert_eq!(
            moved
                .iter()
                .filter_map(|row| row.line)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([7, 9]),
            "physical locations must move while identity stays fixed"
        );
    }
    Ok(())
}

#[test]
fn sarif_identity_keeps_native_suppression_exact_and_legacy_selector_stale() -> Result<(), String> {
    let fixture = Fixture::new("current_two")?;
    let (output, initial) = fixture.check(None)?;
    let selected = output
        .findings
        .iter()
        .find(|finding| finding.probe.location.line == 5)
        .ok_or("missing selected finding")?
        .id
        .clone();
    let retained_id = output
        .findings
        .iter()
        .find(|finding| finding.id != selected)
        .ok_or("separate finding must have another identity")?
        .id
        .clone();
    let initial = policy_rows(&initial)?;
    let retained = initial
        .iter()
        .find(|row| row.line == Some(7))
        .ok_or("missing retained change")?;
    let (_, suppressed) = fixture.check(Some(&selected))?;
    let suppressed_rows = policy_rows(&suppressed)?;
    assert_eq!(
        suppressed_rows.len(),
        1,
        "only the selected behavior is suppressed"
    );
    assert_eq!(suppressed_rows[0].key, retained.key);
    assert_eq!(
        results(&suppressed)?
            .iter()
            .filter(|row| row["suppressions"].is_array())
            .count(),
        1
    );
    assert!(results(&suppressed)?.iter().any(|row| {
        row["properties"]["finding_id"] == retained_id && !row["suppressions"].is_array()
    }));
    let (_, moved) = Fixture::new("shifted_owner")?.check(Some(&selected))?;
    let moved = policy_rows(&moved)?;
    assert_eq!(moved.len(), 1, "native selector follows supported movement");
    assert_eq!(moved[0].key, retained.key);

    let (legacy_output, legacy) =
        fixture.check(Some("probe:lib/App/Discount.pm:3:return_value:perl"))?;
    assert_eq!(
        policy_rows(&legacy)?.len(),
        2,
        "ambiguous legacy owner selector must not alias replacements"
    );
    assert!(
        legacy_output
            .suppression
            .as_ref()
            .ok_or("missing suppression outcome")?
            .warnings
            .iter()
            .any(|warning| warning.contains("probe:lib/App/Discount.pm:3:return_value:perl"))
    );
    Ok(())
}

#[test]
fn sarif_identity_legacy_finding_baselines_require_explicit_transition() -> Result<(), String> {
    let (_, current) = Fixture::new("current_two")?.check(None)?;
    let mut legacy = current.clone();
    let old_results = legacy
        .pointer_mut("/runs/0/results")
        .and_then(Value::as_array_mut)
        .ok_or("missing legacy result fixture")?;
    for result in old_results {
        let line = result
            .pointer("/locations/0/physicalLocation/region/startLine")
            .and_then(Value::as_u64)
            .ok_or("missing physical line")?;
        let rule = result["ruleId"].as_str().ok_or("missing rule")?;
        let old_id = "probe:lib/App/Discount.pm:3:return_value:perl";
        let old_key = format!("{rule}|{old_id}|{old_id}|lib/App/Discount.pm|{line}");
        let fingerprints = serde_json::json!({"riprFingerprintV1": old_key});
        result["partialFingerprints"] = fingerprints.clone();
        result["fingerprints"] = fingerprints;
    }
    let current = policy_rows(&current)?;
    let legacy = policy_rows(&legacy)?;
    assert_eq!(
        legacy.len(),
        2,
        "old location suffix distinguished these changes"
    );
    assert_eq!(
        compare(&current, &legacy).new_results.len(),
        2,
        "policy does not silently alias old fingerprints"
    );
    assert!(
        compare(&current, &current).new_results.is_empty(),
        "reviewed refreshed baseline compares normally"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn sarif_identity_native_paths_survive_policy_baselines_and_line_moves() -> Result<(), String> {
    // Start with a validated, current finding, then deliberately retain its
    // opaque identity at distinct native locations. This tests renderer and
    // policy contracts, not analyzer discovery or physical-line provenance.
    let fixture = Fixture::new("baseline_one")?;
    let (mut output, _) = fixture.check(None)?;
    assert_eq!(output.findings.len(), 1);
    // Different ASCII prefixes keep the Unicode file controls distinct on
    // filesystems that treat canonical Unicode spellings as one filename.
    // The renderer integration controls separately pin both exact spellings.
    let cases = [
        (r"src/a\b.pm", "src/a%5Cb.pm"),
        ("src/a/b.pm", "src/a/b.pm"),
        ("src/a%.pm", "src/a%25.pm"),
        ("src/a%25.pm", "src/a%2525.pm"),
        ("src/composed_dé.pm", "src/composed_d%C3%A9.pm"),
        ("src/decomposed_de\u{301}.pm", "src/decomposed_de%CC%81.pm"),
        ("src/é.pm", "src/%C3%A9.pm"),
        ("src/%C3%A9.pm", "src/%25C3%25A9.pm"),
        ("src/quote\"line\n.pm", "src/quote%22line%0A.pm"),
        ("src/quote%22line%0A.pm", "src/quote%2522line%250A.pm"),
        ("src/space #?.pm", "src/space%20%23%3F.pm"),
        ("src/🦀.pm", "src/%F0%9F%A6%80.pm"),
    ];
    let template = output.findings[0].clone();
    output.findings.clear();
    for (index, (path, _)) in cases.iter().enumerate() {
        let file = fixture.0.join(path);
        std::fs::create_dir_all(file.parent().ok_or("missing parent")?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&file, format!("native-file-{index}\n"))
            .map_err(|error| error.to_string())?;
        let mut finding = template.clone();
        finding.probe.location.file = file;
        finding.probe.location.line = index + 10;
        output.findings.push(finding);
    }
    let render = |output: &CheckOutput| -> Result<Value, String> {
        let text = ripr::render_check(output, &OutputFormat::Sarif)?;
        serde_json::from_str(&text).map_err(|error| error.to_string())
    };
    let current = render(&output)?;
    assert_eq!(results(&current)?.len(), cases.len());
    for (index, (path, uri)) in cases.iter().enumerate() {
        let result = results(&current)?
            .iter()
            .find(|result| {
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] == *uri
            })
            .ok_or_else(|| format!("missing literal URI {uri}"))?;
        assert_eq!(result["fingerprints"], result["partialFingerprints"]);
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            *uri
        );
        // Independent inverse: decode the emitted URI and read the original
        // file, rather than checking only agreement between renderer fields.
        let mut decoded = Vec::new();
        let actual_uri = result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .ok_or("missing emitted URI")?;
        let mut bytes = actual_uri.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%' {
                let high = (bytes.next().ok_or("short URI escape")? as char)
                    .to_digit(16)
                    .ok_or("invalid URI escape")?;
                let low = (bytes.next().ok_or("short URI escape")? as char)
                    .to_digit(16)
                    .ok_or("invalid URI escape")?;
                decoded.push((high * 16 + low) as u8);
            } else {
                decoded.push(byte);
            }
        }
        assert_eq!(decoded, path.as_bytes());
        let native = std::str::from_utf8(&decoded).map_err(|error| error.to_string())?;
        assert_eq!(
            std::fs::read(fixture.0.join(native)).map_err(|error| error.to_string())?,
            format!("native-file-{index}\n").as_bytes()
        );
    }
    let rows = policy_rows(&current)?;
    assert_eq!(rows.len(), 12, "all native identities survive policy dedup");
    assert_eq!(
        rows.iter()
            .map(|row| &row.key)
            .collect::<BTreeSet<_>>()
            .len(),
        12
    );
    for (path, uri) in &cases {
        let row = rows
            .iter()
            .find(|row| row.uri == *uri)
            .ok_or("missing policy URI")?;
        assert_eq!(row.uri, *uri);
        assert_eq!(row.fingerprint.rsplit('|').next(), Some(*path));
    }
    let mut baseline = current.clone();
    baseline["runs"][0]["results"]
        .as_array_mut()
        .ok_or("missing baseline results")?
        .retain(|result| {
            cases[..6].iter().any(|(_, uri)| {
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] == *uri
            })
        });
    let baseline = policy_rows(&baseline)?;
    assert_eq!(baseline.len(), 6);
    let report = compare(&rows, &baseline);
    assert_eq!(report.current_compared_results, 12);
    assert_eq!(report.new_results.len(), 6);
    assert_eq!(
        report
            .new_results
            .iter()
            .map(|row| row.uri.as_str())
            .collect::<BTreeSet<_>>(),
        cases[6..].iter().map(|(_, uri)| *uri).collect()
    );
    assert!(compare(&rows, &rows).new_results.is_empty());
    for finding in &mut output.findings {
        finding.probe.location.line += 100;
    }
    let moved = policy_rows(&render(&output)?)?;
    assert_eq!(moved.len(), 12);
    assert_eq!(
        moved.iter().map(|row| &row.key).collect::<BTreeSet<_>>(),
        rows.iter().map(|row| &row.key).collect()
    );
    assert!(compare(&moved, &rows).new_results.is_empty());
    assert_eq!(
        moved
            .iter()
            .filter_map(|row| row.line)
            .collect::<BTreeSet<_>>(),
        (110..122).collect()
    );
    Ok(())
}
