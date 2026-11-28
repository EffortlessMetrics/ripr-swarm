use serde::{Deserialize, Serialize};

use crate::domain::ExposureClass;

/// Schema version that [`RiprProviderCapabilitySetV1::validate`] requires.
pub const RIPR_PROVIDER_CAPABILITY_SCHEMA_VERSION: &str = "ripr_provider_capabilities.v1";
/// Schema version that [`RiprAnalysisRequestV1::validate`] requires, and that
/// every capability descriptor must name.
pub const RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION: &str = "ripr_analysis_request.v1";
/// Schema version that [`RiprAnalysisReceiptV1::validate`] requires, and that
/// every capability descriptor must name.
pub const RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION: &str = "ripr_analysis_receipt.v1";
/// The exact claim-boundary text that capability descriptors and receipts must
/// carry.
pub const RIPR_PROVIDER_CLAIM_BOUNDARY: &str = "Static RIPR evidence for one exact source snapshot and selected seam; no runtime test execution, mutation adequacy, semantic correctness, or merge-readiness claim.";
/// The claims every descriptor and receipt must exclude, each exactly once.
pub const RIPR_REQUIRED_EXCLUDED_CLAIMS: [&str; 4] = [
    "runtime_test_execution",
    "mutation_adequacy",
    "semantic_correctness",
    "merge_readiness",
];

/// One analysis RIPR offers to an external orchestrator, serialized as its
/// `snake_case` name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprProviderCapabilityV1 {
    /// Summary of how related tests grip the selected seam.
    TestGripSummary,
    /// Activation, propagation and observability analysis.
    ActivationPropagationObservableAnalysis,
    /// Analysis of the discriminators the change requires.
    RequiredDiscriminatorAnalysis,
    /// Analysis of what the related oracles observe.
    OracleScopeAnalysis,
    /// Evidence summary for the selected seam.
    SelectedSeamEvidenceSummary,
    /// Validation of a captured RIPR receipt. The read-only descriptor marks it
    /// as not reading source.
    CapturedRiprReceiptValidation,
}

/// What one capability does and the authority it needs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderCapabilityDescriptorV1 {
    /// The capability described.
    pub capability: RiprProviderCapabilityV1,
    /// Whether the capability reads repository source.
    pub reads_source: bool,
    /// Whether it writes source. Validation requires `false`.
    pub writes_source: bool,
    /// Whether it runs project commands. Validation requires `false`.
    pub executes_project_commands: bool,
    /// Whether it uses the network. Validation requires `false`.
    pub uses_network: bool,
    /// Must be [`RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION`].
    pub supported_request_schema: String,
    /// Must be [`RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION`].
    pub supported_receipt_schema: String,
    /// Must be [`RIPR_PROVIDER_CLAIM_BOUNDARY`].
    pub claim_boundary: String,
    /// Must hold each of [`RIPR_REQUIRED_EXCLUDED_CLAIMS`] exactly once.
    pub excluded_claims: Vec<String>,
}

/// The capabilities one RIPR build advertises.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderCapabilitySetV1 {
    /// Must be [`RIPR_PROVIDER_CAPABILITY_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Provider identity. Validation requires `ripr`.
    pub provider: String,
    /// Nonblank provider version.
    pub provider_version: String,
    /// A nonempty list with no capability repeated.
    pub capabilities: Vec<RiprProviderCapabilityDescriptorV1>,
}

/// Which view of the repository a snapshot names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprSourceViewV1 {
    /// A Git tree object. `snapshot_id` is `git-tree:<object-id>`.
    GitTree,
    /// The Git index. `snapshot_id` is `git-index:<source_digest>`.
    GitIndex,
    /// The working tree. `snapshot_id` is `worktree:<source_digest>`.
    Worktree,
    /// A captured source set. `snapshot_id` is `captured:<source_digest>`.
    CapturedSourceSet,
}

