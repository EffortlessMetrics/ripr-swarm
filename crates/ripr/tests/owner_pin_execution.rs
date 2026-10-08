//! Matched static/runtime controls for RIPR-SPEC-0197 (#4478).
//! Run the same tests against correct and deliberately wrong libraries;
//! compilation success and one executed test precede outcome assertions.

use ripr::{
    CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace, render_check,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-owner-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(program: &Path, arguments: &[&std::ffi::OsStr]) -> Result<Output, String> {
    Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("{}: {error}", program.display()))
}

fn compiles(output: Output, phase: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{phase} compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[test]
fn local_empty_macro_preserves_independent_equality_execution() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/predicate_oracle_execution_direct");
    let original = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let (production, _) = original.split_once("#[cfg(test)]").ok_or("missing tests")?;
    let empty = "macro_rules! discard_tokens { ($($ignored:tt)*) => {} }";
    let returning = "macro_rules! discard_tokens { ($($ignored:tt)*) => { return; } }";
    for (name, definitions, before, admitted, mutant_detected) in [
        (
            "different_name_empty",
            empty.to_string(),
            "".to_string(),
            true,
            true,
        ),
        (
            "empty_discards_return_tokens",
            empty.to_string(),
            "".to_string(),
            true,
            true,
        ),
        (
            "returning",
            returning.to_string(),
            "".to_string(),
            false,
            false,
        ),
        (
            "shadowed",
            empty.to_string(),
            returning.to_string(),
            false,
            false,
        ),
        (
            "ambiguous",
            format!("{empty}\nmod elsewhere {{ {empty} }}"),
            "".to_string(),
            false,
            true,
        ),
        (
            "disabled",
            format!("#[cfg(any())]\n{empty}\n{returning}"),
            "".to_string(),
            false,
            false,
        ),
        (
            "imported",
            format!(
                "mod exports {{ {empty}\npub(crate) use discard_tokens; }}\nuse exports::discard_tokens;"
            ),
            "".to_string(),
            false,
            true,
        ),
        (
            "unproven_matcher",
            "macro_rules! discard_tokens { ($ignored:expr) => {} }".to_string(),
            "".to_string(),
            false,
            true,
        ),
        (
            "opaque_nonempty",
            "macro_rules! discard_tokens { ($($ignored:tt)*) => { let _marker = (); } }"
                .to_string(),
            "".to_string(),
            false,
            true,
        ),
    ] {
        let arguments = if name == "empty_discards_return_tokens" {
            "return;"
        } else {
            "discounted_total(100, 100)"
        };
        let source = format!(
            "{production}\n{definitions}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn boundary() {{\n{before}\ndiscard_tokens!({arguments});\nassert_eq!(discounted_total(100, 100), 90);\n}}\n}}\n"
        );
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::copy(
            fixture.join("input/Cargo.toml"),
            scratch.0.join("Cargo.toml"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        // Establish the real test's value independently of conservative static
        // refusals for opaque, imported or ambiguous macro bindings.
        // Keep compiled variants outside the analyzed project: runtime.rs at
        // its root would add another macro definition and test to RustIndex.
        let runtime_scratch = Scratch::create()?;
        for wrong in [false, true] {
            let runtime_source = runtime_scratch.0.join("runtime.rs");
            assert!(!runtime_source.starts_with(&scratch.0));
            let runtime = runtime_scratch
                .0
                .join(format!("runtime{}", std::env::consts::EXE_SUFFIX));
            let subject = if wrong {
                source.replace(
                    "amount >= discount_threshold",
                    "amount > discount_threshold",
                )
            } else {
                source.clone()
            };
            if wrong {
                assert_ne!(
                    subject, source,
                    "{name}: mutation must change the runtime subject"
                );
            }
            std::fs::write(&runtime_source, subject).map_err(|error| error.to_string())?;
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        "--edition=2024".as_ref(),
                        "--test".as_ref(),
                        runtime_source.as_os_str(),
                        "-o".as_ref(),
                        runtime.as_os_str(),
                    ],
                )?,
                name,
            )?;
            let result = run(&runtime, &[])?;
            assert!(
                String::from_utf8_lossy(&result.stdout).contains("running 1 test"),
                "{name}: nonempty libtest subject"
            );
            assert_eq!(
                result.status.success(),
                !(wrong && mutant_detected),
                "{name}, wrong={wrong}"
            );
        }
        let mut entries = std::fs::read_dir(&scratch.0)
            .map_err(|error| error.to_string())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort();
        assert_eq!(
            entries,
            ["Cargo.toml", "src"],
            "{name}: bounded analyzed root"
        );
        let sources = std::fs::read_dir(scratch.0.join("src"))
            .map_err(|error| error.to_string())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        assert_eq!(
            sources,
            ["lib.rs"],
            "{name}: exactly one analyzed Rust file"
        );
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{name}: unique predicate");
        let related = findings[0]["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        assert_eq!(related.len(), 1, "{name}: one indexed test");
        assert_eq!(
            related[0]["file"], "src/lib.rs",
            "{name}: runtime source is not indexed"
        );
        assert_eq!(findings[0]["probe"]["family"], "predicate", "{name}");
        assert_eq!(
            findings[0]["oracle_strength"],
            if admitted { "strong" } else { "none" },
            "{name}"
        );
        if admitted {
            assert_eq!(findings[0]["classification"], "exposed", "{name}");
        }
    }
    Ok(())
}

#[test]
fn empty_macro_arguments_cannot_activate_a_real_far_oracle() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/predicate_empty_macro_boundary");
    let discarded = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let far = discarded.replace("discard_tokens!(discounted_total(100, 100)); ", "");
    let positive = discarded.replace(
        "assert_eq!(discounted_total(90, 100), 90)",
        "assert_eq!(discounted_total(100, 100), 90)",
    );
    assert_ne!(far, discarded);
    assert_ne!(positive, discarded);
    let mut observations = Vec::new();
    for (name, source, exposed) in [
        ("discarded_boundary_far", discarded, false),
        ("far_only", far, false),
        ("real_boundary_positive", positive, true),
    ] {
        // The existing helper allocates a separate runtime Scratch, so none
        // of the compiled variants can enter the analyzed fixture below.
        source_runtime_control(&source, &format!("{name}-correct"), 1, false)?;
        let wrong = source.replace(
            "amount >= discount_threshold",
            "amount > discount_threshold",
        );
        assert_ne!(wrong, source);
        source_runtime_control(&wrong, &format!("{name}-wrong"), 1, exposed)?;
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::copy(
            fixture.join("input/Cargo.toml"),
            scratch.0.join("Cargo.toml"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        assert_eq!(
            std::fs::read_dir(&scratch.0)
                .map_err(|error| error.to_string())?
                .count(),
            2
        );
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{name}: one predicate");
        let finding = &findings[0];
        assert_eq!(finding["probe"]["family"], "predicate");
        assert_eq!(finding["related_tests"].as_array().map(Vec::len), Some(1));
        assert_eq!(finding["related_tests"][0]["file"], "src/lib.rs");
        assert_eq!(finding["oracle_strength"], "strong", "{name}");
        for stage in ["observe", "discriminate"] {
            assert_eq!(
                finding["ripr"][stage]["state"], "yes",
                "{name}: real far equality survives"
            );
        }
        assert_eq!(
            finding["classification"],
            if exposed { "exposed" } else { "weakly_exposed" },
            "{name}"
        );
        assert_eq!(
            finding["ripr"]["infect"]["state"],
            if exposed { "yes" } else { "weak" },
            "{name}"
        );
        if !exposed {
            assert!(
                finding["ripr"]["infect"]["summary"]
                    .as_str()
                    .is_some_and(|s| s.contains("equality-boundary discriminator is missing"))
            );
        }
        observations.push(finding.clone());
    }
    assert_eq!(
        observations[0]["classification"],
        observations[1]["classification"]
    );
    assert_eq!(
        observations[0]["ripr"]["infect"],
        observations[1]["ripr"]["infect"]
    );
    Ok(())
}

#[test]
fn owner_pin_matched_static_and_runtime_controls() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (case, exposed) in [
        ("direct", true),
        ("called_closure", true),
        ("uncalled_closure", false),
        ("shadowed_assertion", false),
        ("no_assertion", false),
        ("expired_closure_binding", false),
        ("deferred_closure_binding", false),
        ("macro_operand_return", false),
        ("macro_return", false),
        ("if_false", false),
        ("unpolled_async", false),
        ("token_overlap", false),
        ("token_direct", true),
        ("token_called_closure", true),
        ("unknown_singleton", false),
        ("nested_test", false),
        ("cfg_false_module", false),
        ("cfg_false_file", false),
        ("cfg_attr_module", false),
        ("out_of_line_cfg", false),
    ] {
        let fixture = fixtures.join(format!("owner_return_pin_{case}"));
        let report = check_workspace(CheckInput {
            root: fixture.join("input"),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        assert_eq!(
            json["analysis_outcome"]["analysis_complete"], true,
            "{case}"
        );
        assert_eq!(report.findings.len(), 1, "{case}: nonempty unique subject");
        let finding = &report.findings[0];
        assert_eq!(finding.probe.family, ProbeFamily::ReturnValue, "{case}");
        assert_eq!(
            finding.class,
            if exposed {
                ExposureClass::Exposed
            } else {
                ExposureClass::ReachableUnrevealed
            },
            "{case}"
        );
        if !exposed {
            let finding = &json["findings"][0];
            assert_eq!(finding["ripr"]["observe"]["state"], "no", "{case}");
            assert_eq!(finding["ripr"]["discriminate"]["state"], "no", "{case}");
            assert_eq!(finding["oracle_strength"], "none", "{case}");
            assert_eq!(
                finding["confidence"], 0.79,
                "advisory score follows unchanged stage arithmetic"
            );
            let tests = finding["related_tests"]
                .as_array()
                .ok_or("missing related test provenance")?;
            assert!(!tests.is_empty(), "{case}");
            assert!(
                tests
                    .iter()
                    .all(|test| test["oracle"].as_str().is_some_and(str::is_empty)
                        && test["oracle_strength"] == "none"),
                "{case}"
            );
            if case != "no_assertion" {
                assert!(
                    finding["ripr"]["observe"]["summary"]
                        .as_str()
                        .is_some_and(|text| text.contains("rust_assertion_context_unestablished")),
                    "{case}"
                );
                assert!(
                    finding["recommended_next_step"]
                        .as_str()
                        .is_some_and(|text| text.contains("ripr did not credit the `assert_eq!`")
                            && text.contains("\"not credited\" note says why")),
                    "{case}"
                );
            }
        }
        let correct = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            correct.matches("input * 3").count(),
            1,
            "{case}: mutation must alter one subject"
        );
        for broken in [false, true] {
            let scratch = Scratch::create()?;
            let lib = scratch.0.join("lib.rs");
            let rlib = scratch.0.join("libowner_pin_control.rlib");
            let executable = scratch
                .0
                .join(format!("weight_tests{}", std::env::consts::EXE_SUFFIX));
            std::fs::write(
                &lib,
                if broken {
                    correct.replace("input * 3", "input * 2")
                } else {
                    correct.clone()
                },
            )
            .map_err(|error| error.to_string())?;
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        "--edition=2024".as_ref(),
                        "--crate-name=owner_pin_control".as_ref(),
                        "--crate-type=rlib".as_ref(),
                        lib.as_os_str(),
                        "-o".as_ref(),
                        rlib.as_os_str(),
                    ],
                )?,
                "library",
            )?;
            let external = format!("owner_pin_control={}", rlib.display());
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        "--edition=2024".as_ref(),
                        "--test".as_ref(),
                        fixture.join("input/tests/weight_tests.rs").as_os_str(),
                        "--extern".as_ref(),
                        external.as_ref(),
                        "-o".as_ref(),
                        executable.as_os_str(),
                    ],
                )?,
                "test",
            )?;
            let runtime = run(&executable, &["--nocapture".as_ref()])?;
            let stdout = String::from_utf8_lossy(&runtime.stdout);
            assert!(stdout.contains("running 1 test"), "{case}: {stdout}");
            assert_eq!(
                runtime.status.success(),
                !broken || !exposed,
                "{case}, broken={broken}: {stdout}; {}",
                String::from_utf8_lossy(&runtime.stderr)
            );
            if broken && exposed {
                let stderr = String::from_utf8_lossy(&runtime.stderr);
                assert!(
                    stderr.contains("left: 8") && stderr.contains("right: 12"),
                    "{case}: {stderr}"
                );
                assert!(stdout.contains("1 failed"), "{case}: {stdout}");
            } else {
                assert!(stdout.contains("1 passed"), "{case}: {stdout}");
            }
            eprintln!(
                "owner-pin runtime: {case}, broken={broken}, {}, executed=1",
                runtime.status
            );
        }
    }
    Ok(())
}

