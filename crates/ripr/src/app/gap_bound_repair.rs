//! Gap-bound TypeScript repair start (#9103, RIPR-SPEC-0311).
//!
//! The seam inventory behind `ripr repair` is built from the Rust index only,
//! so a TypeScript finding has no seam to bind. A TypeScript finding whose
//! GapRecord passes the shared agent-packet validator can instead start a
//! repair attempt bound to its canonical gap id and the diff base `ripr check`
//! analyzed. This module is the one place that decides whether a finding is
//! startable that way and spells the start command; `ripr check` (the printed
//! start line), `ripr agent repair --gap-id --phase before` (the attempt) and
//! `ripr agent status` (recovery text) all consume it, so they cannot drift.
//!
//! It owns no actionability decision: [`validate_agent_gap_record_packet`] is
//! the only authority, and a record it refuses yields no start.

use crate::agent::loop_commands::shell_arg;
use crate::domain::Finding;
use crate::output::agent_seam_packets::validate_agent_gap_record_packet;
use crate::output::gap_decision_ledger::GapRecord;
use crate::output::typescript_packet_projection::{
    typescript_canonical_gap_id, typescript_gap_record_for,
};

/// The canonical-gap prefix a gap-bound repair start accepts: TypeScript
/// preview gaps only (`typescript_canonical_gap_id` of a preview finding).
pub(crate) const GAP_BOUND_REPAIR_GAP_PREFIX: &str = "gap:typescript:typescript_preview:";

/// The wire spelling of the artifact the before snapshot of a gap-bound
/// attempt is produced as: the same producer as
/// `ripr check --base <ref> --format check-findings-json`.
pub(crate) const GAP_BOUND_BEFORE_SNAPSHOT_FILE: &str = "before.check-findings.json";

/// The manifest artifact role carrying that snapshot. Distinct from the
/// seam-bound `before_snapshot` role (a repo-exposure document) so no
/// consumer reads one artifact kind as the other.
pub(crate) const GAP_BOUND_BEFORE_SNAPSHOT_ROLE: &str = "before_check_findings";

/// The validated GapRecord of a finding that may start a gap-bound repair, or
/// `None` when the finding projects no record or the shared validator
/// refuses it.
pub(crate) fn startable_typescript_gap_record(finding: &Finding) -> Option<GapRecord> {
    let record = typescript_gap_record_for(finding)?;
    validate_agent_gap_record_packet(&record).ok()?;
    Some(record)
}

/// What a canonical gap id resolves to among one check run's findings.
pub(crate) enum GapStartLookup {
    /// A finding carries this gap and its record validates.
    Ready(Box<GapRecord>),
    /// No finding of this run carries the gap: the id is stale or belongs to
    /// another diff.
    NoSuchGap,
    /// A finding carries the gap but its repair packet is not ready.
    NotReady { reason: String },
}

/// Resolve `gap_id` among `findings` through the shared projection and
/// validator. Fail-closed: anything but a validated record is not a start.
pub(crate) fn lookup_gap_start(findings: &[Finding], gap_id: &str) -> GapStartLookup {
    let mut matching = findings
        .iter()
        .filter(|finding| typescript_canonical_gap_id(&finding.id) == gap_id);
    let Some(finding) = matching.next() else {
        return GapStartLookup::NoSuchGap;
    };
    if matching.next().is_some() {
        return GapStartLookup::NotReady {
            reason: "several findings carry this gap id".to_string(),
        };
    }
    let Some(record) = typescript_gap_record_for(finding) else {
        return GapStartLookup::NotReady {
            reason: "the finding projects no complete TypeScript repair record".to_string(),
        };
    };
    match validate_agent_gap_record_packet(&record) {
        Ok(()) => GapStartLookup::Ready(Box::new(record)),
        Err(reason) => GapStartLookup::NotReady { reason },
    }
}

/// The exact gap-bound repair start: `--gap-id` and the diff `--base` the
/// check ran against replace `--seam-id`. `store_flag` is the already-quoted
/// ` --store ...` fragment (empty for the repository-local default).
pub(crate) fn gap_bound_start_command(
    root_display: &str,
    store_flag: &str,
    gap_id: &str,
    base: &str,
) -> String {
    format!(
        "ripr agent repair --gap-id {} --base {} --root {}{store_flag} --phase before",
        shell_arg(gap_id),
        shell_arg(base),
        shell_arg(root_display)
    )
}

/// The documented repair start for one attempt subject. A seam-bound attempt
/// restarts with `--seam-id`; a gap-bound one (`gap_base` is its recorded diff
/// base) restarts with `--gap-id --base`, so recovery text never names a
/// `--seam-id` the Rust seam inventory cannot resolve for a TypeScript gap.
/// The one spelling owner for status, recovery and after-phase narration.
pub(crate) fn repair_start_command(
    root_display: &str,
    store_flag: &str,
    subject: &str,
    gap_base: Option<&str>,
) -> String {
    match gap_base {
        Some(base) => gap_bound_start_command(root_display, store_flag, subject, base),
        None => format!(
            "ripr agent repair --root {}{store_flag} --seam-id {} --phase before",
            shell_arg(root_display),
            shell_arg(subject)
        ),
    }
}

/// The manual verification route a gap-bound attempt names until the
/// gap-bound after phase lands: save the same producer's artifact after the
/// edit and compare by gap id with the retained before snapshot.
pub(crate) fn gap_bound_manual_verify_route(
    root_display: &str,
    before_snapshot: &str,
    gap_id: &str,
    base: &str,
) -> String {
    format!(
        "save `ripr check --root {root} --base {base} --format check-findings-json` after the edit to a file, then run `ripr agent verify --root {root} --before {before} --after <that file> --gap-id {gap} --json`, which reports the gap closed only when it reads exposed in the after snapshot",
        root = shell_arg(root_display),
        base = shell_arg(base),
        before = shell_arg(before_snapshot),
        gap = shell_arg(gap_id),
    )
}
