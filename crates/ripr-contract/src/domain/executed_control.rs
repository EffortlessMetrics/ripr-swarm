//! Executed-control obligation and result vocabulary (RIPR-SPEC-0183 / #4641).
//!
//! This module owns the closed contract that distinguishes an executed
//! discriminating control from an ordinary positive test or a review argument.
//! It does not inspect live GitHub state, enforce merge eligibility, execute
//! commands, or rewrite historical issues as pass.

use super::GitObjectId;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Schema version for executed-control obligation, result, and packet records.
pub const EXECUTED_CONTROL_SCHEMA_VERSION: &str = "1";

/// Kind string for an obligation record.
pub const EXECUTED_CONTROL_OBLIGATION_KIND: &str = "executed_control_obligation";
/// Kind string for a result record.
pub const EXECUTED_CONTROL_RESULT_KIND: &str = "executed_control_result";
/// Kind string for a packet record.
pub const EXECUTED_CONTROL_PACKET_KIND: &str = "executed_control_packet";

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;

/// Exact control class named by an obligation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlClass {
    /// `removed_guard`: the control removes a guard the change adds.
    RemovedGuard,
    /// `wrong_implementation`: the control swaps in a deliberately wrong
    /// implementation.
    WrongImplementation,
    /// `named_mutation`: the control applies one named mutation.
    NamedMutation,
}

impl ControlClass {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RemovedGuard => "removed_guard",
            Self::WrongImplementation => "wrong_implementation",
            Self::NamedMutation => "named_mutation",
        }
    }
}

/// Whether closeout later treats the obligation as mandatory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requiredness {
    /// `required`: closeout treats the obligation as mandatory.
    Required,
    /// `advisory`: closeout reports the obligation but does not require it.
    Advisory,
}

impl Requiredness {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Advisory => "advisory",
        }
    }
}

/// Discriminating outcome the named control must produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscriminatingOutcome {
    /// `fails_before_passes_after`: the control fails on the head without the
    /// change and passes on the head with it.
    FailsBeforePassesAfter,
    /// `rejects_wrong_implementation`: the control rejects the named wrong
    /// implementation.
    RejectsWrongImplementation,
}

impl DiscriminatingOutcome {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FailsBeforePassesAfter => "fails_before_passes_after",
            Self::RejectsWrongImplementation => "rejects_wrong_implementation",
        }
    }
}

/// Evidence forms that may satisfy an executed-control obligation.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceForm {
    /// `retained_artifact`: a retained artifact of the run.
    RetainedArtifact,
    /// `bounded_log_commitment`: a commitment to a bounded run log.
    BoundedLogCommitment,
    /// `declared_substitute`: the substitute the obligation declares.
    DeclaredSubstitute,
}

impl EvidenceForm {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RetainedArtifact => "retained_artifact",
            Self::BoundedLogCommitment => "bounded_log_commitment",
            Self::DeclaredSubstitute => "declared_substitute",
        }
    }
}

/// What a result claims to be offering as evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferedEvidenceKind {
    /// `executed_discriminating_control`: the named control was run against the
    /// named wrong implementation. The only kind a `passed` result may offer.
    ExecutedDiscriminatingControl,
    /// `ordinary_positive_test`: a passing ordinary test; never satisfies an
    /// obligation.
    OrdinaryPositiveTest,
    /// `review_prose`: a review argument; never satisfies an obligation.
    ReviewProse,
    /// `structural_discrimination_claim`: a claim that a fixture discriminates
    /// by its structure; never satisfies an obligation.
    StructuralDiscriminationClaim,
    /// `declared_substitute`: the substitute the obligation declares. The only
    /// kind a `substituted` result may offer.
    DeclaredSubstitute,
}

impl OfferedEvidenceKind {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExecutedDiscriminatingControl => "executed_discriminating_control",
            Self::OrdinaryPositiveTest => "ordinary_positive_test",
            Self::ReviewProse => "review_prose",
            Self::StructuralDiscriminationClaim => "structural_discrimination_claim",
            Self::DeclaredSubstitute => "declared_substitute",
        }
    }
}

/// Closed result state. Non-pass states stay explicit and non-satisfying
/// unless the obligation itself names an accepted substitute.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultState {
    /// `passed`: the control ran and produced the expected discriminating
    /// outcome.
    Passed,
    /// `failed`: the control ran and did not produce the expected outcome.
    Failed,
    /// `not_run`: the control was not run.
    NotRun,
    /// `not_proven`: no evidence shows whether the control ran.
    NotProven,
    /// `substituted`: the declared substitute ran in place of the control.
    Substituted,
    /// `instrument_failure`: the instrument that runs the control failed.
    InstrumentFailure,
}

impl ResultState {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::NotRun => "not_run",
            Self::NotProven => "not_proven",
            Self::Substituted => "substituted",
            Self::InstrumentFailure => "instrument_failure",
        }
    }

    /// Whether this state claims to satisfy an obligation: only `passed` and
    /// `substituted` do.
    pub const fn claims_satisfaction(self) -> bool {
        matches!(self, Self::Passed | Self::Substituted)
    }
}

/// What was observed when the control ran, or why it did not.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedOutcome {
    /// `failed_before_passed_after`: matches `fails_before_passes_after`.
    FailedBeforePassedAfter,
    /// `rejected_wrong_implementation`: matches `rejects_wrong_implementation`.
    RejectedWrongImplementation,
    /// `command_succeeded_without_exercising_subject`: the command ran but
    /// never reached the named wrong implementation; never satisfies.
    CommandSucceededWithoutExercisingSubject,
    /// `ordinary_positive_tests_passed`: only ordinary tests ran.
    OrdinaryPositiveTestsPassed,
    /// `review_argument_only`: only a review argument exists.
    ReviewArgumentOnly,
    /// `structural_discrimination_only`: only a structural claim exists.
    StructuralDiscriminationOnly,
    /// `instrument_unavailable`: the instrument could not run.
    InstrumentUnavailable,
    /// `not_executed`: nothing ran.
    NotExecuted,
}

impl ObservedOutcome {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FailedBeforePassedAfter => "failed_before_passed_after",
            Self::RejectedWrongImplementation => "rejected_wrong_implementation",
            Self::CommandSucceededWithoutExercisingSubject => {
                "command_succeeded_without_exercising_subject"
            }
            Self::OrdinaryPositiveTestsPassed => "ordinary_positive_tests_passed",
            Self::ReviewArgumentOnly => "review_argument_only",
            Self::StructuralDiscriminationOnly => "structural_discrimination_only",
            Self::InstrumentUnavailable => "instrument_unavailable",
            Self::NotExecuted => "not_executed",
        }
    }
}

/// Conditions that invalidate a previously recorded result.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Invalidator {
    /// `source_head_moved`: the source head changed after the result.
    SourceHeadMoved,
    /// `control_contract_changed`: the obligation changed after the result.
    ControlContractChanged,
    /// `artifact_missing`: the retained artifact is gone.
    ArtifactMissing,
    /// `command_identity_changed`: the command or instrument changed.
    CommandIdentityChanged,
}

impl Invalidator {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceHeadMoved => "source_head_moved",
            Self::ControlContractChanged => "control_contract_changed",
            Self::ArtifactMissing => "artifact_missing",
            Self::CommandIdentityChanged => "command_identity_changed",
        }
    }
}

/// Named subject the control must actually exercise.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSubject {
    /// The command or instrument that runs the control; must be non-empty.
    pub command_or_instrument_id: String,
    /// The wrong implementation the run must exercise; must be non-empty.
    pub named_wrong_implementation: String,
    /// The full 40-character Git SHA the run must bind.
    pub required_head: String,
}

/// Explicit substitute declared by the obligation, never inferred later.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermittedSubstitute {
    /// The substitute identity a `substituted` result must name.
    pub substitute_id: String,
    /// The instrument a `substituted` result must run.
    pub instrument_id: String,
    /// The evidence form the substitute produces.
    pub evidence_form: EvidenceForm,
}

/// Logical retained artifact identity. Machine paths are not identity.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    /// The artifact's logical identity; must be non-empty.
    pub logical_id: String,
    /// The artifact digest, as `sha256:<64 lowercase hex>`.
    pub digest: String,
}

/// `executed_control_obligation.v1`
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutedControlObligationV1 {
    /// Must equal [`EXECUTED_CONTROL_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Must equal [`EXECUTED_CONTROL_OBLIGATION_KIND`].
    pub kind: String,
    /// The obligation identity; unique within a packet.
    pub obligation_id: String,
    /// The claim that owns this obligation.
    pub owning_claim: String,
    /// The control class.
    pub control_class: ControlClass,
    /// The wrong implementation or removed guard the control introduces.
    pub intended_wrong_implementation: String,
    /// What the run must exercise and bind.
    pub required_execution_subject: ExecutionSubject,
    /// The outcome a `passed` result must observe.
    pub expected_discriminating_outcome: DiscriminatingOutcome,
    /// The evidence forms that may satisfy the obligation; must be non-empty. A
    /// `passed` result needs `retained_artifact` or `bounded_log_commitment`
    /// here.
    pub acceptable_evidence_forms: Vec<EvidenceForm>,
    /// The one substitute that may satisfy in place of a pass. When set,
    /// `acceptable_evidence_forms` must include `declared_substitute`.
    pub permitted_substitute: Option<PermittedSubstitute>,
    /// Whether closeout requires the obligation.
    pub requiredness: Requiredness,
    /// Conditions that invalidate a recorded result; must be non-empty.
    pub invalidators: Vec<Invalidator>,
}

