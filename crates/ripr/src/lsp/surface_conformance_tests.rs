//! One producer run per existing fixture, inspected through the retained
//! surface owners. Availability is part of the contract: an annotation is
//! not a repair packet, and MCP finding readiness is not seam admission.

use crate::app::{CheckInput, Mode};
use crate::mcp::gaps::GapItem;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

const CASES: &[(&str, &str, &str)] = &[
    (
        "asserting_same_file_test_positive_control",
        "rust",
        "exposed",
    ),
    ("boundary_gap", "rust", "weakly_exposed"),
    (
        "compile_time_marker_test_no_observation",
        "rust",
        "reachable_unrevealed",
    ),
    ("all_no_path_disclosure", "rust", "no_static_path"),
    ("helper_chain_one_hop", "rust", "infection_unknown"),
    (
        "one_hop_field_projection_discarded",
        "rust",
        "propagation_unknown",
    ),
    (
        "atomic_ordering_swap_strong_object_oracle",
        "rust",
        "static_unknown",
    ),
    ("python_assertion_helper_field", "python", "exposed"),
    ("python_boundary_gap", "python", "weakly_exposed"),
    (
        "python_adversarial_free_function_module_collision",
        "python",
        "no_static_path",
    ),
    (
        "python_adversarial_changed_sink_non_delta_operand",
        "python",
        "static_unknown",
    ),
    ("ts_already_observed", "typescript", "exposed"),
    (
        "ts_class_getter_factory_read",
        "typescript",
        "weakly_exposed",
    ),
    ("ts_cross_package_test", "typescript", "no_static_path"),
    (
        "typescript_private_method_unknown",
        "typescript",
        "static_unknown",
    ),
    (
        "typescript_infection_shapes",
        "typescript",
        "reachable_unrevealed",
    ),
    (
        "typescript_rejects_literal_equality",
        "typescript",
        "infection_unknown",
    ),
    ("ts_repair_packet_complete", "typescript", "weakly_exposed"),
    (
        "ts_repair_packet_boundary_unreachable",
        "typescript",
        "weakly_exposed",
    ),
];

fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

fn same(case: &str, pair: &str, field: &str, left: &Value, right: &Value) {
    assert_eq!(left, right, "{case}: {pair}: {field}");
}

