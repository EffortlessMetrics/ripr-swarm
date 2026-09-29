//! Central compatibility catalog for the artifacts used by the agent repair
//! loop (#2646, #2973).
//!
//! The live emitter modules remain authoritative for serialization and own the
//! constants referenced below. This catalog gives reviewers one compile-checked
//! source inventory of artifact names, versions, producers, and version-history
//! notes without creating a second version authority. Adding an agent-loop
//! artifact or changing an existing version must update the focused contract
//! test in this module.

/// `(artifact, version, producer, version history)` for one agent-loop
/// compatibility surface.
pub(crate) type AgentArtifactSchema = (&'static str, &'static str, &'static str, &'static str);

/// The verification-execution response producer currently keeps this envelope
/// version private. The focused source-binding test below makes a change to that
/// producer fail this registry until the catalog is reviewed at the same time.
const VERIFICATION_EXECUTION_RESPONSE_SCHEMA_VERSION: &str = "1";

/// Complete active agent repair-loop schema inventory.
///
/// This is a compatibility catalog, not a claim that differently versioned
/// artifact families share one wire shape. Each producer still validates its
/// own exact schema and fails closed on unsupported input. Binary/runtime
/// discovery is intentionally outside this inventory slice.
pub(crate) const AGENT_ARTIFACT_SCHEMAS: &[AgentArtifactSchema] = &[
    (
        "artifact_identity",
        crate::agent::artifact::ARTIFACT_IDENTITY_SCHEMA_VERSION,
        "agent::artifact",
        "1: initial repo-exposure identity envelope; input identity remains separately versioned.",
    ),
    (
        "repo_exposure",
        crate::output::repo_exposure::REPO_EXPOSURE_SCHEMA_VERSION,
        "output::repo_exposure",
        "0.3: current full-repo exposure artifact shape.",
    ),
    (
        "repo_exposure_summary",
        crate::output::repo_exposure::REPO_EXPOSURE_SUMMARY_SCHEMA_VERSION,
        "output::repo_exposure",
        "0.1: initial bounded repo-exposure summary.",
    ),
    (
        "analysis_outcome_artifact",
        crate::app::CHECK_OUTPUT_SCHEMA_VERSION,
        "app / app::analysis_outcome_artifact",
        "0.2: workflow analysis-outcome.json retains the normal check-output envelope; its nested typed AnalysisOutcome remains separately versioned and validated.",
    ),
    (
        "agent_brief",
        crate::output::agent_brief::AGENT_BRIEF_SCHEMA_VERSION,
        "output::agent_brief",
        "0.1: initial versioned agent brief.",
    ),
    (
        "agent_seam_packet",
        crate::app::AGENT_SEAM_PACKET_SCHEMA_VERSION,
        "app / output::agent_seam_packets",
        "0.4: packet preserves typed analysis outcomes (#2897).",
    ),
    (
        "targeted_test_outcome",
        crate::output::outcome::TARGETED_TEST_OUTCOME_SCHEMA_VERSION,
        "output::outcome",
        "0.1: initial targeted test-outcome artifact.",
    ),
    (
        "agent_verify",
        crate::output::outcome::AGENT_VERIFY_SCHEMA_VERSION,
        "output::outcome",
        "0.3: corrected pair currentness and retained exact-byte artifact bindings (#2922, #3027, #3045).",
    ),
    (
        "verification_execution_response",
        VERIFICATION_EXECUTION_RESPONSE_SCHEMA_VERSION,
        "app::verification_execution",
        "1: initial committed response envelope for the bounded verify-execute process surface (#1979, #2332).",
    ),
    (
        "verification_execution_result",
        crate::domain::VERIFICATION_EXECUTION_RESULT_SCHEMA_VERSION,
        "domain::verification_result / app::verification_execution",
        "1: initial provenance-bound result nested in a successful verification-execution response (#1979, #2332).",
    ),
    (
        "repair_attempt",
        crate::app::repair_attempt::REPAIR_ATTEMPT_SCHEMA_VERSION,
        "app::repair_attempt",
        "0.1: initial durable per-attempt manifest binding retained inputs, transaction state, and the edit-cage finish verdict.",
    ),
    (
        "agent_receipt",
        crate::output::agent_receipt::AGENT_RECEIPT_SCHEMA_VERSION,
        "output::agent_receipt",
        "0.5: receipt preserves typed analysis outcomes and omits invariant-false safe_to_merge (#2595, #2895).",
    ),
    (
        "agent_workflow",
        crate::app::agent_workflow::AGENT_WORKFLOW_SCHEMA_VERSION,
        "app::agent_workflow",
        "0.1: initial versioned workflow manifest.",
    ),
    (
        "agent_status",
        crate::app::agent_status::AGENT_STATUS_SCHEMA_VERSION,
        "app::agent_status",
        "0.1: initial versioned agent status artifact.",
    ),
    (
        "agent_review_summary",
        crate::app::agent_review_summary::types::AGENT_REVIEW_SUMMARY_SCHEMA_VERSION,
        "app::agent_review_summary",
        "0.1: initial versioned review summary.",
    ),
];

