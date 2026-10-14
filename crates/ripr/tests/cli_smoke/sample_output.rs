//! Owned input and retained output oracles for the six #7825 smoke cases.
//! Inputs are embedded at compile time; runtime lookup never visits the checkout.
use super::{
    assert_success, ignore_remove_dir_all, probe_command, run_git, run_owned_stdin_probe,
    unique_external_workspace,
};
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

pub(super) type TestResult = Result<(), Box<dyn std::error::Error>>;
const SAMPLE: &str = "crates/ripr/examples/sample";
const SOURCE: &str = include_str!("../../examples/sample/src/lib.rs");
const TESTS: &str = include_str!("../../examples/sample/tests/pricing.rs");
const DIFF: &str = include_str!("../../examples/sample/example.diff");

pub(super) struct Fixture {
    scratch: PathBuf,
    root: PathBuf,
    cwd: PathBuf,
    setup: Duration,
}

impl Fixture {
    pub(super) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let started = Instant::now();
        let scratch = unique_external_workspace("sample-output")?;
        // Exclusive creation plus the existing PID/sequence/time allocator.
        // Construct the drop owner before any fallible population step.
        std::fs::create_dir(&scratch)?;
        let mut fixture = Self {
            root: scratch.join("owned"),
            cwd: scratch.join("decoy"),
            scratch,
            setup: Duration::ZERO,
        };
        let sample = fixture.root.join(SAMPLE);
        std::fs::create_dir_all(sample.join("src"))?;
        std::fs::create_dir_all(sample.join("tests"))?;
        std::fs::write(
            fixture.root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/ripr/examples/sample\"]\nresolver = \"2\"\n",
        )?;
        std::fs::write(
            sample.join("Cargo.toml"),
            "[package]\nname = \"ripr-sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(sample.join("src/lib.rs"), SOURCE)?;
        std::fs::write(sample.join("tests/pricing.rs"), TESTS)?;
        std::fs::write(fixture.root.join("sample.diff"), DIFF)?;
        run_git(&fixture.root, &["init", "-q"])?;
        run_git(&fixture.root, &["add", "."])?;
        run_git(
            &fixture.root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "sample",
            ],
        )?;
        std::fs::create_dir_all(fixture.cwd.join("src"))?;
        std::fs::write(
            fixture.cwd.join("Cargo.toml"),
            "[package]\nname = \"sample-output-decoy\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )?;
        std::fs::write(
            fixture.cwd.join("src/lib.rs"),
            "pub fn decoy() -> bool { false }\n",
        )?;
        // Root/config fallback to CWD must fail rather than accidentally satisfy
        // a shape-only output assertion on an empty decoy subject.
        std::fs::write(
            fixture.cwd.join("ripr.toml"),
            "this is deliberately invalid TOML [\n",
        )?;
        run_git(&fixture.cwd, &["init", "-q"])?;
        std::fs::create_dir_all(fixture.scratch.join("state/cache"))?;
        std::fs::create_dir_all(fixture.scratch.join("state/tmp"))?;
        fixture.setup = started.elapsed();
        Ok(fixture)
    }

    pub(super) fn run(&self, extra: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
        let started = Instant::now();
        let mut command = probe_command(env!("CARGO_BIN_EXE_ripr"));
        command
            .current_dir(&self.cwd)
            .arg("check")
            .arg("--root")
            .arg(&self.root)
            .arg("--diff")
            .arg(self.root.join("sample.diff"))
            .args(extra)
            .env("RIPR_CACHE_DIR", self.scratch.join("state/cache"))
            .env("TMPDIR", self.scratch.join("state/tmp"))
            .env("TMP", self.scratch.join("state/tmp"))
            .env("TEMP", self.scratch.join("state/tmp"))
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Reuse the harness's owned, drained, deadline-bounded process route.
        let output = run_owned_stdin_probe(command, &[], Duration::from_secs(30))?;
        if std::env::var_os("RIPR_SAMPLE_OUTPUT_TIMINGS").is_some() {
            eprintln!(
                "sample-output setup_ns={} cli_ns={}",
                self.setup.as_nanos(),
                started.elapsed().as_nanos()
            );
        }
        Ok(output)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.scratch);
    }
}

