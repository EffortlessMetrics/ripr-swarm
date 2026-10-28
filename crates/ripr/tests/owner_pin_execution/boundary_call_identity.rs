//! Boundary activation and its oracle must name the same definition.

use super::{Scratch, source_runtime_control};
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::Path;

const SOURCE: &str =
    include_str!("../../../../fixtures/boundary_pairing_sibling_owner/input/src/lib.rs");
const BOUNDARY: &str = "    assert_eq!(b::gate(10), true);";
const CASES: &[(&str, &str, bool, bool)] = &[
    ("sibling_direct", BOUNDARY, false, false),
    (
        "sibling_bound",
        "    let got = b::gate(10);\n    assert_eq!(got, true);",
        false,
        false,
    ),
    (
        "sibling_imported",
        "    use b::gate;\n    assert_eq!(gate(10), true);",
        false,
        false,
    ),
    (
        "owner_direct",
        "    assert_eq!(a::gate(10), true);",
        true,
        true,
    ),
    (
        "owner_bound_beside_sibling_call",
        "    let got = a::gate(10); let _ = b::gate(20);\n    assert_eq!(got, true);",
        true,
        true,
    ),
    (
        "sibling_bound_beside_owner_call",
        "    let got = b::gate(10); let _ = a::gate(20);\n    assert_eq!(got, true);",
        false,
        false,
    ),
    (
        "owner_bound",
        "    let got = a::gate(10);\n    assert_eq!(got, true);",
        true,
        true,
    ),
    (
        "sibling_block_initializer",
        "    let got = { let _padding = (); b::gate(10) };\n    assert_eq!(got, true);",
        false,
        false,
    ),
    (
        "owner_block_initializer",
        "    let got = { let _padding = (); a::gate(10) };\n    assert_eq!(got, true);",
        true,
        true,
    ),
    (
        "constant_beside_sibling_call",
        "    let got = true; let _ = b::gate(10);\n    assert_eq!(got, true);",
        false,
        false,
    ),
    (
        "constant_beside_owner_call",
        "    let got = true; let _ = a::gate(10);\n    assert_eq!(got, true);",
        false,
        false,
    ),
    (
        "owner_relative_import_unresolved",
        "    use a::gate;\n    assert_eq!(gate(10), true);",
        false,
        true,
    ),
    (
        "owner_anchored_import",
        "    use crate::a::gate;\n    assert_eq!(gate(10), true);",
        true,
        true,
    ),
    (
        "sibling_anchored_import",
        "    use crate::b::gate;\n    assert_eq!(gate(10), true);",
        false,
        false,
    ),
];

#[test]
fn boundary_pairing_keeps_the_asserted_calls_definition() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/boundary_pairing_sibling_owner");
    assert_eq!(SOURCE.matches(BOUNDARY).count(), 1);
    for &(case, assertion, exposed, _) in CASES {
        check_case(
            &fixture,
            &SOURCE.replace(BOUNDARY, assertion),
            case,
            "input >= 10",
            exposed,
        )?;
    }
    Ok(())
}

fn check_case(
    fixture: &Path,
    source: &str,
    case: &str,
    expression: &str,
    exposed: bool,
) -> Result<(), String> {
    let scratch = Scratch::create()?;
    std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::copy(
        fixture.join("input/Cargo.toml"),
        scratch.0.join("Cargo.toml"),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    for mode in [Mode::Fast, Mode::Ready] {
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode,
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
        let predicates: Vec<_> = json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .filter(|finding| finding["probe"]["family"] == "predicate")
            .collect();
        assert_eq!(predicates.len(), 1, "{case}: nonempty intended subject");
        let finding = predicates[0];
        assert_eq!(finding["probe"]["line"], 3, "{case}: changed owner a");
        assert_eq!(finding["probe"]["expression"], expression);
        assert_eq!(
            finding["classification"],
            if exposed { "exposed" } else { "weakly_exposed" },
            "{case}: {finding}"
        );
        // The discarded owner boundary input and far-input oracle are
        // genuine evidence; the repair only refuses their false pairing.
        for stage in ["reach", "infect", "propagate", "observe"] {
            assert_eq!(finding["ripr"][stage]["state"], "yes", "{case}: {stage}");
        }
        assert_eq!(finding["oracle_strength"], "strong", "{case}");
        assert_eq!(
            finding["confidence"],
            if exposed { 1.0 } else { 0.92 },
            "{case}"
        );
        assert_eq!(
            finding["ripr"]["discriminate"]["state"],
            if exposed { "yes" } else { "weak" },
            "{case}"
        );
        if !exposed {
            assert!(
                finding["ripr"]["discriminate"]["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.contains("same_test_pairing_missing")),
                "{case}"
            );
        }
    }
    Ok(())
}

#[test]
fn element_boundary_pairing_keeps_the_asserted_calls_definition() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/boundary_pairing_sibling_element");
    let original =
        include_str!("../../../../fixtures/boundary_pairing_sibling_element/input/src/lib.rs");
    assert_eq!(original.matches("x > 0").count(), 2);
    assert_eq!(original.matches("use crate::b::gate;").count(), 1);
    for (case, source, exposed) in [
        ("sibling_element", original.to_string(), false),
        (
            "owner_element",
            original.replace("use crate::b::gate;", "use crate::a::gate;"),
            true,
        ),
        (
            "owner_element_with_sibling_diagnostic",
            original
                .replace("use crate::b::gate;", "use crate::a::gate;")
                .replace(
                    "assert_eq!(gate(&[0]), [false]);",
                    "assert_eq!(\n        gate(&[0]),\n        [false],\n        \"diagnostic {:?}\",\n        b::gate(&[5])\n    );",
                ),
            true,
        ),
        (
            "sibling_element_with_owner_diagnostic",
            original.replace(
                "assert_eq!(gate(&[0]), [false]);",
                "assert_eq!(\n        gate(&[0]),\n        [false],\n        \"diagnostic {:?}\",\n        a::gate(&[0])\n    );",
            ),
            false,
        ),
    ] {
        check_case(&fixture, &source, case, "x > 0", exposed)?;
        for wrong in [false, true] {
            let runtime = if wrong {
                source.replacen("x > 0", "x >= 0", 1)
            } else {
                source.clone()
            };
            source_runtime_control(
                &runtime,
                &format!("{case}, wrong={wrong}"),
                1,
                wrong && exposed,
            )?;
        }
    }
    Ok(())
}

#[test]
fn runtime_controls_distinguish_sibling_boundary_oracles() -> Result<(), String> {
    assert_eq!(SOURCE.matches("input >= 10").count(), 2);
    for &(case, assertion, _, observes_boundary) in CASES {
        let original = SOURCE.replace(BOUNDARY, assertion);
        for wrong in [false, true] {
            let source = if wrong {
                // Only a changes. Changing both owners would invalidate the
                // independent sibling control and hide the composition bug.
                original.replacen("input >= 10", "input > 10", 1)
            } else {
                original.clone()
            };
            source_runtime_control(
                &source,
                &format!("{case}, wrong={wrong}"),
                1,
                wrong && observes_boundary,
            )?;
        }
    }
    Ok(())
}
