//! RIPR-SPEC-0367: a proof harness is verification source, not a test and not
//! a production seam. The role itself is assigned by the facts normalizer
//! (`ProofHarness` or the declared-context `KaniProofHarness`); this module
//! only discloses it:
//!
//! - an edit to a harness yields a typed run limitation naming
//!   `proof_harness_not_evaluated` instead of production probes;
//! - a `no_static_path` finding whose owner a harness calls says so, in place
//!   of the generic "add a co-located test" advice.
//!
//! Nothing here adds credit, changes a class, or reads a verifier result.

use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::RustIndex;
use crate::analysis_outcome::{
    AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use crate::domain::{Finding, StaticLimitKind};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Where a proof harness that calls the changed owner lives.
pub(super) struct HarnessPointer {
    pub(super) name: String,
    pub(super) file: PathBuf,
    pub(super) line: usize,
}

/// The first proof harness (by file, then line) that calls `owner_name`.
///
/// The match is by call name, so it is accepted only when exactly one
/// non-evidence function carries that name: an ambiguous name could point at a
/// different owner, and the disclosure must not name a harness for the wrong
/// code. A missing answer leaves the finding exactly as it was.
pub(super) fn harness_calling_owner(index: &RustIndex, owner_name: &str) -> Option<HarnessPointer> {
    let mut owners = index
        .functions()
        .iter()
        .filter(|function| !function.source_role.is_evidence_role() && function.name == owner_name);
    let (Some(_), None) = (owners.next(), owners.next()) else {
        return None;
    };
    index
        .functions()
        .iter()
        .filter(|function| function.source_role.is_proof_harness())
        .filter(|function| function.calls.iter().any(|call| call.name == owner_name))
        .map(|function| HarnessPointer {
            name: function.name.clone(),
            file: function.file.clone(),
            line: function.start_line,
        })
        .min_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then(left.line.cmp(&right.line))
                .then(left.name.cmp(&right.name))
        })
}

/// Attach the typed limitation and the proof-aware next step. The class,
/// related tests and RIPR stages are untouched: no credit, no new path.
pub(super) fn apply_limit(finding: &mut Finding, owner_name: &str, harness: &HarnessPointer) {
    finding.static_limit_kind = Some(StaticLimitKind::ProofHarnessNotEvaluated);
    let location = format!("{}:{}", harness.file.display(), harness.line);
    finding.evidence.push(format!(
        "proof_harness: `{}` ({location}) carries a proof-harness attribute and calls `{owner_name}`; ripr does not evaluate proofs, so it is neither a test path nor oracle evidence",
        harness.name
    ));
    finding.recommended_next_step = Some(format!(
        "A proof harness `{}` ({location}) calls `{owner_name}`, but ripr does not evaluate proofs, so this limitation does not establish a missing test. Run the verifier for that harness as the discriminating check, or add a test that calls `{owner_name}` and asserts on its result.",
        harness.name
    ));
    finding.missing = vec![
        StaticLimitKind::ProofHarnessNotEvaluated
            .describe()
            .to_string(),
    ];
    finding.activation.missing_discriminators.clear();
}

/// One typed limitation per changed file whose added lines fall inside a
/// proof harness, so an edit to verification source cannot read as a clean
/// run with nothing to check. Added lines only: a deleted harness needs the
/// old source, which this producer does not reconstruct.
pub(super) fn edit_limitations<'a>(
    changed: impl Iterator<Item = &'a ChangedFile>,
    index: &RustIndex,
) -> Result<Vec<AnalysisLimitation>, String> {
    let mut limitations = Vec::new();
    for file in changed {
        // A nested local is verification source, but it is not itself a
        // harness declaration. Disclose the enclosing proof when its body
        // changes, even when the nearest owner is a KaniProofLocal.
        let harnesses: BTreeSet<(usize, &str)> = index
            .functions()
            .iter()
            .filter(|function| {
                function.file == file.path
                    && function.source_role.is_proof_harness()
                    && file.added_lines.iter().any(|line| {
                        function.start_line <= line.line && line.line <= function.end_line
                    })
            })
            .map(|function| (function.start_line, function.name.as_str()))
            .collect();
        if harnesses.is_empty() {
            continue;
        }
        let display = file.path.to_string_lossy().replace('\\', "/");
        let limitation = AnalysisLimitation::new(
            AnalysisLimitationKind::LanguageScopeUnsupported,
            AnalysisStage::LanguageAdapter,
            AnalysisRecovery::new(
                AnalysisRecoveryKind::InspectFailure,
                "proof_harness_not_evaluated: a changed proof harness (`#[kani::proof]` or \
                 `#[kani::proof_for_contract]`) is verification source, so ripr produced no \
                 production probe for it and does not evaluate it. Run the verifier for the \
                 changed harness; this is not a coverage claim."
                    .to_string(),
            )?,
        );
        let affected = u64::try_from(harnesses.len()).unwrap_or(u64::MAX);
        limitations.push(
            limitation
                .clone()
                .with_path(&display)
                .unwrap_or(limitation)
                .with_affected_items(affected)?
                .with_detail(
                    "proof_harness_not_evaluated: lines changed inside a function carrying a \
                     proof-harness attribute. It is not a test and not a production seam, so \
                     it seeds no probe and earns no credit; ripr does not evaluate proofs.",
                )?,
        );
    }
    Ok(limitations)
}