/// `executed_control_result.v1`
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutedControlResultV1 {
    /// Must equal [`EXECUTED_CONTROL_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Must equal [`EXECUTED_CONTROL_RESULT_KIND`].
    pub kind: String,
    /// The obligation this result answers.
    pub obligation_id: String,
    /// The source identity; must equal the packet's `source_identity`.
    pub source_identity: String,
    /// The candidate identity, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    /// The full 40-character Git SHA the run bound. A satisfying result must
    /// match the obligation's `required_head`.
    pub head: String,
    /// The command or instrument that ran. A `passed` result must match the
    /// obligation's; a `substituted` result must match the substitute's
    /// instrument.
    pub command_or_instrument_id: String,
    /// The retained artifact. Required for a satisfying result and for a
    /// `failed` executed control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactIdentity>,
    /// What the run observed.
    pub observed_outcome: ObservedOutcome,
    /// What kind of evidence the result offers.
    pub offered_evidence_kind: OfferedEvidenceKind,
    /// The result state.
    pub state: ResultState,
    /// Why the result is limited, such as why it was not run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limitation: Option<String>,
    /// The substitute a `substituted` result used; must match the declared one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitute_id: Option<String>,
    /// The obligation's
    /// [`semantic_digest`](ExecutedControlObligationV1::semantic_digest) when
    /// the result was recorded. Required for a satisfying result and must match
    /// the current digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_digest: Option<String>,
}

/// One reusable obligation/result set. Closeout enforcement consumes this
/// object without parsing issue prose.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutedControlPacketV1 {
    /// Must equal [`EXECUTED_CONTROL_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Must equal [`EXECUTED_CONTROL_PACKET_KIND`].
    pub kind: String,
    /// The source identity every result must bind.
    pub source_identity: String,
    /// The obligations.
    pub obligations: Vec<ExecutedControlObligationV1>,
    /// The results, each naming one obligation.
    pub results: Vec<ExecutedControlResultV1>,
}

/// Per-obligation satisfaction after a structurally valid packet is accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObligationSatisfaction {
    /// The obligation identity.
    pub obligation_id: String,
    /// Whether closeout requires the obligation.
    pub requiredness: Requiredness,
    /// The satisfying result's state, else the last matching result's state, or
    /// `None` when no result names the obligation.
    pub state: Option<ResultState>,
    /// Whether one result satisfies the obligation.
    pub satisfies: bool,
}

/// Validated packet evaluation. Absence of a result is explicit non-satisfaction,
/// not an inferred pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketEvaluation {
    /// One entry per obligation, in packet order.
    pub satisfactions: Vec<ObligationSatisfaction>,
}

/// Validation failures for obligations, results, and packets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutedControlValidationError {
    /// A required field is empty or blank.
    EmptyField(&'static str),
    /// The schema version is not [`EXECUTED_CONTROL_SCHEMA_VERSION`].
    InvalidSchemaVersion {
        /// The record kind: `obligation`, `result` or `packet`.
        kind: &'static str,
        /// The version found.
        value: String,
    },
    /// The kind string is wrong for the record.
    InvalidKind {
        /// The kind string required.
        expected: &'static str,
        /// The kind string found.
        actual: String,
    },
    /// A head is not a full 40-character SHA-1 Git object id.
    InvalidHead {
        /// The field holding the head.
        field: &'static str,
        /// The value found.
        value: String,
    },
    /// A digest is not `sha256:<64 lowercase hex>`.
    InvalidDigest {
        /// The field holding the digest.
        field: &'static str,
        /// The value found.
        value: String,
    },
    /// Two obligations share an `obligation_id`.
    DuplicateObligationId(String),
    /// A result names an obligation the packet does not hold.
    UnknownObligation(String),
    /// An ordinary positive test claims to satisfy an obligation.
    OrdinaryPositiveTestCannotSatisfy(String),
    /// Review prose claims to satisfy an obligation.
    ReviewProseCannotSatisfy(String),
    /// A structural discrimination claim claims to satisfy an obligation.
    StructuralClaimCannotSatisfy(String),
    /// A satisfying result, or a `failed` executed control, has no artifact.
    MissingArtifact {
        /// The obligation.
        obligation_id: String,
        /// The result state.
        state: ResultState,
    },
    /// A satisfying result binds a head other than the obligation's
    /// `required_head`.
    HeadMismatch {
        /// The obligation.
        obligation_id: String,
        /// The head required.
        expected: String,
        /// The head found.
        actual: String,
    },
    /// A satisfying result ran a command or instrument other than the one
    /// required.
    CommandMismatch {
        /// The obligation.
        obligation_id: String,
        /// The command or instrument required.
        expected: String,
        /// The command or instrument found.
        actual: String,
    },
    /// A satisfying result's command never reached the named wrong
    /// implementation.
    WrongImplementationNotExercised(String),
    /// A satisfying result's `obligation_digest` does not match the
    /// obligation's current digest.
    ObligationDigestMismatch {
        /// The obligation.
        obligation_id: String,
        /// The current digest.
        expected: String,
        /// The digest found.
        actual: String,
    },
    /// A satisfying result has no `obligation_digest`.
    MissingObligationDigest(String),
    /// A `substituted` result answers an obligation that declares no
    /// substitute.
    SubstituteNotDeclared(String),
    /// A `substituted` result names a substitute other than the declared one,
    /// or none.
    SubstituteMismatch {
        /// The obligation.
        obligation_id: String,
        /// The declared substitute.
        expected: String,
        /// The substitute found; empty when none was named.
        actual: String,
    },
    /// More than one result claims to satisfy the same obligation.
    ConflictingSatisfyingResults(String),
    /// A `passed` result offers evidence other than
    /// `executed_discriminating_control`.
    PassedRequiresExecutedControl(String),
    /// A `passed` result observed an outcome other than the one the obligation
    /// expects.
    ObservedOutcomeDoesNotMatch {
        /// The obligation.
        obligation_id: String,
        /// The outcome expected.
        expected: String,
        /// The outcome found.
        actual: String,
    },
    /// A `substituted` result offers evidence other than `declared_substitute`.
    SubstituteRequiresDeclaredEvidence(String),
    /// A result binds a source identity other than the packet's.
    SourceIdentityMismatch {
        /// The obligation.
        obligation_id: String,
        /// The packet's source identity.
        expected: String,
        /// The source identity found.
        actual: String,
    },
    /// The obligation does not accept the evidence form the result state needs.
    EvidenceFormNotAccepted {
        /// The obligation.
        obligation_id: String,
        /// The result state.
        state: ResultState,
    },
    /// A `substituted` result observed an outcome that does not show the
    /// substitute ran, such as `not_executed` or `review_argument_only`.
    UnexecutedSubstituteCannotSatisfy(String),
    /// One obligation has both `passed` and `failed` results for the same head
    /// and command.
    ConflictingSameSubjectResults(String),
}

