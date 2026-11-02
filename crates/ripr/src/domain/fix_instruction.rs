//! A closed, producer-backed fix-instruction state derived from the
//! [`DiagnosticWitness`]. Every surface (diagnostics, hover, code actions,
//! repair packets) projects from this single summary so that the
//! fix-readiness vocabulary is consistent and never re-inferred per surface.
//!
//! See #1663 / #1752. This is NOT a second repair-readiness authority —
//! [`crate::analysis::repair_route::RepairRouteReadiness`] is the route
//! readiness gate; this type is the projection of witness availability.

use super::DiagnosticWitness;
use super::canonical_item::CanonicalItemId;
use std::fmt;

/// The closed 5-state fix-instruction vocabulary. Each state is derived
/// from existing producer-owned witness fields; no field is fabricated.
///
/// States not yet derivable (`exact_edit_ready`, `exact_assertion_ready`,
/// `verification_ready`) are reserved for when #1658/#1660/#1571 land;
/// they are intentionally absent from the current derivation.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FixInstructionState {
    /// A producer-owned fix site (file/line/test) is available, but no
    /// exact assertion or edit is ready. The user can navigate to the
    /// test and inspect the current oracle.
    FixSiteReady,
    /// The diagnostic is a static limitation — the producer could not
    /// resolve a route. The user can inspect the evidence but no bounded
    /// repair instruction is available.
    StaticLimitation,
    /// The snapshot is stale — the analysis inputs have changed since the
    /// diagnostic was produced. The user should refresh before acting.
    Stale,
    /// The diagnostic is informational only (no fix site, no limitation,
    /// no actionable route). The user can inspect but there is nothing
    /// to fix.
    InspectOnly,
    /// No producer-owned witness is available for this diagnostic at all.
    Unavailable,
}

/// Portable instruction meaning for one canonical item (#4805).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct InstructionSemanticId(pub String);

/// One semantic instruction bound to a completed snapshot and source facts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct InstructionInstanceId(pub String);

impl<'de> serde::Deserialize<'de> for InstructionSemanticId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl<'de> serde::Deserialize<'de> for InstructionInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstructionIdentityParseError {
    Empty,
    WrongClass {
        value: String,
        expected: &'static str,
    },
}

impl fmt::Display for InstructionIdentityParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("instruction identity must not be empty"),
            Self::WrongClass { value, expected } => {
                write!(f, "value `{value}` is not a {expected}")
            }
        }
    }
}

impl InstructionSemanticId {
    pub fn parse(value: &str) -> Result<Self, InstructionIdentityParseError> {
        parse_prefixed_id(value, "instruction-semantic:", "InstructionSemanticId")
            .map(InstructionSemanticId)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl InstructionInstanceId {
    pub fn parse(value: &str) -> Result<Self, InstructionIdentityParseError> {
        parse_prefixed_id(value, "instruction-instance:", "InstructionInstanceId")
            .map(InstructionInstanceId)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn parse_prefixed_id(
    value: &str,
    prefix: &str,
    label: &'static str,
) -> Result<String, InstructionIdentityParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(InstructionIdentityParseError::Empty);
    }
    if !trimmed.starts_with(prefix) {
        return Err(InstructionIdentityParseError::WrongClass {
            value: trimmed.to_string(),
            expected: label,
        });
    }
    Ok(trimmed.to_string())
}

/// Completed-snapshot axis for instruction instance identity (#4805).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionSnapshotCurrentness {
    Current,
    AcceptedDirtyDraft,
}

/// Snapshot and source facts needed to mint an instruction instance id.
#[derive(Clone, Debug, PartialEq)]
pub struct FixInstructionIdentityContext<'a> {
    pub canonical_item: Option<&'a CanonicalItemId>,
    pub workspace_identity: Option<&'a str>,
    pub repository_head: Option<&'a str>,
    pub snapshot_currentness: Option<InstructionSnapshotCurrentness>,
    pub witness: Option<&'a DiagnosticWitness>,
}

/// A bounded fix-instruction summary projected from the diagnostic witness.
/// Not a second repair-readiness authority — derived, not authoritative.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FixInstructionSummary {
    pub state: FixInstructionState,
    /// True when a producer-owned fix site (file/line/test) exists.
    pub has_fix_site: bool,
    /// True when a producer-owned suggested assertion exists.
    pub has_suggested_assertion: bool,
    /// The named limitation kinds from the witness, if any.
    pub limitation_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_semantic_id: Option<InstructionSemanticId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_instance_id: Option<InstructionInstanceId>,
}