#[cfg(test)]
mod tests {
    use super::{AGENT_ARTIFACT_SCHEMAS, VERIFICATION_EXECUTION_RESPONSE_SCHEMA_VERSION};
    use std::collections::BTreeSet;

    #[test]
    fn registry_matches_live_agent_artifact_versions() {
        let expected = [
            ("artifact_identity", "1", "agent::artifact"),
            ("repo_exposure", "0.3", "output::repo_exposure"),
            ("repo_exposure_summary", "0.1", "output::repo_exposure"),
            (
                "analysis_outcome_artifact",
                "0.2",
                "app / app::analysis_outcome_artifact",
            ),
            ("agent_brief", "0.1", "output::agent_brief"),
            (
                "agent_seam_packet",
                "0.4",
                "app / output::agent_seam_packets",
            ),
            ("targeted_test_outcome", "0.1", "output::outcome"),
            ("agent_verify", "0.3", "output::outcome"),
            (
                "verification_execution_response",
                "1",
                "app::verification_execution",
            ),
            (
                "verification_execution_result",
                "1",
                "domain::verification_result / app::verification_execution",
            ),
            ("repair_attempt", "0.1", "app::repair_attempt"),
            ("agent_receipt", "0.5", "output::agent_receipt"),
            ("agent_workflow", "0.1", "app::agent_workflow"),
            ("agent_status", "0.1", "app::agent_status"),
            ("agent_review_summary", "0.1", "app::agent_review_summary"),
        ];

        assert_eq!(AGENT_ARTIFACT_SCHEMAS.len(), expected.len());
        let mut names = BTreeSet::new();
        for ((name, version, producer, history), expected_entry) in
            AGENT_ARTIFACT_SCHEMAS.iter().copied().zip(expected)
        {
            assert_eq!((name, version, producer), expected_entry);
            assert!(
                history.starts_with(&format!("{version}:")),
                "{name} history must start with current version {version}: {history}"
            );
            assert!(names.insert(name), "duplicate artifact name: {name}");
        }
    }

    #[test]
    fn verification_execution_response_version_matches_private_producer() {
        const PREFIX: &str = "const RESPONSE_SCHEMA_VERSION: &str = \"";
        let producer_source = include_str!("../app/verification_execution.rs");
        let producer_version = producer_source.lines().find_map(|line| {
            line.trim()
                .strip_prefix(PREFIX)
                .and_then(|value| value.strip_suffix("\";"))
        });

        assert_eq!(
            producer_version,
            Some(VERIFICATION_EXECUTION_RESPONSE_SCHEMA_VERSION),
            "verification-execution response version drifted from the central catalog"
        );
    }

    fn output_schema_doc() -> String {
        let path = crate::output::test_support::repo_root()
            .expect("repo root")
            .join("docs/OUTPUT_SCHEMA.md");
        crate::output::test_support::read_file(&path).expect("read OUTPUT_SCHEMA.md")
    }

    fn command_to_version_table(doc: &str) -> &str {
        const HEADER: &str = "| Output | Field | Current value |";
        let start = doc.find(HEADER).unwrap_or_else(|| {
            panic!("docs/OUTPUT_SCHEMA.md is missing the command-to-version table")
        });
        let table = &doc[start..];
        let end = table.find("\n\n").unwrap_or(table.len());
        &table[..end]
    }

    fn fenced_json_after(doc: &str, marker: &str) -> serde_json::Value {
        let after = doc
            .split_once(marker)
            .unwrap_or_else(|| panic!("docs/OUTPUT_SCHEMA.md is missing marker {marker:?}"))
            .1;
        let json = after
            .split_once("```json")
            .unwrap_or_else(|| panic!("no json fence after {marker:?}"))
            .1
            .split_once("```")
            .unwrap_or_else(|| panic!("unclosed json fence after {marker:?}"))
            .0;
        serde_json::from_str(json)
            .unwrap_or_else(|err| panic!("example after {marker:?} is not JSON: {err}\n{json}"))
    }