impl fmt::Display for ExecutedControlValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "{field} must not be empty"),
            Self::InvalidSchemaVersion { kind, value } => {
                write!(
                    formatter,
                    "unsupported {kind} schema version {value:?}; expected {EXECUTED_CONTROL_SCHEMA_VERSION}"
                )
            }
            Self::InvalidKind { expected, actual } => {
                write!(formatter, "kind must be {expected}, got {actual:?}")
            }
            Self::InvalidHead { field, value } => write!(
                formatter,
                "{field} must be a full 40-character Git SHA, got {value:?}"
            ),
            Self::InvalidDigest { field, value } => write!(
                formatter,
                "{field} must be sha256:<64 lowercase hex>, got {value:?}"
            ),
            Self::DuplicateObligationId(id) => {
                write!(formatter, "duplicated obligation_id {id:?}")
            }
            Self::UnknownObligation(id) => {
                write!(formatter, "result names unknown obligation_id {id:?}")
            }
            Self::OrdinaryPositiveTestCannotSatisfy(id) => write!(
                formatter,
                "ordinary positive test cannot satisfy executed-control obligation {id:?}"
            ),
            Self::ReviewProseCannotSatisfy(id) => write!(
                formatter,
                "review prose cannot satisfy executed-control obligation {id:?}"
            ),
            Self::StructuralClaimCannotSatisfy(id) => write!(
                formatter,
                "structural discrimination claim cannot satisfy executed-control obligation {id:?}"
            ),
            Self::MissingArtifact {
                obligation_id,
                state,
            } => write!(
                formatter,
                "{state:?} result for {obligation_id:?} is missing retained artifact identity"
            ),
            Self::HeadMismatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "result for {obligation_id:?} binds head {actual}, expected {expected}"
            ),
            Self::CommandMismatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "result for {obligation_id:?} binds command {actual:?}, expected {expected:?}"
            ),
            Self::WrongImplementationNotExercised(id) => write!(
                formatter,
                "command succeeded without exercising the named wrong implementation for {id:?}"
            ),
            Self::ObligationDigestMismatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "stale obligation digest for {obligation_id:?}: expected {expected}, got {actual}"
            ),
            Self::MissingObligationDigest(id) => write!(
                formatter,
                "satisfying result for {id:?} is missing obligation_digest"
            ),
            Self::SubstituteNotDeclared(id) => write!(
                formatter,
                "substituted result for {id:?} has no permitted substitute on the obligation"
            ),
            Self::SubstituteMismatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "substitute {actual:?} for {obligation_id:?} does not match declared {expected:?}"
            ),
            Self::ConflictingSatisfyingResults(id) => write!(
                formatter,
                "obligation {id:?} has more than one satisfying result"
            ),
            Self::PassedRequiresExecutedControl(id) => write!(
                formatter,
                "passed result for {id:?} requires offered evidence executed_discriminating_control"
            ),
            Self::ObservedOutcomeDoesNotMatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "observed outcome {actual} for {obligation_id:?} does not match expected {expected}"
            ),
            Self::SubstituteRequiresDeclaredEvidence(id) => write!(
                formatter,
                "substituted result for {id:?} must offer declared_substitute evidence"
            ),
            Self::SourceIdentityMismatch {
                obligation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "result for {obligation_id:?} binds source identity {actual:?}, expected {expected:?}"
            ),
            Self::EvidenceFormNotAccepted {
                obligation_id,
                state,
            } => write!(
                formatter,
                "{state:?} result for {obligation_id:?} is not an acceptable evidence form"
            ),
            Self::UnexecutedSubstituteCannotSatisfy(id) => write!(
                formatter,
                "unexecuted substitute cannot satisfy executed-control obligation {id:?}"
            ),
            Self::ConflictingSameSubjectResults(id) => write!(
                formatter,
                "obligation {id:?} has conflicting passed and failed results for the same head and command"
            ),
        }
    }
}

impl std::error::Error for ExecutedControlValidationError {}

impl ExecutedControlObligationV1 {
    /// Check the obligation's own fields: schema version, kind, non-empty
    /// identities, a full head, at least one evidence form and invalidator, and
    /// a declared substitute that the evidence forms accept.
    pub fn validate(&self) -> Result<(), ExecutedControlValidationError> {
        if self.schema_version != EXECUTED_CONTROL_SCHEMA_VERSION {
            return Err(ExecutedControlValidationError::InvalidSchemaVersion {
                kind: "obligation",
                value: self.schema_version.clone(),
            });
        }
        if self.kind != EXECUTED_CONTROL_OBLIGATION_KIND {
            return Err(ExecutedControlValidationError::InvalidKind {
                expected: EXECUTED_CONTROL_OBLIGATION_KIND,
                actual: self.kind.clone(),
            });
        }
        validate_non_empty("obligation_id", &self.obligation_id)?;
        validate_non_empty("owning_claim", &self.owning_claim)?;
        validate_non_empty(
            "intended_wrong_implementation",
            &self.intended_wrong_implementation,
        )?;
        validate_non_empty(
            "required_execution_subject.command_or_instrument_id",
            &self.required_execution_subject.command_or_instrument_id,
        )?;
        validate_non_empty(
            "required_execution_subject.named_wrong_implementation",
            &self.required_execution_subject.named_wrong_implementation,
        )?;
        validate_head(
            "required_execution_subject.required_head",
            &self.required_execution_subject.required_head,
        )?;
        if self.acceptable_evidence_forms.is_empty() {
            return Err(ExecutedControlValidationError::EmptyField(
                "acceptable_evidence_forms",
            ));
        }
        if let Some(substitute) = &self.permitted_substitute {
            validate_non_empty(
                "permitted_substitute.substitute_id",
                &substitute.substitute_id,
            )?;
            validate_non_empty(
                "permitted_substitute.instrument_id",
                &substitute.instrument_id,
            )?;
            if !self
                .acceptable_evidence_forms
                .contains(&EvidenceForm::DeclaredSubstitute)
            {
                return Err(ExecutedControlValidationError::EmptyField(
                    "acceptable_evidence_forms.declared_substitute",
                ));
            }
        }
        if self.invalidators.is_empty() {
            return Err(ExecutedControlValidationError::EmptyField("invalidators"));
        }
        Ok(())
    }

    /// Digest of the obligation contract. Timestamps, machine paths, and log
    /// order are not part of this identity.
    pub fn semantic_digest(&self) -> String {
        let mut forms: Vec<&str> = self
            .acceptable_evidence_forms
            .iter()
            .map(|form| form.as_str())
            .collect();
        forms.sort_unstable();
        forms.dedup();
        let mut invalidators: Vec<&str> = self
            .invalidators
            .iter()
            .map(|invalidator| invalidator.as_str())
            .collect();
        invalidators.sort_unstable();
        invalidators.dedup();
        let substitute = match &self.permitted_substitute {
            None => "-".to_string(),
            Some(value) => format!(
                "{}|{}|{}",
                value.substitute_id,
                value.instrument_id,
                value.evidence_form.as_str()
            ),
        };
        let required_head = self
            .required_execution_subject
            .required_head
            .to_ascii_lowercase();
        let forms_joined = forms.join(",");
        let invalidators_joined = invalidators.join(",");
        let canonical = [
            self.obligation_id.as_str(),
            self.owning_claim.as_str(),
            self.control_class.as_str(),
            self.intended_wrong_implementation.as_str(),
            self.required_execution_subject
                .command_or_instrument_id
                .as_str(),
            self.required_execution_subject
                .named_wrong_implementation
                .as_str(),
            required_head.as_str(),
            self.expected_discriminating_outcome.as_str(),
            forms_joined.as_str(),
            substitute.as_str(),
            self.requiredness.as_str(),
            invalidators_joined.as_str(),
        ]
        .join("\n");
        format!("{SHA256_PREFIX}{:x}", Sha256::digest(canonical.as_bytes()))
    }
}

impl ExecutedControlResultV1 {
    /// Check the result's own fields: schema version, kind, non-empty
    /// identities, a full head and well-formed digests. Binding to an
    /// obligation is checked by [`ExecutedControlPacketV1::validate`].
    pub fn validate_shape(&self) -> Result<(), ExecutedControlValidationError> {
        if self.schema_version != EXECUTED_CONTROL_SCHEMA_VERSION {
            return Err(ExecutedControlValidationError::InvalidSchemaVersion {
                kind: "result",
                value: self.schema_version.clone(),
            });
        }
        if self.kind != EXECUTED_CONTROL_RESULT_KIND {
            return Err(ExecutedControlValidationError::InvalidKind {
                expected: EXECUTED_CONTROL_RESULT_KIND,
                actual: self.kind.clone(),
            });
        }
        validate_non_empty("obligation_id", &self.obligation_id)?;
        validate_non_empty("source_identity", &self.source_identity)?;
        validate_head("head", &self.head)?;
        validate_non_empty("command_or_instrument_id", &self.command_or_instrument_id)?;
        if let Some(candidate_id) = &self.candidate_id {
            validate_non_empty("candidate_id", candidate_id)?;
        }
        if let Some(limitation) = &self.limitation {
            validate_non_empty("limitation", limitation)?;
        }
        if let Some(substitute_id) = &self.substitute_id {
            validate_non_empty("substitute_id", substitute_id)?;
        }
        if let Some(digest) = &self.obligation_digest {
            validate_sha256("obligation_digest", digest)?;
        }
        if let Some(artifact) = &self.artifact {
            validate_non_empty("artifact.logical_id", &artifact.logical_id)?;
            validate_sha256("artifact.digest", &artifact.digest)?;
        }
        Ok(())
    }
}

