use super::PythonOracleShape;
use super::row_discriminator_coverage::{
    PythonRowDiscriminatorCoverage, evaluate_row_discriminator_coverage,
};
use crate::domain::{MissingDiscriminatorFact, OracleKind, OracleStrength, ProbeFamily};

fn fact(value: &str) -> MissingDiscriminatorFact {
    MissingDiscriminatorFact {
        value: value.to_string(),
        reason: "test".to_string(),
        flow_sink: None,
    }
}

#[test]
fn exact_return_equality_exactly_discriminates() {
    let finding = fact("return value == 42");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::ExactAssertion),
        Some("assert parse() == 42"),
        &OracleKind::ExactValue,
        &OracleStrength::Strong,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::ExactlyDiscriminates
    );
    assert!(result.support.is_none());
}

#[test]
fn broad_return_observer_is_surface_but_not_exact_with_support() {
    let finding = fact("return value == 42");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::BoundaryAssertion),
        Some("assert parse() is not None"),
        &OracleKind::RelationalCheck,
        &OracleStrength::Strong,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
    );
    assert_eq!(
        result.support.as_ref().map(|s| s.value.as_str()),
        Some("return value == 42")
    );
}

#[test]
fn wrong_sink_is_not_applicable() {
    let finding = fact("field port == 9090");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::FieldConstruction,
        Some(&finding),
        false,
        Some(PythonOracleShape::ExactAssertion),
        Some("assert cfg.host == \"localhost\""),
        &OracleKind::ExactValue,
        &OracleStrength::Strong,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::NotApplicable
    );
}

#[test]
fn predicate_family_is_not_applicable() {
    let finding = fact("qty == on_hand");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::Predicate,
        Some(&finding),
        true,
        Some(PythonOracleShape::ExactAssertion),
        Some("assert score(0, 0) == 0"),
        &OracleKind::ExactValue,
        &OracleStrength::Strong,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::NotApplicable
    );
}

#[test]
fn smoke_oracle_does_not_upgrade_to_exact_miss() {
    let finding = fact("return value == 42");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::BroadSmokeAssertion),
        Some("assert True"),
        &OracleKind::SmokeOnly,
        &OracleStrength::Smoke,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::NotApplicable
    );
}

#[test]
fn admission_unknown_is_unresolved() {
    let finding = fact("return value == 42");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::UnknownCustomHelper),
        Some("assert custom_check()"),
        &OracleKind::Unknown,
        &OracleStrength::Unknown,
    );
    assert_eq!(result.coverage, PythonRowDiscriminatorCoverage::Unresolved);
}

#[test]
fn broad_exception_observer_carries_support() {
    let finding = fact("raises ValueError matching \"bad port\"");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ErrorPath,
        Some(&finding),
        true,
        Some(PythonOracleShape::ExceptionAssertion),
        Some("with pytest.raises(ValueError): fn()"),
        &OracleKind::BroadError,
        &OracleStrength::Weak,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
    );
    assert!(result.support.is_some());
}

#[test]
fn per_row_coverage_does_not_cross_rows() {
    let finding = fact("return value == 42");
    let a = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::ExactAssertion),
        Some("assert parse() == 42"),
        &OracleKind::ExactValue,
        &OracleStrength::Strong,
    );
    let b = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::BoundaryAssertion),
        Some("assert parse() is not None"),
        &OracleKind::RelationalCheck,
        &OracleStrength::Strong,
    );
    assert_eq!(
        a.coverage,
        PythonRowDiscriminatorCoverage::ExactlyDiscriminates
    );
    assert_eq!(
        b.coverage,
        PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
    );
    assert!(a.support.is_none());
    assert!(b.support.is_some());
}

#[test]
fn pytest_approx_equality_is_not_exact_pin() {
    let finding = fact("return value == 11");
    let result = evaluate_row_discriminator_coverage(
        &ProbeFamily::ReturnValue,
        Some(&finding),
        true,
        Some(PythonOracleShape::ExactAssertion),
        Some("assert f() == pytest.approx(10, abs=2)"),
        &OracleKind::ExactValue,
        &OracleStrength::Strong,
    );
    assert_eq!(
        result.coverage,
        PythonRowDiscriminatorCoverage::ObservesSurfaceButNotExact
    );
}
