//! Typed v2→domain enum dispositions (#3221).
//!
//! Single mapping authority for Perl packet enums projected into common RIPR
//! evidence. Every variant is handled explicitly in exhaustive matches.

use super::super::{BehaviorHint, OracleKind, OracleStrength, RelationKind};
use crate::domain::ProbeFamily;

pub(in super::super) fn behavior_hint_to_probe_family(hint: BehaviorHint) -> ProbeFamily {
    match hint {
        BehaviorHint::PredicateBoundary => ProbeFamily::Predicate,
        BehaviorHint::ReturnValue => ProbeFamily::ReturnValue,
        BehaviorHint::ExceptionPath => ProbeFamily::ErrorPath,
        BehaviorHint::HashOrObjectField
        | BehaviorHint::OutputObserver
        | BehaviorHint::WarnObserver
        | BehaviorHint::LogObserver
        | BehaviorHint::CallEffect
        | BehaviorHint::Unknown => ProbeFamily::StaticUnknown,
    }
}

pub(in super::super) fn perl_oracle_kind_to_domain(kind: OracleKind) -> crate::domain::OracleKind {
    use crate::domain::OracleKind as DomainOracleKind;
    match kind {
        OracleKind::ExactReturnAssertion => DomainOracleKind::ExactValue,
        OracleKind::PredicateBoundaryAssertion => DomainOracleKind::RelationalCheck,
        OracleKind::SmokeOk => DomainOracleKind::SmokeOnly,
        OracleKind::ExceptionObserver
        | OracleKind::HashOrObjectFieldAssertion
        | OracleKind::OutputObserver
        | OracleKind::WarnObserver
        | OracleKind::LogObserver
        | OracleKind::MentionOnly
        | OracleKind::DiesOnly
        | OracleKind::UnknownHelper
        | OracleKind::DynamicFrameworkIndirection
        | OracleKind::Unknown => DomainOracleKind::Unknown,
    }
}

/// Domain projection for a Perl oracle strength (#3228): same
/// single-authority contract as `perl_oracle_kind_to_domain`.
pub(in super::super) fn perl_oracle_strength_to_domain(
    strength: OracleStrength,
) -> crate::domain::OracleStrength {
    use crate::domain::OracleStrength as DomainOracleStrength;
    match strength {
        OracleStrength::StrongExact => DomainOracleStrength::Strong,
        OracleStrength::WeakSmoke => DomainOracleStrength::Smoke,
        OracleStrength::WeakBroad => DomainOracleStrength::Weak,
        OracleStrength::MentionOnly | OracleStrength::Unknown => DomainOracleStrength::Unknown,
    }
}

pub(in super::super) fn perl_relation_to_domain(
    kind: RelationKind,
) -> (
    Option<crate::domain::RelationReason>,
    Option<crate::domain::RelationConfidence>,
) {
    use crate::domain::{RelationConfidence, RelationReason};
    // Single exhaustive match: maps the Perl relation kind to the domain
    // (RelationReason, RelationConfidence). Advisory-only signals with no
    // clean owner-call mapping (`FileProximity`, `FixtureSetup`, `Unknown`)
    // return (None, None) so they disclose as `relation_reason: null` (legacy
    // advisory style), matching how the TS adapter treats
    // SameFileProximity/DescribeName.
    match kind {
        RelationKind::DirectOwnerCall => (
            Some(RelationReason::DirectOwnerCall),
            Some(RelationConfidence::High),
        ),
        RelationKind::HelperCall => (
            Some(RelationReason::HelperOwnerCall),
            Some(RelationConfidence::High),
        ),
        RelationKind::MethodReceiver => (
            Some(RelationReason::DirectOwnerCall),
            Some(RelationConfidence::High),
        ),
        RelationKind::PackageReference => (
            Some(RelationReason::ImportPathAffinity),
            Some(RelationConfidence::Medium),
        ),
        RelationKind::TestNameMatch => (
            Some(RelationReason::OwnerNamedTest),
            Some(RelationConfidence::Medium),
        ),
        RelationKind::FileProximity | RelationKind::FixtureSetup | RelationKind::Unknown => {
            (None, None)
        }
    }
}