impl ExecutedControlPacketV1 {
    /// Validate every obligation and result, bind each result to its
    /// obligation, and report which obligations are satisfied. Returns the
    /// first error found.
    pub fn validate(&self) -> Result<PacketEvaluation, ExecutedControlValidationError> {
        if self.schema_version != EXECUTED_CONTROL_SCHEMA_VERSION {
            return Err(ExecutedControlValidationError::InvalidSchemaVersion {
                kind: "packet",
                value: self.schema_version.clone(),
            });
        }
        if self.kind != EXECUTED_CONTROL_PACKET_KIND {
            return Err(ExecutedControlValidationError::InvalidKind {
                expected: EXECUTED_CONTROL_PACKET_KIND,
                actual: self.kind.clone(),
            });
        }
        validate_non_empty("source_identity", &self.source_identity)?;

        let mut seen = BTreeSet::new();
        let mut by_id = BTreeMap::new();
        for obligation in &self.obligations {
            obligation.validate()?;
            if !seen.insert(obligation.obligation_id.clone()) {
                return Err(ExecutedControlValidationError::DuplicateObligationId(
                    obligation.obligation_id.clone(),
                ));
            }
            by_id.insert(obligation.obligation_id.clone(), obligation);
        }

        for result in &self.results {
            result.validate_shape()?;
            let Some(obligation) = by_id.get(&result.obligation_id) else {
                return Err(ExecutedControlValidationError::UnknownObligation(
                    result.obligation_id.clone(),
                ));
            };
            bind_result(&self.source_identity, obligation, result)?;
        }

        let mut satisfying: BTreeMap<&str, usize> = BTreeMap::new();
        let mut subject_states: BTreeMap<(String, String, String), Vec<ResultState>> =
            BTreeMap::new();
        for result in &self.results {
            if result.state.claims_satisfaction() {
                *satisfying.entry(result.obligation_id.as_str()).or_insert(0) += 1;
            }
            subject_states
                .entry((
                    result.obligation_id.clone(),
                    result.head.to_ascii_lowercase(),
                    result.command_or_instrument_id.clone(),
                ))
                .or_default()
                .push(result.state);
        }
        for (obligation_id, count) in satisfying {
            if count > 1 {
                return Err(
                    ExecutedControlValidationError::ConflictingSatisfyingResults(
                        obligation_id.to_string(),
                    ),
                );
            }
        }
        for ((obligation_id, _, _), states) in subject_states {
            let has_pass = states.contains(&ResultState::Passed);
            let has_fail = states.contains(&ResultState::Failed);
            if has_pass && has_fail {
                return Err(
                    ExecutedControlValidationError::ConflictingSameSubjectResults(obligation_id),
                );
            }
        }

        let mut satisfactions = Vec::with_capacity(self.obligations.len());
        for obligation in &self.obligations {
            let matching: Vec<&ExecutedControlResultV1> = self
                .results
                .iter()
                .filter(|result| result.obligation_id == obligation.obligation_id)
                .collect();
            let satisfying_result = matching
                .iter()
                .find(|result| result.state.claims_satisfaction())
                .copied();
            let state = satisfying_result
                .map(|result| result.state)
                .or_else(|| matching.last().map(|result| result.state));
            let satisfies = satisfying_result.is_some();
            satisfactions.push(ObligationSatisfaction {
                obligation_id: obligation.obligation_id.clone(),
                requiredness: obligation.requiredness,
                state,
                satisfies,
            });
        }
        Ok(PacketEvaluation { satisfactions })
    }

    /// Sort obligations and results so serialization is independent of input order.
    pub fn canonicalize(&mut self) {
        self.obligations
            .sort_by(|left, right| left.obligation_id.cmp(&right.obligation_id));
        self.results
            .sort_by(|left, right| result_canonical_key(left).cmp(&result_canonical_key(right)));
    }
}

fn result_canonical_key(result: &ExecutedControlResultV1) -> ResultCanonicalKey<'_> {
    ResultCanonicalKey {
        obligation_id: result.obligation_id.as_str(),
        head: result.head.to_ascii_lowercase(),
        state: result.state.as_str(),
        command: result.command_or_instrument_id.as_str(),
        evidence: result.offered_evidence_kind.as_str(),
        observed: result.observed_outcome.as_str(),
        source: result.source_identity.as_str(),
        candidate_id: result.candidate_id.as_deref(),
        substitute_id: result.substitute_id.as_deref(),
        obligation_digest: result.obligation_digest.as_deref(),
        limitation: result.limitation.as_deref(),
        artifact_id: result
            .artifact
            .as_ref()
            .map(|artifact| artifact.logical_id.as_str()),
        artifact_digest: result
            .artifact
            .as_ref()
            .map(|artifact| artifact.digest.as_str()),
    }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct ResultCanonicalKey<'a> {
    obligation_id: &'a str,
    head: String,
    state: &'a str,
    command: &'a str,
    evidence: &'a str,
    observed: &'a str,
    source: &'a str,
    candidate_id: Option<&'a str>,
    substitute_id: Option<&'a str>,
    obligation_digest: Option<&'a str>,
    limitation: Option<&'a str>,
    artifact_id: Option<&'a str>,
    artifact_digest: Option<&'a str>,
}