/// Exact repository source snapshot selected by an external orchestrator.
///
/// This DTO binds identities supplied to RIPR. Resolving a Git object against a
/// repository remains the provider operation's responsibility rather than a
/// deserialization or receipt-validation side effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprRepositorySnapshotV1 {
    /// Nonblank repository identity.
    pub repository_id: String,
    /// Canonical source-view identity. Git trees use
    /// `git-tree:<40-or-64-lowercase-hex-object-id>`; derived views embed their
    /// SHA-256 source digest after the view prefix.
    pub snapshot_id: String,
    /// Which source view `snapshot_id` names.
    pub source_view: RiprSourceViewV1,
    /// Portable SHA-256 identity binding. For a Git tree, hash the exact UTF-8
    /// bytes of the canonical `snapshot_id`, including the `git-tree:` prefix.
    pub source_digest: String,
}

/// The seam and subject a request asks RIPR to analyze.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprEvidenceSubjectV1 {
    /// Optional requirement identity; nonblank when present.
    pub requirement_id: Option<String>,
    /// Optional statement of what the evidence is for; nonblank when present.
    pub evidence_purpose: Option<String>,
    /// Nonblank seam identity. A receipt summary must name the same seam.
    pub seam_id: String,
    /// Nonblank subject identity.
    pub subject_id: String,
    /// `sha256:<64 hex>` digest of the subject body.
    pub subject_body_digest: String,
}

/// An external orchestrator's request for one RIPR analysis of one subject at
/// one exact snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprAnalysisRequestV1 {
    /// Must be [`RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Nonblank request identity.
    pub request_id: String,
    /// The capability requested.
    pub capability: RiprProviderCapabilityV1,
    /// The exact source snapshot to analyze.
    pub repository: RiprRepositorySnapshotV1,
    /// The seam and subject to analyze.
    pub subject: RiprEvidenceSubjectV1,
    /// Nonblank analysis mode.
    pub analysis_mode: String,
    /// Nonblank analysis profile.
    pub profile: String,
    /// `sha256:<64 hex>` digest of the configuration.
    pub config_digest: String,
    /// Nonblank analyzer generation. A receipt must report the same value.
    pub analyzer_generation: String,
    /// Where outputs go: a portable repository-relative path with no empty,
    /// `.`, `..` or colon-bearing component and no backslash.
    pub output_root: String,
    /// Nonblank statement of the claim requested.
    pub requested_claim: String,
}

/// RIPR's own exposure class, as reported on a receipt.
pub type RiprProviderNativeStatusV1 = ExposureClass;

/// How a receipt's analysis ended. Only `Completed` and `Findings` are
/// authoritative: they require a complete, untruncated analysis with a native
/// status and summary. Every other class must carry neither, must not claim a
/// complete analysis, and must disclose a diagnostic or limitation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiprProviderResultClassV1 {
    /// The analysis completed.
    Completed,
    /// The analysis completed with findings.
    Findings,
    /// Only part of the analysis ran.
    Partial,
    /// The input was stale.
    StaleInput,
    /// The request was unsupported.
    Unsupported,
    /// The input was malformed.
    MalformedInput,
    /// The analyzer itself failed.
    InstrumentFailure,
    /// The analysis was cancelled.
    Cancelled,
    /// The analysis did not establish a result.
    NotProven,
}

/// One diagnostic on a receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderDiagnosticV1 {
    /// Nonblank diagnostic code.
    pub code: String,
    /// Nonblank message.
    pub message: String,
    /// Optional portable repository-relative path the diagnostic concerns.
    pub source_path: Option<String>,
    /// 1-based line. Set together with `start_column`, or neither.
    pub start_line: Option<u32>,
    /// 1-based column. Set together with `start_line`, or neither.
    pub start_column: Option<u32>,
    /// Optional next step; nonblank when present.
    pub next_action: Option<String>,
}

/// One related test in a receipt's evidence summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderEvidenceEntryV1 {
    /// Nonblank test name.
    pub test_name: String,
    /// Portable repository-relative path of the test file.
    pub file: String,
    /// Positive line of the test.
    pub line: u64,
    /// An oracle kind wire word.
    pub oracle_kind: String,
    /// An oracle strength wire word.
    pub oracle_strength: String,
    /// A relation reason wire word.
    pub relation_reason: String,
    /// Whether a producer-owned test target identity exists for the test.
    pub has_test_target: bool,
}