pub(super) fn human(output: Output) -> Result<(), String> {
    assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Summary: 4 probe(s)"));
    assert!(stdout.contains("Start here:"));
    assert!(stdout.contains("Static exposure: weak (weakly_exposed, "));
    assert!(stdout.contains("Evidence:"));
    // #4320: the digest missing-discriminator line discloses its one-of-N
    // window, so the pin carries the parenthesized window opening.
    assert!(stdout.contains("Missing discriminator ("));
    assert!(stdout.contains("Next step:"));
    assert!(stdout.contains("lower-priority finding(s) omitted"));
    assert!(stdout.contains("--format human-full"));
    Ok(())
}

pub(super) fn contract(output: Output) -> Result<(), String> {
    assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""schema_version": "0.2""#));
    assert!(stdout.contains(r#""classification": "weakly_exposed""#));
    assert!(stdout.contains(r#""evidence_path""#));
    assert!(stdout.contains(r#""flow_sinks""#));
    assert!(stdout.contains(r#""assertion_texts""#));
    assert!(stdout.contains(r#""activation""#));
    assert!(stdout.contains(r#""missing_discriminators""#));
    assert!(stdout.contains(r#""oracle_kind""#));
    assert!(stdout.contains(r#""recommended_next_step""#));
    assert!(stdout.contains(r#""suggested_next_action""#));
    sample_findings(&output)?;
    Ok(())
}

pub(super) fn success(output: Output) -> Result<(), String> {
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("success stdout should parse as JSON: {err}\n{stdout}"))?;
    if value.get("analysis_scope").is_some() {
        return Err(format!(
            "success output must not carry analysis_scope: {value}"
        ));
    }
    if value.get("run_limitations").is_some() {
        return Err(format!(
            "success output must not carry run_limitations: {value}"
        ));
    }
    if value["schema_version"] != "0.2" {
        return Err(format!("success schema_version must stay 0.2: {value}"));
    }
    if value["findings"].as_array().is_none() {
        return Err(format!("success output must carry findings: {value}"));
    }
    sample_findings(&output)?;
    Ok(())
}

pub(super) fn native(output: Output) -> Result<(), String> {
    assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""schema_version": "0.8""#));
    // Diff-scoped badge JSON never carries a public projection.
    assert!(!stdout.contains(r#""public_projection""#));
    assert!(stdout.contains(r#""kind": "ripr""#));
    assert!(stdout.contains(r#""scope": "diff""#));
    assert!(stdout.contains(r#""basis": "finding_exposure""#));
    assert!(stdout.contains(r#""label": "ripr""#));
    assert!(stdout.contains(r#""counts""#));
    assert!(stdout.contains(r#""reason_counts""#));
    assert!(stdout.contains(r#""policy""#));
    assert!(stdout.contains(r#""unsuppressed_exposure_gaps""#));
    assert!(stdout.contains(r#""duplicate_activation_and_oracle_shape": 0"#));
    assert!(!stdout.contains(r#""schemaVersion""#));
    // The sample diff has 4 weakly_exposed findings after nested call shapes are
    // excluded; the badge headline reflects the surviving semantic probes.
    assert!(stdout.contains(r#""message": "4""#));
    assert!(stdout.contains(r#""status": "warn""#));
    assert!(stdout.contains(r#""color": "orange""#));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("native badge JSON: {error}"))?;
    assert_eq!(value["schema_version"], "0.8");
    assert_eq!(value["counts"]["unsuppressed_exposure_gaps"], 4);
    assert_eq!(value["counts"]["analyzed_findings"], 4);
    Ok(())
}

pub(super) fn shields(output: Output) -> Result<(), String> {
    assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""schemaVersion": 1"#));
    assert!(stdout.contains(r#""label": "ripr""#));
    assert!(stdout.contains(r#""message": "4""#));
    assert!(stdout.contains(r#""color": "orange""#));
    // Native-JSON-only fields must not leak into the Shields shape.
    for forbidden in [
        r#""counts""#,
        r#""reason_counts""#,
        r#""policy""#,
        r#""kind""#,
        r#""status""#,
        r#""scope""#,
        r#""basis""#,
        r#""schema_version""#,
    ] {
        assert!(
            !stdout.contains(forbidden),
            "Shields projection must not contain `{forbidden}`: {stdout}"
        );
    }
    // Message has no denominator and no coverage framing.
    assert!(!stdout.contains('/') || !stdout.contains(r#""message""#));
    assert!(!stdout.to_ascii_lowercase().contains("coverage"));
    assert!(!stdout.to_ascii_lowercase().contains("uncovered"));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Shields badge JSON: {error}"))?;
    let object = value.as_object().ok_or("Shields must be an object")?;
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        ["color", "label", "message", "schemaVersion"]
    );
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["message"], "4");
    Ok(())
}

pub(super) fn badge_exit(output: Output) -> Result<(), String> {
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("badge exit JSON: {error}"))?;
    assert_eq!(value["message"], "4");
    assert_eq!(value["counts"]["unsuppressed_exposure_gaps"], 4);
    assert_eq!(value["policy"]["fail_on_nonzero"], false);
    Ok(())
}

/// A shape-only check must never silently turn into an empty analysis control.
fn sample_findings(output: &Output) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| format!("sample JSON: {error}"))?;
    let findings = value["findings"]
        .as_array()
        .ok_or("findings must be an array")?;
    assert_eq!(findings.len(), 4, "sample must retain four actual findings");
    let mut families = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for finding in findings {
        assert_eq!(finding["classification"], "weakly_exposed");
        assert_eq!(finding["probe"]["file"], format!("./{SAMPLE}/src/lib.rs"));
        ids.insert(
            finding["probe"]["id"]
                .as_str()
                .ok_or("probe id")?
                .to_string(),
        );
        families.insert(
            finding["probe"]["family"]
                .as_str()
                .ok_or("probe family")?
                .to_string(),
        );
    }
    assert_eq!(
        families.into_iter().collect::<Vec<_>>(),
        ["error_path", "predicate", "return_value"]
    );
    assert_eq!(
        ids.into_iter().collect::<Vec<_>>(),
        [
            "probe:crates_ripr_examples_sample_src_lib.rs:error_path:c1a03250",
            "probe:crates_ripr_examples_sample_src_lib.rs:predicate:3d666895",
            "probe:crates_ripr_examples_sample_src_lib.rs:predicate:c636fec0",
            "probe:crates_ripr_examples_sample_src_lib.rs:return_value:808052af",
        ]
    );
    let evidence: Vec<_> = findings
        .iter()
        .flat_map(|finding| finding["evidence_path"].as_array().into_iter().flatten())
        .collect();
    for witness in [
        "reach yes: Related tests appear to reach price: premium_customer_gets_discount",
        "reach yes: Related tests appear to reach validate_currency: rejects_bad_currency",
    ] {
        assert!(
            evidence.iter().any(|value| value.as_str() == Some(witness)),
            "fixture must retain the changed owner's real test witness: {witness}"
        );
    }
    Ok(())
}

fn rejects(oracle: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe) {
    assert!(
        !matches!(std::panic::catch_unwind(oracle), Ok(Ok(()))),
        "a seeded wrong output passed the retained oracle"
    );
}

fn changed_output(output: &Output, from: &str, to: &str) -> Output {
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains(from),
        "mutation must change an actual output field: {from}"
    );
    let mut changed = output.clone();
    changed.stdout = text.replace(from, to).into_bytes();
    assert_ne!(changed.stdout, output.stdout);
    changed
}

#[test]
fn retained_oracles_reject_wrong_contract_shape_count_and_refusal() -> TestResult {
    // Mutate real analyzer/rendered output after the positive oracle passes.
    // These are assertion controls, never substitute inputs to the six cases.
    let fixture = Fixture::new()?;
    let json = fixture.run(&["--json"])?;
    contract(json.clone())?;
    success(json.clone())?;
    for field in [
        "evidence_path",
        "flow_sinks",
        "assertion_texts",
        "activation",
        "missing_discriminators",
        "oracle_kind",
        "recommended_next_step",
        "suggested_next_action",
    ] {
        let wrong = changed_output(&json, &format!("\"{field}\""), "\"wrong_field\"");
        rejects(|| contract(wrong));
    }
    rejects(|| contract(changed_output(&json, "\"0.2\"", "\"999\"")));
    rejects(|| {
        contract(changed_output(
            &json,
            "\"weakly_exposed\"",
            "\"static_unknown\"",
        ))
    });
    let mut missing = serde_json::from_slice::<serde_json::Value>(&json.stdout)?;
    missing["findings"] = serde_json::json!([]);
    let mut wrong = json.clone();
    wrong.stdout = serde_json::to_vec_pretty(&missing)?;
    rejects(|| success(wrong));
    for key in ["analysis_scope", "run_limitations"] {
        let mut refusal = serde_json::from_slice::<serde_json::Value>(&json.stdout)?;
        refusal[key] = serde_json::json!([]);
        let mut wrong = json.clone();
        wrong.stdout = serde_json::to_vec_pretty(&refusal)?;
        rejects(|| success(wrong));
    }
    let badge = fixture.run(&["--format", "badge-json"])?;
    native(badge.clone())?;
    badge_exit(badge.clone())?;
    rejects(|| native(changed_output(&badge, "\"counts\"", "\"wrong_counts\"")));
    rejects(|| native(changed_output(&badge, "\"0.8\"", "\"999\"")));
    rejects(|| {
        native(changed_output(
            &badge,
            "\"message\": \"4\"",
            "\"message\": \"0\"",
        ))
    });
    rejects(|| {
        badge_exit(changed_output(
            &badge,
            "\"message\": \"4\"",
            "\"message\": \"0\"",
        ))
    });
    let failure = fixture.run(&["--format", "not-a-format"])?;
    assert!(!failure.status.success());
    let mut wrong_exit = badge.clone();
    wrong_exit.status = failure.status;
    rejects(|| badge_exit(wrong_exit));
    let shields_output = fixture.run(&["--format", "badge-shields"])?;
    shields(shields_output.clone())?;
    rejects(|| {
        shields(changed_output(
            &shields_output,
            "\"schemaVersion\"",
            "\"schema_version\"",
        ))
    });
    rejects(|| {
        shields(changed_output(
            &shields_output,
            "\"message\": \"4\"",
            "\"message\": \"0\"",
        ))
    });
    let mut extra = serde_json::from_slice::<serde_json::Value>(&shields_output.stdout)?;
    extra["unexpected"] = serde_json::json!(true);
    let mut wrong = shields_output.clone();
    wrong.stdout = serde_json::to_vec_pretty(&extra)?;
    rejects(|| shields(wrong));
    let human_output = fixture.run(&[])?;
    human(human_output.clone())?;
    rejects(|| {
        human(changed_output(
            &human_output,
            "Summary: 4 probe(s)",
            "Summary: 0 probe(s)",
        ))
    });
    rejects(|| {
        human(changed_output(
            &human_output,
            "Missing discriminator (",
            "Wrong discriminator (",
        ))
    });
    Ok(())
}

#[test]
fn parallel_fixture_state_and_cleanup_are_independent() -> TestResult {
    let one = Fixture::new()?;
    let two = Fixture::new()?;
    assert_ne!(one.scratch, two.scratch);
    assert!(!one.scratch.starts_with(&two.scratch));
    assert!(!two.scratch.starts_with(&one.scratch));
    std::fs::write(one.scratch.join("state/cache/sentinel"), "one")?;
    std::fs::write(two.scratch.join("state/cache/sentinel"), "two")?;
    std::thread::scope(|scope| -> TestResult {
        let first = scope.spawn(|| {
            one.run(&["--json"])
                .map_err(|error| error.to_string())
                .and_then(contract)
        });
        contract(two.run(&["--json"])?)?;
        assert!(matches!(first.join(), Ok(Ok(()))));
        Ok(())
    })?;
    let removed = one.scratch.clone();
    drop(one);
    assert!(
        !removed.exists(),
        "first fixture must clean up after its child exits"
    );
    assert_eq!(
        std::fs::read_to_string(two.scratch.join("state/cache/sentinel"))?,
        "two"
    );
    assert!(two.root.exists());
    let removed = two.scratch.clone();
    drop(two);
    assert!(!removed.exists());
    Ok(())
}