fn bind_result(
    packet_source: &str,
    obligation: &ExecutedControlObligationV1,
    result: &ExecutedControlResultV1,
) -> Result<(), ExecutedControlValidationError> {
    if result.source_identity != packet_source {
        return Err(ExecutedControlValidationError::SourceIdentityMismatch {
            obligation_id: result.obligation_id.clone(),
            expected: packet_source.to_string(),
            actual: result.source_identity.clone(),
        });
    }
    match result.offered_evidence_kind {
        OfferedEvidenceKind::OrdinaryPositiveTest if result.state.claims_satisfaction() => {
            return Err(
                ExecutedControlValidationError::OrdinaryPositiveTestCannotSatisfy(
                    result.obligation_id.clone(),
                ),
            );
        }
        OfferedEvidenceKind::ReviewProse if result.state.claims_satisfaction() => {
            return Err(ExecutedControlValidationError::ReviewProseCannotSatisfy(
                result.obligation_id.clone(),
            ));
        }
        OfferedEvidenceKind::StructuralDiscriminationClaim
            if result.state.claims_satisfaction() =>
        {
            return Err(
                ExecutedControlValidationError::StructuralClaimCannotSatisfy(
                    result.obligation_id.clone(),
                ),
            );
        }
        _ => {}
    }

    if result.state == ResultState::Passed
        && result.offered_evidence_kind != OfferedEvidenceKind::ExecutedDiscriminatingControl
    {
        return Err(
            ExecutedControlValidationError::PassedRequiresExecutedControl(
                result.obligation_id.clone(),
            ),
        );
    }
    if result.state == ResultState::Substituted
        && result.offered_evidence_kind != OfferedEvidenceKind::DeclaredSubstitute
    {
        return Err(
            ExecutedControlValidationError::SubstituteRequiresDeclaredEvidence(
                result.obligation_id.clone(),
            ),
        );
    }

    if result.observed_outcome == ObservedOutcome::CommandSucceededWithoutExercisingSubject
        && result.state.claims_satisfaction()
    {
        return Err(
            ExecutedControlValidationError::WrongImplementationNotExercised(
                result.obligation_id.clone(),
            ),
        );
    }

    if result.state == ResultState::Passed
        && !obligation.acceptable_evidence_forms.iter().any(|form| {
            matches!(
                form,
                EvidenceForm::RetainedArtifact | EvidenceForm::BoundedLogCommitment
            )
        })
    {
        return Err(ExecutedControlValidationError::EvidenceFormNotAccepted {
            obligation_id: result.obligation_id.clone(),
            state: result.state,
        });
    }

    if result.state == ResultState::Substituted {
        if obligation.permitted_substitute.is_none() {
            return Err(ExecutedControlValidationError::SubstituteNotDeclared(
                result.obligation_id.clone(),
            ));
        }
        if !obligation
            .acceptable_evidence_forms
            .contains(&EvidenceForm::DeclaredSubstitute)
        {
            return Err(ExecutedControlValidationError::EvidenceFormNotAccepted {
                obligation_id: result.obligation_id.clone(),
                state: result.state,
            });
        }
        if substitute_outcome_is_unexecuted(result.observed_outcome) {
            return Err(
                ExecutedControlValidationError::UnexecutedSubstituteCannotSatisfy(
                    result.obligation_id.clone(),
                ),
            );
        }
    }

    let requires_artifact = result.state.claims_satisfaction()
        || (result.state == ResultState::Failed
            && result.offered_evidence_kind == OfferedEvidenceKind::ExecutedDiscriminatingControl);
    if requires_artifact && result.artifact.is_none() {
        return Err(ExecutedControlValidationError::MissingArtifact {
            obligation_id: result.obligation_id.clone(),
            state: result.state,
        });
    }

    if result.state.claims_satisfaction() {
        let expected_head = obligation
            .required_execution_subject
            .required_head
            .to_ascii_lowercase();
        if result.head.to_ascii_lowercase() != expected_head {
            return Err(ExecutedControlValidationError::HeadMismatch {
                obligation_id: result.obligation_id.clone(),
                expected: expected_head,
                actual: result.head.to_ascii_lowercase(),
            });
        }
        if result.command_or_instrument_id
            != obligation
                .required_execution_subject
                .command_or_instrument_id
            && result.state == ResultState::Passed
        {
            return Err(ExecutedControlValidationError::CommandMismatch {
                obligation_id: result.obligation_id.clone(),
                expected: obligation
                    .required_execution_subject
                    .command_or_instrument_id
                    .clone(),
                actual: result.command_or_instrument_id.clone(),
            });
        }
        if result.state == ResultState::Substituted {
            let Some(declared) = &obligation.permitted_substitute else {
                return Err(ExecutedControlValidationError::SubstituteNotDeclared(
                    result.obligation_id.clone(),
                ));
            };
            let Some(actual) = &result.substitute_id else {
                return Err(ExecutedControlValidationError::SubstituteMismatch {
                    obligation_id: result.obligation_id.clone(),
                    expected: declared.substitute_id.clone(),
                    actual: String::new(),
                });
            };
            if actual != &declared.substitute_id {
                return Err(ExecutedControlValidationError::SubstituteMismatch {
                    obligation_id: result.obligation_id.clone(),
                    expected: declared.substitute_id.clone(),
                    actual: actual.clone(),
                });
            }
            if result.command_or_instrument_id != declared.instrument_id {
                return Err(ExecutedControlValidationError::CommandMismatch {
                    obligation_id: result.obligation_id.clone(),
                    expected: declared.instrument_id.clone(),
                    actual: result.command_or_instrument_id.clone(),
                });
            }
        }
        if result.state == ResultState::Passed {
            let expected_outcome = match obligation.expected_discriminating_outcome {
                DiscriminatingOutcome::FailsBeforePassesAfter => {
                    ObservedOutcome::FailedBeforePassedAfter
                }
                DiscriminatingOutcome::RejectsWrongImplementation => {
                    ObservedOutcome::RejectedWrongImplementation
                }
            };
            if result.observed_outcome != expected_outcome {
                return Err(
                    ExecutedControlValidationError::ObservedOutcomeDoesNotMatch {
                        obligation_id: result.obligation_id.clone(),
                        expected: expected_outcome.as_str().to_string(),
                        actual: result.observed_outcome.as_str().to_string(),
                    },
                );
            }
        }
        let expected_digest = obligation.semantic_digest();
        match &result.obligation_digest {
            None => {
                return Err(ExecutedControlValidationError::MissingObligationDigest(
                    result.obligation_id.clone(),
                ));
            }
            Some(actual) if actual != &expected_digest => {
                return Err(ExecutedControlValidationError::ObligationDigestMismatch {
                    obligation_id: result.obligation_id.clone(),
                    expected: expected_digest,
                    actual: actual.clone(),
                });
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn substitute_outcome_is_unexecuted(outcome: ObservedOutcome) -> bool {
    matches!(
        outcome,
        ObservedOutcome::NotExecuted
            | ObservedOutcome::OrdinaryPositiveTestsPassed
            | ObservedOutcome::ReviewArgumentOnly
            | ObservedOutcome::StructuralDiscriminationOnly
            | ObservedOutcome::CommandSucceededWithoutExercisingSubject
    )
}

fn validate_non_empty(
    field: &'static str,
    value: &str,
) -> Result<(), ExecutedControlValidationError> {
    if value.trim().is_empty() {
        Err(ExecutedControlValidationError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn validate_head(field: &'static str, value: &str) -> Result<(), ExecutedControlValidationError> {
    match GitObjectId::parse(value) {
        Ok(parsed) if parsed.hash_format() == super::GitHashFormat::Sha1 => Ok(()),
        Ok(_) | Err(_) => Err(ExecutedControlValidationError::InvalidHead {
            field,
            value: value.to_string(),
        }),
    }
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), ExecutedControlValidationError> {
    let Some(hex) = value.strip_prefix(SHA256_PREFIX) else {
        return Err(ExecutedControlValidationError::InvalidDigest {
            field,
            value: value.to_string(),
        });
    };
    if hex.len() != SHA256_HEX_LENGTH
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ExecutedControlValidationError::InvalidDigest {
            field,
            value: value.to_string(),
        });
    }
    Ok(())
}

#[cfg(kani)]
#[path = "executed_control_kani.rs"]
mod kani_proofs;

// Shared test fixtures: facade tests import them across the crate boundary
// (ADR 0027), so they compile under `test` or the `test-fixtures` feature
// rather than in production builds.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod tests {
    use super::*;

    #[cfg(test)]
    #[test]
    fn every_substitute_outcome_has_its_contract_execution_partition() {
        use crate::domain::proof_domains::{OUTCOMES, expected_unexecuted};
        for &outcome in OUTCOMES {
            assert_eq!(
                substitute_outcome_is_unexecuted(outcome),
                expected_unexecuted(outcome),
                "{outcome:?}"
            );
        }
    }

    pub const HEAD_AFTER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    pub const HEAD_BEFORE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    pub const HEAD_OTHER: &str = "cccccccccccccccccccccccccccccccccccccccc";
    pub const ARTIFACT_DIGEST: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    pub const COMMAND: &str = "cargo test -p example -- removed_guard_control";
    pub const OBLIGATION_ID: &str = "claim:example:removed-guard";

    pub fn sample_obligation() -> ExecutedControlObligationV1 {
        ExecutedControlObligationV1 {
            schema_version: EXECUTED_CONTROL_SCHEMA_VERSION.to_string(),
            kind: EXECUTED_CONTROL_OBLIGATION_KIND.to_string(),
            obligation_id: OBLIGATION_ID.to_string(),
            owning_claim: "issue:4641".to_string(),
            control_class: ControlClass::RemovedGuard,
            intended_wrong_implementation: "eager parse of the lazy tail".to_string(),
            required_execution_subject: ExecutionSubject {
                command_or_instrument_id: COMMAND.to_string(),
                named_wrong_implementation: "eager_file_count_parse".to_string(),
                required_head: HEAD_AFTER.to_string(),
            },
            expected_discriminating_outcome: DiscriminatingOutcome::FailsBeforePassesAfter,
            acceptable_evidence_forms: vec![
                EvidenceForm::RetainedArtifact,
                EvidenceForm::BoundedLogCommitment,
            ],
            permitted_substitute: None,
            requiredness: Requiredness::Required,
            invalidators: vec![
                Invalidator::SourceHeadMoved,
                Invalidator::ControlContractChanged,
                Invalidator::ArtifactMissing,
                Invalidator::CommandIdentityChanged,
            ],
        }
    }

    fn artifact() -> ArtifactIdentity {
        ArtifactIdentity {
            logical_id: "artifact:removed-guard-log".to_string(),
            digest: ARTIFACT_DIGEST.to_string(),
        }
    }

    pub fn passing_result(obligation: &ExecutedControlObligationV1) -> ExecutedControlResultV1 {
        ExecutedControlResultV1 {
            schema_version: EXECUTED_CONTROL_SCHEMA_VERSION.to_string(),
            kind: EXECUTED_CONTROL_RESULT_KIND.to_string(),
            obligation_id: obligation.obligation_id.clone(),
            source_identity: "EffortlessMetrics/ripr-swarm".to_string(),
            candidate_id: Some("candidate:example".to_string()),
            head: HEAD_AFTER.to_string(),
            command_or_instrument_id: obligation
                .required_execution_subject
                .command_or_instrument_id
                .clone(),
            artifact: Some(artifact()),
            observed_outcome: ObservedOutcome::FailedBeforePassedAfter,
            offered_evidence_kind: OfferedEvidenceKind::ExecutedDiscriminatingControl,
            state: ResultState::Passed,
            limitation: None,
            substitute_id: None,
            obligation_digest: Some(obligation.semantic_digest()),
        }
    }

    pub fn failed_before_result(
        obligation: &ExecutedControlObligationV1,
    ) -> ExecutedControlResultV1 {
        ExecutedControlResultV1 {
            head: HEAD_BEFORE.to_string(),
            observed_outcome: ObservedOutcome::RejectedWrongImplementation,
            state: ResultState::Failed,
            obligation_digest: None,
            ..passing_result(obligation)
        }
    }

    pub fn packet(
        obligations: Vec<ExecutedControlObligationV1>,
        results: Vec<ExecutedControlResultV1>,
    ) -> ExecutedControlPacketV1 {
        ExecutedControlPacketV1 {
            schema_version: EXECUTED_CONTROL_SCHEMA_VERSION.to_string(),
            kind: EXECUTED_CONTROL_PACKET_KIND.to_string(),
            source_identity: "EffortlessMetrics/ripr-swarm".to_string(),
            obligations,
            results,
        }
    }

    pub fn require_valid(packet: ExecutedControlPacketV1) -> Result<PacketEvaluation, String> {
        packet
            .validate()
            .map_err(|error| format!("unexpected validation error: {error}"))
    }

    pub fn require_invalid(
        packet: ExecutedControlPacketV1,
    ) -> Result<ExecutedControlValidationError, String> {
        match packet.validate() {
            Err(error) => Ok(error),
            Ok(_) => Err("expected validation to fail".to_string()),
        }
    }

    pub fn issue_3858_obligation() -> ExecutedControlObligationV1 {
        ExecutedControlObligationV1 {
            obligation_id: "issue:3858:eager-file-count-removal-control".to_string(),
            owning_claim: "issue:3858".to_string(),
            intended_wrong_implementation:
                "eager parse / removed lazy file-count guard (admit_file_count)".to_string(),
            required_execution_subject: ExecutionSubject {
                command_or_instrument_id: "cargo test -- removed_guard_or_eager_file_count_control"
                    .to_string(),
                named_wrong_implementation: "eager_admit_file_count".to_string(),
                required_head: HEAD_AFTER.to_string(),
            },
            ..sample_obligation()
        }
    }

    #[test]
    fn executed_removal_control_fails_before_and_passes_after_the_repair() -> Result<(), String> {
        let obligation = sample_obligation();
        let evaluation = require_valid(packet(
            vec![obligation.clone()],
            vec![
                failed_before_result(&obligation),
                passing_result(&obligation),
            ],
        ))?;
        assert_eq!(evaluation.satisfactions.len(), 1);
        assert!(evaluation.satisfactions[0].satisfies);
        assert_eq!(evaluation.satisfactions[0].state, Some(ResultState::Passed));
        Ok(())
    }

    #[test]
    fn ordinary_positive_test_cannot_silently_satisfy_the_obligation() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.offered_evidence_kind = OfferedEvidenceKind::OrdinaryPositiveTest;
        result.observed_outcome = ObservedOutcome::OrdinaryPositiveTestsPassed;
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::OrdinaryPositiveTestCannotSatisfy(_)
        ));
        Ok(())
    }

    #[test]
    fn prose_claim_without_artifact_cannot_pass() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.offered_evidence_kind = OfferedEvidenceKind::ReviewProse;
        result.observed_outcome = ObservedOutcome::ReviewArgumentOnly;
        result.artifact = None;
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::ReviewProseCannotSatisfy(_)
        ));
        Ok(())
    }

    #[test]
    fn control_run_on_another_head_cannot_pass() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.head = HEAD_OTHER.to_string();
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::HeadMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn command_success_without_named_wrong_implementation_cannot_pass() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.observed_outcome = ObservedOutcome::CommandSucceededWithoutExercisingSubject;
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::WrongImplementationNotExercised(_)
        ));
        Ok(())
    }

    #[test]
    fn unavailable_instrument_with_declared_substitute_is_explicit_and_satisfying()
    -> Result<(), String> {
        let mut obligation = sample_obligation();
        obligation
            .acceptable_evidence_forms
            .push(EvidenceForm::DeclaredSubstitute);
        obligation.permitted_substitute = Some(PermittedSubstitute {
            substitute_id: "hosted-eager-variant".to_string(),
            instrument_id: "hosted-mutation-runner".to_string(),
            evidence_form: EvidenceForm::DeclaredSubstitute,
        });
        let result = ExecutedControlResultV1 {
            command_or_instrument_id: "hosted-mutation-runner".to_string(),
            offered_evidence_kind: OfferedEvidenceKind::DeclaredSubstitute,
            observed_outcome: ObservedOutcome::InstrumentUnavailable,
            state: ResultState::Substituted,
            substitute_id: Some("hosted-eager-variant".to_string()),
            obligation_digest: Some(obligation.semantic_digest()),
            limitation: Some(
                "local eager-variant instrument unavailable; declared hosted substitute ran"
                    .to_string(),
            ),
            ..passing_result(&obligation)
        };
        let evaluation = require_valid(packet(vec![obligation], vec![result]))?;
        assert!(evaluation.satisfactions[0].satisfies);
        assert_eq!(
            evaluation.satisfactions[0].state,
            Some(ResultState::Substituted)
        );
        Ok(())
    }

    #[test]
    fn unavailable_instrument_without_substitute_stays_explicit_and_non_satisfying()
    -> Result<(), String> {
        let obligation = sample_obligation();
        let result = ExecutedControlResultV1 {
            offered_evidence_kind: OfferedEvidenceKind::ExecutedDiscriminatingControl,
            observed_outcome: ObservedOutcome::InstrumentUnavailable,
            state: ResultState::InstrumentFailure,
            artifact: None,
            obligation_digest: None,
            limitation: Some("eager-variant instrument was not available".to_string()),
            ..passing_result(&obligation)
        };
        let evaluation = require_valid(packet(vec![obligation], vec![result]))?;
        assert!(!evaluation.satisfactions[0].satisfies);
        assert_eq!(
            evaluation.satisfactions[0].state,
            Some(ResultState::InstrumentFailure)
        );
        Ok(())
    }

    #[test]
    fn inferred_substitute_without_declaration_is_rejected() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.state = ResultState::Substituted;
        result.offered_evidence_kind = OfferedEvidenceKind::DeclaredSubstitute;
        result.substitute_id = Some("invented-after-the-fact".to_string());
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::SubstituteNotDeclared(_)
        ));
        Ok(())
    }

    #[test]
    fn duplicated_obligation_ids_are_rejected() -> Result<(), String> {
        let obligation = sample_obligation();
        let error = require_invalid(packet(
            vec![obligation.clone(), obligation.clone()],
            vec![passing_result(&obligation)],
        ))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::DuplicateObligationId(_)
        ));
        Ok(())
    }

    #[test]
    fn result_for_unknown_obligation_is_rejected() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.obligation_id = "claim:unknown".to_string();
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::UnknownObligation(_)
        ));
        Ok(())
    }

    #[test]
    fn stale_obligation_digest_cannot_pass() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.obligation_digest = Some(
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string(),
        );
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::ObligationDigestMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn issue_3858_documentation_fixture_stays_not_proven() -> Result<(), String> {
        let obligation = issue_3858_obligation();
        let result = ExecutedControlResultV1 {
            offered_evidence_kind: OfferedEvidenceKind::ReviewProse,
            observed_outcome: ObservedOutcome::NotExecuted,
            state: ResultState::NotProven,
            artifact: None,
            obligation_digest: None,
            limitation: Some(
                "Issue #3858 / PR #4063 recorded no retained eager or removed-guard execution artifact. Execution is not_proven and must not be rewritten as passed."
                    .to_string(),
            ),
            ..passing_result(&obligation)
        };
        let evaluation = require_valid(packet(vec![obligation], vec![result]))?;
        assert!(!evaluation.satisfactions[0].satisfies);
        assert_eq!(
            evaluation.satisfactions[0].state,
            Some(ResultState::NotProven)
        );
        Ok(())
    }

    #[test]
    fn semantic_digest_is_independent_of_form_and_invalidator_order() {
        let mut left = sample_obligation();
        let mut right = sample_obligation();
        left.acceptable_evidence_forms = vec![
            EvidenceForm::BoundedLogCommitment,
            EvidenceForm::RetainedArtifact,
        ];
        right.acceptable_evidence_forms = vec![
            EvidenceForm::RetainedArtifact,
            EvidenceForm::BoundedLogCommitment,
        ];
        left.invalidators = vec![
            Invalidator::ArtifactMissing,
            Invalidator::SourceHeadMoved,
            Invalidator::CommandIdentityChanged,
            Invalidator::ControlContractChanged,
        ];
        assert_eq!(left.semantic_digest(), right.semantic_digest());
    }

    #[test]
    fn canonicalize_orders_obligations_and_results_deterministically() {
        let first = sample_obligation();
        let mut second = sample_obligation();
        second.obligation_id = "claim:example:other".to_string();
        let mut reversed = packet(
            vec![second.clone(), first.clone()],
            vec![passing_result(&second), passing_result(&first)],
        );
        let mut ordered = packet(
            vec![first.clone(), second.clone()],
            vec![passing_result(&first), passing_result(&second)],
        );
        reversed.canonicalize();
        ordered.canonicalize();
        assert_eq!(reversed, ordered);
    }

    #[test]
    fn missing_result_is_non_satisfying_not_an_inferred_pass() -> Result<(), String> {
        let evaluation = require_valid(packet(vec![sample_obligation()], Vec::new()))?;
        assert!(!evaluation.satisfactions[0].satisfies);
        assert_eq!(evaluation.satisfactions[0].state, None);
        Ok(())
    }

    #[test]
    fn structural_discrimination_claim_cannot_pass() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.offered_evidence_kind = OfferedEvidenceKind::StructuralDiscriminationClaim;
        result.observed_outcome = ObservedOutcome::StructuralDiscriminationOnly;
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::StructuralClaimCannotSatisfy(_)
        ));
        Ok(())
    }

    #[test]
    fn foreign_source_identity_cannot_satisfy() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        result.source_identity = "other/repo".to_string();
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::SourceIdentityMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn unexecuted_substitute_cannot_satisfy() -> Result<(), String> {
        let mut obligation = sample_obligation();
        obligation
            .acceptable_evidence_forms
            .push(EvidenceForm::DeclaredSubstitute);
        obligation.permitted_substitute = Some(PermittedSubstitute {
            substitute_id: "hosted-eager-variant".to_string(),
            instrument_id: "hosted-mutation-runner".to_string(),
            evidence_form: EvidenceForm::DeclaredSubstitute,
        });
        let result = ExecutedControlResultV1 {
            command_or_instrument_id: "hosted-mutation-runner".to_string(),
            offered_evidence_kind: OfferedEvidenceKind::DeclaredSubstitute,
            observed_outcome: ObservedOutcome::NotExecuted,
            state: ResultState::Substituted,
            substitute_id: Some("hosted-eager-variant".to_string()),
            obligation_digest: Some(obligation.semantic_digest()),
            ..passing_result(&obligation)
        };
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::UnexecutedSubstituteCannotSatisfy(_)
        ));
        Ok(())
    }

    #[test]
    fn passed_result_requires_an_artifact_backed_acceptable_form() -> Result<(), String> {
        let mut obligation = sample_obligation();
        obligation.acceptable_evidence_forms = vec![EvidenceForm::DeclaredSubstitute];
        obligation.permitted_substitute = Some(PermittedSubstitute {
            substitute_id: "hosted-eager-variant".to_string(),
            instrument_id: "hosted-mutation-runner".to_string(),
            evidence_form: EvidenceForm::DeclaredSubstitute,
        });
        let result = passing_result(&obligation);
        let error = require_invalid(packet(vec![obligation], vec![result]))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::EvidenceFormNotAccepted { .. }
        ));
        Ok(())
    }

    #[test]
    fn same_head_pass_and_fail_are_rejected() -> Result<(), String> {
        let obligation = sample_obligation();
        let mut failed = passing_result(&obligation);
        failed.state = ResultState::Failed;
        failed.observed_outcome = ObservedOutcome::RejectedWrongImplementation;
        failed.obligation_digest = None;
        let error = require_invalid(packet(
            vec![obligation.clone()],
            vec![failed, passing_result(&obligation)],
        ))?;
        assert!(matches!(
            error,
            ExecutedControlValidationError::ConflictingSameSubjectResults(_)
        ));
        Ok(())
    }

    #[test]
    fn canonicalize_orders_equal_key_results_by_payload() {
        let obligation = sample_obligation();
        let mut left = passing_result(&obligation);
        left.state = ResultState::NotProven;
        left.offered_evidence_kind = OfferedEvidenceKind::ReviewProse;
        left.observed_outcome = ObservedOutcome::NotExecuted;
        left.artifact = None;
        left.obligation_digest = None;
        left.limitation = Some("runner A unavailable".to_string());
        let mut right = left.clone();
        right.limitation = Some("runner B unavailable".to_string());
        let mut reversed = packet(vec![obligation.clone()], vec![right.clone(), left.clone()]);
        let mut ordered = packet(vec![obligation], vec![left, right]);
        reversed.canonicalize();
        ordered.canonicalize();
        assert_eq!(reversed, ordered);
    }
}

