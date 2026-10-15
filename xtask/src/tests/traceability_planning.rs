//! Accepted document disposition is independent of current evidence existence (#8002).

use std::fs;
use std::path::Path;

use super::with_temp_cwd;
use crate::{check_spec_format, check_spec_numbering, check_traceability};

const SPEC: &str = "docs/specs/RIPR-SPEC-9999-obligation.md";
const PLANNED: &str = "Planned tests:\n\n- `boundary_disclosure_is_not_promoted`\n";

fn fixture(
    root: &Path,
    status: &str,
    mapping: &str,
    tests: &str,
    fixtures: &str,
) -> Result<(), String> {
    for directory in ["docs/specs", ".ripr", "tests", "fixtures"] {
        fs::create_dir_all(root.join(directory)).map_err(|error| error.to_string())?;
    }
    let text = format!(
        "# RIPR-SPEC-9999: Evidence obligation\n\nStatus: {status}\n\n\
         ## Problem\nKeep acceptance independent.\n\n\
         ## Behavior\nDisclose an unknown boundary.\n\n\
         ## Required Evidence\nA boundary discriminator is owed.\n\n\
         ## Non-Goals\nNo implementation or support promotion.\n\n\
         ## Acceptance Examples\nMissing proof stays missing.\n\n\
         ## Test Mapping\n{mapping}\n\
         ## Implementation Mapping\nNo implementation claim.\n\n\
         ## Metrics\nboundary_disclosure\n"
    );
    fs::write(root.join(SPEC), text).map_err(|error| error.to_string())?;
    fs::write(
        root.join("docs/specs/README.md"),
        "[RIPR-SPEC-9999](RIPR-SPEC-9999-obligation.md)\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("tests/current.rs"),
        "// File reference only; no symbol or execution proof.\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("fixtures/current.txt"),
        "Fixture reference only.\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join(".ripr/traceability.toml"), format!(
        "[[behavior]]\nid = \"RIPR-SPEC-9999\"\nname = \"Evidence obligation\"\n\
         spec = \"{SPEC}\"\ntests = {tests}\nfixtures = {fixtures}\ncode = []\noutputs = []\nmetrics = [\"boundary_disclosure\"]\n"
    )).map_err(|error| error.to_string())?;
    Ok(())
}

#[test]
fn accepted_planned_obligation_reaches_real_checks_without_fake_current_path() -> Result<(), String>
{
    with_temp_cwd("trace-planned-obligation", |root| {
        fixture(root, "accepted", PLANNED, "[]", "[]")?;
        check_spec_format()?;
        check_spec_numbering()?;
        check_traceability()?;
        let report = fs::read_to_string("target/ripr/reports/traceability.md")
            .map_err(|error| error.to_string())?;
        assert!(
            report.contains(
                "explicit planned test mapping and no current test or fixture references"
            )
        );
        assert!(
            report.contains("obligation, not implemented behavior or executed/sufficient evidence")
        );
        assert!(
            !root
                .join("tests/boundary_disclosure_is_not_promoted.rs")
                .exists()
        );
        Ok(())
    })
}