#[test]
fn owner_pin_token_overlap_cannot_bypass_oracle_admission() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_token_overlap");
    let report = check_workspace(CheckInput {
        root: fixture.join("input"),
        diff_file: Some(fixture.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    assert_eq!(
        report.findings.len(),
        1,
        "nonempty changed return-value subject"
    );
    assert_eq!(report.findings[0].class, ExposureClass::ReachableUnrevealed);
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    assert_eq!(json["findings"][0]["ripr"]["observe"]["state"], "no");
    assert_eq!(json["findings"][0]["ripr"]["discriminate"]["state"], "no");
    Ok(())
}

#[test]
fn owner_pin_shared_admission_keeps_credit_on_one_admitted_oracle() -> Result<(), String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    for (body, exposed, strength) in [
        (
            "let input = 4;\nassert_eq!(weight(input), 12);",
            true,
            "strong",
        ),
        (
            "let input = 4;\nlet check = || assert_eq!(weight(input), 12);\ncheck();",
            true,
            "strong",
        ),
        (
            "let input = 4;\nlet _later = || assert_eq!(weight(input), 12);\nassert_eq!(2 + 2, 4);",
            false,
            "strong",
        ),
        (
            "let input = 4;\nlet _later = || assert_eq!(weight(input), 12);\nassert!(weight(input) > 0);",
            false,
            "weak",
        ),
        (
            "let input = 4;\nassert_eq!(weight(input), 12);\nlet _later = || assert_eq!(weight(input), 12);",
            true,
            "strong",
        ),
        (
            "let input = 4;\nlet _later = || assert_eq!(weight(input), 12);\nassert_eq!(weight(input), 12);",
            true,
            "strong",
        ),
    ] {
        let scratch = Scratch::create()?;
        for directory in ["src", "tests"] {
            std::fs::create_dir(scratch.0.join(directory)).map_err(|error| error.to_string())?;
        }
        for file in ["Cargo.toml", "src/lib.rs"] {
            std::fs::copy(base.join("input").join(file), scratch.0.join(file))
                .map_err(|error| error.to_string())?;
        }
        std::fs::write(
            scratch.0.join("tests/weight_tests.rs"),
            format!("use owner_pin_control::weight;\n#[test]\nfn checks_weight() {{\n{body}\n}}\n"),
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(base.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(report.findings.len(), 1, "{body}");
        assert_eq!(
            report.findings[0].class == ExposureClass::Exposed,
            exposed,
            "{body}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        assert_eq!(
            json["analysis_outcome"]["analysis_complete"], true,
            "{body}"
        );
        assert_eq!(json["findings"][0]["oracle_strength"], strength, "{body}");
    }
    Ok(())
}

#[test]
fn owner_pin_refused_rows_do_not_crowd_out_admitted_oracles() -> Result<(), String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    let scratch = Scratch::create()?;
    for directory in ["src", "tests"] {
        std::fs::create_dir(scratch.0.join(directory)).map_err(|error| error.to_string())?;
    }
    for file in ["Cargo.toml", "src/lib.rs"] {
        std::fs::copy(base.join("input").join(file), scratch.0.join(file))
            .map_err(|error| error.to_string())?;
    }
    let tests = (0..8).map(|index| format!("#[test]\nfn checks_weight_{index}() {{\nlet input=4;\nlet _later = || assert_eq!(weight(input), 12);\nassert_eq!(weight(input), 12);\n}}\n")).collect::<String>();
    std::fs::write(
        scratch.0.join("tests/weight_tests.rs"),
        format!("use owner_pin_control::weight;\n{tests}"),
    )
    .map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(base.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].class, ExposureClass::Exposed);
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    let tests = json["findings"][0]["related_tests"]
        .as_array()
        .ok_or("missing related test rows")?;
    assert_eq!(tests.len(), 8);
    assert!(
        tests
            .iter()
            .all(|test| test["oracle_strength"] == "strong" && test["oracle"].is_string())
    );
    assert_eq!(json["findings"][0]["oracle_strength"], "strong");
    Ok(())
}

#[test]
fn owner_pin_review_admission_controls() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut mismatches = Vec::new();
    for case in [
        "unknown_singleton",
        "nested_test",
        "cfg_false_module",
        "cfg_false_file",
        "cfg_attr_module",
        "out_of_line_cfg",
    ] {
        let fixture = fixtures.join(format!("owner_return_pin_{case}"));
        let report = check_workspace(CheckInput {
            root: fixture.join("input"),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(report.findings.len(), 1, "{case}: nonempty unique subject");
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let finding = &json["findings"][0];
        assert_eq!(
            json["analysis_outcome"]["analysis_complete"], true,
            "{case}"
        );
        if report.findings[0].class != ExposureClass::ReachableUnrevealed
            || finding["ripr"]["observe"]["state"] != "no"
            || finding["ripr"]["discriminate"]["state"] != "no"
            || finding["oracle_strength"] != "none"
        {
            mismatches.push(format!(
                "{case}: class={:?}, observe={}, discriminate={}, strength={}",
                report.findings[0].class,
                finding["ripr"]["observe"]["state"],
                finding["ripr"]["discriminate"]["state"],
                finding["oracle_strength"]
            ));
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches.join("\n"))
    }
}

/// Lexical enum facts remain useful even when the assertion is not admitted.
/// Their presentation must not imply the refused oracle observed a value.
#[test]
fn lexical_source_values_do_not_claim_observed_oracles() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for case in [
        "direct",
        "called",
        "uncalled",
        "reached_uncalled",
        "shadowed",
        "false_branch",
    ] {
        let fixture = fixtures.join(format!("error_path_oracle_execution_{case}"));
        let report = check_workspace(CheckInput {
            root: fixture.join("input"),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let selected = json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .filter(|finding| finding["probe"]["family"] == "error_path")
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "{case}: selected ErrorPath only");
        let finding = selected[0];
        let admitted = matches!(case, "direct" | "called");
        assert_eq!(
            finding["classification"],
            if admitted {
                "exposed"
            } else {
                "reachable_unrevealed"
            },
            "{case}"
        );
        assert_eq!(
            finding["oracle_strength"],
            if admitted { "strong" } else { "none" },
            "{case}"
        );
        for stage in ["observe", "discriminate"] {
            assert_eq!(
                finding["ripr"][stage]["state"],
                if admitted { "yes" } else { "no" },
                "{case}/{stage}"
            );
        }
        let facts = finding["observed_values"]
            .as_array()
            .ok_or("missing source values")?;
        let variants = facts
            .iter()
            .filter(|fact| {
                fact["context"] == "enum_variant" && fact["value"] == "io::ErrorKind::Other"
            })
            .collect::<Vec<_>>();
        assert!(!variants.is_empty(), "{case}: retain lexical enum facts");
        assert_eq!(
            finding["observed_values"],
            finding["activation"]["observed_values"]
        );
        let evidence = finding["evidence_path"]
            .as_array()
            .ok_or("missing evidence path")?;
        let human = render_check(&report, &OutputFormat::HumanFull)?;
        for variant in variants {
            let line = variant["line"].as_u64().ok_or("missing fact line")?;
            assert!(
                finding["assertion_texts"][line.to_string()]
                    .as_str()
                    .is_some_and(|text| text.contains("io::ErrorKind::Other")),
                "{case}: source provenance remains"
            );
            let label = format!("source enum variant value io::ErrorKind::Other at line {line}");
            assert!(
                evidence
                    .iter()
                    .any(|entry| entry.as_str() == Some(label.as_str())),
                "{case}: neutral structured evidence"
            );
            assert!(human.contains(&label), "{case}: neutral human evidence");
        }
        assert!(!evidence.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|text| text.starts_with("observed enum variant value "))
        }));
        assert!(!human.contains("observed enum variant value "));
    }
    Ok(())
}

