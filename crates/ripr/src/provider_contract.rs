//! Exact-snapshot RIPR provider contracts for external proof orchestration (#3298).
//!
//! The contract exposes RIPR-owned static evidence. It does not run project
//! tests, mutation tools, proof packs, builds, network clients, or source edits.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::TestEvidenceSummary;

pub const RIPR_PROVIDER_CAPABILITY_SCHEMA_VERSION: &str = "ripr_provider_capabilities.v1";
pub const RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION: &str = "ripr_analysis_request.v1";
pub const RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION: &str = "ripr_analysis_receipt.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprProviderCapabilityV1 {
    TestGripSummary,
    ActivationPropagationObservableAnalysis,
    RequiredDiscriminatorAnalysis,
    OracleScopeAnalysis,
    SelectedSeamEvidenceSummary,
    CapturedRiprReceiptValidation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderCapabilityDescriptorV1 {
    pub capability: RiprProviderCapabilityV1,
    pub reads_source: bool,
    pub writes_source: bool,
    pub executes_project_commands: bool,
    pub uses_network: bool,
    pub supported_request_schema: String,
    pub supported_receipt_schema: String,
    pub claim_boundary: String,
    pub excluded_claims: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderCapabilitySetV1 {
    pub schema_version: String,
    pub provider: String,
    pub provider_version: String,
    pub capabilities: Vec<RiprProviderCapabilityDescriptorV1>,
}

impl RiprProviderCapabilitySetV1 {
    pub fn read_only(provider_version: impl Into<String>) -> Self {
        let capabilities = [
            RiprProviderCapabilityV1::TestGripSummary,
            RiprProviderCapabilityV1::ActivationPropagationObservableAnalysis,
            RiprProviderCapabilityV1::RequiredDiscriminatorAnalysis,
            RiprProviderCapabilityV1::OracleScopeAnalysis,
            RiprProviderCapabilityV1::SelectedSeamEvidenceSummary,
            RiprProviderCapabilityV1::CapturedRiprReceiptValidation,
        ]
        .into_iter()
        .map(read_only_descriptor)
        .collect();

        Self {
            schema_version: RIPR_PROVIDER_CAPABILITY_SCHEMA_VERSION.into(),
            provider: "ripr".into(),
            provider_version: provider_version.into(),
            capabilities,
        }
    }

    pub fn validate(&self) -> Result<(), RiprProviderContractErrorV1> {
        if self.schema_version != RIPR_PROVIDER_CAPABILITY_SCHEMA_VERSION {
            return Err(error(
                RiprProviderContractErrorCodeV1::UnsupportedSchema,
                "unsupported RIPR capability schema",
            ));
        }
        require_non_empty("provider", &self.provider)?;
        require_non_empty("provider_version", &self.provider_version)?;
        if self.capabilities.is_empty() {
            return Err(error(
                RiprProviderContractErrorCodeV1::MissingField,
                "capability set is empty",
            ));
        }

        let mut capabilities = self
            .capabilities
            .iter()
            .map(|descriptor| descriptor.capability)
            .collect::<Vec<_>>();
        capabilities.sort();
        capabilities.dedup();
        if capabilities.len() != self.capabilities.len() {
            return Err(error(
                RiprProviderContractErrorCodeV1::DuplicateCapability,
                "capability set contains a duplicate capability",
            ));
        }

        for descriptor in &self.capabilities {
            if descriptor.writes_source
                || descriptor.executes_project_commands
                || descriptor.uses_network
            {
                return Err(error(
                    RiprProviderContractErrorCodeV1::AuthorityViolation,
                    "RIPR provider capabilities must remain read-only, offline, and non-executing",
                ));
            }
            require_non_empty("claim_boundary", &descriptor.claim_boundary)?;
            if descriptor.supported_request_schema != RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION
                || descriptor.supported_receipt_schema != RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION
            {
                return Err(error(
                    RiprProviderContractErrorCodeV1::UnsupportedSchema,
                    "capability descriptor references an unsupported request or receipt schema",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprSourceViewV1 {
    GitTree,
    GitIndex,
    Worktree,
    CapturedSourceSet,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprRepositorySnapshotV1 {
    pub repository_id: String,
    pub snapshot_id: String,
    pub source_view: RiprSourceViewV1,
    pub source_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprEvidenceSubjectV1 {
    pub requirement_id: Option<String>,
    pub evidence_purpose: Option<String>,
    pub seam_id: String,
    pub subject_id: String,
    pub subject_body_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprAnalysisRequestV1 {
    pub schema_version: String,
    pub request_id: String,
    pub capability: RiprProviderCapabilityV1,
    pub repository: RiprRepositorySnapshotV1,
    pub subject: RiprEvidenceSubjectV1,
    pub analysis_mode: String,
    pub profile: String,
    pub config_digest: String,
    pub analyzer_generation: String,
    pub output_root: String,
    pub requested_claim: String,
}

impl RiprAnalysisRequestV1 {
    pub fn validate(&self) -> Result<(), RiprProviderContractErrorV1> {
        if self.schema_version != RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION {
            return Err(error(
                RiprProviderContractErrorCodeV1::UnsupportedSchema,
                "unsupported RIPR analysis request schema",
            ));
        }
        for (field, value) in [
            ("request_id", self.request_id.as_str()),
            ("repository_id", self.repository.repository_id.as_str()),
            ("snapshot_id", self.repository.snapshot_id.as_str()),
            ("source_digest", self.repository.source_digest.as_str()),
            ("seam_id", self.subject.seam_id.as_str()),
            ("subject_id", self.subject.subject_id.as_str()),
            ("subject_body_digest", self.subject.subject_body_digest.as_str()),
            ("analysis_mode", self.analysis_mode.as_str()),
            ("profile", self.profile.as_str()),
            ("config_digest", self.config_digest.as_str()),
            ("analyzer_generation", self.analyzer_generation.as_str()),
            ("output_root", self.output_root.as_str()),
            ("requested_claim", self.requested_claim.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        validate_relative_output_root(&self.output_root)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprProviderNativeStatusV1 {
    LikelyDiscriminating,
    LikelyRelevantWithLimitations,
    PartiallyGripped,
    MissingActivation,
    MissingPropagation,
    MissingObservable,
    MissingRequiredDiscriminator,
    OracleTooBroad,
    WrongSeamOrOwner,
    OpaqueOrUnsupported,
    KnownAnalyzerLimitation,
    NotEvaluated,
    NotProven,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprProviderResultClassV1 {
    Completed,
    Findings,
    Partial,
    StaleInput,
    Unsupported,
    MalformedInput,
    InstrumentFailure,
    Cancelled,
    NotProven,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderDiagnosticV1 {
    pub code: String,
    pub message: String,
    pub source_path: Option<String>,
    pub start_line: Option<u32>,
    pub start_column: Option<u32>,
    pub next_action: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprAnalysisReceiptV1 {
    pub schema_version: String,
    pub request: RiprAnalysisRequestV1,
    pub provider_version: String,
    pub binary_digest: String,
    pub analyzer_generation: String,
    pub result_class: RiprProviderResultClassV1,
    pub native_status: Option<RiprProviderNativeStatusV1>,
    pub analysis_complete: bool,
    pub truncated: bool,
    pub summary: Option<TestEvidenceSummary>,
    pub diagnostics: Vec<RiprProviderDiagnosticV1>,
    pub limitations: Vec<String>,
    pub claim_boundary: String,
    pub excluded_claims: Vec<String>,
}

impl RiprAnalysisReceiptV1 {
    pub fn validate(&self) -> Result<(), RiprProviderContractErrorV1> {
        if self.schema_version != RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION {
            return Err(error(
                RiprProviderContractErrorCodeV1::UnsupportedSchema,
                "unsupported RIPR analysis receipt schema",
            ));
        }
        self.request.validate()?;
        require_non_empty("provider_version", &self.provider_version)?;
        require_non_empty("binary_digest", &self.binary_digest)?;
        require_non_empty("analyzer_generation", &self.analyzer_generation)?;
        require_non_empty("claim_boundary", &self.claim_boundary)?;
        if self.analyzer_generation != self.request.analyzer_generation {
            return Err(error(
                RiprProviderContractErrorCodeV1::IdentityMismatch,
                "receipt analyzer generation does not match the request",
            ));
        }
        if self.analysis_complete && self.truncated {
            return Err(error(
                RiprProviderContractErrorCodeV1::CompletenessConflict,
                "a truncated analysis cannot be complete",
            ));
        }

        match self.result_class {
            RiprProviderResultClassV1::Completed | RiprProviderResultClassV1::Findings => {
                if !self.analysis_complete || self.truncated {
                    return Err(error(
                        RiprProviderContractErrorCodeV1::CompletenessConflict,
                        "complete or findings results require a complete, untruncated analysis",
                    ));
                }
                if self.native_status.is_none() || self.summary.is_none() {
                    return Err(error(
                        RiprProviderContractErrorCodeV1::MissingField,
                        "complete or findings results require native status and summary",
                    ));
                }
            }
            RiprProviderResultClassV1::Partial => {
                if self.analysis_complete && !self.truncated {
                    return Err(error(
                        RiprProviderContractErrorCodeV1::CompletenessConflict,
                        "partial result must disclose incomplete or truncated analysis",
                    ));
                }
            }
            RiprProviderResultClassV1::StaleInput
            | RiprProviderResultClassV1::Unsupported
            | RiprProviderResultClassV1::MalformedInput
            | RiprProviderResultClassV1::InstrumentFailure
            | RiprProviderResultClassV1::Cancelled
            | RiprProviderResultClassV1::NotProven => {
                if self.analysis_complete {
                    return Err(error(
                        RiprProviderContractErrorCodeV1::CompletenessConflict,
                        "non-authoritative result class cannot claim complete analysis",
                    ));
                }
            }
        }

        if let Some(summary) = &self.summary {
            if summary.seam_id != self.request.subject.seam_id {
                return Err(error(
                    RiprProviderContractErrorCodeV1::IdentityMismatch,
                    "summary seam identity does not match the request",
                ));
            }
            require_non_empty("summary.fingerprint", &summary.fingerprint)?;
        }

        if self.excluded_claims.is_empty() {
            return Err(error(
                RiprProviderContractErrorCodeV1::MissingField,
                "receipt must retain explicit excluded claims",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiprProviderContractErrorCodeV1 {
    MissingField,
    UnsupportedSchema,
    DuplicateCapability,
    AuthorityViolation,
    UnsafeOutputRoot,
    IdentityMismatch,
    CompletenessConflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiprProviderContractErrorV1 {
    pub code: RiprProviderContractErrorCodeV1,
    pub message: String,
}

impl fmt::Display for RiprProviderContractErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RiprProviderContractErrorV1 {}

fn read_only_descriptor(capability: RiprProviderCapabilityV1) -> RiprProviderCapabilityDescriptorV1 {
    RiprProviderCapabilityDescriptorV1 {
        capability,
        reads_source: capability != RiprProviderCapabilityV1::CapturedRiprReceiptValidation,
        writes_source: false,
        executes_project_commands: false,
        uses_network: false,
        supported_request_schema: RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION.into(),
        supported_receipt_schema: RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION.into(),
        claim_boundary: "RIPR static evidence for one exact supplied source snapshot".into(),
        excluded_claims: vec![
            "runtime_test_execution".into(),
            "mutation_adequacy".into(),
            "semantic_correctness".into(),
            "merge_readiness".into(),
        ],
    }
}

fn require_non_empty(
    field: &str,
    value: &str,
) -> Result<(), RiprProviderContractErrorV1> {
    if value.trim().is_empty() {
        return Err(error(
            RiprProviderContractErrorCodeV1::MissingField,
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn validate_relative_output_root(value: &str) -> Result<(), RiprProviderContractErrorV1> {
    if value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.split('/').any(|component| component == "..")
        || value
            .split('/')
            .next()
            .is_some_and(|component| component.contains(':'))
    {
        return Err(error(
            RiprProviderContractErrorCodeV1::UnsafeOutputRoot,
            "output_root must be a portable repository-relative path without traversal",
        ));
    }
    Ok(())
}

fn error(
    code: RiprProviderContractErrorCodeV1,
    message: impl Into<String>,
) -> RiprProviderContractErrorV1 {
    RiprProviderContractErrorV1 { code, message: message.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TestEvidenceEntry;

    fn request() -> RiprAnalysisRequestV1 {
        RiprAnalysisRequestV1 {
            schema_version: RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION.into(),
            request_id: "request-1".into(),
            capability: RiprProviderCapabilityV1::TestGripSummary,
            repository: RiprRepositorySnapshotV1 {
                repository_id: "repo".into(),
                snapshot_id: "tree:abc".into(),
                source_view: RiprSourceViewV1::GitTree,
                source_digest: "sha256:source".into(),
            },
            subject: RiprEvidenceSubjectV1 {
                requirement_id: Some("REQ-1".into()),
                evidence_purpose: Some("boundary discriminator".into()),
                seam_id: "seam-1".into(),
                subject_id: "test-1".into(),
                subject_body_digest: "sha256:test".into(),
            },
            analysis_mode: "ready".into(),
            profile: "rust".into(),
            config_digest: "sha256:config".into(),
            analyzer_generation: "ripr-analyzer.v1".into(),
            output_root: "target/ripr/provider".into(),
            requested_claim: "test_grip_summary".into(),
        }
    }

    fn summary() -> TestEvidenceSummary {
        let entries = vec![TestEvidenceEntry {
            test_name: "boundary".into(),
            file: "tests/boundary.rs".into(),
            line: 10,
            oracle_kind: "exact_value".into(),
            oracle_strength: "strong".into(),
            relation_reason: "direct_owner_call".into(),
            has_test_target: true,
        }];
        TestEvidenceSummary {
            seam_id: "seam-1".into(),
            related_tests: entries.clone(),
            missing_discriminator_count: 0,
            strongest_oracle: TestEvidenceSummary::strongest_oracle_from(&entries),
            fingerprint: TestEvidenceSummary::compute_fingerprint(&entries),
        }
    }

    fn receipt() -> RiprAnalysisReceiptV1 {
        RiprAnalysisReceiptV1 {
            schema_version: RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION.into(),
            request: request(),
            provider_version: "0.12.0".into(),
            binary_digest: "sha256:binary".into(),
            analyzer_generation: "ripr-analyzer.v1".into(),
            result_class: RiprProviderResultClassV1::Completed,
            native_status: Some(RiprProviderNativeStatusV1::LikelyDiscriminating),
            analysis_complete: true,
            truncated: false,
            summary: Some(summary()),
            diagnostics: Vec::new(),
            limitations: Vec::new(),
            claim_boundary: "static grip only".into(),
            excluded_claims: vec![
                "runtime_test_execution".into(),
                "semantic_correctness".into(),
            ],
        }
    }

    #[test]
    fn supported_capabilities_are_read_only_offline_and_non_executing() {
        let capabilities = RiprProviderCapabilitySetV1::read_only("0.12.0");
        capabilities.validate().expect("capability set should validate");
        assert!(capabilities.capabilities.iter().all(|descriptor| {
            !descriptor.writes_source
                && !descriptor.executes_project_commands
                && !descriptor.uses_network
        }));
        assert!(capabilities.capabilities.iter().all(|descriptor| {
            descriptor.excluded_claims.iter().any(|claim| claim == "runtime_test_execution")
        }));
    }

    #[test]
    fn exact_complete_receipt_validates() {
        receipt().validate().expect("receipt should validate");
    }

    #[test]
    fn partial_receipt_cannot_claim_complete_analysis() {
        let mut receipt = receipt();
        receipt.result_class = RiprProviderResultClassV1::Partial;
        let error = receipt.validate().expect_err("partial complete receipt must fail");
        assert_eq!(error.code, RiprProviderContractErrorCodeV1::CompletenessConflict);
    }

    #[test]
    fn summary_must_match_requested_seam() {
        let mut receipt = receipt();
        receipt.summary.as_mut().expect("summary").seam_id = "other-seam".into();
        let error = receipt.validate().expect_err("cross-seam receipt must fail");
        assert_eq!(error.code, RiprProviderContractErrorCodeV1::IdentityMismatch);
    }

    #[test]
    fn stale_result_cannot_look_complete() {
        let mut receipt = receipt();
        receipt.result_class = RiprProviderResultClassV1::StaleInput;
        let error = receipt.validate().expect_err("stale complete receipt must fail");
        assert_eq!(error.code, RiprProviderContractErrorCodeV1::CompletenessConflict);
    }

    #[test]
    fn output_root_must_be_portable_and_relative() {
        let mut request = request();
        request.output_root = "../outside".into();
        let error = request.validate().expect_err("traversal must fail");
        assert_eq!(error.code, RiprProviderContractErrorCodeV1::UnsafeOutputRoot);

        request.output_root = "C:/outside".into();
        let error = request.validate().expect_err("drive path must fail");
        assert_eq!(error.code, RiprProviderContractErrorCodeV1::UnsafeOutputRoot);
    }

    #[test]
    fn favorable_static_status_does_not_remove_runtime_non_claim() {
        let receipt = receipt();
        assert_eq!(receipt.native_status, Some(RiprProviderNativeStatusV1::LikelyDiscriminating));
        assert!(receipt.excluded_claims.iter().any(|claim| claim == "runtime_test_execution"));
    }
}