#[test]
fn existing_fixture_surface_conformance() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut covered = BTreeSet::new();
    for &(name, language, class) in CASES {
        if (language == "rust" && !cfg!(feature = "lang-rust"))
            || (language == "python" && !cfg!(feature = "lang-python"))
            || (language == "typescript" && !cfg!(feature = "lang-typescript"))
        {
            continue;
        }
        let fixture = repo.join("fixtures").join(name);
        let root = fixture.join("input");
        let config = crate::config::load_for_root(&root)?;
        let output = crate::app::check_workspace_with_config(
            CheckInput {
                root: root.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                ..CheckInput::default()
            },
            &config,
        )
        .map_err(|error| format!("{name}: setup: {error}"))?;
        assert!(
            output.findings.iter().any(|f| f.class.as_str() == class),
            "{name}: fixture no longer exercises {class}"
        );
        covered.insert((language, class));
        // Render the same output; no renderer may reclassify the subject.
        let check = parse(&crate::output::json::render_with_config(
            &output, &config, None,
        ))?;
        let checks = check["findings"]
            .as_array()
            .ok_or("JSON findings missing")?;
        for finding in &output.findings {
            let json = checks
                .iter()
                .find(|row| row["id"] == finding.id)
                .ok_or_else(|| format!("{name}: JSON lost {}", finding.id))?;
            let diagnostic = super::diagnostics::diagnostic_for_finding(&root, finding);
            let lsp = diagnostic.data.ok_or("LSP data missing")?;
            let packet = parse(&crate::output::json::render_context_packet(
                finding,
                usize::MAX,
                &config,
                &root,
            ))?;
            let gap = GapItem::from_finding(finding, &root)?;
            // Literal fixture contracts, not a class-to-readiness policy.
            // helper_chain_one_hop deliberately remains finding-ready despite
            // infection_unknown: it has a canonical discriminator and a strong
            // direct test. Final seam admission is a separate authority.
            // These two weak-test rows are owned by the active MCP readiness
            // repair (#13409); only list/detail agreement is pinned for them.
            if !matches!(
                name,
                "ts_repair_packet_complete" | "ts_repair_packet_boundary_unreachable"
            ) {
                let expected_ready = matches!(name, "boundary_gap" | "helper_chain_one_hop");
                assert_eq!(
                    gap.list_summary["repair_packet_ready"], expected_ready,
                    "{name}: MCP finding-readiness fixture contract"
                );
            }
            assert_eq!(json["language"], language, "{name}: producer language");
            same(
                name,
                "JSON/context packet",
                "probe ID",
                &json["probe"]["id"],
                &packet["probe"]["id"],
            );
            let document = gap.document("conformance");
            let mcp = &document["item"];
            same(
                name,
                "JSON/LSP",
                "finding ID",
                &json["id"],
                &lsp["finding_id"],
            );
            same(
                name,
                "JSON/MCP",
                "finding ID",
                &json["id"],
                &mcp["finding_id"],
            );
            same(
                name,
                "JSON/LSP",
                "class",
                &json["classification"],
                &lsp["classification"],
            );
            same(
                name,
                "JSON/context packet",
                "class",
                &json["classification"],
                &packet["classification"],
            );
            same(
                name,
                "JSON/MCP",
                "class",
                &json["classification"],
                &gap.list_summary["class"],
            );
            same(
                name,
                "JSON/LSP",
                "language",
                &json["language"],
                &lsp["language"],
            );
            same(
                name,
                "JSON/MCP",
                "language",
                &json["language"],
                &gap.list_summary["language"],
            );
            same(
                name,
                "JSON/LSP",
                "confidence",
                &json["confidence"],
                &lsp["confidence"],
            );
            same(
                name,
                "JSON/context packet",
                "confidence",
                &json["confidence"],
                &packet["confidence"],
            );
            same(
                name,
                "MCP list/detail",
                "readiness",
                &gap.list_summary["repair_packet_ready"],
                &mcp["readiness"]["repair_packet_ready"],
            );
            let canonical = finding
                .canonical_gap
                .as_ref()
                .map_or(finding.id.as_str(), |g| g.id.as_str());
            assert_eq!(
                gap.list_summary["canonical_id"], canonical,
                "{name}: JSON/MCP canonical ID"
            );
            if finding.canonical_gap.is_some() {
                same(
                    name,
                    "JSON/context packet",
                    "canonical ID",
                    &json["canonical_gap_id"],
                    &packet["canonical_gap_id"],
                );
                same(
                    name,
                    "JSON/LSP",
                    "canonical ID",
                    &json["canonical_gap_id"],
                    &lsp["canonical_gap_id"],
                );
            }
            // Full diagnostics preserve the evidence, while the actionable
            // profile is tested against the witness actually serialized to the
            // editor and agent packet. Neither is MCP transaction authority.
            same(
                name,
                "LSP/context packet",
                "witness",
                &lsp["witness"],
                &packet["witness"],
            );
            let witness_missing = lsp["witness"]["missing_discriminators"]
                .as_array()
                .is_some_and(|values| !values.is_empty());
            let witness_site = lsp["witness"]
                .get("fix_site")
                .is_some_and(|value| !value.is_null());
            let gap_class = matches!(
                finding.class.as_str(),
                "weakly_exposed" | "reachable_unrevealed" | "no_static_path"
            );
            let expected_visible = json["source_currentness"] == "candidate_current"
                && gap_class
                && witness_missing
                && witness_site;
            assert_eq!(
                super::diagnostics::finding_is_visible_in_profile(
                    crate::config::LspDiagnosticProfile::Actionable,
                    finding
                ),
                expected_visible,
                "{name}: JSON/LSP actionable-profile witness contract"
            );
            assert!(
                super::diagnostics::finding_is_visible_in_profile(
                    crate::config::LspDiagnosticProfile::Full,
                    finding
                ),
                "{name}: LSP full-profile disclosure"
            );
            let missing = json["missing_discriminators"]
                .as_array()
                .ok_or("JSON missing discriminators absent")?;
            let packet_missing = packet["missing_discriminators"]
                .as_array()
                .ok_or("packet missing discriminators absent")?;
            let mcp_missing = mcp["discriminator_availability"]["missing_discriminators"]
                .as_array()
                .ok_or("MCP missing discriminators absent")?;
            assert_eq!(
                missing.is_empty(),
                packet_missing.is_empty(),
                "{name}: JSON/context packet missing discriminator presence"
            );
            assert_eq!(
                missing.is_empty(),
                mcp_missing.is_empty(),
                "{name}: JSON/MCP missing discriminator presence"
            );
            // The actual agent-packet surface consumes GapRecords, not the
            // finding class. Exercise its existing preview producer/validator;
            // do not synthesize a Rust seam or equate packet presence with
            // authority to start an MCP transaction.
            if language == "typescript" {
                let record =
                    crate::output::typescript_packet_projection::typescript_gap_record_for(finding);
                let rendered = record.as_ref().map(|record| {
                    crate::output::agent_seam_packets::render_agent_gap_record_packet_json(
                        "conformance-gap-ledger.json",
                        record,
                    )
                });
                let ready = rendered.as_ref().is_some_and(|packet| packet.is_ok());
                let advertised = json["preview_actionability"]["repair_packet_ready"]
                    .as_bool()
                    .unwrap_or(false);
                assert_eq!(ready, advertised, "{name}: JSON/agent packet availability");
                if name == "ts_repair_packet_complete" {
                    assert!(ready, "{name}: positive agent packet control did not run");
                }
                if name == "ts_repair_packet_boundary_unreachable" {
                    assert!(
                        !ready,
                        "{name}: unreachable discriminator acquired an agent packet"
                    );
                }
                if let Some(Ok(rendered)) = rendered {
                    let agent = parse(&rendered)?;
                    assert_eq!(
                        agent["packets_total"], 1,
                        "{name}: agent packet denominator"
                    );
                    let agent = &agent["packets"][0];
                    same(
                        name,
                        "JSON/agent packet",
                        "canonical ID",
                        &json["canonical_gap_id"],
                        &agent["canonical_gap_id"],
                    );
                    same(
                        name,
                        "JSON/agent packet",
                        "language",
                        &json["language"],
                        &agent["language"],
                    );
                    same(
                        name,
                        "JSON/agent packet",
                        "gap state",
                        &json["preview_actionability"]["gap_state"],
                        &agent["gap_state"],
                    );
                    assert!(
                        agent.get("classification").is_none(),
                        "{name}: an agent GapRecord is not a finding-class projection"
                    );
                }
            }
            // Isolate each finding to prevent same-line SARIF folding from
            // making an identity check vacuous. This is still the unchanged
            // producer finding, with the same root/config and run disclosure.
            let mut single = output.clone();
            single.findings = vec![finding.clone()];
            let sarif = parse(&crate::output::sarif::render_findings_sarif(
                &single,
                &config,
                &[],
            ))?;
            let results = sarif["runs"][0]["results"]
                .as_array()
                .ok_or("SARIF results missing")?;
            let github = crate::output::github::render_with_config(&single, &config);
            let severity = json["severity"].as_str().ok_or("JSON severity missing")?;
            let visible = finding.is_candidate_actionable() && severity != "off";
            assert_eq!(
                results.len(),
                usize::from(visible),
                "{name}: JSON/SARIF visibility"
            );
            if visible {
                let result = &results[0];
                let properties = &result["properties"];
                same(
                    name,
                    "JSON/SARIF",
                    "finding ID",
                    &json["id"],
                    &properties["finding_id"],
                );
                let sarif_missing = properties["missing_discriminators"]
                    .as_array()
                    .ok_or("SARIF missing discriminators absent")?;
                assert_eq!(
                    missing.is_empty(),
                    sarif_missing.is_empty(),
                    "{name}: JSON/SARIF missing discriminator presence"
                );
                for field in [
                    "classification",
                    "language",
                    "confidence",
                    "preview_actionability",
                ] {
                    same(name, "JSON/SARIF", field, &json[field], &properties[field]);
                }
                // Literal contract mapping, independent of SARIF's mapper.
                let level = if severity == "warning" {
                    "warning"
                } else {
                    "note"
                };
                assert_eq!(result["level"], level, "{name}: JSON/SARIF severity");
                let annotation_level = if severity == "warning" {
                    "warning"
                } else {
                    "notice"
                };
                let title = format!("title=ripr {}::", finding.class.as_str());
                let annotations: Vec<_> = github
                    .lines()
                    .filter(|line| line.contains(&title))
                    .collect();
                assert_eq!(annotations.len(), 1, "{name}: JSON/GitHub class: {github}");
                assert!(
                    annotations[0].starts_with(&format!("::{annotation_level} ")),
                    "{name}: JSON/GitHub severity: {github}"
                );
                if finding.canonical_gap.is_some() {
                    assert!(
                        annotations[0].contains(canonical),
                        "{name}: JSON/GitHub canonical ID: {github}"
                    );
                }
            }
        }
    }
    for &(name, language, class) in CASES {
        if (language == "rust" && cfg!(feature = "lang-rust"))
            || (language == "python" && cfg!(feature = "lang-python"))
            || (language == "typescript" && cfg!(feature = "lang-typescript"))
        {
            assert!(
                covered.contains(&(language, class)),
                "{name}: missing coverage"
            );
        }
    }
    Ok(())
}