/// Shared execution provenance is independent of the changed behavior's family.
/// The error assertions use semantic operands, never diagnostic text (#5027).
#[test]
fn equality_oracle_family_matched_static_and_runtime_controls() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (family, probe_family, before, after) in [
        (
            "error_path",
            ProbeFamily::ErrorPath,
            "rdr.read(&mut buf).unwrap_or(0)",
            "rdr.read(&mut buf)?",
        ),
        (
            "predicate",
            ProbeFamily::Predicate,
            "amount > discount_threshold",
            "amount >= discount_threshold",
        ),
    ] {
        for case in [
            "direct",
            "called",
            "uncalled",
            "false_branch",
            "shadowed",
            "no_assertion",
            "reached_uncalled",
        ] {
            let exposed = matches!(case, "direct" | "called");
            let fixture = fixtures.join(format!("{family}_oracle_execution_{case}"));
            let report = check_workspace(CheckInput {
                root: fixture.join("input"),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let findings: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.probe.family == probe_family)
                .collect();
            assert_eq!(findings.len(), 1, "{family}/{case}: unique family subject");
            assert_eq!(
                findings[0].class,
                if exposed {
                    ExposureClass::Exposed
                } else {
                    ExposureClass::ReachableUnrevealed
                },
                "{family}/{case}"
            );
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|error| error.to_string())?;
            assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
            let finding = json["findings"]
                .as_array()
                .and_then(|findings| {
                    findings
                        .iter()
                        .find(|finding| finding["probe"]["family"] == family)
                })
                .ok_or("selected family disappeared from JSON")?;
            for stage in ["observe", "discriminate"] {
                assert_eq!(
                    finding["ripr"][stage]["state"],
                    if exposed { "yes" } else { "no" },
                    "{family}/{case}/{stage}"
                );
            }
            assert_eq!(
                finding["oracle_strength"],
                if exposed { "strong" } else { "none" },
                "{family}/{case}"
            );
            let related = finding["related_tests"]
                .as_array()
                .ok_or("missing related test provenance")?;
            assert_eq!(related.len(), 1, "{family}/{case}");
            if !exposed {
                assert_eq!(related[0]["oracle_strength"], "none");
                assert_eq!(related[0]["oracle"], "");
                if case != "no_assertion" {
                    assert!(
                        finding["ripr"]["observe"]["summary"]
                            .as_str()
                            .is_some_and(
                                |summary| summary.contains("rust_assertion_context_unestablished")
                            ),
                        "{family}/{case}"
                    );
                }
            }

            let correct = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
                .map_err(|error| error.to_string())?;
            assert_eq!(correct.matches(after).count(), 1, "unique mutation subject");
            for broken in [false, true] {
                // Generated mutation sources must remain outside analyzed roots.
                let scratch = Scratch::create()?;
                let source = scratch.0.join("subject.rs");
                let executable = scratch
                    .0
                    .join(format!("family_tests{}", std::env::consts::EXE_SUFFIX));
                std::fs::write(
                    &source,
                    if broken {
                        correct.replace(after, before)
                    } else {
                        correct.clone()
                    },
                )
                .map_err(|error| error.to_string())?;
                compiles(
                    run(
                        Path::new("rustc"),
                        &[
                            "--edition=2024".as_ref(),
                            "--crate-name=oracle_family_control".as_ref(),
                            "--test".as_ref(),
                            source.as_os_str(),
                            "-o".as_ref(),
                            executable.as_os_str(),
                        ],
                    )?,
                    "family fixture",
                )?;
                let listed = run(&executable, &["--list".as_ref()])?;
                assert!(listed.status.success(), "{family}/{case}: list failed");
                assert!(
                    String::from_utf8_lossy(&listed.stdout)
                        .trim_end()
                        .ends_with("1 test, 0 benchmarks"),
                    "{family}/{case}: unique runtime subject"
                );
                let runtime = run(&executable, &["--nocapture".as_ref()])?;
                let stdout = String::from_utf8_lossy(&runtime.stdout);
                assert!(
                    stdout.contains("running 1 test"),
                    "{family}/{case}: {stdout}"
                );
                assert_eq!(
                    runtime.status.code(),
                    Some(if broken && exposed { 101 } else { 0 }),
                    "{family}/{case}, broken={broken}: {stdout}; {}",
                    String::from_utf8_lossy(&runtime.stderr)
                );
                eprintln!(
                    "family-oracle runtime: {family}/{case}, broken={broken}, {}, executed=1",
                    runtime.status
                );
            }
        }
    }
    Ok(())
}

#[test]
fn equality_family_admission_preserves_mixed_oracle_identity_and_projection() -> Result<(), String>
{
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (family, strong, weak, binding) in [
        (
            "error_path",
            "assert_eq!(read_all(rdr).unwrap_err().kind(), io::ErrorKind::Other);",
            "assert!(read_all(rdr).is_err());",
            "let rdr = BrokenReader;",
        ),
        (
            "predicate",
            "assert_eq!(discounted_total(100, 100), 90);",
            "assert!(discounted_total(100, 100) > 0);",
            "",
        ),
    ] {
        let fixture = fixtures.join(format!("{family}_oracle_execution_direct"));
        let original = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
            .map_err(|error| error.to_string())?;
        let (prefix, _) = original
            .split_once("    #[test]\n")
            .ok_or("missing fixture test boundary")?;
        let deferred = format!("let _later = || {{ {binding} {strong} }};");
        for (body, expected_strength, exposed, copies) in [
            (format!("{strong}\n{deferred}"), "strong", true, 1),
            (format!("{deferred}\n{strong}"), "strong", true, 1),
            (format!("{weak}\n{deferred}"), "weak", false, 1),
            (format!("{deferred}\n{weak}"), "weak", false, 1),
            (format!("{deferred}\nassert_ready(true);"), "none", false, 1),
            (format!("{deferred}\n{strong}"), "strong", true, 10),
        ] {
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
            std::fs::copy(
                fixture.join("input/Cargo.toml"),
                scratch.0.join("Cargo.toml"),
            )
            .map_err(|error| error.to_string())?;
            let functions = (0..copies)
                .map(|index| {
                    format!("    #[test]\n    fn checks_{index}() {{\n{binding}\n{body}\n    }}\n")
                })
                .collect::<String>();
            std::fs::write(
                scratch.0.join("src/lib.rs"),
                format!("{prefix}    fn assert_ready(_: bool) {{}}\n{functions}}}\n"),
            )
            .map_err(|error| error.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|error| error.to_string())?;
            let findings = json["findings"]
                .as_array()
                .ok_or("missing findings")?
                .iter()
                .filter(|finding| finding["probe"]["family"] == family)
                .collect::<Vec<_>>();
            assert_eq!(findings.len(), 1, "{family}: {body}");
            let finding = findings[0];
            assert_eq!(
                finding["classification"] == "exposed",
                exposed,
                "{family}: {body}"
            );
            assert_eq!(
                finding["oracle_strength"], expected_strength,
                "{family}: {body}"
            );
            assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
            let related = finding["related_tests"]
                .as_array()
                .ok_or("missing related-test rows")?;
            assert_eq!(related.len(), copies.min(8), "{family}: {body}");
            assert!(
                related
                    .iter()
                    .all(|test| test["oracle_strength"] == expected_strength),
                "{family}: {body}"
            );
            if expected_strength == "none" {
                assert_eq!(finding["ripr"]["observe"]["state"], "no");
                assert_eq!(finding["ripr"]["discriminate"]["state"], "no");
            }
        }
    }
    Ok(())
}

#[test]
fn equality_execution_uses_statement_prefix_and_closure_invocation() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (family, assertion, binding, correct_expression, wrong_expression) in [
        (
            "return_value",
            "assert_eq!(weight(4), 12);",
            "",
            "input * 3",
            "input * 2",
        ),
        (
            "error_path",
            "assert_eq!(read_all(rdr).unwrap_err().kind(), io::ErrorKind::Other);",
            "let rdr = BrokenReader;",
            "rdr.read(&mut buf)?",
            "rdr.read(&mut buf).unwrap_or(0)",
        ),
        (
            "predicate",
            "assert_eq!(discounted_total(100, 100), 90);",
            "",
            "amount >= discount_threshold",
            "amount > discount_threshold",
        ),
    ] {
        let fixture = fixtures.join(if family == "return_value" {
            "owner_return_pin_direct".to_string()
        } else {
            format!("{family}_oracle_execution_direct")
        });
        let original = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
            .map_err(|error| error.to_string())?;
        let prefix = original
            .split_once("    #[test]\n")
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_else(|| {
                format!("{original}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n")
            });
        let direct = format!("{binding} {assertion}");
        // Keep this execution control independent of the narrower owner-pin
        // binding gate, which conservatively refuses any nested function.
        // The existing token-direct ReturnValue fixture uses this same shape.
        let helper_direct = if family == "return_value" {
            "let input = 4;\nassert_eq!(weight(input), 12);".to_string()
        } else {
            direct.clone()
        };
        for (case, body, exposed) in [
            ("later_return", format!("{direct}\nreturn;"), true),
            ("earlier_return", format!("return;\n{direct}"), false),
            (
                "invoked_before_return",
                format!("let check = || {{ {direct} }};\ncheck();\nreturn;"),
                true,
            ),
            (
                "invoked_after_return",
                format!("let check = || {{ {direct} }};\nreturn;\ncheck();"),
                false,
            ),
            (
                "closure_escape",
                format!("let check = || {{ return;\n{direct} }};\ncheck();"),
                false,
            ),
            (
                "nested_helper_return",
                format!("fn unrelated() {{ return; }}\n{helper_direct}"),
                true,
            ),
            (
                "disabled_nested_helper_return",
                format!("#[cfg(any())]\nfn unrelated() {{ return; }}\n{helper_direct}"),
                true,
            ),
            (
                "invoked_after_nested_helper_return",
                format!(
                    "fn unrelated() {{ return; }}\nlet check = || {{ {helper_direct} }};\ncheck();"
                ),
                true,
            ),
            (
                "invoked_after_disabled_nested_helper_return",
                format!(
                    "#[cfg(any())]\nfn unrelated() {{ return; }}\nlet check = || {{ {helper_direct} }};\ncheck();"
                ),
                true,
            ),
            (
                "async_return_direct",
                format!("let _future = async {{ return; }};\n{direct}"),
                true,
            ),
            (
                "async_return_invoked",
                format!(
                    "let _future = async {{ return; }};\nlet check = || {{ {direct} }}; check();"
                ),
                true,
            ),
            (
                "async_move_return_direct",
                format!("let _future = async move {{ return; }};\n{direct}"),
                true,
            ),
            (
                "async_move_return_invoked",
                format!(
                    "let _future = async move {{ return; }};\nlet check = || {{ {direct} }}; check();"
                ),
                true,
            ),
            (
                "async_return_then_outer_return",
                format!("let _future = async {{ return; }};\nreturn;\n{direct}"),
                false,
            ),
            (
                "assertion_inside_unpolled_async",
                format!("let _future = async {{ return;\n{direct} }};"),
                false,
            ),
            // `loop` runs its body at least once; the first iteration reaches
            // an assertion that no earlier `break`/`continue` can skip.
            (
                "loop_then_break",
                format!("loop {{ {direct}\nbreak; }}"),
                true,
            ),
            (
                "loop_counted_break_after",
                format!(
                    "let mut runs = 0u8;\nloop {{ {direct}\nruns += 1;\nif runs == 3 {{ break; }} }}"
                ),
                true,
            ),
            (
                "loop_exhaustive_match_break_after",
                format!(
                    "let mut step = 0u8;\nloop {{ {direct}\nmatch step.checked_add(64) {{ Some(next) => step = next, None => break }} }}"
                ),
                true,
            ),
            (
                "labeled_nested_loop",
                format!("'outer: loop {{ loop {{ {direct}\nbreak 'outer; }} }}"),
                true,
            ),
            (
                "break_before_assertion_in_loop",
                format!("let stop = true;\nloop {{ if stop {{ break; }}\n{direct} }}"),
                false,
            ),
            (
                "inner_loop_break_before_assertion",
                format!(
                    "let stop = true;\n'outer: loop {{ loop {{ if stop {{ break 'outer; }}\n{direct} }} }}"
                ),
                false,
            ),
            (
                "unrelated_break_before_assertion",
                format!("loop {{ break; }}\n{direct}"),
                true,
            ),
            (
                "zero_iteration_for",
                format!("for _ in 0..0 {{ {direct} }}"),
                false,
            ),
            (
                "zero_iteration_while",
                format!("while false {{ {direct} }}"),
                false,
            ),
            // A `for` over a non-empty constant-row table runs its body at
            // least once (#5328); an empty table or an earlier `continue`
            // can skip the assertion.
            (
                "constant_table_for",
                format!("for _ in [1u8, 2] {{ {direct} }}"),
                true,
            ),
            (
                "bound_constant_table_for",
                format!("let rows = [(1u8, Some(2u8)), (3, None)];\nfor _ in &rows {{ {direct} }}"),
                true,
            ),
            (
                "empty_constant_table_for",
                format!("let rows: [u8; 0] = [];\nfor _ in rows {{ {direct} }}"),
                false,
            ),
            (
                "continue_before_assertion_in_table",
                format!("for skip in [true] {{ if skip {{ continue; }}\n{direct} }}"),
                false,
            ),
        ] {
            let scratch = Scratch::create()?;
            let source = format!("{prefix}    #[test]\n    fn checks() {{\n{body}\n    }}\n}}\n");
            std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
            std::fs::copy(
                fixture.join("input/Cargo.toml"),
                scratch.0.join("Cargo.toml"),
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(scratch.0.join("src/lib.rs"), &source)
                .map_err(|error| error.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|error| error.to_string())?;
            let findings = json["findings"]
                .as_array()
                .ok_or("missing findings")?
                .iter()
                .filter(|finding| finding["probe"]["family"] == family)
                .collect::<Vec<_>>();
            assert_eq!(findings.len(), 1, "{family}/{case}");
            assert_eq!(
                findings[0]["classification"],
                if exposed {
                    "exposed"
                } else {
                    "reachable_unrevealed"
                },
                "{family}/{case}"
            );
            assert_eq!(
                findings[0]["oracle_strength"],
                if exposed { "strong" } else { "none" },
                "{family}/{case}"
            );
            assert_eq!(source.matches(correct_expression).count(), 1);
            for wrong in [false, true] {
                let runtime_source = if wrong {
                    source.replace(correct_expression, wrong_expression)
                } else {
                    source.clone()
                };
                source_runtime_control(
                    &runtime_source,
                    &format!("prefix runtime: {family}/{case}, wrong={wrong}"),
                    1,
                    wrong && exposed,
                )?;
            }
        }
    }
    Ok(())
}