#[cfg(test)]
mod shape_and_binding_refusal_tests {
    use super::tests::{
        ARTIFACT_DIGEST, HEAD_AFTER, OBLIGATION_ID, failed_before_result, packet, passing_result,
        require_invalid, require_valid, sample_obligation,
    };
    use super::*;

    type Refusal = ExecutedControlValidationError;

    fn obligation_with(edit: impl FnOnce(&mut ExecutedControlObligationV1)) -> Result<(), Refusal> {
        let mut obligation = sample_obligation();
        edit(&mut obligation);
        obligation.validate()
    }

    fn result_with(edit: impl FnOnce(&mut ExecutedControlResultV1)) -> Result<(), Refusal> {
        let mut result = passing_result(&sample_obligation());
        edit(&mut result);
        result.validate_shape()
    }

    /// Validates a packet of the sample obligation and one edited passing result.
    fn packet_with(
        edit: impl FnOnce(&mut ExecutedControlResultV1),
    ) -> Result<PacketEvaluation, Refusal> {
        let obligation = sample_obligation();
        let mut result = passing_result(&obligation);
        edit(&mut result);
        packet(vec![obligation], vec![result]).validate()
    }

    fn substitute() -> PermittedSubstitute {
        PermittedSubstitute {
            substitute_id: "substitute:manual-replay".to_string(),
            instrument_id: "instrument:manual-replay".to_string(),
            evidence_form: EvidenceForm::DeclaredSubstitute,
        }
    }