impl FixInstructionSummary {
    /// Derive the summary from a [`DiagnosticWitness`]. The derivation is
    /// conservative: when in doubt, prefer a more limited state.
    pub fn from_witness(witness: &DiagnosticWitness) -> Self {
        let has_fix_site = witness.fix_site.is_some();
        let has_suggested_assertion = witness.suggested_assertion.is_some();
        let limitation_kinds = witness
            .limitations
            .iter()
            .map(|limitation| limitation.kind.clone())
            .collect::<Vec<_>>();

        let is_stale = witness
            .limitations
            .iter()
            .any(|limitation| limitation.kind.contains("stale"));

        let is_static_limitation = witness.limitations.iter().any(|limitation| {
            limitation.kind.contains("missing_discriminator")
                || limitation.kind.contains("static_limit")
        });

        let state = if is_stale {
            FixInstructionState::Stale
        } else if is_static_limitation {
            FixInstructionState::StaticLimitation
        } else if has_fix_site {
            FixInstructionState::FixSiteReady
        } else if limitation_kinds.is_empty() {
            // No limitations and no fix site — informational only.
            FixInstructionState::InspectOnly
        } else {
            FixInstructionState::Unavailable
        };

        Self {
            state,
            has_fix_site,
            has_suggested_assertion,
            limitation_kinds,
            instruction_semantic_id: None,
            instruction_instance_id: None,
        }
    }

    /// Attach typed semantic/instance identities when the producer supplies
    /// canonical item and snapshot context.
    pub fn attach_identities(&mut self, context: &FixInstructionIdentityContext<'_>) {
        let Some(canonical_item) = context.canonical_item else {
            self.instruction_semantic_id = None;
            self.instruction_instance_id = None;
            return;
        };
        self.instruction_semantic_id = Some(mint_instruction_semantic_id(
            canonical_item,
            self,
            context.witness,
        ));
        self.instruction_instance_id =
            mint_instruction_instance_id(self.instruction_semantic_id.as_ref(), context);
    }

    /// A stale or snapshot-mismatched instance must not authorize a current op.
    pub fn instance_authorizes_current_operation(
        &self,
        context: &FixInstructionIdentityContext<'_>,
    ) -> bool {
        if self.state == FixInstructionState::Stale {
            return false;
        }
        if context.workspace_identity.is_none()
            || context.repository_head.is_none()
            || context.snapshot_currentness.is_none()
        {
            return false;
        }
        let Some(expected) =
            mint_instruction_instance_id(self.instruction_semantic_id.as_ref(), context)
        else {
            return false;
        };
        self.instruction_instance_id.as_ref() == Some(&expected)
    }

    /// Derive the summary for a diagnostic with no witness at all.
    pub fn unavailable() -> Self {
        Self {
            state: FixInstructionState::Unavailable,
            has_fix_site: false,
            has_suggested_assertion: false,
            limitation_kinds: Vec::new(),
            instruction_semantic_id: None,
            instruction_instance_id: None,
        }
    }
}

fn mint_instruction_semantic_id(
    canonical_item: &CanonicalItemId,
    summary: &FixInstructionSummary,
    witness: Option<&DiagnosticWitness>,
) -> InstructionSemanticId {
    let mut kinds = summary.limitation_kinds.clone();
    kinds.sort();
    kinds.dedup();
    let witness_kind = witness.map(|value| value.kind.as_str()).unwrap_or("");
    let digest = identity_digest(&[
        ("canonical_item", canonical_item.as_str()),
        ("state", state_token(summary.state)),
        ("has_fix_site", if summary.has_fix_site { "1" } else { "0" }),
        ("limitation_kinds", &kinds.join("|")),
        ("witness_kind", witness_kind),
    ]);
    InstructionSemanticId(format!("instruction-semantic:{digest:016x}"))
}

