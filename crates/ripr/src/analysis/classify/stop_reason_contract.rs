//! Normative emission contract for [`StopReason`] (RIPR-SPEC-0411, #5514).
//!
//! Test-only registry: the module is `#[cfg(test)]`, so it carries no
//! dead-code suppression and ships no production surface.

use crate::domain::StopReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum StopReasonEmissionRole {
    /// Valid wire token with gloss and next-step guidance, but no production
    /// push site on current main.
    ReservedWire,
    /// Back-filled by `ensure_unknown_stop_reason` when an unknown class would
    /// otherwise emit an empty `stop_reasons` list.
    UnknownClassBackfill,
    /// Added by `decision::stop_reasons` substring heuristics; annotates only.
    HeuristicAnnotation,
    /// Reach walk limitation (Rust or Python adapter).
    ReachLimitation,
    /// Mapped from a preview `StaticLimitKind` (Python today).
    StaticLimitMapping,
    /// Gap withheld as unknown in `gap_admission`.
    GapAdmission,
    /// Helper-chain identity stop (RIPR-SPEC-0159).
    HelperIdentity,
    /// Compile-time assertion shape (#11547).
    CompileTimeInvariant,
    /// Unsafe contract the compiler does not check (#11549).
    UnsafeObligation,
}

pub(in crate::analysis) struct StopReasonContractEntry {
    pub reason: StopReason,
    pub role: StopReasonEmissionRole,
    /// When true, emitting this reason can withhold or move exposure class.
    pub changes_classification: bool,
    pub governing_spec: &'static str,
}

pub(in crate::analysis) const STOP_REASON_CONTRACT: &[StopReasonContractEntry] = &[
    StopReasonContractEntry {
        reason: StopReason::MaxDepthReached,
        role: StopReasonEmissionRole::ReservedWire,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::ExternalCrateBoundary,
        role: StopReasonEmissionRole::ReservedWire,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::DynamicDispatchUnresolved,
        role: StopReasonEmissionRole::StaticLimitMapping,
        changes_classification: true,
        governing_spec: "RIPR-SPEC-0028",
    },
    StopReasonContractEntry {
        reason: StopReason::ProcMacroOpaque,
        role: StopReasonEmissionRole::HeuristicAnnotation,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::FixtureOpaque,
        role: StopReasonEmissionRole::HeuristicAnnotation,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::FeatureUnknown,
        role: StopReasonEmissionRole::ReservedWire,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::AsyncBoundaryOpaque,
        role: StopReasonEmissionRole::HeuristicAnnotation,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::NoChangedRustLine,
        role: StopReasonEmissionRole::HeuristicAnnotation,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::InfectionEvidenceUnknown,
        role: StopReasonEmissionRole::UnknownClassBackfill,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::PropagationEvidenceUnknown,
        role: StopReasonEmissionRole::UnknownClassBackfill,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::StaticProbeUnknown,
        role: StopReasonEmissionRole::UnknownClassBackfill,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::TransitiveReachUnresolved,
        role: StopReasonEmissionRole::ReachLimitation,
        changes_classification: true,
        governing_spec: "RIPR-SPEC-0114",
    },
    StopReasonContractEntry {
        reason: StopReason::MacroReachUnresolved,
        role: StopReasonEmissionRole::ReachLimitation,
        changes_classification: true,
        governing_spec: "RIPR-SPEC-0117",
    },
    StopReasonContractEntry {
        reason: StopReason::GapEvidenceUnresolved,
        role: StopReasonEmissionRole::GapAdmission,
        changes_classification: true,
        governing_spec: "RIPR-SPEC-0240",
    },
    StopReasonContractEntry {
        reason: StopReason::HelperIdentityUnresolved,
        role: StopReasonEmissionRole::HelperIdentity,
        changes_classification: true,
        governing_spec: "RIPR-SPEC-0159",
    },
    StopReasonContractEntry {
        reason: StopReason::CompileTimeInvariant,
        role: StopReasonEmissionRole::CompileTimeInvariant,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0411",
    },
    StopReasonContractEntry {
        reason: StopReason::UnsafeObligation,
        role: StopReasonEmissionRole::UnsafeObligation,
        changes_classification: false,
        governing_spec: "RIPR-SPEC-0168",
    },
];

