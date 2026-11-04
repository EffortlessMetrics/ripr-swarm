//! Claims payload-equality credit shared with diff analysis (#12394 item 3).
//!
//! Extraction lists no oracle for `assert_ok_eq!(call, literal)` /
//! `assert_some_eq!(call, literal)` (RIPR-SPEC-0120), so the repo seam stages
//! that read `test.assertions` saw none, while `ripr check` nominates the
//! statement and grades it through the owner pin. This module is the repo
//! side of that same decision: the nomination is the diff path's own
//! `claims_payload_eq_candidates`, and the grade is the owner pin's own
//! `admits_claims_payload_eq` (verified import, direction, literal expected
//! operand, owner identity). A refusal there leaves the statement out of the
//! view, so the audit can only credit what the diff path credits.

use super::{FunctionSummary, OracleFact, RepoSeam, RustIndex, SeamKind, TestSummary};
use crate::analysis::classify::{
    OwnerPinSyntax, OwnerReturnPin, ProbeContext, claims_payload_eq_candidates,
};
use crate::domain::{
    DeltaKind, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, SourceLocation,
};

/// Macro names that nominate a claims payload-equality statement.
const CLAIMS_EQ_NAMES: [&str; 2] = ["assert_ok_eq", "assert_some_eq"];

pub(super) struct ClaimsPayloadPin<'a> {
    owner: &'a FunctionSummary,
    probe: Probe,
    pin: OwnerReturnPin,
}

impl<'a> ClaimsPayloadPin<'a> {
    /// `None` unless this is a return-value seam, one related test mentions a
    /// claims equality macro, and the owner pin establishes the owner. The
    /// mention check keeps the owner-side scans off every seam without one.
    pub(super) fn establish<'t>(
        seam: &RepoSeam,
        owner: Option<&'a FunctionSummary>,
        index: &RustIndex,
        mut related: impl Iterator<Item = &'t TestSummary>,
    ) -> Option<Self> {
        // The diff path nominates for the return-value family only.
        if seam.kind() != SeamKind::ReturnValue {
            return None;
        }
        if !related.any(|test| CLAIMS_EQ_NAMES.iter().any(|name| test.body.contains(name))) {
            return None;
        }
        let owner = owner?;
        if owner.file != seam.file() || owner.id.0.replace('\\', "/") != seam.owner() {
            return None;
        }
        // `ripr check` refuses every owner-pin route for a `Display::fmt`
        // tail edit unless a display pin admits it. That refusal reads the
        // diff, which the audit has none of, so the audit keeps it whole.
        if crate::analysis::classify::is_display_fmt_owner(owner) {
            return None;
        }
        // A private input to the existing owner-pin authority, as
        // `BooleanBoundaryPin` builds one; not a diff claim.
        let probe = Probe {
            id: ProbeId(format!("repo-claims-payload:{}", seam.id().as_str())),
            location: SourceLocation::new(seam.file().to_path_buf(), seam.display_line(), 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Unknown,
            before: None,
            after: Some(seam.expression().to_string()),
            expression: seam.expression().to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let pin = OwnerReturnPin::establish(&probe, owner, index)?
            .with_current_owner_literal_check(owner);
        Some(Self { owner, probe, pin })
    }

    /// The claims statements of `test` the owner pin admits, graded as the
    /// diff path grades them. Empty when none is admitted.
    pub(super) fn admitted_oracles(
        &self,
        test: &TestSummary,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> Vec<OracleFact> {
        let candidates = claims_payload_eq_candidates(test);
        if candidates.is_empty() {
            return Vec::new();
        }
        let context = ProbeContext::new(
            &self.probe,
            Some(self.owner),
            Vec::new(),
            false,
            index,
            false,
        );
        candidates
            .into_iter()
            .filter(|candidate| {
                self.pin.admits_claims_payload_eq(
                    test,
                    candidate,
                    index,
                    // An unindexed test file cannot show its import is the
                    // owner's, so it refuses.
                    &|file, name| {
                        index.files().get(file).is_none_or(|facts| {
                            context.test_file_imports_foreign_callee_name(file, &facts.source, name)
                        })
                    },
                    syntax,
                )
            })
            .map(|candidate| OracleFact {
                kind: OracleKind::ExactValue,
                strength: OracleStrength::Strong,
                ..candidate
            })
            .collect()
    }
}