fn mint_instruction_instance_id(
    semantic: Option<&InstructionSemanticId>,
    context: &FixInstructionIdentityContext<'_>,
) -> Option<InstructionInstanceId> {
    let semantic = semantic?;
    if context.workspace_identity.is_none()
        || context.repository_head.is_none()
        || context.snapshot_currentness.is_none()
    {
        return None;
    }
    let workspace = context.workspace_identity.unwrap_or("");
    let head = context.repository_head.unwrap_or("");
    let currentness = match context.snapshot_currentness {
        Some(InstructionSnapshotCurrentness::Current) => "current",
        Some(InstructionSnapshotCurrentness::AcceptedDirtyDraft) => "accepted_dirty_draft",
        None => "",
    };
    let mut source_facts: Vec<(&str, String)> = Vec::new();
    if let Some(witness) = context.witness {
        source_facts.push(("changed_expression", witness.changed_expression.clone()));
        if let Some(site) = &witness.fix_site {
            source_facts.push(("fix_site_file", site.file.clone()));
            source_facts.push(("fix_site_line", site.line.to_string()));
            source_facts.push(("fix_site_test", site.test_name.clone()));
        }
    }
    let mut parts: Vec<(&str, &str)> = vec![
        ("instruction_semantic", semantic.as_str()),
        ("workspace_identity", workspace),
        ("repository_head", head),
        ("snapshot_currentness", currentness),
    ];
    for (key, value) in &source_facts {
        parts.push((key, value.as_str()));
    }
    let digest = identity_digest(&parts);
    Some(InstructionInstanceId(format!(
        "instruction-instance:{digest:016x}"
    )))
}

fn state_token(state: FixInstructionState) -> &'static str {
    match state {
        FixInstructionState::FixSiteReady => "fix_site_ready",
        FixInstructionState::StaticLimitation => "static_limitation",
        FixInstructionState::Stale => "stale",
        FixInstructionState::InspectOnly => "inspect_only",
        FixInstructionState::Unavailable => "unavailable",
    }
}