pub(in crate::analysis) fn contract_entry(
    reason: &StopReason,
) -> Option<&'static StopReasonContractEntry> {
    STOP_REASON_CONTRACT
        .iter()
        .find(|entry| &entry.reason == reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const PRODUCTION_STOP_REASON_SOURCES: &[&str] = &[
        include_str!("decision.rs"),
        include_str!("gap_admission.rs"),
        include_str!("../language/python.rs"),
        include_str!("../language/rust/mod.rs"),
        include_str!("../language/rust/dependent_scope.rs"),
        include_str!("../language/python/transitive_reach.rs"),
        include_str!("../classifier/finding.rs"),
    ];

    fn all_stop_reason_variants() -> [StopReason; 17] {
        [
            StopReason::MaxDepthReached,
            StopReason::ExternalCrateBoundary,
            StopReason::DynamicDispatchUnresolved,
            StopReason::ProcMacroOpaque,
            StopReason::FixtureOpaque,
            StopReason::FeatureUnknown,
            StopReason::AsyncBoundaryOpaque,
            StopReason::NoChangedRustLine,
            StopReason::InfectionEvidenceUnknown,
            StopReason::PropagationEvidenceUnknown,
            StopReason::StaticProbeUnknown,
            StopReason::TransitiveReachUnresolved,
            StopReason::MacroReachUnresolved,
            StopReason::GapEvidenceUnresolved,
            StopReason::HelperIdentityUnresolved,
            StopReason::CompileTimeInvariant,
            StopReason::UnsafeObligation,
        ]
    }

    #[test]
    fn stop_reason_contract_registry_is_exhaustive_and_unique() {
        let variants = all_stop_reason_variants();
        assert_eq!(
            STOP_REASON_CONTRACT.len(),
            variants.len(),
            "update STOP_REASON_CONTRACT when adding a StopReason variant"
        );
        for reason in variants {
            assert!(
                contract_entry(&reason).is_some(),
                "missing contract entry for {}",
                reason.as_str()
            );
        }
        for entry in STOP_REASON_CONTRACT {
            assert!(
                entry.governing_spec.starts_with("RIPR-SPEC-"),
                "{} must name its governing spec",
                entry.reason.as_str()
            );
            if entry.role == StopReasonEmissionRole::ReservedWire {
                assert!(
                    !entry.changes_classification,
                    "{} is never emitted so it cannot change classification",
                    entry.reason.as_str()
                );
            }
        }
        let mut wires = STOP_REASON_CONTRACT
            .iter()
            .map(|entry| entry.reason.as_str())
            .collect::<Vec<_>>();
        wires.sort_unstable();
        wires.dedup();
        assert_eq!(wires.len(), STOP_REASON_CONTRACT.len());
    }

    #[test]
    fn reserved_wire_stop_reasons_have_no_production_push_sites() {
        for entry in STOP_REASON_CONTRACT {
            if entry.role != StopReasonEmissionRole::ReservedWire {
                continue;
            }
            let needle = format!(".push({}::", stringify_token(&entry.reason));
            for source in PRODUCTION_STOP_REASON_SOURCES {
                assert!(
                    !source.contains(&needle),
                    "{} must stay reserved (no .push site) but found in production source",
                    entry.reason.as_str()
                );
            }
        }
    }

    #[test]
    fn stop_reason_contract_spec_documents_every_wire_value() -> Result<(), String> {
        let spec_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs/RIPR-SPEC-0411-stop-reason-contract.md");
        let spec = std::fs::read_to_string(&spec_path).map_err(|err| {
            format!(
                "RIPR-SPEC-0411 must exist at {}: {err}",
                spec_path.display()
            )
        })?;
        for entry in STOP_REASON_CONTRACT {
            let wire = entry.reason.as_str();
            if !spec.contains(&format!("`{wire}`")) {
                return Err(format!("spec must document wire value `{wire}`"));
            }
        }
        if !spec.contains("Reserved wire values") {
            return Err("spec must name reserved-not-emitted tokens explicitly".to_string());
        }
        Ok(())
    }

    fn stringify_token(reason: &StopReason) -> &'static str {
        match reason {
            StopReason::MaxDepthReached => "StopReason::MaxDepthReached",
            StopReason::ExternalCrateBoundary => "StopReason::ExternalCrateBoundary",
            StopReason::DynamicDispatchUnresolved => "StopReason::DynamicDispatchUnresolved",
            StopReason::ProcMacroOpaque => "StopReason::ProcMacroOpaque",
            StopReason::FixtureOpaque => "StopReason::FixtureOpaque",
            StopReason::FeatureUnknown => "StopReason::FeatureUnknown",
            StopReason::AsyncBoundaryOpaque => "StopReason::AsyncBoundaryOpaque",
            StopReason::NoChangedRustLine => "StopReason::NoChangedRustLine",
            StopReason::InfectionEvidenceUnknown => "StopReason::InfectionEvidenceUnknown",
            StopReason::PropagationEvidenceUnknown => "StopReason::PropagationEvidenceUnknown",
            StopReason::StaticProbeUnknown => "StopReason::StaticProbeUnknown",
            StopReason::TransitiveReachUnresolved => "StopReason::TransitiveReachUnresolved",
            StopReason::MacroReachUnresolved => "StopReason::MacroReachUnresolved",
            StopReason::GapEvidenceUnresolved => "StopReason::GapEvidenceUnresolved",
            StopReason::HelperIdentityUnresolved => "StopReason::HelperIdentityUnresolved",
            StopReason::CompileTimeInvariant => "StopReason::CompileTimeInvariant",
            StopReason::UnsafeObligation => "StopReason::UnsafeObligation",
        }
    }
}
