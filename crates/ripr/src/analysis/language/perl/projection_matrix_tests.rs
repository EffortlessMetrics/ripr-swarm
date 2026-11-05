//! Matrix conformance for v2 enum dispositions (#3221).

use super::{
    BehaviorHint, OracleKind, OracleStrength, RelationKind, behavior_hint_to_probe_family,
    v2_projection::{
        perl_oracle_kind_to_domain, perl_oracle_strength_to_domain, perl_relation_to_domain,
    },
};
use crate::domain::{
    OracleKind as DomainOracleKind, OracleStrength as DomainOracleStrength, RelationConfidence,
    RelationReason,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const MATRIX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/perl_packet_contract_migration/projection_matrix.v1.json"
));

#[derive(Debug, Deserialize)]
struct MatrixDoc {
    variants: Vec<MatrixRow>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MatrixRow {
    enum_name: String,
    variant: String,
    disposition: String,
    target: String,
    confidence: Option<String>,
}

impl<'de> Deserialize<'de> for MatrixRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(rename = "enum")]
            enum_name: String,
            variant: String,
            disposition: String,
            target: String,
            #[serde(default)]
            confidence: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            enum_name: raw.enum_name,
            variant: raw.variant,
            disposition: raw.disposition,
            target: raw.target,
            confidence: raw.confidence,
        })
    }
}

fn matrix_rows() -> Result<Vec<MatrixRow>, String> {
    serde_json::from_str::<MatrixDoc>(MATRIX)
        .map(|doc| doc.variants)
        .map_err(|err| format!("projection matrix fixture must parse: {err}"))
}

fn row<'a>(
    by_key: &'a BTreeMap<(String, String), MatrixRow>,
    enum_name: &str,
    variant: &str,
) -> Result<&'a MatrixRow, String> {
    by_key
        .get(&(enum_name.to_string(), variant.to_string()))
        .ok_or_else(|| format!("missing matrix row for {enum_name}::{variant}"))
}

#[test]
fn projection_matrix_has_unique_rows_per_enum_variant() -> Result<(), String> {
    let rows = matrix_rows()?;
    let mut seen = BTreeSet::new();
    for row in &rows {
        let key = (&row.enum_name, &row.variant);
        if !seen.insert(key) {
            return Err(format!("duplicate matrix row for {key:?}"));
        }
    }
    if rows.len() < 30 {
        return Err("matrix must cover every reviewed enum variant".to_string());
    }
    Ok(())
}

