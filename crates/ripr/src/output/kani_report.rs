//! Human and JSON views of the same imported native observation.

use crate::app::kani_import::KaniReport;

use super::markdown::{code_span, table_code_span};

pub(crate) fn render_json(report: &KaniReport) -> Result<String, String> {
    serde_json::to_string_pretty(report)
        .map(|json| format!("{json}\n"))
        .map_err(|err| format!("render Kani report JSON: {err}"))
}

pub(crate) fn render_markdown(report: &KaniReport) -> Result<String, String> {
    let mut out = String::from("# Kani observation\n\n");
    out.push_str("Import succeeded. Native verification outcomes are listed separately below.\n\n");
    out.push_str(
        "Provenance: **external, unattested**. Source binding: **listed files match**.\n\n",
    );
    out.push_str(&format!(
        "- Report schema: {}\n- Adapter profile: {}\n- Checked root: {}\n- Native artifact: {}\n- Binding artifact: {}\n- Selected harnesses: {}\n- Imported harnesses: {}\n- Missing selected harnesses: {}\n\n",
        code_span(&report.schema_version),
        code_span(&report.adapter_profile),
        code_span(&report.checked_root),
        code_span(&report.artifact_sha256),
        code_span(&report.binding_sha256),
        report.native.run.matched_count,
        report.native.run.harnesses.len(),
        report.native.run.missing_count,
    ));
    out.push_str("## Native outcomes\n\n");
    out.push_str(&format!(
        "Run state: {}. This is the export's completion state, not an overall proof verdict.\n\n",
        code_span(&report.native.document["run_state"].to_string())
    ));
    out.push_str("| Harness | Native outcome | Expected panic | Successful checks | Total checks | Total covers |\n");
    out.push_str("| --- | --- | --- | ---: | ---: | ---: |\n");
    for harness in &report.native.run.harnesses {
        let outcome = serde_json::to_string(&harness.outcome)
            .map_err(|err| format!("render Kani native outcome: {err}"))?;
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            table_code_span(&format!("{}::{}", harness.crate_name, harness.name)),
            table_code_span(&outcome),
            harness.should_panic,
            count(harness.checks_success),
            count(harness.checks_total),
            count(harness.covers_total),
        ));
    }
    if report.native.run.harnesses.is_empty() {
        out.push_str("\nNo harness result is present; no harness-level success is established.\n");
    }
    out.push_str("\n## Interpretation limits\n\n");
    for limitation in &report.limitations {
        out.push_str(&format!("- {limitation}\n"));
    }
    out.push_str("\n## Complete observation\n\n");
    out.push_str("The following is the same report as the JSON output, including native check and cover buckets, bounds, stubs, warnings, tools and supplied context.\n\n");
    // Indented code cannot be terminated by a fence inside an imported string.
    for line in render_json(report)?.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

fn count(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".into(), |count| count.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honesty_corpus_matches_real_import_projection() -> Result<(), String> {
        let root = crate::app::kani_import::tests_root("honesty")?;
        let (native, binding) = crate::app::kani_import::test_inputs(&root)?;
        let report = crate::app::kani_import::import_kani_report(&root, &native, &binding)?;
        let mut actual: serde_json::Value = serde_json::from_str(&render_json(&report)?)
            .map_err(|err| format!("parse rendered Kani report: {err}"))?;
        // Keep every owned field observable; mask only volatile metadata and
        // raw external payloads, whose full retention is checked separately.
        for (pointer, marker) in [
            ("/adapter_version", "<normalized>"),
            ("/checked_root", "<normalized>"),
            ("/artifact_sha256", "<normalized>"),
            ("/binding_sha256", "<normalized>"),
            ("/binding", "<external payload>"),
            ("/native/document", "<external payload>"),
        ] {
            *actual
                .pointer_mut(pointer)
                .ok_or_else(|| format!("Kani projection is missing {pointer}"))? = marker.into();
        }
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../fixtures/evidence-promotion-honesty-corpus/reports/kani-import-unattested.json"
        ))
        .map_err(|err| format!("read Kani honesty projection: {err}"))?;
        assert_eq!(actual, expected);
        std::fs::remove_dir_all(root).map_err(|err| format!("remove Kani honesty test: {err}"))?;
        Ok(())
    }

    #[test]
    fn human_and_json_preserve_the_same_complete_observation() -> Result<(), String> {
        let root = crate::app::kani_import::tests_root("output")?;
        let (native, binding) = crate::app::kani_import::test_inputs(&root)?;
        let report = crate::app::kani_import::import_kani_report(&root, &native, &binding)?;
        let json = render_json(&report)?;
        let markdown = render_markdown(&report)?;
        let embedded = markdown
            .lines()
            .filter_map(|line| line.strip_prefix("    "))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(embedded, json.trim_end());
        assert!(markdown.contains("external, unattested"));
        assert!(markdown.contains("FAILURE"));
        assert!(markdown.contains("check_two.unsupported_construct.1"));
        assert!(markdown.contains("Successful checks"));
        assert!(!markdown.contains("proof passed"));
        std::fs::remove_dir_all(root).map_err(|err| format!("remove Kani output test: {err}"))?;
        Ok(())
    }

    #[test]
    fn captured_vacuous_success_matches_honesty_projection_and_complete_markdown()
    -> Result<(), String> {
        let root = crate::app::kani_import::tests_root("captured-vacuous-output")?;
        let (native, binding) = crate::app::kani_import::captured_test_inputs(&root, "vacuous")?;
        let report = crate::app::kani_import::import_kani_report(&root, native, &binding)?;
        let json = render_json(&report)?;
        let markdown = render_markdown(&report)?;
        let embedded = markdown
            .lines()
            .filter_map(|line| line.strip_prefix("    "))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(embedded, json.trim_end());
        assert!(markdown.contains("external, unattested"));
        assert!(markdown.contains("c_vacuous.assertion.1"));
        assert!(markdown.contains("c_vacuous.cover.1"));
        assert!(
            markdown
                .lines()
                .any(|line| line.contains("proof::c_vacuous")
                    && line.contains("SUCCESS")
                    && line.ends_with("| false | 0 | 1 | 1 |"))
        );
        let mut actual: serde_json::Value =
            serde_json::from_str(&json).map_err(|err| err.to_string())?;
        // The same six permitted masks as the contract-fixture oracle above.
        // Every other owned field remains observable, including new fields.
        for (pointer, marker) in [
            ("/adapter_version", "<normalized>"),
            ("/checked_root", "<normalized>"),
            ("/artifact_sha256", "<normalized>"),
            ("/binding_sha256", "<normalized>"),
            ("/binding", "<external payload>"),
            ("/native/document", "<external payload>"),
        ] {
            *actual
                .pointer_mut(pointer)
                .ok_or_else(|| format!("missing Kani field {pointer}"))? = marker.into();
        }
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../fixtures/evidence-promotion-honesty-corpus/reports/kani-vacuous-capture-unattested.json"
        ))
        .map_err(|err| err.to_string())?;
        assert_eq!(actual, expected);
        std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }
}
