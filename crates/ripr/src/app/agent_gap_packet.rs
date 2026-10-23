use crate::output;
use std::path::Path;

/// Render one GapRecord-backed agent packet after adapting the CLI request.
/// Artifact loading and packet orchestration belong to the application layer;
/// the CLI only supplies the requested root, ledger, and gap identity.
/// Read the user-named `--gap-ledger` input with the shared missing-input
/// diagnostic (#9851): plain `not found` — never a raw OS errno — plus the
/// producer command, in the sibling `reports gap-ledger` vocabulary.
fn read_gap_ledger(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| {
        let mut message = format!(
            "agent packet --gap-ledger {} is invalid: {}",
            output::outcome::display_path(path),
            crate::bounded_input::read_failure_detail(&err)
        );
        if err.kind() == std::io::ErrorKind::NotFound {
            message.push_str(
                ". Generate it first (for example `ripr reports gap-ledger --root .`), or pass the correct path",
            );
        }
        message
    })
}

pub(crate) fn render_agent_packet_from_gap_ledger(
    root: &Path,
    gap_ledger: &Path,
    gap_id: &str,
) -> Result<String, String> {
    let contents = read_gap_ledger(gap_ledger)?;
    let source =
        output::gap_decision_ledger_live::parse_gap_record_source_with_provenance_json(&contents)
            .map_err(|err| {
            format!(
                "agent packet --gap-ledger {} is invalid: {err}",
                gap_ledger.display()
            )
        })?;
    let record = source
        .records
        .iter()
        .find(|record| record.gap_id == gap_id || record.canonical_gap_id == gap_id)
        .ok_or_else(|| format!("agent packet gap_id {gap_id} was not found"))?;
    let currentness = output::agent_seam_packets::evaluate_gap_record_source_currentness(
        output::agent_seam_packets::GapRecordSourceInput {
            root,
            gap_ledger_path: gap_ledger,
            ledger_root: source.root.as_deref(),
            source_kind: source.source_kind.as_deref(),
            records_path: source.records_path.as_deref(),
            source_identity_error: source.source_identity_error.as_deref(),
            records: &source.records,
        },
    );
    let (causal_projection, causal_projection_warning) =
        crate::app::causal_projection::CausalDeltaArtifact::load_optional(root);
    if let Some(warning) = causal_projection_warning {
        eprintln!(
            "{}",
            output::human::terminal_safe(format!("ripr agent packet: {warning}"))
        );
    }
    output::agent_seam_packets::render_agent_gap_record_packet_json_with_live_currentness(
        &output::outcome::display_path(gap_ledger),
        record,
        causal_projection.as_ref(),
        &currentness,
    )
    .map_err(|err| format!("agent packet gap_id {gap_id} {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_gap_ledger_names_path_and_recovery_route() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-agent-packet-missing-ledger-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| format!("create root: {error}"))?;
        let missing = root.join("missing-gap-ledger.json");
        let err = render_agent_packet_from_gap_ledger(&root, &missing, "gap:demo")
            .err()
            .ok_or("missing gap ledger must fail")?;
        let expected_path = output::outcome::display_path(&missing);
        assert!(
            err.contains(&format!(
                "agent packet --gap-ledger {expected_path} is invalid:"
            )),
            "{err}"
        );
        assert!(err.contains("not found"), "{err}");
        assert!(
            err.contains("ripr reports gap-ledger --root ."),
            "error should name the gap-ledger recovery route, got: {err}"
        );
        assert!(!err.contains("os error"), "{err}");
        std::fs::remove_dir_all(&root).map_err(|error| format!("remove root: {error}"))?;
        Ok(())
    }
}