fn identity_digest(parts: &[(&str, &str)]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for (key, value) in parts {
        for byte in key.as_bytes().iter().chain([0].iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for byte in value.as_bytes().iter().chain([0].iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DiagnosticConfidence, DiagnosticWitness, DiagnosticWitnessLimitation};

    fn witness_with_fix_site() -> DiagnosticWitness {
        DiagnosticWitness {
            kind: "static_discriminator_gap".to_string(),
            probe_family: "test".to_string(),
            changed_expression: "expr".to_string(),
            before: None,
            after: None,
            expected_sink: None,
            missing_discriminators: Vec::new(),
            fix_site: Some(crate::domain::DiagnosticFixSite {
                file: "src/lib.rs".to_string(),
                line: 10,
                test_name: "test_foo".to_string(),
                current_oracle: None,
                oracle_kind: "exact_value".to_string(),
                oracle_strength: "strong".to_string(),
                oracle_location: None,
            }),
            suggested_assertion: None,
            explain_command: "ripr explain".to_string(),
            confidence: DiagnosticConfidence {
                value: Some(0.8),
                basis: "static_only".to_string(),
            },
            limitations: Vec::new(),
        }
    }

    fn witness_with_limitation(kind: &str) -> DiagnosticWitness {
        let mut witness = witness_with_fix_site();
        witness.fix_site = None;
        witness.limitations = vec![DiagnosticWitnessLimitation {
            kind: kind.to_string(),
            detail: "test limitation".to_string(),
        }];
        witness
    }

    fn bare_witness() -> DiagnosticWitness {
        let mut witness = witness_with_fix_site();
        witness.fix_site = None;
        witness.limitations = Vec::new();
        witness
    }

    #[test]
    fn fix_site_ready_when_fix_site_present_and_no_limitation() {
        let summary = FixInstructionSummary::from_witness(&witness_with_fix_site());
        assert_eq!(summary.state, FixInstructionState::FixSiteReady);
        assert!(summary.has_fix_site);
    }

    #[test]
    fn static_limitation_when_missing_discriminator() {
        let witness = witness_with_limitation("missing_discriminator_unavailable");
        let summary = FixInstructionSummary::from_witness(&witness);
        assert_eq!(summary.state, FixInstructionState::StaticLimitation);
    }

    #[test]
    fn stale_when_stale_limitation_present() {
        let witness = witness_with_limitation("snapshot_stale");
        let summary = FixInstructionSummary::from_witness(&witness);
        assert_eq!(summary.state, FixInstructionState::Stale);
    }

    #[test]
    fn inspect_only_when_no_fix_site_and_no_limitations() {
        let summary = FixInstructionSummary::from_witness(&bare_witness());
        assert_eq!(summary.state, FixInstructionState::InspectOnly);
    }

    #[test]
    fn unavailable_when_no_witness() {
        let summary = FixInstructionSummary::unavailable();
        assert_eq!(summary.state, FixInstructionState::Unavailable);
        assert!(!summary.has_fix_site);
    }

    #[test]
    fn stale_takes_precedence_over_static_limitation() {
        let mut witness = witness_with_limitation("missing_discriminator_unavailable");
        witness.limitations.push(DiagnosticWitnessLimitation {
            kind: "snapshot_stale".to_string(),
            detail: "stale".to_string(),
        });
        let summary = FixInstructionSummary::from_witness(&witness);
        assert_eq!(summary.state, FixInstructionState::Stale);
    }

    fn identity_context<'a>(
        canonical: &'a CanonicalItemId,
        head: &'a str,
        witness: &'a DiagnosticWitness,
    ) -> FixInstructionIdentityContext<'a> {
        FixInstructionIdentityContext {
            canonical_item: Some(canonical),
            workspace_identity: Some("ws:demo"),
            repository_head: Some(head),
            snapshot_currentness: Some(InstructionSnapshotCurrentness::Current),
            witness: Some(witness),
        }
    }

    #[test]
    fn semantic_id_stable_when_only_snapshot_head_moves() -> Result<(), String> {
        let witness = witness_with_fix_site();
        let canonical = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        let mut first = FixInstructionSummary::from_witness(&witness);
        first.attach_identities(&identity_context(&canonical, "aaa", &witness));
        let mut second = FixInstructionSummary::from_witness(&witness);
        second.attach_identities(&identity_context(&canonical, "bbb", &witness));
        assert_eq!(
            first.instruction_semantic_id, second.instruction_semantic_id,
            "semantic id must not move for snapshot-only change"
        );
        assert_ne!(
            first.instruction_instance_id, second.instruction_instance_id,
            "instance id must move when snapshot head moves"
        );
        Ok(())
    }

    #[test]
    fn semantic_id_changes_when_instruction_state_changes() -> Result<(), String> {
        let ready_witness = witness_with_fix_site();
        let limited_witness = witness_with_limitation("missing_discriminator_unavailable");
        let canonical = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        let mut ready = FixInstructionSummary::from_witness(&ready_witness);
        ready.attach_identities(&identity_context(&canonical, "aaa", &ready_witness));
        let mut limited = FixInstructionSummary::from_witness(&limited_witness);
        limited.attach_identities(&identity_context(&canonical, "aaa", &limited_witness));
        assert_ne!(
            ready.instruction_semantic_id,
            limited.instruction_semantic_id
        );
        Ok(())
    }

    #[test]
    fn stale_instance_does_not_authorize_current_operation() -> Result<(), String> {
        let witness = witness_with_limitation("snapshot_stale");
        let canonical = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        let mut summary = FixInstructionSummary::from_witness(&witness);
        let context = identity_context(&canonical, "aaa", &witness);
        summary.attach_identities(&context);
        assert!(!summary.instance_authorizes_current_operation(&context));
        Ok(())
    }

    #[test]
    fn typed_ids_reject_cross_class_parse() {
        assert!(InstructionSemanticId::parse("instruction-instance:deadbeef").is_err());
        assert!(InstructionInstanceId::parse("instruction-semantic:deadbeef").is_err());
        assert!(InstructionSemanticId::parse("gap:67f9714ba37b3967").is_err());
    }

    #[test]
    fn json_deserialize_rejects_cross_class_instruction_ids() {
        assert!(
            serde_json::from_str::<InstructionSemanticId>("\"instruction-instance:abc\"").is_err()
        );
        assert!(
            serde_json::from_str::<InstructionInstanceId>("\"instruction-semantic:abc\"").is_err()
        );
    }

    #[test]
    fn instance_id_requires_completed_snapshot_fields() -> Result<(), String> {
        let witness = witness_with_fix_site();
        let canonical = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        let mut summary = FixInstructionSummary::from_witness(&witness);
        let partial = FixInstructionIdentityContext {
            canonical_item: Some(&canonical),
            workspace_identity: None,
            repository_head: None,
            snapshot_currentness: None,
            witness: Some(&witness),
        };
        summary.attach_identities(&partial);
        assert!(summary.instruction_semantic_id.is_some());
        assert!(summary.instruction_instance_id.is_none());
        assert!(!summary.instance_authorizes_current_operation(&partial));
        Ok(())
    }
}
