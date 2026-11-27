//! Evidence limitations (unreadable unchanged Rust paths, #13624) in the pilot
//! agent packet envelope: repairs are withheld and the run never reads complete.

use super::{
    CausalDeltaArtifact, ClassifiedSeam, PacketBound, PacketCommandContext, RepoExposureLimit,
    TASK_WRITE_TARGETED_TEST, render_agent_seam_packets_json_with_root, task_for,
};
use crate::analysis_outcome::AnalysisLimitation;

/// Status of an envelope whose inventory skipped evidence.
pub(super) const LIMITED_EVIDENCE_RUN_STATUS: &str = "limited_seam_evidence";

/// Whether a seam's packet stays visible. With evidence limitations a
/// `write_targeted_test` packet and its loop commands would be a repair
/// signal over incomplete evidence, so only inspection packets remain.
pub(super) fn packet_visible_under_limitations(
    entry: &ClassifiedSeam,
    limitations: &[AnalysisLimitation],
) -> bool {
    limitations.is_empty() || task_for(entry) != TASK_WRITE_TARGETED_TEST
}

/// The shared `limitations[]` entries, same shape as `repo-exposure.json`.
pub(super) fn limitation_entries(limitations: &[AnalysisLimitation]) -> Vec<String> {
    let mut entries = Vec::new();
    for limitation in limitations {
        entries.push(crate::output::repo_exposure::evidence_limitation_json_object(limitation, 4));
    }
    entries
}

/// Pilot's packet artifact: [`super::render_agent_seam_packets_json_for_root`]
/// plus the inventory's evidence limitations.
pub(crate) fn render_agent_seam_packets_json_for_root_with_limitations(
    classified: &[ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    causal_projection: Option<&CausalDeltaArtifact>,
    root: &str,
    declined: &[crate::analysis::DeclinedRustFile],
    bound: Option<PacketBound>,
    evidence_limitations: &[AnalysisLimitation],
) -> String {
    render_agent_seam_packets_json_with_root(
        classified,
        limit_info,
        causal_projection,
        None,
        false,
        PacketCommandContext::Standalone { root },
        declined,
        bound,
        evidence_limitations,
    )
}
