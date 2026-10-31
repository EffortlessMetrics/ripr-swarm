//! #8610 public consumer: the printed repair continuation must retain the
//! natural boolean assertion's owner-bound movement in its receipt.

use super::*;

const BOOL_BEFORE: &str = "pub fn is_adult(age: u32) -> bool {\n    age > 18\n}\n";
const BOOL_AFTER: &str = "pub fn is_adult(age: u32) -> bool {\n    age >= 18\n}\n";
const BOOL_TESTS: &str = "use boundary_gap_closure::is_adult;\n\n#[test]\nfn adult_over() {\n    assert!(is_adult(30));\n}\n";

fn boolean_seam(report: &Value) -> Result<&Value, String> {
    let selected = report
        .get("seams")
        .and_then(Value::as_array)
        .ok_or("repo exposure has no seams")?
        .iter()
        .filter(|seam| {
            seam.get("kind").and_then(Value::as_str) == Some("predicate_boundary")
                && seam.get("expression").and_then(Value::as_str) == Some("age >= 18")
        })
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err(format!(
            "expected one boolean boundary seam, got {selected:?}"
        ));
    }
    Ok(selected[0])
}

#[test]
fn natural_boolean_repair_receipt_requires_the_boundary_result() -> Result<(), String> {
    for (label, body, expected_class, expected_change) in [
        (
            "boolean-result",
            "assert!(is_adult(18));",
            "strongly_gripped",
            "improved",
        ),
        (
            "boolean-unobserved-boundary",
            "let _unobserved = is_adult(18);\n    assert!(is_adult(30));",
            "weakly_gripped",
            "unchanged",
        ),
    ] {
        let roots = journey_roots(label)?;
        write_crate(&roots.fixture, BOOL_BEFORE, BOOL_TESTS)?;
        boundary_gap::init_git(&roots.fixture)?;
        boundary_gap::generate_lockfile(&roots.fixture, &roots.cargo_target)?;
        commit_all(&roots.fixture, "strict boolean boundary")?;
        std::fs::write(roots.fixture.join("src/lib.rs"), BOOL_AFTER)
            .map_err(|error| format!("write inclusive boundary: {error}"))?;
        commit_all(&roots.fixture, "inclusive boolean boundary, weak tests")?;

        let root = path_arg(&roots.fixture);
        let before_output = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("boolean repo exposure before", &before_output)?;
        let before = boundary_gap::parse_json(
            "boolean repo exposure before",
            &stdout_text("boolean repo exposure before", &before_output)?,
        )?;
        let selected = boolean_seam(&before)?;
        assert_eq!(
            selected.get("grip_class").and_then(Value::as_str),
            Some("weakly_gripped")
        );
        let selected_id = seam_id(selected)?.to_string();

        let before_phase = run_workspace_ripr(
            &roots.fixture,
            &[
                "agent",
                "repair",
                "--root",
                &root,
                "--seam-id",
                &selected_id,
                "--phase",
                "before",
            ],
        )?;
        require_success("boolean agent repair before", &before_phase)?;
        let printed =
            printed_next_command(&String::from_utf8_lossy(&before_phase.stderr))?.to_string();
        let tokens = split_printed_ripr_command(&printed)?;
        assert_eq!(tokens.get(1).map(String::as_str), Some("continue"));

        let tests = format!("{BOOL_TESTS}\n#[test]\nfn adult_boundary() {{\n    {body}\n}}\n");
        std::fs::write(roots.fixture.join("tests/pricing.rs"), tests)
            .map_err(|error| format!("write boolean boundary test: {error}"))?;
        let execution =
            run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "adult_boundary")?;
        assert_eq!(execution.exit_code, Some(0), "{label}: {execution:?}");
        assert_eq!(execution.selected, 1, "{label}: {execution:?}");
        assert_eq!(execution.executed, 1, "{label}: {execution:?}");
        assert_eq!(execution.passed, 1, "{label}: {execution:?}");
        assert_eq!(execution.failed, 0, "{label}: {execution:?}");

        let args = tokens
            .iter()
            .skip(1)
            .map(String::as_str)
            .collect::<Vec<_>>();
        let after_phase = run_workspace_ripr(&roots.fixture, &args)?;
        require_success("printed boolean repair continuation", &after_phase)?;

        let after_path = roots
            .fixture
            .join("target/ripr/workflow/after.repo-exposure.json");
        let after = boundary_gap::parse_json(
            "boolean repair after snapshot",
            &std::fs::read_to_string(&after_path)
                .map_err(|error| format!("read {}: {error}", after_path.display()))?,
        )?;
        let after_seam = boolean_seam(&after)?;
        assert_eq!(seam_id(after_seam)?, selected_id, "{label}: same behavior");
        assert_eq!(
            after_seam.get("grip_class").and_then(Value::as_str),
            Some(expected_class),
            "{label}: {after_seam}"
        );

        let receipt_path = roots.fixture.join("target/ripr/reports/agent-receipt.json");
        let receipt = boundary_gap::parse_json(
            "boolean repair receipt",
            &std::fs::read_to_string(&receipt_path)
                .map_err(|error| format!("read {}: {error}", receipt_path.display()))?,
        )?;
        assert_eq!(
            receipt.pointer("/seam/seam_id").and_then(Value::as_str),
            Some(selected_id.as_str())
        );
        assert_eq!(
            receipt.pointer("/seam/before").and_then(Value::as_str),
            Some("weakly_gripped")
        );
        assert_eq!(
            receipt.pointer("/seam/after").and_then(Value::as_str),
            Some(expected_class)
        );
        assert_eq!(
            receipt.pointer("/seam/change").and_then(Value::as_str),
            Some(expected_change)
        );
        assert_eq!(verification_status(&receipt)?, "verification_not_run");
        assert_eq!(
            std::fs::read_to_string(roots.fixture.join("src/lib.rs"))
                .map_err(|error| format!("read production source: {error}"))?,
            BOOL_AFTER,
            "the repair changed only tests"
        );
    }
    Ok(())
}
