//! Boolean predicate credit shared with diff analysis, scoped to #8610.
//!
//! The extractor still calls a bare assertion weak. A repo boundary may raise
//! its owner-relative strength only after the existing whole-return pin,
//! primitive boolean consistency and same-test boundary pairing all admit it.

use super::value_contradiction::{ExactValueVerdict, boolean_assertion_verdict_for_test};
use super::{FunctionSummary, OracleFact, RepoSeam, RustIndex, SeamKind, TestSummary};
use crate::analysis::classify::{
    OwnerPinSyntax, OwnerReturnPin, ProbeContext, activation_and_boundary_input,
    has_same_test_boundary_oracle_pairing,
};
use crate::domain::{
    DeltaKind, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, SourceLocation,
};

pub(super) struct BooleanBoundaryPin<'a> {
    owner: &'a FunctionSummary,
    probe: Probe,
    pin: OwnerReturnPin,
}

impl<'a> BooleanBoundaryPin<'a> {
    pub(super) fn establish(
        seam: &RepoSeam,
        owner: Option<&'a FunctionSummary>,
        index: &RustIndex,
    ) -> Option<Self> {
        // #11111: a bool owner whose whole tail is the comparison carries it
        // as both a predicate seam and a return-value seam; the same pinned,
        // consistent, boundary-paired assertion observes both, so both read
        // one verdict (`ripr check` already credits the return value).
        if !matches!(
            seam.kind(),
            SeamKind::PredicateBoundary | SeamKind::ReturnValue
        ) {
            return None;
        }
        let owner = owner?;
        if owner.file != seam.file() || owner.id.0.replace('\\', "/") != seam.owner() {
            return None;
        }
        // This is a private input to the existing predicate authorities,
        // not a diff claim or a general seam-to-probe conversion.
        let probe = Probe {
            id: ProbeId(format!("repo-boolean-boundary:{}", seam.id().as_str())),
            location: SourceLocation::new(seam.file().to_path_buf(), seam.display_line(), 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Unknown,
            before: None,
            after: Some(seam.expression().to_string()),
            expression: seam.expression().to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let pin = OwnerReturnPin::establish(&probe, owner, index)?;
        pin.is_unconditional_boolean_tail()
            .then_some(Self { owner, probe, pin })
    }

    pub(super) fn admits(
        &self,
        test: &TestSummary,
        oracle: &OracleFact,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        if oracle.kind != OracleKind::RelationalCheck || oracle.strength != OracleStrength::Weak {
            return false;
        }
        let context = ProbeContext::new(
            &self.probe,
            Some(self.owner),
            Vec::new(),
            false,
            index,
            false,
        );
        if !self.pin.admits(
            test,
            oracle,
            index,
            &|file, name| {
                index.files().get(file).is_none_or(|facts| {
                    context.test_file_imports_foreign_callee_name(file, &facts.source, name)
                })
            },
            syntax,
        ) || !matches!(
            boolean_assertion_verdict_for_test(self.owner, &oracle.text, test, index),
            ExactValueVerdict::Consistent
        ) {
            return false;
        }

        // Never lend another test's (or another call's) boundary input to
        // this assertion. Preserve the original TestSummary and assertion
        // cardinality; filtering a clone could invent singleton admission.
        let activation = activation_and_boundary_input(
            &self.probe,
            Some(self.owner),
            &[test],
            &[],
            None,
            index,
            false,
            None,
        );
        if activation.unresolved_boundary.is_some() {
            return false;
        }
        let selected_assertion = |candidate_test: &TestSummary, candidate: &OracleFact| {
            std::ptr::eq(candidate_test, test) && std::ptr::eq(candidate, oracle)
        };
        has_same_test_boundary_oracle_pairing(
            &self.probe,
            Some(self.owner),
            &[test],
            &activation.activation,
            &selected_assertion,
            &selected_assertion,
            &|_, _| None,
            None,
            index,
        )
    }
}
