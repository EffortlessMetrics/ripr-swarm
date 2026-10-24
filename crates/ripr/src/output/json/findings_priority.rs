//! Which findings survive the check-JSON findings-array byte budget (#10093).
//!
//! The budget used to cut the array in `sort_findings` order (file, line,
//! family), so the last files lost findings first regardless of how actionable
//! they were. When the budget engages, findings now render in a deterministic
//! priority order and the kept set is emitted in the unchanged sort order, so a
//! budget that does not engage stays byte-identical.
//!
//! Priority only chooses what renders. It never changes a class, a count, a
//! currentness or any other field of a finding.

use crate::domain::{ExposureClass, Finding, SourceCurrentness};
use std::collections::BTreeMap;

/// Most files named by name in the omitted-set disclosure; the rest are counted.
const OMITTED_FILES_NAMED: usize = 5;

/// `(non-current, class gap strength, no repair route)`: lower renders first.
/// Candidate-current findings are edit targets, base-side and unresolved ones
/// are not. Within a currentness band the gap classes lead (`reachable_unrevealed`
/// and `weakly_exposed` name a missing discriminator), `exposed` goes last, and a
/// recommended next step breaks ties. Equal keys keep the sort order.
fn priority_key(finding: &Finding) -> (u8, u8, u8) {
    (
        u8::from(finding.source_currentness != SourceCurrentness::CandidateCurrent),
        class_rank(&finding.class),
        u8::from(finding.recommended_next_step.is_none()),
    )
}

fn class_rank(class: &ExposureClass) -> u8 {
    match class {
        ExposureClass::ReachableUnrevealed => 0,
        ExposureClass::WeaklyExposed => 1,
        ExposureClass::NoStaticPath => 2,
        ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => 3,
        ExposureClass::Exposed => 4,
    }
}

/// Indices of `findings` in render-priority order. A stable sort, so findings
/// with equal keys keep their (deterministic) position in `findings`.
pub(super) fn priority_order(findings: &[Finding]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..findings.len()).collect();
    order.sort_by_key(|&index| priority_key(&findings[index]));
    order
}

/// Names what the budget dropped: counts per class and per file (bounded), plus
/// the non-candidate count when any was dropped. `kept` holds indices into
/// `findings`.
pub(super) fn omitted_summary(findings: &[Finding], kept: &[usize]) -> String {
    let mut is_kept = vec![false; findings.len()];
    for &index in kept {
        if let Some(slot) = is_kept.get_mut(index) {
            *slot = true;
        }
    }
    let mut by_class: BTreeMap<(u8, &'static str), usize> = BTreeMap::new();
    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    let mut omitted = 0usize;
    let mut non_current = 0usize;
    for (finding, kept) in findings.iter().zip(&is_kept) {
        if *kept {
            continue;
        }
        omitted += 1;
        *by_class
            .entry((class_rank(&finding.class), finding.class.as_str()))
            .or_default() += 1;
        *by_file
            .entry(crate::output::path::display_path(
                &finding.probe.location.file,
            ))
            .or_default() += 1;
        if finding.source_currentness != SourceCurrentness::CandidateCurrent {
            non_current += 1;
        }
    }
    let classes = by_class
        .iter()
        .map(|((_, label), count)| format!("{label}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut files: Vec<(&String, &usize)> = by_file.iter().collect();
    files.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let named = files
        .iter()
        .take(OMITTED_FILES_NAMED)
        .map(|(file, count)| format!("{file}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let more = files.len().saturating_sub(OMITTED_FILES_NAMED);
    let mut summary = format!("omitted {omitted}: by class {classes}; by file {named}");
    if more > 0 {
        summary.push_str(&format!(" (+{more} more file(s))"));
    }
    if non_current > 0 {
        summary.push_str(&format!(
            "; {non_current} base-side or unresolved-currentness"
        ));
    }
    summary
}
