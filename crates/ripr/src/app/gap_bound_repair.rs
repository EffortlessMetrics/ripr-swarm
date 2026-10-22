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

/// The workflow-side copy of a gap-bound attempt's repair packet. Distinct
/// from the seam loop's `agent-packet.json` so a gap-bound start never
/// overwrites a seam-bound projection nor makes `ripr agent status` read a
/// gap id as a seam it can build `--seam-id` commands for.
pub(crate) const GAP_BOUND_PACKET_ARTIFACT: &str = "target/ripr/workflow/gap-repair-packet.json";

/// The manifest artifact role carrying that snapshot. Distinct from the
/// seam-bound `before_snapshot` role (a repo-exposure document) so no
/// consumer reads one artifact kind as the other.
pub(crate) const GAP_BOUND_BEFORE_SNAPSHOT_ROLE: &str = "before_check_findings";

/// Whether a finding is static-limited: the producer's named limitation or an
/// unknown/no-path class. `ripr check` offers no start for one, so the before
/// phase must not accept one either.
pub(crate) fn finding_is_static_limited(finding: &Finding) -> bool {
    finding.static_limit_kind.is_some()
        || matches!(
            finding.class,
            crate::domain::ExposureClass::NoStaticPath
                | crate::domain::ExposureClass::InfectionUnknown
                | crate::domain::ExposureClass::PropagationUnknown
                | crate::domain::ExposureClass::StaticUnknown
        )
}

/// The validated GapRecord of a finding that may start a gap-bound repair, or
/// `None` when the finding projects no record or the shared validator
/// refuses it.
pub(crate) fn startable_typescript_gap_record(finding: &Finding) -> Option<GapRecord> {
    if finding_is_static_limited(finding) {
        return None;
    }
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
/// The printed start line replays only `--base` and `--root`; the finding is
/// re-derived from the live diff, so a drifted tree can refuse here.
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
    if finding_is_static_limited(finding) {
        return GapStartLookup::NotReady {
            reason: "the finding is static-limited, so `ripr check` offers no start for it"
                .to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OracleStrength, ProbeFamily, StageState};
    use crate::output::typescript_packet_projection::tests::complete_finding;

    fn gap_of(finding: &Finding) -> String {
        typescript_canonical_gap_id(&finding.id)
    }

    /// An effect-family finding whose strong borrowed assertion is not
    /// confirmed still projects a diagnostic record, but with
    /// `agent_packet.eligible = false`; only the shared validator refuses it.
    fn ineligible_finding() -> Finding {
        let mut finding = complete_finding();
        finding.probe.family = ProbeFamily::SideEffect;
        finding.related_tests[0].oracle_strength = OracleStrength::Strong;
        finding.ripr.reveal.observe.state = StageState::Weak;
        finding
    }

    #[test]
    fn a_validated_record_is_startable_and_resolves() -> Result<(), String> {
        let finding = complete_finding();
        if startable_typescript_gap_record(&finding).is_none() {
            return Err("control: the complete finding must be startable".to_string());
        }
        match lookup_gap_start(std::slice::from_ref(&finding), &gap_of(&finding)) {
            GapStartLookup::Ready(_) => Ok(()),
            _ => Err("control: the complete finding must resolve Ready".to_string()),
        }
    }

    #[test]
    fn a_record_the_shared_validator_refuses_is_neither_startable_nor_ready() -> Result<(), String>
    {
        let finding = ineligible_finding();
        let record = typescript_gap_record_for(&finding)
            .ok_or("the ineligible finding must retain a diagnostic record")?;
        if validate_agent_gap_record_packet(&record).is_ok() {
            return Err("the record must be refused by the shared validator".to_string());
        }
        if startable_typescript_gap_record(&finding).is_some() {
            return Err("an ineligible record must not be startable".to_string());
        }
        match lookup_gap_start(std::slice::from_ref(&finding), &gap_of(&finding)) {
            GapStartLookup::NotReady { .. } => Ok(()),
            _ => Err("an ineligible record must resolve NotReady".to_string()),
        }
    }

    #[test]
    fn a_static_limited_finding_is_not_a_start() -> Result<(), String> {
        let mut finding = complete_finding();
        finding.class = crate::domain::ExposureClass::StaticUnknown;
        if startable_typescript_gap_record(&finding).is_some() {
            return Err("a static-limited finding must not be startable".to_string());
        }
        match lookup_gap_start(std::slice::from_ref(&finding), &gap_of(&finding)) {
            GapStartLookup::NotReady { .. } => Ok(()),
            _ => Err("a static-limited finding must resolve NotReady".to_string()),
        }
    }

    #[test]
    fn an_unknown_gap_id_is_no_such_gap() -> Result<(), String> {
        let finding = complete_finding();
        match lookup_gap_start(
            std::slice::from_ref(&finding),
            "gap:typescript:typescript_preview:00000000",
        ) {
            GapStartLookup::NoSuchGap => Ok(()),
            _ => Err("an unknown id must resolve NoSuchGap".to_string()),
        }
    }
}