    #[test]
    fn every_wire_token_is_the_snake_case_name_of_its_variant() {
        assert_eq!(
            [
                ControlClass::RemovedGuard.as_str(),
                ControlClass::WrongImplementation.as_str(),
                ControlClass::NamedMutation.as_str(),
            ],
            ["removed_guard", "wrong_implementation", "named_mutation"]
        );
        assert_eq!(
            [
                Requiredness::Required.as_str(),
                Requiredness::Advisory.as_str()
            ],
            ["required", "advisory"]
        );
        assert_eq!(
            [
                DiscriminatingOutcome::FailsBeforePassesAfter.as_str(),
                DiscriminatingOutcome::RejectsWrongImplementation.as_str(),
            ],
            ["fails_before_passes_after", "rejects_wrong_implementation"]
        );
        assert_eq!(
            [
                EvidenceForm::RetainedArtifact.as_str(),
                EvidenceForm::BoundedLogCommitment.as_str(),
                EvidenceForm::DeclaredSubstitute.as_str(),
            ],
            [
                "retained_artifact",
                "bounded_log_commitment",
                "declared_substitute"
            ]
        );
        assert_eq!(
            [
                OfferedEvidenceKind::ExecutedDiscriminatingControl.as_str(),
                OfferedEvidenceKind::OrdinaryPositiveTest.as_str(),
                OfferedEvidenceKind::ReviewProse.as_str(),
                OfferedEvidenceKind::StructuralDiscriminationClaim.as_str(),
                OfferedEvidenceKind::DeclaredSubstitute.as_str(),
            ],
            [
                "executed_discriminating_control",
                "ordinary_positive_test",
                "review_prose",
                "structural_discrimination_claim",
                "declared_substitute",
            ]
        );
        assert_eq!(
            [
                ResultState::Passed.as_str(),
                ResultState::Failed.as_str(),
                ResultState::NotRun.as_str(),
                ResultState::NotProven.as_str(),
                ResultState::Substituted.as_str(),
                ResultState::InstrumentFailure.as_str(),
            ],
            [
                "passed",
                "failed",
                "not_run",
                "not_proven",
                "substituted",
                "instrument_failure",
            ]
        );
        assert_eq!(
            [
                ObservedOutcome::FailedBeforePassedAfter.as_str(),
                ObservedOutcome::RejectedWrongImplementation.as_str(),
                ObservedOutcome::CommandSucceededWithoutExercisingSubject.as_str(),
                ObservedOutcome::OrdinaryPositiveTestsPassed.as_str(),
                ObservedOutcome::ReviewArgumentOnly.as_str(),
                ObservedOutcome::StructuralDiscriminationOnly.as_str(),
                ObservedOutcome::InstrumentUnavailable.as_str(),
                ObservedOutcome::NotExecuted.as_str(),
            ],
            [
                "failed_before_passed_after",
                "rejected_wrong_implementation",
                "command_succeeded_without_exercising_subject",
                "ordinary_positive_tests_passed",
                "review_argument_only",
                "structural_discrimination_only",
                "instrument_unavailable",
                "not_executed",
            ]
        );
        assert_eq!(
            [
                Invalidator::SourceHeadMoved.as_str(),
                Invalidator::ControlContractChanged.as_str(),
                Invalidator::ArtifactMissing.as_str(),
                Invalidator::CommandIdentityChanged.as_str(),
            ],
            [
                "source_head_moved",
                "control_contract_changed",
                "artifact_missing",
                "command_identity_changed",
            ]
        );
    }

    #[test]
    fn an_obligation_names_the_first_field_that_is_malformed() {
        assert_eq!(obligation_with(|_| {}), Ok(()));
        assert_eq!(
            obligation_with(|obligation| obligation.schema_version = "2".into()),
            Err(Refusal::InvalidSchemaVersion {
                kind: "obligation",
                value: "2".into()
            })
        );
        assert_eq!(
            obligation_with(|obligation| obligation.kind = EXECUTED_CONTROL_RESULT_KIND.into()),
            Err(Refusal::InvalidKind {
                expected: EXECUTED_CONTROL_OBLIGATION_KIND,
                actual: EXECUTED_CONTROL_RESULT_KIND.into()
            })
        );

        type Edit = fn(&mut ExecutedControlObligationV1);
        let blank_fields: [(&str, Edit); 9] = [
            ("obligation_id", |o| o.obligation_id = " ".into()),
            ("owning_claim", |o| o.owning_claim.clear()),
            ("intended_wrong_implementation", |o| {
                o.intended_wrong_implementation = "\t".into();
            }),
            ("required_execution_subject.command_or_instrument_id", |o| {
                o.required_execution_subject
                    .command_or_instrument_id
                    .clear();
            }),
            (
                "required_execution_subject.named_wrong_implementation",
                |o| {
                    o.required_execution_subject
                        .named_wrong_implementation
                        .clear();
                },
            ),
            ("acceptable_evidence_forms", |o| {
                o.acceptable_evidence_forms.clear();
            }),
            ("invalidators", |o| o.invalidators.clear()),
            ("permitted_substitute.substitute_id", |o| {
                o.acceptable_evidence_forms
                    .push(EvidenceForm::DeclaredSubstitute);
                o.permitted_substitute = Some(PermittedSubstitute {
                    substitute_id: String::new(),
                    ..substitute()
                });
            }),
            ("permitted_substitute.instrument_id", |o| {
                o.acceptable_evidence_forms
                    .push(EvidenceForm::DeclaredSubstitute);
                o.permitted_substitute = Some(PermittedSubstitute {
                    instrument_id: " ".into(),
                    ..substitute()
                });
            }),
        ];
        for (field, edit) in blank_fields {
            assert_eq!(obligation_with(edit), Err(Refusal::EmptyField(field)));
        }

        // A permitted substitute needs the declared-substitute evidence form.
        assert_eq!(
            obligation_with(|o| o.permitted_substitute = Some(substitute())),
            Err(Refusal::EmptyField(
                "acceptable_evidence_forms.declared_substitute"
            ))
        );
        assert_eq!(
            obligation_with(|o| {
                o.acceptable_evidence_forms
                    .push(EvidenceForm::DeclaredSubstitute);
                o.permitted_substitute = Some(substitute());
            }),
            Ok(())
        );
    }

