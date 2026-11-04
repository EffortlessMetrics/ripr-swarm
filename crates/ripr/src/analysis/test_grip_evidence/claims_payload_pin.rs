//! Owner-pin decisions the repo-wide audit shares with diff analysis
//! (#12394 item 3, #13005).
//!
//! Claims payload equality (#12394 item 3):
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
//!
//! Plain `assert_eq!` literal check (#13005): the audit credits a plain
//! `assert_eq!` from its extracted oracle, never asking the owner pin, so a
//! literal the current owner provably does not return read as an exact-value
//! oracle. `ripr check` withholds a literal neither owner version returns
//! (#12716). The audit has one version, so it reads the current owner and
//! drops only the assertions that check alone refuses.

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
    /// A related test mentions a claims equality macro.
    claims: bool,
    /// A related test mentions `assert_eq!` and the closed subset reads the
    /// owner, so the current-owner literal check can withhold.
    plain_eq: bool,
}

impl<'a> ClaimsPayloadPin<'a> {
    /// `None` unless this is a return-value seam, one related test mentions a
    /// claims equality macro (or `assert_eq!` over an owner the closed subset
    /// reads), and the owner pin establishes the owner. The mention check keeps
    /// the owner-side scans off every seam without one.
    pub(super) fn establish<'t>(
        seam: &RepoSeam,
        owner: Option<&'a FunctionSummary>,
        index: &RustIndex,
        related: impl Iterator<Item = &'t TestSummary>,
    ) -> Option<Self> {
        // The diff path nominates for the return-value family only.
        if seam.kind() != SeamKind::ReturnValue {
            return None;
        }
        let owner = owner?;
        // A claims macro nominates the claims credit; a plain `assert_eq!`
        // the literal check, and only when the closed subset reads the owner.
        let (mut claims, mut plain_eq) = (false, false);
        let literal_readable = OwnerReturnPin::owner_literal_readable(owner);
        for test in related {
            claims |= CLAIMS_EQ_NAMES.iter().any(|name| test.body.contains(name));
            plain_eq |= literal_readable && test.body.contains("assert_eq!");
            if claims && plain_eq {
                break;
            }
        }
        if !claims && !plain_eq {
            return None;
        }
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
        Some(Self {
            owner,
            probe,
            pin,
            claims,
            plain_eq,
        })
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
        if !self.claims || candidates.is_empty() {
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
    /// #13005: the assertions of `test` the current-owner literal check alone
    /// refuses: a plain `assert_eq!` whose literal the current owner provably
    /// does not return. Empty when none, or when no related test mentions one.
    pub(super) fn literal_withheld(
        &self,
        test: &TestSummary,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> Vec<usize> {
        if !self.plain_eq {
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
        test.assertions
            .iter()
            .enumerate()
            .filter(|(_, assertion)| assertion.text.contains("assert_eq!"))
            .filter(|(_, assertion)| {
                self.pin.literal_check_withholds(
                    test,
                    assertion,
                    index,
                    &|file, name| {
                        index.files().get(file).is_none_or(|facts| {
                            context.test_file_imports_foreign_callee_name(file, &facts.source, name)
                        })
                    },
                    syntax,
                )
            })
            .map(|(position, _)| position)
            .collect()
    }
}