#[test]
fn projection_matrix_matches_production_dispositions() -> Result<(), String> {
    let rows = matrix_rows()?;
    let mut by_key: BTreeMap<(String, String), MatrixRow> = BTreeMap::new();
    for row in rows {
        by_key.insert((row.enum_name.clone(), row.variant.clone()), row);
    }

    for (kind, variant) in [
        (OracleKind::ExactReturnAssertion, "exact_return_assertion"),
        (
            OracleKind::PredicateBoundaryAssertion,
            "predicate_boundary_assertion",
        ),
        (OracleKind::SmokeOk, "smoke_ok"),
        (OracleKind::ExceptionObserver, "exception_observer"),
        (
            OracleKind::HashOrObjectFieldAssertion,
            "hash_or_object_field_assertion",
        ),
        (OracleKind::OutputObserver, "output_observer"),
        (OracleKind::WarnObserver, "warn_observer"),
        (OracleKind::LogObserver, "log_observer"),
        (OracleKind::MentionOnly, "mention_only"),
        (OracleKind::DiesOnly, "dies_only"),
        (OracleKind::UnknownHelper, "unknown_helper"),
        (
            OracleKind::DynamicFrameworkIndirection,
            "dynamic_framework_indirection",
        ),
        (OracleKind::Unknown, "unknown"),
    ] {
        let matrix_row = row(&by_key, "OracleKind", variant)?;
        let mapped = format!("{:?}", perl_oracle_kind_to_domain(kind));
        if !mapped.contains(&oracle_target_label(&matrix_row.target)) {
            return Err(format!(
                "OracleKind {variant} matrix target {} vs mapped {mapped}",
                matrix_row.target
            ));
        }
    }

    for (strength, variant) in [
        (OracleStrength::StrongExact, "strong_exact"),
        (OracleStrength::WeakSmoke, "weak_smoke"),
        (OracleStrength::WeakBroad, "weak_broad"),
        (OracleStrength::MentionOnly, "mention_only"),
        (OracleStrength::Unknown, "unknown"),
    ] {
        let matrix_row = row(&by_key, "OracleStrength", variant)?;
        let mapped = format!("{:?}", perl_oracle_strength_to_domain(strength));
        if !mapped.contains(&strength_target_label(&matrix_row.target)) {
            return Err(format!(
                "OracleStrength {variant} matrix target {} vs mapped {mapped}",
                matrix_row.target
            ));
        }
    }

    for (hint, variant) in [
        (BehaviorHint::PredicateBoundary, "predicate_boundary"),
        (BehaviorHint::ReturnValue, "return_value"),
        (BehaviorHint::ExceptionPath, "exception_path"),
        (BehaviorHint::HashOrObjectField, "hash_or_object_field"),
        (BehaviorHint::OutputObserver, "output_observer"),
        (BehaviorHint::WarnObserver, "warn_observer"),
        (BehaviorHint::LogObserver, "log_observer"),
        (BehaviorHint::CallEffect, "call_effect"),
        (BehaviorHint::Unknown, "unknown"),
    ] {
        let matrix_row = row(&by_key, "BehaviorHint", variant)?;
        let mapped = format!("{:?}", behavior_hint_to_probe_family(hint));
        if !mapped.contains(&matrix_row.target) {
            return Err(format!(
                "BehaviorHint {variant} matrix target {} vs mapped {mapped}",
                matrix_row.target
            ));
        }
    }

    for (kind, variant) in [
        (RelationKind::DirectOwnerCall, "direct_owner_call"),
        (RelationKind::HelperCall, "helper_call"),
        (RelationKind::MethodReceiver, "method_receiver"),
        (RelationKind::PackageReference, "package_reference"),
        (RelationKind::TestNameMatch, "test_name_match"),
        (RelationKind::FileProximity, "file_proximity"),
        (RelationKind::FixtureSetup, "fixture_setup"),
        (RelationKind::Unknown, "unknown"),
    ] {
        let matrix_row = row(&by_key, "RelationKind", variant)?;
        let (reason, confidence) = perl_relation_to_domain(kind);
        let expected_confidence = matrix_row
            .confidence
            .as_deref()
            .map(relation_confidence_from_matrix_label)
            .transpose()?;
        if confidence != expected_confidence {
            return Err(format!(
                "RelationKind {variant} matrix confidence {:?} vs mapped {confidence:?}",
                matrix_row.confidence
            ));
        }
        let ok = match matrix_row.target.as_str() {
            "advisory_null_reason" => reason.is_none(),
            "DirectOwnerCall" => reason == Some(RelationReason::DirectOwnerCall),
            "HelperOwnerCall" => reason == Some(RelationReason::HelperOwnerCall),
            "ImportPathAffinity" => reason == Some(RelationReason::ImportPathAffinity),
            "OwnerNamedTest" => reason == Some(RelationReason::OwnerNamedTest),
            other => return Err(format!("unexpected relation target {other}")),
        };
        if !ok {
            return Err(format!(
                "RelationKind {variant} target {} vs mapped {reason:?}",
                matrix_row.target
            ));
        }
    }
    Ok(())
}

fn oracle_target_label(target: &str) -> String {
    match target {
        "ExactValue" => format!("{:?}", DomainOracleKind::ExactValue),
        "RelationalCheck" => format!("{:?}", DomainOracleKind::RelationalCheck),
        "SmokeOnly" => format!("{:?}", DomainOracleKind::SmokeOnly),
        "Unknown" => format!("{:?}", DomainOracleKind::Unknown),
        other => other.to_string(),
    }
}

fn strength_target_label(target: &str) -> String {
    match target {
        "Strong" => format!("{:?}", DomainOracleStrength::Strong),
        "Smoke" => format!("{:?}", DomainOracleStrength::Smoke),
        "Weak" => format!("{:?}", DomainOracleStrength::Weak),
        "Unknown" => format!("{:?}", DomainOracleStrength::Unknown),
        other => other.to_string(),
    }
}

fn relation_confidence_from_matrix_label(label: &str) -> Result<RelationConfidence, String> {
    match label {
        "High" => Ok(RelationConfidence::High),
        "Medium" => Ok(RelationConfidence::Medium),
        "Low" => Ok(RelationConfidence::Low),
        other => Err(format!("unexpected relation confidence label {other}")),
    }
}

#[test]
fn mapping_authority_is_not_duplicated_in_mod_rs() -> Result<(), String> {
    let mod_rs = include_str!("../mod.rs");
    if mod_rs.contains("fn perl_oracle_kind_to_domain") {
        return Err("OracleKind mapping must live only in v2_projection/dispositions.rs".into());
    }
    if mod_rs.contains("fn perl_relation_to_domain") {
        return Err("RelationKind mapping must live only in v2_projection/dispositions.rs".into());
    }
    Ok(())
}

#[test]
fn projection_matrix_removal_proof_control() -> Result<(), String> {
    if !matrix_rows()?
        .iter()
        .any(|row| row.enum_name == "OracleKind" && row.variant == "smoke_ok")
    {
        return Err("smoke_ok row is required for removal-proof coverage".into());
    }
    Ok(())
}