/// Copy a row from the analyzer's portable report projection. Receipt validation
/// checks its wire vocabulary and path alongside the rest of the summary.
impl From<crate::domain::TestEvidenceEntry> for RiprProviderEvidenceEntryV1 {
    fn from(entry: crate::domain::TestEvidenceEntry) -> Self {
        Self {
            test_name: entry.test_name,
            file: entry.file,
            line: entry.line as u64,
            oracle_kind: entry.oracle_kind,
            oracle_strength: entry.oracle_strength,
            relation_reason: entry.relation_reason,
            has_test_target: entry.has_test_target,
        }
    }
}

/// The evidence summary an authoritative receipt carries for its seam.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprProviderEvidenceSummaryV1 {
    /// Must match the request's `seam_id`.
    pub seam_id: String,
    /// Number of subjects analyzed. Nonzero on an authoritative receipt.
    pub analyzed_subject_count: u64,
    /// Related tests. May be empty only when the native status is
    /// `no_static_path`.
    pub related_tests: Vec<RiprProviderEvidenceEntryV1>,
    /// Subjects missing a discriminator; at most `analyzed_subject_count`.
    pub missing_discriminator_count: u64,
    /// The strongest `oracle_strength` among `related_tests`, or `none` when
    /// there are none.
    pub strongest_oracle: String,
    /// Nonblank evidence fingerprint.
    pub fingerprint: String,
}

/// RIPR's answer to one [`RiprAnalysisRequestV1`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiprAnalysisReceiptV1 {
    /// Must be [`RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION`].
    pub schema_version: String,
    /// The request this receipt answers, validated with it.
    pub request: RiprAnalysisRequestV1,
    /// Nonblank provider version.
    pub provider_version: String,
    /// `sha256:<64 hex>` digest of the RIPR binary.
    pub binary_digest: String,
    /// Must equal the request's `analyzer_generation`.
    pub analyzer_generation: String,
    /// How the analysis ended.
    pub result_class: RiprProviderResultClassV1,
    /// The exposure class; present exactly on authoritative results.
    pub native_status: Option<RiprProviderNativeStatusV1>,
    /// Whether the analysis completed. Never together with `truncated`.
    pub analysis_complete: bool,
    /// Whether the analysis was truncated.
    pub truncated: bool,
    /// The evidence summary; present exactly on authoritative results.
    pub summary: Option<RiprProviderEvidenceSummaryV1>,
    /// Diagnostics about the run.
    pub diagnostics: Vec<RiprProviderDiagnosticV1>,
    /// Stated limitations of the result.
    pub limitations: Vec<String>,
    /// Must be [`RIPR_PROVIDER_CLAIM_BOUNDARY`].
    pub claim_boundary: String,
    /// Must hold each of [`RIPR_REQUIRED_EXCLUDED_CLAIMS`] exactly once.
    pub excluded_claims: Vec<String>,
}

/// Which rule a provider contract document broke.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiprProviderContractErrorCodeV1 {
    /// A required value is blank or absent, or a required set is incomplete.
    MissingField,
    /// A schema version is not the supported one.
    UnsupportedSchema,
    /// A capability set lists a capability twice.
    DuplicateCapability,
    /// A capability is not read-only, offline and non-executing, or the claim
    /// boundary differs from the canonical text.
    AuthorityViolation,
    /// The request's `output_root` is not a portable relative path.
    UnsafeOutputRoot,
    /// Two identities that must agree do not.
    IdentityMismatch,
    /// Result class, completeness, truncation, status and summary disagree.
    CompletenessConflict,
    /// An identity, digest, path, position or wire word is malformed.
    MalformedIdentity,
}

/// A provider contract validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiprProviderContractErrorV1 {
    /// The rule broken.
    pub code: RiprProviderContractErrorCodeV1,
    /// Human description of the failure.
    pub message: String,
}

impl std::fmt::Display for RiprProviderContractErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RiprProviderContractErrorV1 {}