/// `#[macro_use]` on a module only widens the textual scope of that module's
/// `macro_rules!`, so it is admitted when the module's source is scanned and
/// refused when its file is outside discovery. Each row records whether the
/// compiled test catches the wrong library; the unresolved row stays
/// refused although it would, which is the conservative direction.
#[test]
fn macro_use_module_admission_matches_runtime() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    let production = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let unrelated = "macro_rules! twice { ($value:expr) => { $value * 2 }; }";
    let shadowing = "macro_rules! assert_eq { ($left:expr, $right:expr $(,)?) => { let _ = (&$left, &$right); }; }";
    for (case, declaration, module, exposed, runtime_catches) in [
        (
            "inline_unrelated",
            format!("#[macro_use]\nmod helpers {{ {unrelated} }}"),
            None,
            true,
            true,
        ),
        (
            "inline_shadows_assert_eq",
            format!("#[macro_use]\nmod helpers {{ {shadowing} }}"),
            None,
            false,
            false,
        ),
        (
            "out_of_line_unrelated",
            "#[macro_use]\nmod helpers;".to_string(),
            Some(("helpers.rs", unrelated)),
            true,
            true,
        ),
        (
            "out_of_line_shadows_assert_eq",
            "#[macro_use]\nmod helpers;".to_string(),
            Some(("helpers.rs", shadowing)),
            false,
            false,
        ),
        (
            "undiscovered_module_file",
            "#[macro_use]\n#[path = \"target/helpers.rs\"]\nmod helpers;".to_string(),
            Some(("target/helpers.rs", unrelated)),
            false,
            true,
        ),
    ] {
        let source = format!(
            "{production}\n{declaration}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn checks() {{\n        assert_eq!(weight(4), 12);\n    }}\n}}\n"
        );
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::copy(
            fixture.join("input/Cargo.toml"),
            scratch.0.join("Cargo.toml"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        if let Some((relative, text)) = module {
            let path = scratch.0.join("src").join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(path, text).map_err(|error| error.to_string())?;
        }
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{case}");
        // RIPR-SPEC-0240: an unresolved `#[macro_use]` only may rebind
        // `assert_eq!`, and the runtime control catches the mutant, so ripr
        // reports the limit instead of a gap. A real rebinding stays a gap.
        assert_eq!(
            findings[0]["classification"],
            if exposed {
                "exposed"
            } else if runtime_catches {
                "static_unknown"
            } else {
                "reachable_unrevealed"
            },
            "{case}"
        );
        assert_eq!(
            findings[0]["static_limit_kind"].as_str(),
            (!exposed && runtime_catches).then_some("rust_assertion_context_unresolved"),
            "{case}"
        );
        assert_eq!(
            findings[0]["oracle_strength"],
            if exposed { "strong" } else { "none" },
            "{case}"
        );
        let modules: Vec<_> = module.into_iter().collect();
        for wrong in [false, true] {
            let runtime_source = if wrong {
                source.replace("input * 3", "input * 2")
            } else {
                source.clone()
            };
            source_runtime_control_with(
                &runtime_source,
                &modules,
                &format!("macro_use runtime: {case}, wrong={wrong}"),
                1,
                wrong && runtime_catches,
            )?;
        }
    }
    Ok(())
}

/// A macro whose arguments invoke `assert_eq!(..)` receives those tokens as
/// a pattern. `define!(assert_eq!(mod tests;))` expands to `macro_rules!
/// assert_eq { .. } mod tests;`, and the test module, from the same
/// expansion, compiles against the shadow (no E0659). So any mention in a
/// macro's arguments keeps the name ambiguous, and `no_implicit_prelude` or
/// `macro_use` hidden in macro arguments counts as written. Each row records
/// whether the compiled test catches the wrong library; the pass-through row
/// stays refused although it would, which is the conservative direction.
#[test]
fn wrapper_argument_admission_matches_runtime() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    let production = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let inline_tests = "#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn checks() {\n        assert_eq!(weight(4), 12);\n    }\n}\n";
    let test_file = "use super::*;\n#[test]\nfn checks() {\n    assert_eq!(weight(4), 12);\n}\n";
    let define = "macro_rules! define { ($name:ident ! ($($rest:tt)*)) => { macro_rules! $name { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; } $($rest)* }; }";
    let wrap = "macro_rules! wrap { ($($tokens:tt)*) => { $($tokens)* }; }";
    for (case, items, module, exposed, runtime_catches) in [
        (
            "pass_through",
            format!("{wrap}\nfn _uses() {{ wrap!(assert_eq!(1, 1)); }}\n{inline_tests}"),
            None,
            false,
            true,
        ),
        (
            "shadow_and_test_module_from_one_expansion",
            format!("{define}\ndefine!(assert_eq!(#[cfg(test)] mod tests;));"),
            Some(("tests.rs", test_file)),
            false,
            false,
        ),
        (
            "no_implicit_prelude_hidden_in_arguments",
            format!(
                "{wrap}\nmacro_rules! shadow {{ ($name:ident ! $($rest:tt)*) => {{ macro_rules! $name {{ ($a:expr, $b:expr) => {{{{ let _ = (&$a, &$b); }}}}; }} }}; }}\nshadow!(assert_eq!());\nwrap! {{ #[cfg(test)] #[no_implicit_prelude] mod tests; }}"
            ),
            Some(("tests.rs", test_file)),
            false,
            false,
        ),
    ] {
        let source = format!("{production}\n{items}\n");
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::copy(
            fixture.join("input/Cargo.toml"),
            scratch.0.join("Cargo.toml"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        if let Some((relative, text)) = module {
            std::fs::write(scratch.0.join("src").join(relative), text)
                .map_err(|error| error.to_string())?;
        }
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{case}");
        assert_ne!(
            findings[0]["classification"] == "exposed",
            !exposed,
            "{case}: {}",
            findings[0]["classification"]
        );
        let modules: Vec<_> = module.into_iter().collect();
        for wrong in [false, true] {
            let runtime_source = if wrong {
                source.replace("input * 3", "input * 2")
            } else {
                source.clone()
            };
            source_runtime_control_with(
                &runtime_source,
                &modules,
                &format!("wrapper runtime: {case}, wrong={wrong}"),
                1,
                wrong && runtime_catches,
            )?;
        }
    }
    Ok(())
}

/// `use pretty_assertions::assert_eq;` is the standard assertion only when
/// Cargo resolves that crate name to the registry package. A dependency key
/// renamed onto another package compiles the same source against a
/// different macro, which here never panics.
#[test]
fn drop_in_crate_admission_matches_cargo_resolution() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    let production = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let manifest = std::fs::read_to_string(fixture.join("input/Cargo.toml"))
        .map_err(|error| error.to_string())?;
    let source = format!(
        "{production}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    use pretty_assertions::assert_eq;\n    #[test]\n    fn checks() {{\n        assert_eq!(weight(4), 12);\n    }}\n}}\n"
    );
    for (case, dependency, exposed) in [
        ("registry", "pretty_assertions = \"1\"", true),
        (
            "renamed_package",
            "pretty_assertions = { package = \"fake-assertions\", path = \"../fake\" }",
            false,
        ),
    ] {
        // The fake package sits outside the analyzed root, as a registry or
        // path dependency would.
        let scratch = Scratch::create()?;
        let root = scratch.0.join("ws");
        let fake = scratch.0.join("fake");
        for directory in [root.join("src"), fake.join("src")] {
            std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        }
        let write = |path: PathBuf, text: &str| {
            std::fs::write(path, text).map_err(|error| error.to_string())
        };
        write(
            root.join("Cargo.toml"),
            // `[workspace]` keeps an enclosing checkout from claiming it.
            &format!("{manifest}\n[workspace]\n\n[dev-dependencies]\n{dependency}\n"),
        )?;
        write(root.join("src/lib.rs"), &source)?;
        write(
            fake.join("Cargo.toml"),
            "[package]\nname = \"fake-assertions\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write(
            fake.join("src/lib.rs"),
            "#[macro_export]\nmacro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; }\n",
        )?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{case}");
        assert_eq!(
            findings[0]["classification"] == "exposed",
            exposed,
            "{case}: {}",
            findings[0]["classification"]
        );
        if exposed {
            // Running the registry case needs the network; the shipped crate
            // panics exactly when the standard macro does.
            continue;
        }
        // Offline: the renamed package resolves from its path.
        write(
            root.join("src/lib.rs"),
            &source.replace("input * 3", "input * 2"),
        )?;
        let cargo = PathBuf::from(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        let manifest_path = root.join("Cargo.toml");
        let target_dir = scratch.0.join("target");
        let result = run(
            &cargo,
            &[
                "test".as_ref(),
                "--offline".as_ref(),
                "--quiet".as_ref(),
                "--manifest-path".as_ref(),
                manifest_path.as_os_str(),
                "--target-dir".as_ref(),
                target_dir.as_os_str(),
            ],
        )?;
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            result.status.success() && stdout.contains("1 passed; 0 failed;"),
            "{case}: the wrong library must pass under the renamed macro: {stdout}; {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}

/// A test in one workspace member that imports the owner from another
/// member (`use pricing::score;`) pins it only when the manifests bind that
/// crate name to the owner's package. When the dependency under that key is
/// another package (`package = "fake-pricing"` at another path), the same
/// import binds it, and a wrong owner then passes the test. So it does when
/// the owner's library re-exports a foreign item under the owner's name.
#[test]
fn member_crate_import_admission_matches_cargo_resolution() -> Result<(), String> {
    let owner = "pub fn score(points: i64) -> i64 {\n    3 * points\n}\n";
    let diff = |path: &str| {
        format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n pub fn score(points: i64) -> i64 {{\n-    points * 3\n+    3 * points\n }}\n"
        )
    };
    let test = "use pricing::score;\n\n#[test]\nfn score_triples_points() {\n    assert_eq!(score(7), 21);\n}\n";
    let package = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    };
    let path_dependency = "pricing = { path = \"../pricing\" }";
    // (case, orders' dependency, owner file, pricing's lib.rs when the owner
    // is elsewhere, pricing's dependencies, exposed)
    for (case, dependency, owner_path, library, pricing_dependencies, exposed) in [
        (
            "path_dependency",
            path_dependency,
            "pricing/src/lib.rs",
            None,
            "",
            true,
        ),
        (
            "renamed_package",
            "pricing = { package = \"fake-pricing\", path = \"../../fake\" }",
            "pricing/src/lib.rs",
            None,
            "",
            false,
        ),
        (
            "foreign_reexport",
            path_dependency,
            "pricing/src/a.rs",
            Some("pub mod a;\npub use fake_pricing::score;\n"),
            "fake-pricing = { path = \"../../fake\" }",
            false,
        ),
    ] {
        // The fake package sits outside the analyzed root, as a registry or
        // vendored dependency would.
        let scratch = Scratch::create()?;
        let root = scratch.0.join("ws");
        let fake = scratch.0.join("fake");
        for directory in [
            root.join("pricing/src"),
            root.join("orders/src"),
            root.join("orders/tests"),
            fake.join("src"),
        ] {
            std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        }
        let write = |path: PathBuf, text: &str| {
            std::fs::write(path, text).map_err(|error| error.to_string())
        };
        write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"pricing\", \"orders\"]\nresolver = \"2\"\n",
        )?;
        write(
            root.join("pricing/Cargo.toml"),
            &format!(
                "{}\n[dependencies]\n{pricing_dependencies}\n",
                package("pricing")
            ),
        )?;
        if let Some(library) = library {
            write(root.join("pricing/src/lib.rs"), library)?;
        }
        write(root.join(owner_path), owner)?;
        write(
            root.join("orders/Cargo.toml"),
            &format!("{}\n[dependencies]\n{dependency}\n", package("orders")),
        )?;
        write(root.join("orders/src/lib.rs"), "")?;
        write(root.join("orders/tests/orders.rs"), test)?;
        write(fake.join("Cargo.toml"), &package("fake-pricing"))?;
        write(
            fake.join("src/lib.rs"),
            "pub fn score(points: i64) -> i64 {\n    points * 3\n}\n",
        )?;
        let diff_file = scratch.0.join("diff.patch");
        write(diff_file.clone(), &diff(owner_path))?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(diff_file),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(report.findings.len(), 1, "{case}");
        assert_eq!(
            report.findings[0].class == ExposureClass::Exposed,
            exposed,
            "{case}: {:?}",
            report.findings[0].class
        );
        // A wrong owner fails the test exactly when the import binds it.
        write(
            root.join(owner_path),
            &owner.replace("3 * points", "2 * points"),
        )?;
        let cargo = PathBuf::from(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        let manifest_path = root.join("Cargo.toml");
        let target_dir = scratch.0.join("target");
        let result = run(
            &cargo,
            &[
                "test".as_ref(),
                "--offline".as_ref(),
                "--quiet".as_ref(),
                "--workspace".as_ref(),
                "--manifest-path".as_ref(),
                manifest_path.as_os_str(),
                "--target-dir".as_ref(),
                target_dir.as_os_str(),
            ],
        )?;
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stdout.contains("running 1 test"),
            "{case}: the cross-crate test must compile and run: {stdout}; {stderr}"
        );
        assert_eq!(
            !result.status.success(),
            exposed,
            "{case}: the wrong owner must fail exactly when the import binds it: {stdout}; {stderr}"
        );
    }
    Ok(())
}

#[test]
fn async_test_discovery_does_not_supply_execution_provenance() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust_async_fn_owner");
    let report = check_workspace(CheckInput {
        root: fixture.join("input"),
        diff_file: Some(fixture.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    assert_eq!(report.findings.len(), 1, "retain the actual async owner");
    assert_eq!(report.findings[0].class, ExposureClass::PropagationUnknown);
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
    let finding = &json["findings"][0];
    assert_eq!(finding["probe"]["family"], "predicate");
    assert_eq!(finding["ripr"]["observe"]["state"], "no");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "no");
    assert_eq!(finding["oracle_strength"], "none");
    let tests = finding["related_tests"]
        .as_array()
        .ok_or("missing indexed async test")?;
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0]["name"], "above_threshold_gets_reduced");
    assert_eq!(tests[0]["oracle_strength"], "none");
    // #5040 owns establishing the missing macro-binding/polling edge. The
    // independently executed Tokio removal controls show the real test works;
    // source discovery alone must not manufacture that execution provenance.
    Ok(())
}

