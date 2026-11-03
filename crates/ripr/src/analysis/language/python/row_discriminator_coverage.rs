//! Row-local exact-discriminator coverage for Python related tests (#5575).
//!
//! Finding-level `python_missing_discriminators` names what would distinguish
//! the change; this pass asks, for one related row, whether its family-selected
//! assertion observes the changed sink and pins that discriminator exactly.

use super::PythonOracleShape;
use crate::domain::{
    MissingDiscriminatorFact, OracleKind, OracleStrength, ProbeFamily, RelatedTestMissSupport,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonRowDiscriminatorCoverage {
    ExactlyDiscriminates,
    ObservesSurfaceButNotExact,
    Unresolved,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonRowDiscriminatorResult {
    pub(super) coverage: PythonRowDiscriminatorCoverage,
    pub(super) support: Option<RelatedTestMissSupport>,
}

pub(super) fn evaluate_row_discriminator_coverage(
    probe_family: &ProbeFamily,
    finding_discriminator: Option<&MissingDiscriminatorFact>,
    sink_observes: bool,
    oracle_shape: Option<PythonOracleShape>,
    oracle_text: Option<&str>,
    oracle_kind: &OracleKind,
    oracle_strength: &OracleStrength,
) -> PythonRowDiscriminatorResult {
    let not_applicable = || PythonRowDiscriminatorResult {
        coverage: PythonRowDiscriminatorCoverage::NotApplicable,
        support: None,
    };
    let unresolved = || PythonRowDiscriminatorResult {
        coverage: PythonRowDiscriminatorCoverage::Unresolved,
        support: None,
    };

    match probe_family {
        ProbeFamily::Predicate | ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => {
            return not_applicable();
        }
        _ => {}
    }
    if !sink_observes {
        return not_applicable();
    }
    if oracle_text.is_none() {
        return not_applicable();
    }
    if oracle_shape == Some(PythonOracleShape::UnknownCustomHelper) {
        return unresolved();
    }
    if oracle_strength.rank() < OracleStrength::Weak.rank() {
        return not_applicable();
    }
    if matches!(
        oracle_strength,
        OracleStrength::Smoke | OracleStrength::Unknown
    ) || matches!(oracle_kind, OracleKind::SmokeOnly)
        || oracle_shape == Some(PythonOracleShape::BroadSmokeAssertion)
    {
        return not_applicable();
    }
    let Some(finding_discriminator) = finding_discriminator else {
        return not_applicable();
    };

    let coverage = family_exactness(
        probe_family,
        finding_discriminator,
        oracle_shape,
        oracle_text.unwrap_or_default(),
        oracle_kind,
        oracle_strength,
    );
    let support = (coverage == PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact)
        .then(|| RelatedTestMissSupport::from_fact(finding_discriminator));
    PythonRowDiscriminatorResult { coverage, support }
}

fn family_exactness(
    probe_family: &ProbeFamily,
    _finding_discriminator: &MissingDiscriminatorFact,
    oracle_shape: Option<PythonOracleShape>,
    oracle_text: &str,
    oracle_kind: &OracleKind,
    oracle_strength: &OracleStrength,
) -> PythonRowDiscriminatorCoverage {
    let gate = oracle_text.trim();
    match probe_family {
        ProbeFamily::ReturnValue => return_value_exactness(gate, oracle_kind, oracle_shape),
        ProbeFamily::FieldConstruction => {
            field_exactness(gate, oracle_kind, oracle_shape, oracle_strength)
        }
        ProbeFamily::ErrorPath => error_path_exactness(gate, oracle_kind, oracle_strength),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            mock_or_output_exactness(gate, oracle_kind, oracle_shape)
        }
        _ => PythonRowDiscriminatorCoverage::NotApplicable,
    }
}

fn return_value_exactness(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_shape: Option<PythonOracleShape>,
) -> PythonRowDiscriminatorCoverage {
    if oracle_pins_return_exactly(gate, oracle_kind, oracle_shape) {
        PythonRowDiscriminatorCoverage::ExactlyDiscriminates
    } else if oracle_observes_return_broadly(gate, oracle_kind, oracle_shape) {
        PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
    } else {
        PythonRowDiscriminatorCoverage::Unresolved
    }
}

fn oracle_pins_return_exactly(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_shape: Option<PythonOracleShape>,
) -> bool {
    if gate.contains("approx(") || gate.contains("pytest.approx") {
        return false;
    }
    *oracle_kind == OracleKind::ExactValue
        && oracle_shape == Some(PythonOracleShape::ExactAssertion)
        && gate.contains("==")
}

fn oracle_observes_return_broadly(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_shape: Option<PythonOracleShape>,
) -> bool {
    if oracle_pins_return_exactly(gate, oracle_kind, oracle_shape) {
        return false;
    }
    matches!(
        oracle_shape,
        Some(
            PythonOracleShape::BoundaryAssertion
                | PythonOracleShape::FieldAssertion
                | PythonOracleShape::OutputAssertion
        )
    ) || gate.contains(" is not None")
        || gate.contains("isinstance(")
        || gate.contains("len(")
        || gate.contains("approx(")
        || gate.contains("pytest.approx")
        || matches!(oracle_kind, OracleKind::RelationalCheck)
}

fn field_exactness(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_shape: Option<PythonOracleShape>,
    oracle_strength: &OracleStrength,
) -> PythonRowDiscriminatorCoverage {
    if *oracle_kind == OracleKind::ExactValue
        && oracle_strength.rank() >= OracleStrength::Strong.rank()
        && matches!(
            oracle_shape,
            Some(PythonOracleShape::ExactAssertion | PythonOracleShape::FieldAssertion)
        )
        && gate.contains("==")
    {
        return PythonRowDiscriminatorCoverage::ExactlyDiscriminates;
    }
    if gate.contains(" in ")
        || gate.contains(" is not None")
        || gate.contains(" is None")
        || matches!(
            oracle_shape,
            Some(PythonOracleShape::BoundaryAssertion | PythonOracleShape::FieldAssertion)
        )
        || (*oracle_kind == OracleKind::RelationalCheck
            && oracle_strength.rank() >= OracleStrength::Weak.rank())
    {
        return PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact;
    }
    PythonRowDiscriminatorCoverage::Unresolved
}

fn error_path_exactness(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_strength: &OracleStrength,
) -> PythonRowDiscriminatorCoverage {
    match oracle_kind {
        OracleKind::ExactErrorVariant
            if oracle_strength.rank() >= OracleStrength::Strong.rank() =>
        {
            let _ = gate;
            // Row-local exactness does not re-prove message discrimination (#9185).
            PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
        }
        OracleKind::BroadError | OracleKind::ExactErrorVariant => {
            PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
        }
        _ => PythonRowDiscriminatorCoverage::Unresolved,
    }
}

fn mock_or_output_exactness(
    gate: &str,
    oracle_kind: &OracleKind,
    oracle_shape: Option<PythonOracleShape>,
) -> PythonRowDiscriminatorCoverage {
    if *oracle_kind == OracleKind::MockExpectation {
        if gate.contains("assert_called_once_with")
            || gate.contains("assert_called_with(")
            || gate.contains("assert_has_calls")
        {
            return PythonRowDiscriminatorCoverage::ExactlyDiscriminates;
        }
        if gate.contains("assert_called")
            || gate.contains("call_count")
            || gate.contains("assert_any_call")
        {
            return PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact;
        }
    }
    if *oracle_kind == OracleKind::ExactValue
        && oracle_shape == Some(PythonOracleShape::OutputAssertion)
        && gate.contains("==")
    {
        return PythonRowDiscriminatorCoverage::ExactlyDiscriminates;
    }
    if matches!(
        oracle_shape,
        Some(PythonOracleShape::OutputAssertion | PythonOracleShape::BoundaryAssertion)
    ) || gate.contains("call_count")
    {
        return PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact;
    }
    PythonRowDiscriminatorCoverage::Unresolved
}
