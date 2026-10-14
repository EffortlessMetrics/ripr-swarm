//! Literal public-path and compiling runtime controls for #7966.

use super::{Scratch, compiles, run};
use ripr::{
    CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace, render_check,
};
use std::path::Path;

// Independent literals: case, observation credited, wrong value rejected,
// deleted push rejected. Custom length equality has deliberately asymmetric grip.
const CASES: &[(&str, bool, bool, bool)] = &[
    ("tuple", true, true, true),
    ("named", true, true, true),
    ("plain", true, true, true),
    ("wrong_field", false, false, false),
    ("wrong_receiver", false, false, false),
    ("unrelated", false, false, false),
    ("rebound", false, false, false),
    ("mutated", false, false, false),
    ("raw_mutated", false, false, false),
    ("raw_rebound", false, false, false),
    ("alias", false, true, true),
    ("ambiguous", false, false, false),
    ("guarded", false, false, false),
    ("computed_expected", false, false, false),
    ("custom_equality", false, false, true),
    ("coerced_equality", false, false, true),
    ("constructor_shadow", false, false, true),
    ("qualified_vec", true, true, true),
    ("root_shadow", false, false, true),
    ("raw_type_alias", false, false, true),
    ("raw_imported_vec", false, false, true),
    ("imported_constructor", false, false, true),
    ("macro_constructor", false, false, true),
];

#[test]
fn field_projected_collection_public_check_pins_exact_observation() -> Result<(), String> {
    for &(case, credited, _, _) in CASES {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../fixtures/collection_argument_{case}"));
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
        let effects: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                matches!(
                    finding.probe.family,
                    ProbeFamily::CallDeletion | ProbeFamily::SideEffect
                )
            })
            .collect();
        assert_eq!(effects.len(), 2, "{case}: nonempty intended probe families");
        for finding in effects {
            assert_eq!(
                finding.probe.expression.trim().trim_end_matches(';'),
                "v.push(x * 1)",
                "{case}: exact changed sink"
            );
            assert_eq!(
                finding.class,
                if credited {
                    ExposureClass::Exposed
                } else {
                    ExposureClass::WeaklyExposed
                },
                "{case}: {finding:?}"
            );
            assert_eq!(finding.related_tests.len(), 1, "{case}");
            assert_eq!(
                finding.related_tests[0].name, "records_into_the_bag",
                "{case}"
            );
            assert_eq!(
                finding.related_tests[0].file,
                Path::new("src/lib.rs"),
                "{case}"
            );
            assert_eq!(
                finding.related_tests[0]
                    .relation_reason
                    .map(|reason| reason.as_str()),
                Some("direct_owner_call"),
                "{case}: preserve call identity"
            );
        }
    }
    Ok(())
}

#[test]
fn field_projected_collection_compiling_behavioral_mutants() -> Result<(), String> {
    for &(case, _, catches_wrong_value, catches_deleted_push) in CASES {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../fixtures/collection_argument_{case}/input/src/lib.rs"
        ));
        let original = std::fs::read_to_string(&fixture).map_err(|error| error.to_string())?;
        assert_eq!(original.matches("v.push(x * 1);").count(), 1, "{case}");
        for (version, replacement, expected_success) in [
            ("neutral", "v.push(x * 1);", true),
            ("wrong_value", "v.push(x + 1);", !catches_wrong_value),
            ("deleted_push", "let _ = x;", !catches_deleted_push),
        ] {
            let scratch = Scratch::create()?;
            let source = scratch.0.join("lib.rs");
            for companion in std::fs::read_dir(fixture.parent().ok_or("fixture parent missing")?)
                .map_err(|error| error.to_string())?
            {
                let companion = companion.map_err(|error| error.to_string())?;
                if companion.path().extension() == Some("rs".as_ref())
                    && companion.file_name() != "lib.rs"
                {
                    std::fs::copy(companion.path(), scratch.0.join(companion.file_name()))
                        .map_err(|error| error.to_string())?;
                }
            }
            let executable = scratch
                .0
                .join(format!("control{}", std::env::consts::EXE_SUFFIX));
            std::fs::write(&source, original.replace("v.push(x * 1);", replacement))
                .map_err(|error| error.to_string())?;
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        "--edition=2024".as_ref(),
                        "--test".as_ref(),
                        source.as_os_str(),
                        "-o".as_ref(),
                        executable.as_os_str(),
                    ],
                )?,
                &format!("{case}/{version}"),
            )?;
            let result = run(&executable, &["--nocapture".as_ref()])?;
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(
                stdout.contains("running 1 test") && stdout.contains("records_into_the_bag"),
                "{case}/{version}: intended test must execute: {stdout}"
            );
            assert!(stdout.contains("0 ignored"), "{case}/{version}: {stdout}");
            assert_eq!(
                result.status.success(),
                expected_success,
                "{case}/{version}: {stdout} {stderr}"
            );
            if !expected_success {
                assert!(
                    stderr.contains("assertion `left == right` failed"),
                    "{case}/{version}: intended behavioral failure: {stderr}"
                );
                assert!(stdout.contains("1 failed"), "{case}/{version}: {stdout}");
            }
        }
    }
    Ok(())
}
