//! Read-back of the caller-reported focused run a finished attempt retained
//! (#14079). ripr never ran that test: this only reads the record the finish
//! committed, and only when the terminal binding names exactly those bytes.

use super::{
    RepairAttemptManifest, RepairAttemptState, TERMINAL_FOCUSED_RUN_ROLE,
    find_terminal_artifact_by_role, sha256_bytes,
};
use crate::app::focused_run::read_retained_focused_run;
use crate::output::agent_receipt::ReportedTestRun;

/// The caller-reported focused run a finished attempt retained, read from the
/// record the finish committed in the manifest. It is reported only when the
/// terminal `focused_test_result` binding names exactly those bytes, so an
/// interrupted finish (no terminal binding yet) and a binding that names other
/// bytes both read as no reported run.
pub(crate) fn retained_focused_run(manifest: &RepairAttemptManifest) -> Option<ReportedTestRun> {
    if manifest.state != RepairAttemptState::ReadyToFinish {
        return None;
    }
    let pending = manifest.after.as_ref()?.focused_result.as_ref()?;
    let artifact = find_terminal_artifact_by_role(manifest, TERMINAL_FOCUSED_RUN_ROLE)?;
    if artifact.sha256 != sha256_bytes(pending.record.as_bytes())
        || artifact.bytes != pending.record.len() as u64
    {
        return None;
    }
    read_retained_focused_run(&pending.record, manifest.repair_attempt_id.as_str())
}