    fn prose_after_json_example<'a>(doc: &'a str, marker: &str) -> &'a str {
        let after = doc
            .split_once(marker)
            .unwrap_or_else(|| panic!("docs/OUTPUT_SCHEMA.md is missing marker {marker:?}"))
            .1;
        let after_fence = after
            .split_once("```json")
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(_, rest)| rest)
            .unwrap_or_else(|| panic!("no closed json fence after {marker:?}"));
        let end = after_fence.find("\n## ").unwrap_or(after_fence.len());
        &after_fence[..end]
    }

    #[test]
    fn command_version_table_names_live_public_outputs() {
        let doc = output_schema_doc();
        let table = command_to_version_table(&doc);
        let required_rows = [
            (
                "`ripr diff --json`",
                "0.1",
                "crates/ripr/src/output/diff_report.rs DiffReport.schema_version",
            ),
            (
                "`ripr check --format repo-exposure-json`",
                crate::output::repo_exposure::REPO_EXPOSURE_SCHEMA_VERSION,
                "output::repo_exposure::REPO_EXPOSURE_SCHEMA_VERSION",
            ),
            (
                "`ripr rerun --json`",
                "ripr-targeted-rerun-v1",
                "crates/ripr/src/cli/rerun.rs TargetedRerunReport.schema_version",
            ),
            (
                "`ripr agent brief`",
                crate::output::agent_brief::AGENT_BRIEF_SCHEMA_VERSION,
                "output::agent_brief::AGENT_BRIEF_SCHEMA_VERSION",
            ),
            (
                "repair_after_refusal",
                "0.2",
                "cli::commands::agent::REPAIR_AFTER_REFUSAL_SCHEMA_VERSION",
            ),
            (
                "`ripr swarm queue --json`",
                "0.2",
                "output::agent_seam_packets gap_record_queue_envelope_value",
            ),
            (
                "`ripr cache status --json`",
                "0.1",
                "cli::commands::cache::CACHE_STATUS_SCHEMA_VERSION",
            ),
        ];
        for (command, version, owner) in required_rows {
            let row = table.lines().find(|line| line.contains(command));
            let row =
                row.unwrap_or_else(|| panic!("command-to-version table omits {command} ({owner})"));
            assert!(
                row.contains(&format!("`{version}`")),
                "{command} row must document producer version {version} from {owner}; got {row}"
            );
        }
    }

    #[test]
    fn swarm_queue_example_matches_live_envelope_contract() {
        let doc = output_schema_doc();
        let example = fenced_json_after(&doc, "The queue envelope is:");
        assert_eq!(
            example
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some("0.2"),
            "swarm queue example must use the live envelope version from gap_record_queue_envelope_value"
        );
        assert_eq!(
            example.get("report").and_then(serde_json::Value::as_str),
            Some("swarm-queue")
        );

        let follow_on = prose_after_json_example(&doc, "The queue envelope is:");
        for field in [
            "`analysis_outcome`",
            "`analysis_outcome_error`",
            "`analysis_outcome_status`",
            "`assignment_policy`",
            "`must_not_infer`",
            "`source_currentness`",
        ] {
            assert!(
                follow_on.contains(field),
                "queue example follow-on must name omitted live field {field}"
            );
        }
        assert!(
            follow_on.contains("defaults to `python`") || follow_on.contains("defaults to python"),
            "queue docs must say --language defaults to python: {follow_on}"
        );
        assert!(
            follow_on.contains("language_records_total") && follow_on.contains("--language rust"),
            "queue docs must say a Rust-only ledger is empty under the python default: {follow_on}"
        );
    }

    #[test]
    fn rerun_example_nested_versions_match_live_cache_identity() {
        let doc = output_schema_doc();
        let example = fenced_json_after(&doc, "targeted-rerun receipt shape:");
        assert_eq!(
            example
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some("ripr-targeted-rerun-v1")
        );
        let cache = example
            .get("cache")
            .unwrap_or_else(|| panic!("rerun example missing cache"));
        assert_eq!(
            cache
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some(crate::analysis::seam_cache::FILE_FACT_CACHE_SCHEMA_VERSION),
            "cache.schema_version must track FILE_FACT_CACHE_SCHEMA_VERSION"
        );
        let fingerprint = cache
            .get("input_fingerprint")
            .unwrap_or_else(|| panic!("rerun example missing cache.input_fingerprint"));
        assert_eq!(
            fingerprint
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some(crate::analysis::seam_cache::CACHE_SCHEMA_VERSION),
            "input_fingerprint.schema_version must track CACHE_SCHEMA_VERSION"
        );
        assert_eq!(
            fingerprint
                .get("analyzer_version")
                .and_then(serde_json::Value::as_str),
            Some(env!("CARGO_PKG_VERSION")),
            "input_fingerprint.analyzer_version must track CARGO_PKG_VERSION"
        );

        let follow_on = prose_after_json_example(&doc, "targeted-rerun receipt shape:");
        assert!(
            follow_on.contains("cache.schema_version")
                && follow_on.contains("input_fingerprint.schema_version")
                && follow_on.contains("opaque")
                && follow_on.contains("top-level"),
            "rerun docs must say nested cache identity versions move and are opaque to report dispatch: {follow_on}"
        );
    }

    #[test]
    fn cache_status_docs_name_live_status_field_contract() {
        let doc = output_schema_doc();
        let start = doc
            .find("`ripr cache status --json` (schema")
            .expect("cache-status field contract must appear beside the version table");
        let window = doc
            .get(start..)
            .and_then(|rest| rest.get(..2000))
            .unwrap_or(&doc[start..]);
        for status in ["ok", "not_found", "partial", "unavailable"] {
            assert!(
                window.contains(&format!("`{status}`")),
                "cache-status field contract must document status {status} from inspect_cache_dir: {window}"
            );
        }
        for field in ["`cache_dir`", "`entry_count`", "`total_size_bytes`"] {
            assert!(
                window.contains(field),
                "cache-status field contract must document {field}"
            );
        }
    }
}