#[test]
fn accepted_empty_placeholder_fenced_and_unrelated_plans_remain_failures() -> Result<(), String> {
    let mappings = [
        "",
        "Planned tests:\n",
        "Planned tests:\n- TODO: add tests\n",
        "Planned tests:\n- TBD\n",
        "Planned tests:\n- `TBD.`\n",
        "Planned tests:\n- TODO.\n",
        "Planned tests:\n- None.\n",
        "Planned tests:\n- N/A.\n",
        "Planned tests:\n- planned.\n",
        "Planned tests:\n- ...\n",
        "Planned tests:\n- !!!\n",
        "Planned tests:\n- [ ]\n",
        "Planned tests:\n- [x]\n",
        "Planned tests:\n- ***\n",
        "Planned tests:\n- [ ] **TODO**\n",
        "Planned tests:\n- None yet\n",
        "Planned tests:\n- planned\n",
        "Planned tests:\n- <test name>\n",
        "```text\nPlanned tests:\n- boundary_obligation\n```\n",
        "````text\n```\nPlanned tests:\n- boundary_obligation\n````\n",
        "    Planned tests:\n    - boundary_obligation\n",
        "```text\n```not-a-close\nPlanned tests:\n- boundary_obligation\n```\n",
        "<!--\nPlanned tests:\n- boundary_obligation\n-->\n",
        "Visible prose <!--\nPlanned tests:\n- boundary_obligation\n-->\n",
        "Visible prose <!-- closed --> <!--\nPlanned tests:\n- boundary_obligation\n-->\n",
        "Visible prose <!--\n```\n-->\n```\nPlanned tests:\n- boundary_obligation\n```\n",
        "A planned release is not a test mapping.\n",
        "Planned tests:\n\n## Implementation Mapping\n- boundary_obligation\n",
        "\n## Other Section\nPlanned tests:\n- boundary_obligation\n",
    ];
    for mapping in mappings {
        with_temp_cwd("trace-planned-negative", |root| -> Result<(), String> {
            fixture(root, "accepted", mapping, "[]", "[]")?;
            let result = check_traceability();
            assert!(
                result.is_err(),
                "unsupported mapping cleared acceptance: {mapping:?}"
            );
            let report = fs::read_to_string("target/ripr/reports/traceability.md")
                .map_err(|error| error.to_string())?;
            assert!(
                report.contains(
                    "no current test or fixture mapping or explicit planned test mapping"
                )
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn real_planned_mapping_after_closed_example_remains_valid() -> Result<(), String> {
    with_temp_cwd("trace-planned-after-example", |root| {
        let mapping = format!("````text\n```\nExample only.\n````\n\n{PLANNED}");
        fixture(root, "accepted", &mapping, "[]", "[]")?;
        check_traceability()
    })
}

#[test]
fn planned_prose_does_not_excuse_broken_current_references() -> Result<(), String> {
    for (tests, fixtures) in [
        ("[\"tests/missing.rs\"]", "[]"),
        ("[]", "[\"fixtures/missing.txt\"]"),
        ("[\"\"]", "[]"),
    ] {
        with_temp_cwd(
            "trace-planned-broken-current",
            |root| -> Result<(), String> {
                fixture(root, "accepted", PLANNED, tests, fixtures)?;
                assert!(check_traceability().is_err());
                let report = fs::read_to_string("target/ripr/reports/traceability.md")
                    .map_err(|error| error.to_string())?;
                assert!(
                    report.contains("path does not exist") || report.contains("empty `tests` path")
                );
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[test]
fn current_test_fixture_and_advisory_symbol_mappings_keep_their_contract() -> Result<(), String> {
    for (tests, fixtures, symbol) in [
        ("[\"tests/current.rs\"]", "[]", false),
        ("[]", "[\"fixtures/current.txt\"]", false),
        ("[\"tests/current.rs::nonexistent_symbol\"]", "[]", true),
    ] {
        with_temp_cwd("trace-current-mapping", |root| -> Result<(), String> {
            fixture(
                root,
                "accepted",
                "Current reference in the manifest.\n",
                tests,
                fixtures,
            )?;
            check_spec_format()?;
            check_traceability()?;
            let report = fs::read_to_string("target/ripr/reports/traceability.md")
                .map_err(|error| error.to_string())?;
            assert_eq!(
                report.contains("symbol suffix `nonexistent_symbol`"),
                symbol
            );
            assert!(!report.contains("accepted with an explicit planned test mapping"));
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn malformed_identity_manifest_and_absent_row_still_fail_real_traceability() -> Result<(), String> {
    for case in [
        "duplicate",
        "malformed-id",
        "absent-row",
        "malformed-manifest",
        "wrong-spec-id",
    ] {
        with_temp_cwd("trace-structural-negative", |root| -> Result<(), String> {
            fixture(root, "accepted", PLANNED, "[]", "[]")?;
            let path = root.join(".ripr/traceability.toml");
            let original = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let malformed = match case {
                "duplicate" => format!("{original}\n{original}"),
                "malformed-id" => {
                    original.replace("id = \"RIPR-SPEC-9999\"", "id = \"RIPR-SPEC-999\"")
                }
                "absent-row" => String::new(),
                "malformed-manifest" => original.replace("tests = []", "tests = [\n"),
                _ => original.replace("id = \"RIPR-SPEC-9999\"", "id = \"RIPR-SPEC-9998\""),
            };
            fs::write(path, malformed).map_err(|error| error.to_string())?;
            assert!(
                check_traceability().is_err(),
                "structural control passed: {case}"
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn planned_obligation_does_not_excuse_broken_code_or_output_paths() -> Result<(), String> {
    for field in ["code", "outputs"] {
        with_temp_cwd("trace-planned-other-path", |root| -> Result<(), String> {
            fixture(root, "accepted", PLANNED, "[]", "[]")?;
            let path = root.join(".ripr/traceability.toml");
            let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            fs::write(
                path,
                text.replace(
                    &format!("{field} = []"),
                    &format!("{field} = [\"missing/reference.rs\"]"),
                ),
            )
            .map_err(|error| error.to_string())?;
            assert!(check_traceability().is_err());
            let report = fs::read_to_string("target/ripr/reports/traceability.md")
                .map_err(|error| error.to_string())?;
            assert!(report.contains(&format!("`{field}` path does not exist")));
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn duplicate_spec_files_still_fail_the_numbering_owner() -> Result<(), String> {
    with_temp_cwd("trace-duplicate-spec-file", |root| {
        fixture(root, "accepted", PLANNED, "[]", "[]")?;
        fs::copy(
            root.join(SPEC),
            root.join("docs/specs/RIPR-SPEC-9999-duplicate.md"),
        )
        .map_err(|error| error.to_string())?;
        assert!(check_spec_numbering().is_err());
        Ok(())
    })
}

#[test]
fn planned_deprecated_and_invalid_lifecycle_keep_existing_owners() -> Result<(), String> {
    for status in ["planned", "deprecated"] {
        with_temp_cwd("trace-nonaccepted", |root| {
            fixture(root, status, "No current test claim.\n", "[]", "[]")?;
            check_spec_format()?;
            check_traceability()
        })?;
    }
    with_temp_cwd("trace-invalid-lifecycle", |root| {
        fixture(root, "implemented", PLANNED, "[]", "[]")?;
        assert!(
            check_spec_format().is_err(),
            "format owner must reject invalid lifecycle"
        );
        Ok(())
    })
}