fn source_runtime_control(
    source: &str,
    label: &str,
    expected_tests: usize,
    should_fail: bool,
) -> Result<(), String> {
    source_runtime_control_with(source, &[], label, expected_tests, should_fail)
}

/// `modules` are written next to the subject, as rustc resolves `mod name;`.
fn source_runtime_control_with(
    source: &str,
    modules: &[(&str, &str)],
    label: &str,
    expected_tests: usize,
    should_fail: bool,
) -> Result<(), String> {
    let runtime = Scratch::create()?;
    let path = runtime.0.join("subject.rs");
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    for (relative, module) in modules {
        let module_path = runtime.0.join(relative);
        if let Some(parent) = module_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(module_path, module).map_err(|error| error.to_string())?;
    }
    let executable = runtime
        .0
        .join(format!("test{}", std::env::consts::EXE_SUFFIX));
    compiles(
        run(
            Path::new("rustc"),
            &[
                "--edition=2024".as_ref(),
                "--test".as_ref(),
                path.as_os_str(),
                "-o".as_ref(),
                executable.as_os_str(),
            ],
        )?,
        label,
    )?;
    let listed = run(&executable, &["--list".as_ref()])?;
    assert!(listed.status.success(), "{label}");
    let suffix = format!(
        "{expected_tests} test{}, 0 benchmarks",
        if expected_tests == 1 { "" } else { "s" }
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout)
            .trim_end()
            .ends_with(&suffix),
        "{label}: wrong runtime subject inventory"
    );
    let result = run(&executable, &["--nocapture".as_ref()])?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains(&format!("running {expected_tests} test")),
        "{label}: {stdout}"
    );
    let failed = usize::from(should_fail);
    assert!(
        stdout.contains(&format!(
            "{} passed; {failed} failed;",
            expected_tests - failed
        )),
        "{label}: expected all declared subjects to execute: {stdout}"
    );
    assert_eq!(
        result.status.code(),
        Some(if should_fail { 101 } else { 0 }),
        "{label}: {stdout}; {}",
        String::from_utf8_lossy(&result.stderr)
    );
    eprintln!("{label}, {}, executed={expected_tests}", result.status);
    Ok(())
}