    #[test]
    fn a_required_head_must_be_a_full_sha1_object_id() {
        let sha256_shaped = "a".repeat(64);
        for head in ["", "abc1234", "main", sha256_shaped.as_str()] {
            assert_eq!(
                obligation_with(|o| o.required_execution_subject.required_head = head.into()),
                Err(Refusal::InvalidHead {
                    field: "required_execution_subject.required_head",
                    value: head.into()
                }),
                "{head:?}"
            );
            assert_eq!(
                result_with(|result| result.head = head.into()),
                Err(Refusal::InvalidHead {
                    field: "head",
                    value: head.into()
                }),
                "{head:?}"
            );
        }
    }

    #[test]
    fn a_result_names_the_first_field_that_is_malformed() {
        assert_eq!(result_with(|_| {}), Ok(()));
        assert_eq!(
            result_with(|result| result.schema_version.clear()),
            Err(Refusal::InvalidSchemaVersion {
                kind: "result",
                value: String::new()
            })
        );
        assert_eq!(
            result_with(|result| result.kind = EXECUTED_CONTROL_PACKET_KIND.into()),
            Err(Refusal::InvalidKind {
                expected: EXECUTED_CONTROL_RESULT_KIND,
                actual: EXECUTED_CONTROL_PACKET_KIND.into()
            })
        );

        type Edit = fn(&mut ExecutedControlResultV1);
        let blank_fields: [(&str, Edit); 7] = [
            ("obligation_id", |r| r.obligation_id.clear()),
            ("source_identity", |r| r.source_identity = " ".into()),
            ("command_or_instrument_id", |r| {
                r.command_or_instrument_id.clear();
            }),
            ("candidate_id", |r| r.candidate_id = Some(String::new())),
            ("limitation", |r| r.limitation = Some(" ".into())),
            ("substitute_id", |r| r.substitute_id = Some(String::new())),
            ("artifact.logical_id", |r| {
                r.artifact = Some(ArtifactIdentity {
                    logical_id: String::new(),
                    digest: ARTIFACT_DIGEST.into(),
                });
            }),
        ];
        for (field, edit) in blank_fields {
            assert_eq!(result_with(edit), Err(Refusal::EmptyField(field)));
        }

        // Optional fields may be absent.
        assert_eq!(
            result_with(|result| {
                result.candidate_id = None;
                result.artifact = None;
                result.obligation_digest = None;
            }),
            Ok(())
        );
    }

    #[test]
    fn digests_must_be_a_sha256_prefix_and_64_lowercase_hex_characters() {
        let hex = "a".repeat(64);
        let malformed = [
            hex.clone(),
            format!("sha1:{hex}"),
            format!("sha256:{}", "a".repeat(63)),
            format!("sha256:{}", "a".repeat(65)),
            format!("sha256:{}g", "a".repeat(63)),
            format!("sha256:{}A", "a".repeat(63)),
            "sha256:".to_string(),
        ];
        for digest in malformed {
            assert_eq!(
                result_with(|result| result.obligation_digest = Some(digest.clone())),
                Err(Refusal::InvalidDigest {
                    field: "obligation_digest",
                    value: digest.clone()
                })
            );
            assert_eq!(
                result_with(|result| {
                    result.artifact = Some(ArtifactIdentity {
                        logical_id: "artifact:log".into(),
                        digest: digest.clone(),
                    });
                }),
                Err(Refusal::InvalidDigest {
                    field: "artifact.digest",
                    value: digest.clone()
                })
            );
        }
        assert_eq!(
            result_with(|result| {
                result.obligation_digest = Some(format!("sha256:{}", "0123456789abcdef".repeat(4)));
            }),
            Ok(())
        );
    }

    #[test]
    fn a_packet_refuses_its_own_malformed_envelope_and_members() {
        let obligation = sample_obligation();
        let valid = || packet(vec![obligation.clone()], vec![passing_result(&obligation)]);
        let refused = |edit: fn(&mut ExecutedControlPacketV1)| {
            let mut packet = valid();
            edit(&mut packet);
            packet.validate()
        };
        assert_eq!(
            refused(|packet| packet.schema_version = "0".into()),
            Err(Refusal::InvalidSchemaVersion {
                kind: "packet",
                value: "0".into()
            })
        );
        assert_eq!(
            refused(|packet| packet.kind = "packet".into()),
            Err(Refusal::InvalidKind {
                expected: EXECUTED_CONTROL_PACKET_KIND,
                actual: "packet".into()
            })
        );
        assert_eq!(
            refused(|packet| packet.source_identity = " ".into()),
            Err(Refusal::EmptyField("source_identity"))
        );
        // A malformed member is refused through the packet, not skipped.
        assert_eq!(
            refused(|packet| packet.obligations[0].owning_claim.clear()),
            Err(Refusal::EmptyField("owning_claim"))
        );
        assert_eq!(
            refused(|packet| packet.results[0].head = "main".into()),
            Err(Refusal::InvalidHead {
                field: "head",
                value: "main".into()
            })
        );
    }

    #[test]
    fn a_satisfying_or_executed_failed_result_must_retain_its_artifact() {
        assert_eq!(
            packet_with(|result| result.artifact = None),
            Err(Refusal::MissingArtifact {
                obligation_id: OBLIGATION_ID.into(),
                state: ResultState::Passed
            })
        );

        let obligation = sample_obligation();
        let failed_without_artifact = |kind: OfferedEvidenceKind| {
            let mut result = failed_before_result(&obligation);
            result.artifact = None;
            result.offered_evidence_kind = kind;
            packet(vec![obligation.clone()], vec![result]).validate()
        };
        assert_eq!(
            failed_without_artifact(OfferedEvidenceKind::ExecutedDiscriminatingControl),
            Err(Refusal::MissingArtifact {
                obligation_id: OBLIGATION_ID.into(),
                state: ResultState::Failed
            })
        );
        // A failed result that never ran the control has no artifact to retain.
        let evaluation = failed_without_artifact(OfferedEvidenceKind::OrdinaryPositiveTest);
        assert_eq!(
            evaluation.map(|evaluation| evaluation.satisfactions[0].satisfies),
            Ok(false)
        );
    }

    #[test]
    fn non_executed_evidence_is_accepted_only_when_it_claims_nothing() -> Result<(), String> {
        for kind in [
            OfferedEvidenceKind::OrdinaryPositiveTest,
            OfferedEvidenceKind::ReviewProse,
            OfferedEvidenceKind::StructuralDiscriminationClaim,
        ] {
            let obligation = sample_obligation();
            let mut result = passing_result(&obligation);
            result.offered_evidence_kind = kind;
            result.state = ResultState::NotProven;
            let evaluation = require_valid(packet(vec![obligation], vec![result]))?;
            assert_eq!(evaluation.satisfactions.len(), 1, "{kind:?}");
            assert!(!evaluation.satisfactions[0].satisfies, "{kind:?}");
            assert_eq!(
                evaluation.satisfactions[0].state,
                Some(ResultState::NotProven),
                "{kind:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn two_satisfying_results_for_one_obligation_are_refused() -> Result<(), String> {
        let obligation = sample_obligation();
        let twice = packet(
            vec![obligation.clone()],
            vec![passing_result(&obligation), passing_result(&obligation)],
        );
        assert_eq!(
            require_invalid(twice)?,
            Refusal::ConflictingSatisfyingResults(OBLIGATION_ID.into())
        );
        Ok(())
    }

    #[test]
    fn refusals_read_as_one_sentence_naming_the_subject() {
        for (refusal, message) in [
            (
                Refusal::EmptyField("owning_claim"),
                "owning_claim must not be empty".to_string(),
            ),
            (
                Refusal::InvalidSchemaVersion {
                    kind: "packet",
                    value: "0".into(),
                },
                "unsupported packet schema version \"0\"; expected 1".to_string(),
            ),
            (
                Refusal::InvalidHead {
                    field: "head",
                    value: "main".into(),
                },
                "head must be a full 40-character Git SHA, got \"main\"".to_string(),
            ),
            (
                Refusal::InvalidDigest {
                    field: "artifact.digest",
                    value: "sha256:".into(),
                },
                "artifact.digest must be sha256:<64 lowercase hex>, got \"sha256:\"".to_string(),
            ),
            (
                Refusal::MissingArtifact {
                    obligation_id: "claim:a".into(),
                    state: ResultState::Passed,
                },
                "Passed result for \"claim:a\" is missing retained artifact identity".to_string(),
            ),
            (
                Refusal::HeadMismatch {
                    obligation_id: "claim:a".into(),
                    expected: HEAD_AFTER.into(),
                    actual: "b".repeat(40),
                },
                format!(
                    "result for \"claim:a\" binds head {}, expected {HEAD_AFTER}",
                    "b".repeat(40)
                ),
            ),
            (
                Refusal::ConflictingSatisfyingResults("claim:a".into()),
                "obligation \"claim:a\" has more than one satisfying result".to_string(),
            ),
        ] {
            assert_eq!(refusal.to_string(), message);
        }
    }
}