#[test]
fn predicate_pairing_cannot_reuse_refused_boundary_equalities() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/predicate_oracle_execution_direct");
    let original = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let (production, _) = original
        .split_once("#[cfg(test)]")
        .ok_or("missing test boundary")?;
    let boundary = "assert_eq!(discounted_total(100, 100), 90);";
    let far = "assert_eq!(discounted_total(101, 100), 91);";
    for separate in [true, false] {
        for (case, body, exposed) in [
            ("false_branch", format!("if false {{ {boundary} }}"), false),
            (
                "uncalled",
                format!("let _unused = || {{ {boundary} }};"),
                false,
            ),
            (
                "unpolled",
                format!("let _future = async {{ {boundary} }};"),
                false,
            ),
            ("direct", boundary.to_string(), true),
            (
                "invoked",
                format!("let check = || {{ {boundary} }}; check();"),
                true,
            ),
            (
                "removed",
                "let _ = discounted_total(100, 100);".to_string(),
                false,
            ),
        ] {
            let tests = if separate {
                format!("#[test]\nfn boundary() {{ {body} }}\n#[test]\nfn far() {{ {far} }}")
            } else {
                format!("#[test]\nfn mixed() {{ {body}\n{far} }}")
            };
            let source =
                format!("{production}#[cfg(test)]\nmod tests {{\nuse super::*;\n{tests}\n}}\n");
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
            std::fs::copy(
                fixture.join("input/Cargo.toml"),
                scratch.0.join("Cargo.toml"),
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(scratch.0.join("src/lib.rs"), &source)
                .map_err(|error| error.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|error| error.to_string())?;
            assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
            let findings = json["findings"]
                .as_array()
                .ok_or("missing findings")?
                .iter()
                .filter(|f| f["probe"]["family"] == "predicate")
                .collect::<Vec<_>>();
            assert_eq!(findings.len(), 1, "{case}, separate={separate}");
            let finding = findings[0];
            assert_eq!(
                finding["classification"],
                if exposed { "exposed" } else { "weakly_exposed" },
                "{case}, separate={separate}"
            );
            // The far oracle remains real strong evidence. It cannot donate
            // its strength to a refused boundary oracle or be erased wholesale.
            assert_eq!(finding["oracle_strength"], "strong", "{case}");
            assert_eq!(finding["ripr"]["observe"]["state"], "yes", "{case}");
            assert_eq!(
                finding["ripr"]["discriminate"]["state"],
                if exposed { "yes" } else { "weak" },
                "{case}"
            );
            if !exposed {
                assert!(
                    finding["ripr"]["discriminate"]["summary"]
                        .as_str()
                        .is_some_and(|s| s.contains("same_test_pairing_missing")
                            && !s.contains("different tests")),
                    "{case}"
                );
            }
            assert_eq!(source.matches("amount >= discount_threshold").count(), 1);
            for wrong in [false, true] {
                let runtime_source = if wrong {
                    source.replace(
                        "amount >= discount_threshold",
                        "amount > discount_threshold",
                    )
                } else {
                    source.clone()
                };
                source_runtime_control(
                    &runtime_source,
                    &format!("pairing runtime: {case}, separate={separate}, wrong={wrong}"),
                    if separate { 2 } else { 1 },
                    wrong && exposed,
                )?;
            }
        }
    }
    Ok(())
}

/// A bare `assert!` on a bool owner pins its whole result: the predicate that
/// is the owner's tail reads `exposed` only when one test pins both sides of
/// the boundary, and a shadowed or one-sided pin stays below `exposed`. Each
/// row runs the same test against the rewrite and a `<` mutant.
#[test]
fn bool_owner_assert_pin_matched_static_and_runtime_controls() -> Result<(), String> {
    let production = "pub fn gate(value: u32) -> bool {\n    10 <= value\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn gate(value: u32) -> bool {\n-    value >= 10\n+    10 <= value\n }\n";
    for (case, body, exposed) in [
        (
            "both_sides",
            "assert!(gate(10));\n        assert!(!gate(9));",
            true,
        ),
        (
            "let_bound_inputs",
            "let n = 10;\n        assert!(gate(n), \"ten passes\");\n        let m = 9;\n        assert!(!gate(m));",
            true,
        ),
        (
            "far_input_only",
            "assert!(gate(50));\n        assert!(!gate(3));",
            false,
        ),
        ("below_side_only", "assert!(!gate(9));", false),
        (
            "shadowed_owner",
            "let gate = |value: u32| value >= 10;\n        assert!(gate(10));\n        assert!(!gate(9));",
            false,
        ),
        (
            "boundary_only_in_message",
            "let got = gate(10);\n        assert!(gate(50), \"{got}\");",
            false,
        ),
        (
            "boundary_call_in_message",
            "assert!(gate(50), \"{}\", gate(10));",
            false,
        ),
        (
            "assert_eq_boundary_only_in_message",
            "let got = gate(10);\n        assert_eq!(gate(50), true, \"{got}\");",
            false,
        ),
        (
            "boundary_binding_only_in_operand_comment",
            "let got = gate(10);\n        assert_eq!(gate(50), true /* got */);",
            false,
        ),
        (
            "same_line_unasserted_boundary",
            "let got = gate(10); assert!(gate(50), \"{got}\");",
            false,
        ),
        (
            "same_line_unasserted_negated_boundary",
            "let _ = gate(10); assert!(!gate(3));",
            false,
        ),
        (
            "assert_eq_same_line_unasserted_boundary",
            "let _ = gate(10); assert_eq!(gate(50), true);",
            false,
        ),
        (
            "uncalled_closure",
            "let _check = || assert!(gate(10));\n        assert!(!gate(9));",
            false,
        ),
    ] {
        let tests = format!(
            "#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn gate_boundary() {{\n        {body}\n    }}\n}}\n"
        );
        let workspace = Scratch::create()?;
        std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            workspace.0.join("Cargo.toml"),
            "[package]\nname = \"bool_pin_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            workspace.0.join("src/lib.rs"),
            format!("{production}\n{tests}"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: workspace.0.clone(),
            diff_file: Some(workspace.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        let predicate = report
            .findings
            .iter()
            .find(|finding| finding.probe.family == ProbeFamily::Predicate)
            .ok_or(format!("{case}: no predicate finding on the changed tail"))?;
        assert_eq!(
            predicate.class == ExposureClass::Exposed,
            exposed,
            "{case}: {:?}",
            predicate.class
        );
        // The rewrite keeps the test green; only a discriminating test
        // notices the `<` mutant.
        for (label, tail, should_fail) in [
            ("rewrite", "10 <= value", false),
            ("mutant", "10 < value", exposed),
        ] {
            source_runtime_control(
                &format!("{}\n{tests}", production.replace("10 <= value", tail)),
                &format!("bool pin {case} {label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

/// #7083: a unit struct's own name types the receiver of a kept trait
/// default. An impl that overrides the default runs the override instead,
/// so its test passes a wrong default and must not be credited.
#[test]
fn unit_struct_receiver_matched_static_and_runtime_controls() -> Result<(), String> {
    let kept = "impl Counter for Unit {\n    fn step(&self) -> u32 {\n        2\n    }\n}\n";
    let overridden = "impl Counter for Unit {\n    fn step(&self) -> u32 {\n        2\n    }\n\n    fn advance(&self) -> u32 {\n        8\n    }\n}\n";
    for (case, unit_impl, body, exposed) in [
        (
            "unit_receiver",
            kept,
            "assert_eq!(Unit.advance(), 8);",
            true,
        ),
        (
            "let_bound_unit",
            kept,
            "let unit = Unit;\n        assert_eq!(unit.advance(), 8);",
            true,
        ),
        (
            "overridden_default",
            overridden,
            "assert_eq!(Unit.advance(), 8);",
            false,
        ),
    ] {
        let production = format!(
            "pub trait Counter {{\n    fn step(&self) -> u32;\n\n    fn advance(&self) -> u32 {{\n        4 * self.step()\n    }}\n}}\n\npub struct Unit;\n\n{unit_impl}"
        );
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,5 +3,5 @@ pub trait Counter {\n \n     fn advance(&self) -> u32 {\n-        self.step() * 4\n+        4 * self.step()\n     }\n }\n";
        let tests = format!(
            "#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn advances() {{\n        {body}\n    }}\n}}\n"
        );
        let workspace = Scratch::create()?;
        std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            workspace.0.join("Cargo.toml"),
            "[package]\nname = \"unit_receiver_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            workspace.0.join("src/lib.rs"),
            format!("{production}\n{tests}"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: workspace.0.clone(),
            diff_file: Some(workspace.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        let finding = report
            .findings
            .iter()
            .find(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 5
            })
            .ok_or(format!(
                "{case}: no return_value finding on the changed tail"
            ))?;
        assert_eq!(
            finding.class == ExposureClass::Exposed,
            exposed,
            "{case}: {:?}",
            finding.class
        );
        // The rewrite keeps the test green; only a test that runs the
        // default notices the `+` mutant.
        for (label, tail, should_fail) in [
            ("rewrite", "4 * self.step()", false),
            ("mutant", "4 + self.step()", exposed),
        ] {
            source_runtime_control(
                &format!("{}\n{tests}", production.replace("4 * self.step()", tail)),
                &format!("unit receiver {case} {label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

/// #7098 review: `Iterator::is_partitioned` is still unstable on the
/// supported 1.95 toolchain, so even a receiver that implements `Iterator`
/// runs the custom default. The mutant proves dispatch: it fails only if
/// the call reaches the changed default, and the static verdict credits it.
#[test]
fn unstable_is_partitioned_custom_default_matched_controls() -> Result<(), String> {
    let production = "pub trait Counter {\n    fn step(&self) -> u32;\n\n    fn is_partitioned(&self) -> u32 {\n        4 * self.step()\n    }\n}\n\npub struct Unit;\n\nimpl Counter for Unit {\n    fn step(&self) -> u32 {\n        2\n    }\n}\n\nimpl Iterator for Unit {\n    type Item = u32;\n\n    fn next(&mut self) -> Option<u32> {\n        None\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,5 +3,5 @@ pub trait Counter {\n \n     fn is_partitioned(&self) -> u32 {\n-        self.step() * 4\n+        4 * self.step()\n     }\n }\n";
    let tests = "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn advances() {\n        assert_eq!(Unit.is_partitioned(), 8);\n    }\n}\n";
    let workspace = Scratch::create()?;
    std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("Cargo.toml"),
        "[package]\nname = \"partitioned_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("src/lib.rs"),
        format!("{production}\n{tests}"),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: workspace.0.clone(),
        diff_file: Some(workspace.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = report
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 5
        })
        .ok_or("no return_value finding on the changed tail")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the custom default is the only callee: {:?}",
        finding.class
    );
    for (label, tail, should_fail) in [
        ("rewrite", "4 * self.step()", false),
        ("mutant", "4 + self.step()", true),
    ] {
        source_runtime_control(
            &format!("{}\n{tests}", production.replace("4 * self.step()", tail)),
            &format!("partitioned default {label}"),
            1,
            should_fail,
        )?;
    }
    Ok(())
}

/// #7098 review: rustc accepts `#[r#path = ..]`, and the shadow module it
/// names can define a rival `Unit` the test imports explicitly. The mutant
/// stays green (the call never reaches the production default), so the
/// static verdict must refuse the pin.
#[test]
fn raw_path_shadow_module_keeps_the_mutant_green() -> Result<(), String> {
    let production = "pub trait Counter {\n    fn step(&self) -> u32;\n\n    fn advance(&self) -> u32 {\n        4 * self.step()\n    }\n}\n\npub struct Unit;\n\nimpl Counter for Unit {\n    fn step(&self) -> u32 {\n        2\n    }\n}\n\n#[r#path = \"shadow.in\"]\nmod shadow;\n";
    let shadow = "pub struct Unit;\n\nimpl Unit {\n    pub fn advance(&self) -> u32 {\n        8\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,5 +3,5 @@ pub trait Counter {\n \n     fn advance(&self) -> u32 {\n-        self.step() * 4\n+        4 * self.step()\n     }\n }\n";
    let tests = "#[cfg(test)]\nmod tests {\n    use super::*;\n    use crate::shadow::Unit;\n\n    #[test]\n    fn advances() {\n        assert_eq!(Unit.advance(), 8);\n    }\n}\n";
    let workspace = Scratch::create()?;
    std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("Cargo.toml"),
        "[package]\nname = \"raw_path_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("src/lib.rs"),
        format!("{production}\n{tests}"),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(workspace.0.join("src/shadow.in"), shadow).map_err(|error| error.to_string())?;
    std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: workspace.0.clone(),
        diff_file: Some(workspace.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = report
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 5
        })
        .ok_or("no return_value finding on the changed tail")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "the raw-path shadow refuses the pin"
    );
    // Rewrite and mutant both stay green: the test calls the shadow's
    // inherent method either way, which is exactly why the pin refuses.
    for (label, tail) in [
        ("rewrite", "4 * self.step()"),
        ("mutant", "4 + self.step()"),
    ] {
        source_runtime_control_with(
            &format!("{}\n{tests}", production.replace("4 * self.step()", tail)),
            &[("shadow.in", shadow)],
            &format!("raw-path shadow {label}"),
            1,
            false,
        )?;
    }
    Ok(())
}

/// #7083 review: a lower-case `pub use std::u32::MAX;` re-export puts
/// `u32::MAX` under the unit struct's name, so `MAX.count_ones()` runs the
/// inherent `u32::count_ones` and a wrong default passes. The import refuses
/// the bare-name receiver.
#[test]
fn reexported_value_under_a_unit_struct_name_is_not_credited() -> Result<(), String> {
    let production = "pub trait Counter {\n    fn step(&self) -> u32;\n\n    fn count_ones(&self) -> u32 {\n        4 * self.step()\n    }\n}\n\n#[allow(non_camel_case_types)]\npub struct MAX;\n\nimpl Counter for MAX {\n    fn step(&self) -> u32 {\n        8\n    }\n}\n\npub mod limits;\n";
    let limits = "#![allow(deprecated)]\npub use std::u32::MAX;\n";
    let tests = "#[cfg(test)]\nmod tests {\n    use super::Counter;\n    use crate::limits::MAX;\n\n    #[test]\n    fn counts() {\n        assert_eq!(MAX.count_ones(), 32);\n    }\n}\n";
    unit_struct_shadow_is_not_credited(
        "reexported value",
        production,
        &[("limits.rs", limits)],
        tests,
    )
}

/// #7098 review: `use std::u32 as nums;` at the crate root and an unrelated
/// `mod nums` elsewhere. `use crate::nums::*` reaches std's `MAX`, so a
/// same-named module declared somewhere else must not admit the glob.
#[test]
fn aliased_outside_module_beside_a_same_named_module_is_not_credited() -> Result<(), String> {
    let production = "#![allow(deprecated)]\npub trait Counter {\n    fn step(&self) -> u32;\n\n    fn count_ones(&self) -> u32 {\n        4 * self.step()\n    }\n}\n\n#[allow(non_camel_case_types)]\npub struct MAX;\n\nimpl Counter for MAX {\n    fn step(&self) -> u32 {\n        8\n    }\n}\n\nuse std::u32 as nums;\npub mod other {\n    pub mod nums {}\n}\n";
    let tests = "#[cfg(test)]\nmod tests {\n    use super::Counter;\n    use crate::nums::*;\n\n    #[test]\n    fn counts() {\n        assert_eq!(MAX.count_ones(), 32);\n    }\n}\n";
    unit_struct_shadow_is_not_credited("aliased module glob", production, &[], tests)
}

/// The changed `Counter::count_ones` default on line 5 reads weakly_exposed,
/// and the mutant passes because the test's `MAX` is not the unit struct.
fn unit_struct_shadow_is_not_credited(
    label: &str,
    production: &str,
    modules: &[(&str, &str)],
    tests: &str,
) -> Result<(), String> {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,5 +3,5 @@ pub trait Counter {\n \n     fn count_ones(&self) -> u32 {\n-        self.step() * 4\n+        4 * self.step()\n     }\n }\n";
    let changed_line = production
        .lines()
        .position(|line| line.contains("4 * self.step()"))
        .map(|index| index + 1)
        .ok_or("production has no changed tail")?;
    let diff = diff.replace(
        "@@ -3,5 +3,5",
        &format!("@@ -{0},5 +{0},5", changed_line - 2),
    );
    let workspace = Scratch::create()?;
    std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("Cargo.toml"),
        "[package]\nname = \"unit_struct_shadow_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        workspace.0.join("src/lib.rs"),
        format!("{production}\n{tests}"),
    )
    .map_err(|error| error.to_string())?;
    for (name, text) in modules {
        std::fs::write(workspace.0.join("src").join(name), text)
            .map_err(|error| error.to_string())?;
    }
    std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: workspace.0.clone(),
        diff_file: Some(workspace.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = report
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue
                && finding.probe.location.line == changed_line
        })
        .ok_or("no return_value finding on the changed tail")?;
    // Refused receiver typing leaves the proximity-only gap, not a credit.
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{label}: {:?}",
        finding.class
    );
    // The mutant passes: the test never runs the default.
    for (variant, tail) in [
        ("rewrite", "4 * self.step()"),
        ("mutant", "4 + self.step()"),
    ] {
        source_runtime_control_with(
            &format!("{}\n{tests}", production.replace("4 * self.step()", tail)),
            modules,
            &format!("{label} {variant}"),
            1,
            false,
        )?;
    }
    Ok(())
}

/// A block-level `extern crate thread;` shadows the module's
/// `use std::thread;` (#6966 review). When a dependency is named `thread`,
/// `thread::spawn` is that crate's, and a fake `spawn` that never runs the
/// closure lets a wrong owner pass. ripr refuses the credit.
#[test]
fn an_extern_crate_named_thread_refuses_spawned_thread_credit() -> Result<(), String> {
    fake_thread_crate_refuses_credit(
        "#[cfg(test)]\nmod tests {\n    use super::*;\n    use std::thread;\n    #[test]\n    fn weight_in_worker() {\n        extern crate thread;\n        thread::spawn(|| assert_eq!(weight(4), 12)).join().unwrap();\n    }\n}\n",
    )
}

/// In edition 2018 and later, `::thread::spawn` names the extern crate
/// `thread`, not the `use std::thread;` import (#7022 review). The fake
/// crate's `spawn` never runs the closure, so ripr refuses the credit.
#[test]
fn a_leading_colon_thread_path_refuses_spawned_thread_credit() -> Result<(), String> {
    fake_thread_crate_refuses_credit(
        "#[cfg(test)]\nmod tests {\n    use super::*;\n    use std::thread;\n    #[test]\n    fn weight_in_worker() {\n        let _ = thread::current();\n        ::thread::spawn(|| assert_eq!(weight(4), 12)).join().unwrap();\n    }\n}\n",
    )
}

/// Runs `tests` against a dev-dependency named `thread` whose `spawn` never
/// runs its closure: ripr must refuse the credit and the mutant must survive.
fn fake_thread_crate_refuses_credit(tests: &str) -> Result<(), String> {
    let production = "pub fn weight(input: u32) -> u32 {\n    3 * input\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn weight(input: u32) -> u32 {\n-    input * 3\n+    3 * input\n }\n";
    let scratch = Scratch::create()?;
    let root = scratch.0.join("ws");
    let fake = scratch.0.join("fake");
    for directory in [root.join("src"), fake.join("src")] {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let write =
        |path: PathBuf, text: &str| std::fs::write(path, text).map_err(|error| error.to_string());
    write(
        root.join("Cargo.toml"),
        "[package]\nname = \"thread_extern_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dev-dependencies]\nthread = { package = \"fake-thread\", path = \"../fake\" }\n",
    )?;
    write(root.join("src/lib.rs"), &format!("{production}{tests}"))?;
    write(root.join("diff.patch"), diff)?;
    write(
        fake.join("Cargo.toml"),
        "[package]\nname = \"fake-thread\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    write(
        fake.join("src/lib.rs"),
        "pub struct Handle;\nimpl Handle {\n    pub fn join(self) -> Result<(), ()> {\n        Ok(())\n    }\n}\npub fn spawn<F: FnOnce()>(_f: F) -> Handle {\n    Handle\n}\n",
    )?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.probe.family == ProbeFamily::ReturnValue)
        .ok_or("no return-value finding")?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    // Offline: the mutant passes because the fake `spawn` never runs the
    // closure, so the credit would have been false.
    write(
        root.join("src/lib.rs"),
        &format!("{}{tests}", production.replace("3 * input", "input * 2")),
    )?;
    let cargo = PathBuf::from(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    let manifest_path = root.join("Cargo.toml");
    let target_dir = scratch.0.join("target");
    let result = run(
        &cargo,
        &[
            "test".as_ref(),
            "--offline".as_ref(),
            "--quiet".as_ref(),
            "--manifest-path".as_ref(),
            manifest_path.as_os_str(),
            "--target-dir".as_ref(),
            target_dir.as_os_str(),
        ],
    )?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success() && stdout.contains("1 passed; 0 failed;"),
        "the mutant must survive under the fake crate: {stdout}; {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

/// A `use super::*;` at the top of an out-of-line test module globs a
/// parent in another file, which may declare its own `std` (#7022 review).
/// The fake `std::thread::spawn` never runs the closure, so a wrong owner
/// passes; ripr refuses the credit.
#[test]
fn an_out_of_line_super_glob_refuses_spawned_thread_credit() -> Result<(), String> {
    let production = "pub fn weight(input: u32) -> u32 {\n    3 * input\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn weight(input: u32) -> u32 {\n-    input * 3\n+    3 * input\n }\n";
    let parent = "pub mod std {\n    pub mod thread {\n        pub struct Handle;\n        impl Handle {\n            pub fn join(self) -> Result<(), ()> {\n                Ok(())\n            }\n        }\n        pub fn spawn<F: FnOnce()>(_f: F) -> Handle {\n            Handle\n        }\n    }\n}\n#[cfg(test)]\nmod tests;\n";
    let tests = "use super::*;\n#[test]\nfn weight_in_worker() {\n    std::thread::spawn(|| assert_eq!(weight(4), 12)).join().unwrap();\n}\n";
    let scratch = Scratch::create()?;
    let root = scratch.0.join("ws");
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let write =
        |path: PathBuf, text: &str| std::fs::write(path, text).map_err(|error| error.to_string());
    write(
        root.join("Cargo.toml"),
        "[package]\nname = \"thread_glob_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(root.join("src/lib.rs"), &format!("{production}{parent}"))?;
    write(root.join("src/tests.rs"), tests)?;
    write(root.join("diff.patch"), diff)?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.probe.family == ProbeFamily::ReturnValue)
        .ok_or("no return-value finding")?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    // The mutant passes: the glob's `std` shadows the standard library.
    write(
        root.join("src/lib.rs"),
        &format!("{}{parent}", production.replace("3 * input", "input * 2")),
    )?;
    let cargo = PathBuf::from(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    let manifest_path = root.join("Cargo.toml");
    let target_dir = scratch.0.join("target");
    let result = run(
        &cargo,
        &[
            "test".as_ref(),
            "--offline".as_ref(),
            "--quiet".as_ref(),
            "--manifest-path".as_ref(),
            manifest_path.as_os_str(),
            "--target-dir".as_ref(),
            target_dir.as_os_str(),
        ],
    )?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success() && stdout.contains("1 passed; 0 failed;"),
        "the mutant must survive under the parent's fake std: {stdout}; {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

/// #6966: an assertion inside a spawned thread earns exact credit only where
/// the thread's panic reaches the test thread. Each row runs the same test
/// against the rewrite and an `input * 2` mutant; `kills` is the runtime
/// truth, and a refused row that still kills is a conservative refusal.
#[test]
fn spawned_thread_assertion_matched_static_and_runtime_controls() -> Result<(), String> {
    let production = "pub fn weight(input: u32) -> u32 {\n    3 * input\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn weight(input: u32) -> u32 {\n-    input * 3\n+    3 * input\n }\n";
    let assertion = "assert_eq!(weight(4), 12)";
    let fake_thread = "mod thread {\n        pub struct Handle;\n        impl Handle {\n            pub fn join(self) -> Result<(), ()> {\n                Ok(())\n            }\n        }\n        pub fn spawn<F: FnOnce()>(_f: F) -> Handle {\n            Handle\n        }\n    }\n";
    let fake_module = "pub mod thread {\n    pub struct Handle;\n    impl Handle {\n        pub fn join(self) -> Result<(), ()> {\n            Ok(())\n        }\n    }\n    pub fn spawn<F: FnOnce()>(_f: F) -> Handle {\n        Handle\n    }\n}\npub mod std {\n    pub use super::thread;\n}\n#[allow(unused_macros)]\nmacro_rules! setup {\n    () => {\n        use crate::fake::*;\n    };\n}\n";
    for (case, prelude, items, body, exposed, kills) in [
        (
            "joined_unwrap",
            "",
            "",
            format!("std::thread::spawn(|| {assertion}).join().unwrap();"),
            true,
            true,
        ),
        (
            "joined_expect_move",
            "",
            "",
            format!("::std::thread::spawn(move || {{ {assertion}; }}).join().expect(\"worker\");"),
            true,
            true,
        ),
        (
            "imported_module",
            "",
            "use std::thread;",
            format!("thread::spawn(|| {assertion}).join().unwrap();"),
            true,
            true,
        ),
        (
            "scoped_statement",
            "",
            "",
            format!(
                "std::thread::scope(|s| {{\n            s.spawn(|| {assertion});\n        }});"
            ),
            true,
            true,
        ),
        (
            "scoped_joined",
            "",
            "",
            format!(
                "std::thread::scope(|s| {{\n            s.spawn(|| {assertion}).join().unwrap();\n        }});"
            ),
            true,
            true,
        ),
        (
            "scope_body",
            "",
            "",
            format!("std::thread::scope(|_| {assertion});"),
            true,
            true,
        ),
        (
            "self_in_unrelated_use_list",
            "",
            "use std::fmt::{self, Write as _};\n    fn _render() -> fmt::Result {\n        let mut text = String::new();\n        write!(text, \"x\")\n    }",
            format!("std::thread::spawn(|| {assertion}).join().unwrap();"),
            true,
            true,
        ),
        (
            "item_macro_import",
            "#[macro_use]\nmod fake;\n",
            "setup!();",
            format!("std::thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            "detached",
            "",
            "",
            format!("std::thread::spawn(|| {assertion});"),
            false,
            false,
        ),
        (
            "join_ok",
            "",
            "",
            format!("std::thread::spawn(|| {assertion}).join().ok();"),
            false,
            false,
        ),
        (
            "join_discarded",
            "",
            "",
            format!("let _ = std::thread::spawn(|| {assertion}).join();"),
            false,
            false,
        ),
        (
            "scoped_join_discarded",
            "",
            "",
            format!(
                "std::thread::scope(|s| {{\n            let _ = s.spawn(|| {assertion}).join();\n        }});"
            ),
            false,
            false,
        ),
        (
            "local_thread_module",
            "use std::thread;\n",
            fake_thread,
            format!("thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            "unimported_thread_path",
            "",
            fake_thread,
            format!("thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            "imported_other_thread",
            "mod fake;\n",
            "use crate::fake::thread;",
            format!("thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            // The block's nested list re-imports `thread` from the fake
            // module, shadowing the module-level `use std::thread;`.
            "nested_list_shadows_thread",
            "mod fake;\n",
            "use std::thread;",
            format!(
                "use crate::fake::{{std::thread}};\n        thread::spawn(|| {assertion}).join().unwrap();"
            ),
            false,
            false,
        ),
        (
            "imported_other_std",
            "mod fake;\n",
            "use crate::fake::std;",
            format!("std::thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            "glob_other_std",
            "mod fake;\n",
            "use crate::fake::*;",
            format!("std::thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            false,
        ),
        (
            "import_in_other_module",
            "use std::thread;\n",
            "",
            format!("thread::spawn(|| {assertion}).join().unwrap();"),
            false,
            true,
        ),
        (
            "scope_parameter_pattern",
            "",
            "",
            format!(
                "std::thread::scope(|ref s| {{\n            s.spawn(|| {assertion});\n        }});"
            ),
            false,
            true,
        ),
        (
            "bound_handle",
            "",
            "",
            format!(
                "let handle = std::thread::spawn(|| {assertion});\n        handle.join().unwrap();"
            ),
            false,
            true,
        ),
    ] {
        let tests = format!(
            "#[cfg(test)]\nmod tests {{\n    use super::*;\n    {items}\n    #[test]\n    fn weight_in_worker() {{\n        {body}\n    }}\n}}\n"
        );
        let workspace = Scratch::create()?;
        std::fs::create_dir(workspace.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            workspace.0.join("Cargo.toml"),
            "[package]\nname = \"thread_pin_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        // The prelude goes after the changed function so the diff lines hold.
        std::fs::write(
            workspace.0.join("src/lib.rs"),
            format!("{production}{prelude}\n{tests}"),
        )
        .map_err(|error| error.to_string())?;
        // `fake.rs` exports a `thread` (and a `std::thread`) whose `spawn`
        // never runs the closure; ripr reads it as a separate file.
        let modules: &[(&str, &str)] = if prelude.contains("mod fake;") {
            &[("fake.rs", fake_module)]
        } else {
            &[]
        };
        for (relative, module) in modules {
            std::fs::write(workspace.0.join("src").join(relative), module)
                .map_err(|error| error.to_string())?;
        }
        std::fs::write(workspace.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: workspace.0.clone(),
            diff_file: Some(workspace.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.probe.family == ProbeFamily::ReturnValue)
            .ok_or(format!("{case}: no return-value finding"))?;
        assert_eq!(
            finding.class == ExposureClass::Exposed,
            exposed,
            "{case}: {:?}",
            finding.class
        );
        for (label, tail, should_fail) in [
            ("rewrite", "3 * input", false),
            ("mutant", "input * 2", kills),
        ] {
            source_runtime_control_with(
                &format!(
                    "{}{prelude}\n{tests}",
                    production.replace("3 * input", tail)
                ),
                modules,
                &format!("thread pin {case} {label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

/// A constant-row table feeds the owner one input row per table row, and
/// cells of one row stay together (#5328). The predicate reads `exposed`
/// exactly when a row reaches the changed `>=` boundary, which is when the
/// compiled test catches the wrong `>`.
#[test]
fn constant_row_table_pairs_each_row_with_its_boundary_input() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/predicate_oracle_execution_direct");
    let original = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let Some((prefix, _)) = original.split_once("    #[test]\n") else {
        return Err("fixture lost its test".to_string());
    };
    let pinned = "assert_eq!(discounted_total(amount, threshold), want);";
    for (case, body, exposed) in [
        (
            "boundary_row",
            "for (amount, want) in [(99, 99), (100, 90), (150, 140)] {\nassert_eq!(discounted_total(amount, 100), want);\n}"
                .to_string(),
            true,
        ),
        (
            "no_boundary_row",
            "let rows = [(99, 99), (150, 140)];\nfor (amount, want) in rows {\nassert_eq!(discounted_total(amount, 100), want);\n}"
                .to_string(),
            false,
        ),
        (
            "both_columns_meet_in_one_row",
            format!("for (amount, threshold, want) in [(150, 100, 140), (100, 100, 90)] {{\n{pinned}\n}}"),
            true,
        ),
        // Each column holds 100, but never in the same row.
        (
            "columns_meet_only_across_rows",
            format!("for (amount, threshold, want) in [(100, 99, 90), (99, 100, 99)] {{\n{pinned}\n}}"),
            false,
        ),
        // An unasserted call meets the boundary, and the asserted table
        // never does: the columns must not meet as unordered sets.
        (
            "unasserted_boundary_call_beside_a_table",
            format!("let _ = discounted_total(100, 100);\nfor (amount, threshold, want) in [(100, 99, 90), (99, 100, 99)] {{\n{pinned}\n}}"),
            false,
        ),
    ] {
        let scratch = Scratch::create()?;
        let source = format!("{prefix}    #[test]\n    fn checks() {{\n{body}\n    }}\n}}\n");
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::copy(
            fixture.join("input/Cargo.toml"),
            scratch.0.join("Cargo.toml"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .filter(|finding| finding["probe"]["family"] == "predicate")
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 1, "{case}");
        assert_eq!(
            findings[0]["classification"] == "exposed",
            exposed,
            "{case}: {}",
            findings[0]["classification"]
        );
        let correct = "amount >= discount_threshold";
        assert_eq!(source.matches(correct).count(), 1);
        for wrong in [false, true] {
            let runtime_source = if wrong {
                source.replace(correct, "amount > discount_threshold")
            } else {
                source.clone()
            };
            source_runtime_control(
                &runtime_source,
                &format!("table rows: {case}, wrong={wrong}"),
                1,
                wrong && exposed,
            )?;
        }
    }
    Ok(())
}
