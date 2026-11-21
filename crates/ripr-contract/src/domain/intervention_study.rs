//! Matched RIPR intervention-study preregistration (RIPR-SPEC-0216 / #4649).
//!
//! This module owns the frozen protocol object and the study-law validator.
//! It does not execute agents, grade repairs, or emit an intervention-value
//! conclusion. JSON and Markdown projections live in `output`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Wire schema identity for a preregistered matched intervention study.
pub const RIPR_INTERVENTION_STUDY_SCHEMA_VERSION: &str = "ripr_intervention_study.v1";
/// The `kind` every study document carries.
pub const RIPR_INTERVENTION_STUDY_KIND: &str = "ripr_intervention_study";
/// The only accepted `implementation_state`: the document is a frozen
/// protocol, not a run or a result.
pub const RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE: &str = "preregistration_only";

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;
const GIT_SHA_LENGTH: usize = 40;

/// Stable rejection codes for the ten issue #4649 falsifiers plus digest/shape.
pub mod codes {
    /// The schema version, kind, implementation state or study phase is not the
    /// supported one.
    pub const UNSUPPORTED_SCHEMA: &str = "unsupported_schema";
    /// A required field or list is empty.
    pub const MISSING_FIELD: &str = "missing_field";
    /// An identity is malformed, duplicated or names something unknown, such as
    /// a task in an unknown stratum.
    pub const MALFORMED_IDENTITY: &str = "malformed_identity";
    /// Assignment could be chosen or changed after an outcome or grader signal
    /// is visible.
    pub const ASSIGNMENT_AFTER_OUTCOME: &str = "assignment_after_outcome";
    /// One condition gets extra tokens, time, tools or retries.
    pub const UNEQUAL_CONDITION_BUDGETS: &str = "unequal_condition_budgets";
    /// The control condition may read RIPR outputs.
    pub const CONTROL_CAN_READ_RIPR_OUTPUTS: &str = "control_can_read_ripr_outputs";
    /// The assisted condition gets repository context it does not declare.
    pub const ASSISTED_UNDECLARED_EXTRA_CONTEXT: &str = "assisted_undeclared_extra_context";
    /// A task may be replaced after a difficult failure.
    pub const TASK_REPLACEMENT_AFTER_FAILURE: &str = "task_replacement_after_failure";
    /// The retry policy differs between conditions.
    pub const RETRY_ONLY_IN_WEAKER_CONDITION: &str = "retry_only_in_weaker_condition";
    /// Timeouts or invalid attempts may be dropped from the denominator.
    pub const DROP_INVALID_FROM_DENOMINATOR: &str = "drop_timeouts_or_invalid_from_denominator";
    /// The study may stop on a favorable interim estimate.
    pub const STOPPING_ON_FAVORABLE_INTERIM: &str = "stopping_on_favorable_interim";
    /// The grader identity or rubric version is missing.
    pub const GRADER_OR_RUBRIC_ABSENT: &str = "grader_or_rubric_absent";
    /// A started protocol was changed under the same `study_id`.
    pub const PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID: &str =
        "protocol_mutation_requires_new_study_id";
    /// Outcome axes are combined into a score where one axis can make up for
    /// another.
    pub const COMPENSATING_OUTCOME_SCORE: &str = "compensating_outcome_score";
    /// The recorded protocol digest does not match the sealed payload.
    pub const PROTOCOL_DIGEST_MISMATCH: &str = "protocol_digest_mismatch";
}

/// Typed failure from the preregistration validator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterventionStudyError {
    /// The stable rejection code, one of the constants in [`codes`].
    pub code: &'static str,
    /// A human-readable explanation of the rejection.
    pub message: String,
}

impl fmt::Display for InterventionStudyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

fn error(code: &'static str, message: impl Into<String>) -> InterventionStudyError {
    InterventionStudyError {
        code,
        message: message.into(),
    }
}

/// Frozen preregistration for one matched RIPR intervention study.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiprInterventionStudyV1 {
    /// Must be [`RIPR_INTERVENTION_STUDY_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Must be [`RIPR_INTERVENTION_STUDY_KIND`].
    pub kind: String,
    /// Must be [`RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE`].
    pub implementation_state: String,
    /// The study's identity. A changed protocol needs a new one once the
    /// study has started.
    pub study_id: String,
    /// The protocol's version label.
    pub protocol_version: String,
    /// The `sha256:<64 lowercase hex>` digest sealed over
    /// [`RiprInterventionStudyV1::canonical_protocol_payload`].
    pub protocol_digest: String,
    /// The issue the study belongs to.
    pub parent_issue: String,
    /// The study's place in its sequence.
    pub sequence: String,
    /// Must be [`StudyPhase::Preregistered`] on this document.
    pub study_phase: StudyPhase,
    /// The exact repository and head the study analyzes.
    pub repository: RepositoryIdentity,
    /// The task series the study draws from.
    pub task_series: TaskSeries,
    /// Which tasks may and may not be selected.
    pub eligibility: EligibilityRules,
    /// The matched strata tasks are grouped into.
    pub strata: Vec<MatchedStratum>,
    /// The tasks selected before any outcome exists.
    pub tasks: Vec<StudyTask>,
    /// The two conditions, `control` and `ripr_assisted`.
    pub conditions: Vec<StudyCondition>,
    /// How tasks are assigned to conditions and in which order.
    pub assignment: AssignmentLaw,
    /// The model, operator, runtime, tool and resource budget both
    /// conditions share.
    pub shared_budget: ResourceBudget,
    /// Per-condition additions to the shared budget, keyed by condition id.
    pub condition_budget_overlays: BTreeMap<String, BudgetOverlay>,
    /// The RIPR evidence surface only the assisted condition may read.
    pub intervention_surface: InterventionSurface,
    /// Contamination and prior-exposure controls.
    pub leakage_controls: LeakageControls,
    /// The attempt, retry and task-replacement policy.
    pub attempt_policy: AttemptPolicy,
    /// The primary outcome axes, kept separate rather than combined.
    pub outcome_axes: Vec<OutcomeAxis>,
    /// An optional summary that may not stand in for the outcome axes.
    pub non_authoritative_summary: Option<NonAuthoritativeSummary>,
    /// The independent adjudication plan.
    pub adjudication: AdjudicationPlan,
    /// How missing, invalid and instrument-failure outcomes are counted.
    pub invalid_attempt_rules: InvalidAttemptRules,
    /// When the study may stop.
    pub stopping_rule: StoppingRule,
    /// The analysis planned before outcomes exist.
    pub planned_analysis: PlannedAnalysis,
    /// The strongest claim the study may support.
    pub claim_ceiling: ClaimCeiling,
    /// The lock that freezes the protocol once attempts start.
    pub protocol_lock: ProtocolLock,
    /// Claims the study explicitly does not make; at least one is required.
    pub non_claims: Vec<String>,
}

/// The phase a study document is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyPhase {
    /// `preregistered`: frozen before any outcome; the only phase this
    /// document accepts.
    Preregistered,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// The exact repository a study analyzes.
pub struct RepositoryIdentity {
    /// The repository's identity.
    pub repository_id: String,
    /// Where the repository is cloned from.
    pub clone_url: String,
    /// The full 40-character lowercase commit the study analyzes.
    pub analyzed_head_sha: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// The task series a study draws its tasks from.
pub struct TaskSeries {
    /// The series identity.
    pub task_series_id: String,
    /// What the series contains.
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Which tasks may be selected for the study.
pub struct EligibilityRules {
    /// Inclusion rules.
    pub include: Vec<String>,
    /// Exclusion rules.
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// A group of tasks matched on the properties that could affect outcomes.
pub struct MatchedStratum {
    /// The stratum identity, unique within the study.
    pub stratum_id: String,
    /// The family of behavior the tasks change.
    pub behavior_family: String,
    /// What proof already exists for the behavior.
    pub existing_proof_state: String,
    /// How complete the repair route is.
    pub route_completeness: String,
    /// The tasks' difficulty class.
    pub difficulty_class: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// One task selected for the study.
pub struct StudyTask {
    /// The task identity, unique within the study.
    pub task_id: String,
    /// The stratum the task belongs to; must name a declared stratum.
    pub stratum_id: String,
    /// The behavior the task is about.
    pub canonical_behavior: String,
    /// What proof already exists for the behavior.
    pub existing_proof_state: String,
    /// The fixture the task uses.
    pub fixture_id: String,
}

/// A study condition.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ConditionId {
    /// `control`: no access to RIPR outputs.
    Control,
    /// `ripr_assisted`: may read the declared RIPR evidence surface.
    RiprAssisted,
}

impl ConditionId {
    /// The snake_case wire token for this condition.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::RiprAssisted => "ripr_assisted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// One condition a task is run under.
pub struct StudyCondition {
    /// Which condition this is.
    pub condition_id: ConditionId,
    /// What the condition provides.
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// How tasks are assigned to conditions. Assignment must be fixed before any
/// outcome or grader signal is visible.
pub struct AssignmentLaw {
    /// Must be true: assignment was chosen before outcomes existed.
    pub selected_before_outcomes: bool,
    /// When assignment freezes; must be
    /// [`AssignmentFreezeRule::BeforeFirstOutcomeOrGraderSignal`].
    pub freeze_rule: AssignmentFreezeRule,
    /// Must be false: assignment cannot change once an outcome or grader
    /// signal is visible.
    pub may_change_after_outcome_or_grader_signal: bool,
    /// How tasks are paired across conditions.
    pub pairing: PairingLaw,
    /// One pair per selected task, together counterbalancing condition order.
    pub pairs: Vec<AssignmentPair>,
    /// The preregistered seed the assignment was drawn with.
    pub assignment_seed: String,
}

/// When assignment freezes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentFreezeRule {
    /// Before the first outcome or grader signal; the only accepted rule.
    BeforeFirstOutcomeOrGraderSignal,
    /// After an outcome is visible; rejected as `assignment_after_outcome`.
    AfterOutcomeVisible,
}

/// How tasks are paired across conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingLaw {
    /// Each task runs under both conditions, with condition order
    /// counterbalanced across matched strata.
    MatchedStratumCounterbalanced,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// The order one task runs under the two conditions.
pub struct AssignmentPair {
    /// The pair identity.
    pub pair_id: String,
    /// The task this pair assigns; each task appears in exactly one pair.
    pub task_id: String,
    /// The condition the task runs under first.
    pub first_condition: ConditionId,
    /// The condition the task runs under second; must differ from the first.
    pub second_condition: ConditionId,
}

/// The model, operator, runtime, tool, time, token, and retry budget that both
/// conditions share.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBudget {
    /// The model identity; must be non-empty.
    pub model_id: String,
    /// The operator profile; must be non-empty.
    pub operator_profile: String,
    /// The runtime identity; must be non-empty.
    pub runtime_id: String,
    /// The tools available to every attempt; must name at least one.
    pub tool_ids: Vec<String>,
    /// The wall-clock budget, in milliseconds.
    pub wall_clock_ms: u64,
    /// The token budget.
    pub token_budget: u64,
    /// The retry limit.
    pub retry_limit: u32,
    /// The command authority an attempt has, such as `test_only_repair`.
    pub command_authority: String,
}

/// What one condition adds to the shared budget. Validation accepts only an
/// overlay that adds nothing, so both conditions run on equal budgets.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetOverlay {
    /// Extra wall-clock time, in milliseconds; must be zero.
    pub additional_wall_clock_ms: u64,
    /// Extra tokens; must be zero.
    pub additional_token_budget: u64,
    /// Extra retries; must be zero.
    pub additional_retries: u32,
    /// Extra tools; must be empty.
    pub additional_tools: Vec<String>,
}

/// The RIPR evidence each condition may see. This is the only declared
/// difference between the conditions.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterventionSurface {
    /// The control surface; must not allow any RIPR receipt, view, command,
    /// or output.
    pub control: ConditionEvidenceSurface,
    /// The assisted surface; must name exact receipts, views, or commands.
    pub ripr_assisted: ConditionEvidenceSurface,
}

/// The RIPR evidence and repository context one condition may use.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionEvidenceSurface {
    /// RIPR receipt kinds the condition may read, such as `agent_receipt.v0.5`.
    pub allowed_ripr_receipts: Vec<String>,
    /// RIPR views the condition may read.
    pub allowed_ripr_views: Vec<String>,
    /// Exact RIPR command lines the condition may run.
    pub allowed_ripr_commands: Vec<String>,
    /// Whether the condition may read RIPR outputs at all.
    pub may_read_ripr_outputs: bool,
    /// Repository context given beyond the declared surface; must be empty
    /// for both conditions.
    pub undeclared_repository_context: Vec<String>,
}

/// How each contamination route between attempts is controlled. Recorded in
/// the protocol; `validate` does not check these values.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeakageControls {
    /// An operator who has seen the task before.
    pub prior_exposure: IsolationPolicy,
    /// Reuse of one attempt's branch by another attempt.
    pub branch_reuse: IsolationPolicy,
    /// Artifacts from one condition reaching the other.
    pub cross_condition_artifacts: IsolationPolicy,
    /// Scratch state shared between attempts.
    pub shared_scratch_state: IsolationPolicy,
    /// One operator carrying knowledge between conditions.
    pub operator_leakage: IsolationPolicy,
}

/// How one contamination route is handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationPolicy {
    /// `forbidden`: the route must not occur.
    Forbidden,
    /// `disclose_and_exclude`: an affected attempt is disclosed and excluded.
    DiscloseAndExclude,
    /// `independent_operators`: separate operators run each condition.
    IndependentOperators,
}

/// Rules for replacing tasks and retrying attempts.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptPolicy {
    /// Whether a task may be replaced after a failure; must be `forbidden`.
    pub task_replacement_after_failure: TaskReplacementPolicy,
    /// Whether both conditions get the same retry policy; must be `true`.
    pub retry_policy_equal_across_conditions: bool,
}

/// Whether a task may be replaced after it fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskReplacementPolicy {
    /// `forbidden`: the only accepted policy.
    Forbidden,
    /// `allowed_after_difficult_failure`: rejected by validation.
    AllowedAfterDifficultFailure,
}

/// One preregistered outcome axis. Validation requires the eight axes named in
/// RIPR-SPEC-0216, each scored on its own.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeAxis {
    /// The axis identity, such as `behavior_alignment`.
    pub axis_id: String,
    /// What the axis measures.
    pub description: String,
    /// Whether a high score on this axis may offset a low score on another;
    /// must be `false`.
    pub compensating: bool,
    /// Whether the axis can be observed on its own; must be `true`.
    pub independently_observable: bool,
}

/// A combined score reported alongside the outcome axes. Allowed only when it
/// is declared non-authoritative.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonAuthoritativeSummary {
    /// The summary's name.
    pub name: String,
    /// Whether the summary is authoritative; must be `false`.
    pub authoritative: bool,
}

/// How attempts are graded.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjudicationPlan {
    /// The graders; must name at least one.
    pub grader_identities: Vec<String>,
    /// The rubric identity; must be non-empty.
    pub rubric_id: String,
    /// The rubric version; must be non-empty.
    pub rubric_version: String,
    /// Whether graders are blind to the condition; must be `true`.
    pub blinded_to_condition: bool,
    /// The minimum number of independent graders per attempt; must be at
    /// least one.
    pub min_independent_graders: u32,
    /// What happens when graders disagree.
    pub disagreement_policy: DisagreementPolicy,
}

/// How grader disagreement is recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisagreementPolicy {
    /// `retain_per_axis_do_not_average_correctness`: keep each grader's score
    /// per axis and never average correctness.
    RetainPerAxisDoNotAverageCorrectness,
}

/// How invalid and incomplete attempts are counted. Every attempt stays in the
/// denominator.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidAttemptRules {
    /// The outcome cells an attempt can land in; must include all six.
    pub outcome_cells: Vec<OutcomeCell>,
    /// Keep timed-out attempts; must be `true`.
    pub retain_timeouts: bool,
    /// Keep cancelled attempts; must be `true`.
    pub retain_cancellations: bool,
    /// Keep invalid attempts; must be `true`.
    pub retain_invalid_attempts: bool,
    /// Keep attempts with missing evidence; must be `true`.
    pub retain_missing_evidence: bool,
    /// Keep attempts whose instrument failed; must be `true`.
    pub retain_instrument_failures: bool,
    /// Drop timed-out attempts; must be `false`.
    pub drop_timeouts: bool,
    /// Drop invalid attempts; must be `false`.
    pub drop_invalid_attempts: bool,
}

/// The typed outcome of one attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeCell {
    /// `pass`.
    Pass,
    /// `fail`.
    Fail,
    /// `partial`.
    Partial,
    /// `not_proven`: the attempt ended without the evidence to decide.
    NotProven,
    /// `invalid`: the attempt broke the protocol.
    Invalid,
    /// `instrument_failure`: the measuring harness failed, not the attempt.
    InstrumentFailure,
}

/// When the study stops collecting attempts.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoppingRule {
    /// The stopping kind; must be `fixed_sample`.
    pub kind: StoppingKind,
    /// The number of pairs to run; must equal the number of assignment pairs.
    pub planned_pair_count: u32,
    /// Whether stopping may depend on an interim estimate; must be `false`.
    pub may_depend_on_interim_estimate: bool,
    /// Whether a favorable interim result may stop the study; must be
    /// `forbidden`.
    pub favorable_interim_stop: FavorableInterimStop,
}

/// How the sample size is decided.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoppingKind {
    /// `fixed_sample`: run every preregistered pair; the only accepted kind.
    FixedSample,
    /// `stop_on_favorable_interim`: rejected by validation.
    StopOnFavorableInterim,
}

/// Whether a favorable interim result may end the study early.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FavorableInterimStop {
    /// `forbidden`: the only accepted value.
    Forbidden,
    /// `allowed`: rejected by validation.
    Allowed,
}

/// The preregistered analysis.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedAnalysis {
    /// The unit of analysis, such as one exact repository, head, and behavior
    /// task.
    pub unit: String,
    /// The conditions compared.
    pub compares: Vec<String>,
    /// Whether results report exact numerators and denominators; must be
    /// `true`.
    pub reports_exact_numerators_and_denominators: bool,
}

/// The limit on what the study's conclusions may claim.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCeiling {
    /// Conclusions hold only for this task series; must be `true`.
    pub bounded_to_task_series: bool,
    /// Conclusions hold only for this model and operator profile; must be
    /// `true`.
    pub bounded_to_model_operator_profile: bool,
    /// Conclusions hold only for this intervention form; must be `true`.
    pub bounded_to_intervention_form: bool,
    /// Whether the preregistration itself shows the intervention has value;
    /// must be `false`.
    pub preregistration_proves_intervention_value: bool,
}

/// Whether the protocol may change once attempts begin.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolLock {
    /// Whether the protocol is frozen after the first attempt; must be `true`.
    pub immutable_after_first_attempt: bool,
    /// Whether a first attempt has been recorded.
    pub first_attempt_recorded: bool,
    /// Whether the protocol may change without a new `study_id`; must be
    /// `forbidden`.
    pub mutation_without_new_study_id: MutationPolicy,
}

/// Whether a protocol change may keep the same `study_id`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationPolicy {
    /// `forbidden`: the only accepted value.
    Forbidden,
    /// `allowed`: rejected by validation.
    Allowed,
}

impl RiprInterventionStudyV1 {
    /// Validate study laws for one protocol document. Digest matching against
    /// rendered JSON is owned by the output projection.
    pub fn validate(&self) -> Result<(), InterventionStudyError> {
        self.validate_identity()?;
        self.validate_tasks_and_assignment()?;
        self.validate_budgets_and_retries()?;
        self.validate_intervention_and_leakage()?;
        self.validate_attempts_and_denominator()?;
        self.validate_outcomes_and_adjudication()?;
        self.validate_stopping_and_claims()?;
        Ok(())
    }

    /// Reject a successor that mutates a started study without a new identity.
    pub fn validate_successor(&self, successor: &Self) -> Result<(), InterventionStudyError> {
        self.validate()?;
        successor.validate()?;
        if self.study_id != successor.study_id {
            return Ok(());
        }
        // The recorded-attempt lock is monotonic under one study identity:
        // clearing it would reopen the protocol for later same-ID mutation.
        if self.protocol_lock.first_attempt_recorded
            && !successor.protocol_lock.first_attempt_recorded
        {
            return Err(error(
                codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
                "a recorded first attempt cannot be cleared under the same study_id",
            ));
        }
        // Either side carrying a recorded attempt freezes the payload, so a
        // mutation cannot ride along with the transition that records the
        // first attempt.
        if (self.protocol_lock.first_attempt_recorded
            || successor.protocol_lock.first_attempt_recorded)
            && self.canonical_protocol_payload() != successor.canonical_protocol_payload()
        {
            return Err(error(
                codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
                "protocol mutation after the first attempt requires a new study_id",
            ));
        }
        Ok(())
    }

    /// Semantic payload used for digest and mutation identity.
    ///
    /// `protocol_digest` is derived from this payload. `first_attempt_recorded`
    /// is observed lock state, not a protocol amendment.
    pub fn canonical_protocol_payload(&self) -> Self {
        let mut payload = self.clone();
        payload.protocol_digest.clear();
        payload.protocol_lock.first_attempt_recorded = false;
        payload
    }

    fn validate_identity(&self) -> Result<(), InterventionStudyError> {
        if self.schema_version != RIPR_INTERVENTION_STUDY_SCHEMA_VERSION {
            return Err(error(
                codes::UNSUPPORTED_SCHEMA,
                format!("schema_version must be {RIPR_INTERVENTION_STUDY_SCHEMA_VERSION}"),
            ));
        }
        if self.kind != RIPR_INTERVENTION_STUDY_KIND {
            return Err(error(
                codes::UNSUPPORTED_SCHEMA,
                format!("kind must be {RIPR_INTERVENTION_STUDY_KIND}"),
            ));
        }
        if self.implementation_state != RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE {
            return Err(error(
                codes::UNSUPPORTED_SCHEMA,
                format!(
                    "implementation_state must be {RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE}"
                ),
            ));
        }
        if self.study_phase != StudyPhase::Preregistered {
            return Err(error(
                codes::UNSUPPORTED_SCHEMA,
                "study_phase must remain preregistered on this protocol document",
            ));
        }
        require_non_empty("study_id", &self.study_id)?;
        require_non_empty("protocol_version", &self.protocol_version)?;
        require_sha256("protocol_digest", &self.protocol_digest)?;
        require_non_empty("parent_issue", &self.parent_issue)?;
        require_non_empty("sequence", &self.sequence)?;
        require_non_empty("repository_id", &self.repository.repository_id)?;
        require_non_empty("clone_url", &self.repository.clone_url)?;
        require_git_sha("analyzed_head_sha", &self.repository.analyzed_head_sha)?;
        require_non_empty("task_series_id", &self.task_series.task_series_id)?;
        if self.non_claims.is_empty() {
            return Err(error(
                codes::MISSING_FIELD,
                "non_claims must name at least one excluded claim",
            ));
        }
        Ok(())
    }

    fn validate_tasks_and_assignment(&self) -> Result<(), InterventionStudyError> {
        if self.strata.is_empty() || self.tasks.is_empty() {
            return Err(error(
                codes::MISSING_FIELD,
                "matched strata and tasks must be selected before outcomes",
            ));
        }
        let stratum_ids: BTreeSet<&str> = self
            .strata
            .iter()
            .map(|stratum| stratum.stratum_id.as_str())
            .collect();
        if stratum_ids.len() != self.strata.len() {
            return Err(error(
                codes::MALFORMED_IDENTITY,
                "stratum_id values must be unique",
            ));
        }
        let mut task_ids = BTreeSet::new();
        for task in &self.tasks {
            require_non_empty("task_id", &task.task_id)?;
            if !task_ids.insert(task.task_id.as_str()) {
                return Err(error(
                    codes::MALFORMED_IDENTITY,
                    "task_id values must be unique",
                ));
            }
            if !stratum_ids.contains(task.stratum_id.as_str()) {
                return Err(error(
                    codes::MALFORMED_IDENTITY,
                    format!(
                        "task {} names unknown stratum {}",
                        task.task_id, task.stratum_id
                    ),
                ));
            }
        }
        if !self.assignment.selected_before_outcomes
            || self.assignment.may_change_after_outcome_or_grader_signal
            || self.assignment.freeze_rule != AssignmentFreezeRule::BeforeFirstOutcomeOrGraderSignal
        {
            return Err(error(
                codes::ASSIGNMENT_AFTER_OUTCOME,
                "assignment cannot be chosen or changed after an attempt outcome or grader signal is visible",
            ));
        }
        if self.assignment.pairs.len() != self.tasks.len() {
            return Err(error(
                codes::MISSING_FIELD,
                "every selected task must appear in exactly one assignment pair",
            ));
        }
        let mut assigned = BTreeSet::new();
        let mut first_conditions = BTreeSet::new();
        for pair in &self.assignment.pairs {
            require_non_empty("pair_id", &pair.pair_id)?;
            if !task_ids.contains(pair.task_id.as_str()) {
                return Err(error(
                    codes::MALFORMED_IDENTITY,
                    format!(
                        "assignment pair {} names unknown task {}",
                        pair.pair_id, pair.task_id
                    ),
                ));
            }
            if !assigned.insert(pair.task_id.as_str()) {
                return Err(error(
                    codes::MALFORMED_IDENTITY,
                    format!("task {} is assigned more than once", pair.task_id),
                ));
            }
            if pair.first_condition == pair.second_condition
                || pair.first_condition != ConditionId::Control
                    && pair.second_condition != ConditionId::Control
                || pair.first_condition != ConditionId::RiprAssisted
                    && pair.second_condition != ConditionId::RiprAssisted
            {
                return Err(error(
                    codes::MALFORMED_IDENTITY,
                    format!(
                        "pair {} must counterbalance control and ripr_assisted",
                        pair.pair_id
                    ),
                ));
            }
            first_conditions.insert(pair.first_condition);
        }
        if first_conditions.len() < 2 {
            return Err(error(
                codes::MALFORMED_IDENTITY,
                "matched pairs must counterbalance condition order",
            ));
        }
        Ok(())
    }

    fn validate_budgets_and_retries(&self) -> Result<(), InterventionStudyError> {
        require_non_empty("model_id", &self.shared_budget.model_id)?;
        require_non_empty("operator_profile", &self.shared_budget.operator_profile)?;
        require_non_empty("runtime_id", &self.shared_budget.runtime_id)?;
        if self.shared_budget.tool_ids.is_empty() {
            return Err(error(codes::MISSING_FIELD, "shared tool_ids must be named"));
        }
        let required = [
            ConditionId::Control.as_str(),
            ConditionId::RiprAssisted.as_str(),
        ];
        if self.conditions.len() != 2
            || required.iter().any(|id| {
                !self
                    .conditions
                    .iter()
                    .any(|condition| condition.condition_id.as_str() == *id)
            })
        {
            return Err(error(
                codes::MISSING_FIELD,
                "protocol must declare exactly control and ripr_assisted",
            ));
        }
        for condition in &self.conditions {
            let overlay = self
                .condition_budget_overlays
                .get(condition.condition_id.as_str())
                .ok_or_else(|| {
                    error(
                        codes::MISSING_FIELD,
                        format!(
                            "condition {} is missing a budget overlay",
                            condition.condition_id.as_str()
                        ),
                    )
                })?;
            if overlay.additional_wall_clock_ms != 0
                || overlay.additional_token_budget != 0
                || overlay.additional_retries != 0
                || !overlay.additional_tools.is_empty()
            {
                return Err(error(
                    codes::UNEQUAL_CONDITION_BUDGETS,
                    "control and ripr_assisted must share model, operator, runtime, tool, time, token, and retry budgets except for the declared intervention",
                ));
            }
        }
        if !self.attempt_policy.retry_policy_equal_across_conditions {
            return Err(error(
                codes::RETRY_ONLY_IN_WEAKER_CONDITION,
                "retries cannot be granted only in the weaker condition",
            ));
        }
        Ok(())
    }

    fn validate_intervention_and_leakage(&self) -> Result<(), InterventionStudyError> {
        let control = &self.intervention_surface.control;
        if control.may_read_ripr_outputs
            || !control.allowed_ripr_receipts.is_empty()
            || !control.allowed_ripr_views.is_empty()
            || !control.allowed_ripr_commands.is_empty()
        {
            return Err(error(
                codes::CONTROL_CAN_READ_RIPR_OUTPUTS,
                "control attempts must not read RIPR receipts, views, or commands",
            ));
        }
        let assisted = &self.intervention_surface.ripr_assisted;
        if assisted.allowed_ripr_receipts.is_empty()
            && assisted.allowed_ripr_views.is_empty()
            && assisted.allowed_ripr_commands.is_empty()
        {
            return Err(error(
                codes::MISSING_FIELD,
                "ripr_assisted must name exact receipts, views, or commands rather than summarizing use RIPR",
            ));
        }
        if !assisted.undeclared_repository_context.is_empty()
            || !control.undeclared_repository_context.is_empty()
        {
            return Err(error(
                codes::ASSISTED_UNDECLARED_EXTRA_CONTEXT,
                "a RIPR-assisted attempt cannot receive undeclared extra repository context",
            ));
        }
        Ok(())
    }

    fn validate_attempts_and_denominator(&self) -> Result<(), InterventionStudyError> {
        if self.attempt_policy.task_replacement_after_failure != TaskReplacementPolicy::Forbidden {
            return Err(error(
                codes::TASK_REPLACEMENT_AFTER_FAILURE,
                "tasks cannot be replaced after a difficult failure",
            ));
        }
        let rules = &self.invalid_attempt_rules;
        let required_cells = [
            OutcomeCell::Pass,
            OutcomeCell::Fail,
            OutcomeCell::Partial,
            OutcomeCell::NotProven,
            OutcomeCell::Invalid,
            OutcomeCell::InstrumentFailure,
        ];
        for cell in required_cells {
            if !rules.outcome_cells.contains(&cell) {
                return Err(error(
                    codes::MISSING_FIELD,
                    "invalid and incomplete attempts need typed denominator-preserving outcome cells",
                ));
            }
        }
        if rules.drop_timeouts
            || rules.drop_invalid_attempts
            || !rules.retain_timeouts
            || !rules.retain_cancellations
            || !rules.retain_invalid_attempts
            || !rules.retain_missing_evidence
            || !rules.retain_instrument_failures
        {
            return Err(error(
                codes::DROP_INVALID_FROM_DENOMINATOR,
                "timeouts, cancellations, invalid attempts, missing evidence, and instrument failures remain in the denominator",
            ));
        }
        Ok(())
    }

    fn validate_outcomes_and_adjudication(&self) -> Result<(), InterventionStudyError> {
        let required_axes = [
            "behavior_alignment",
            "discriminating_proof_quality",
            "change_scope_and_churn",
            "review_findings_and_repair_burden",
            "completion_terminal_state",
            "time_and_bounded_resource_use",
            "maintainability_readability",
            "false_confidence_or_unsupported_claim_events",
        ];
        let present: BTreeSet<&str> = self
            .outcome_axes
            .iter()
            .map(|axis| axis.axis_id.as_str())
            .collect();
        for axis_id in required_axes {
            if !present.contains(axis_id) {
                return Err(error(
                    codes::MISSING_FIELD,
                    format!("outcome axis {axis_id} must be preregistered"),
                ));
            }
        }
        for axis in &self.outcome_axes {
            if axis.compensating || !axis.independently_observable {
                return Err(error(
                    codes::COMPENSATING_OUTCOME_SCORE,
                    "outcome axes must remain non-compensating and independently observable",
                ));
            }
        }
        if let Some(summary) = &self.non_authoritative_summary
            && summary.authoritative
        {
            return Err(error(
                codes::COMPENSATING_OUTCOME_SCORE,
                "a combined score is allowed only when explicitly declared non-authoritative",
            ));
        }
        if self.adjudication.grader_identities.is_empty()
            || self.adjudication.rubric_id.trim().is_empty()
            || self.adjudication.rubric_version.trim().is_empty()
            || self.adjudication.min_independent_graders == 0
            || !self.adjudication.blinded_to_condition
        {
            return Err(error(
                codes::GRADER_OR_RUBRIC_ABSENT,
                "independent adjudication requires grader identity, a rubric version, and condition blinding",
            ));
        }
        Ok(())
    }

    fn validate_stopping_and_claims(&self) -> Result<(), InterventionStudyError> {
        if self.stopping_rule.kind != StoppingKind::FixedSample
            || self.stopping_rule.may_depend_on_interim_estimate
            || self.stopping_rule.favorable_interim_stop != FavorableInterimStop::Forbidden
        {
            return Err(error(
                codes::STOPPING_ON_FAVORABLE_INTERIM,
                "the stopping rule cannot depend on a favorable interim estimate",
            ));
        }
        // A count mismatch is a malformed fixed sample, not an interim stop;
        // runners and adjudicators key on the code, so keep them distinct.
        if self.stopping_rule.planned_pair_count == 0
            || u32::try_from(self.assignment.pairs.len()).ok()
                != Some(self.stopping_rule.planned_pair_count)
        {
            return Err(error(
                codes::MALFORMED_IDENTITY,
                "planned_pair_count must equal the number of preregistered assignment pairs",
            ));
        }
        if !self.protocol_lock.immutable_after_first_attempt
            || self.protocol_lock.mutation_without_new_study_id != MutationPolicy::Forbidden
        {
            return Err(error(
                codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
                "protocol mutation after the first attempt requires a new study_id",
            ));
        }
        if self.claim_ceiling.preregistration_proves_intervention_value
            || !self.claim_ceiling.bounded_to_task_series
            || !self.claim_ceiling.bounded_to_model_operator_profile
            || !self.claim_ceiling.bounded_to_intervention_form
            || !self
                .planned_analysis
                .reports_exact_numerators_and_denominators
        {
            return Err(error(
                codes::MISSING_FIELD,
                "public conclusions are bounded to the exact task series, model/operator profile, and intervention form",
            ));
        }
        Ok(())
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), InterventionStudyError> {
    if value.trim().is_empty() {
        Err(error(
            codes::MISSING_FIELD,
            format!("{field} must be non-empty"),
        ))
    } else {
        Ok(())
    }
}

fn require_sha256(field: &str, value: &str) -> Result<(), InterventionStudyError> {
    let hex = value.strip_prefix(SHA256_PREFIX).ok_or_else(|| {
        error(
            codes::MALFORMED_IDENTITY,
            format!("{field} must use {SHA256_PREFIX}<64-hex>"),
        )
    })?;
    if hex.len() != SHA256_HEX_LENGTH
        || !hex.chars().all(|ch| ch.is_ascii_hexdigit())
        || hex != hex.to_ascii_lowercase()
    {
        return Err(error(
            codes::MALFORMED_IDENTITY,
            format!("{field} must be 64 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn require_git_sha(field: &str, value: &str) -> Result<(), InterventionStudyError> {
    if value.len() != GIT_SHA_LENGTH
        || !value.chars().all(|ch| ch.is_ascii_hexdigit())
        || value != value.to_ascii_lowercase()
    {
        return Err(error(
            codes::MALFORMED_IDENTITY,
            format!("{field} must be a 40-character lowercase git SHA"),
        ));
    }
    Ok(())
}

/// Example IV01 preregistration used by tests and the committed fixture.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn example_preregistered_study() -> RiprInterventionStudyV1 {
    let dummy_digest = format!("{SHA256_PREFIX}{}", "0".repeat(SHA256_HEX_LENGTH));
    RiprInterventionStudyV1 {
        schema_version: RIPR_INTERVENTION_STUDY_SCHEMA_VERSION.to_string(),
        kind: RIPR_INTERVENTION_STUDY_KIND.to_string(),
        implementation_state: RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE.to_string(),
        study_id: "study:ripr-intervention:iv01:matched-rust-boundary".to_string(),
        protocol_version: "1".to_string(),
        protocol_digest: dummy_digest,
        parent_issue: "#3751".to_string(),
        sequence: "IV01".to_string(),
        study_phase: StudyPhase::Preregistered,
        repository: RepositoryIdentity {
            repository_id: "EffortlessMetrics/ripr-swarm".to_string(),
            clone_url: "https://github.com/EffortlessMetrics/ripr-swarm".to_string(),
            analyzed_head_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        },
        task_series: TaskSeries {
            task_series_id: "series:rust-boundary-missing-discriminator".to_string(),
            description: "Matched ordinary Rust missing-discriminator repair tasks".to_string(),
        },
        eligibility: EligibilityRules {
            include: vec![
                "producer-owned Rust route with retained exact identity".to_string(),
                "test-only repair contract".to_string(),
            ],
            exclude: vec![
                "unsafe or unresolved target".to_string(),
                "production-code edit".to_string(),
            ],
        },
        strata: vec![MatchedStratum {
            stratum_id: "stratum:rust-boundary-missing-discriminator".to_string(),
            behavior_family: "predicate_boundary".to_string(),
            existing_proof_state: "missing_discriminator".to_string(),
            route_completeness: "complete_repair_route".to_string(),
            difficulty_class: "ordinary".to_string(),
        }],
        tasks: vec![
            StudyTask {
                task_id: "task:boundary-oracle-a".to_string(),
                stratum_id: "stratum:rust-boundary-missing-discriminator".to_string(),
                canonical_behavior: "equality boundary on parse_limit".to_string(),
                existing_proof_state: "missing_discriminator".to_string(),
                fixture_id: "fixtures/boundary_gap".to_string(),
            },
            StudyTask {
                task_id: "task:boundary-oracle-b".to_string(),
                stratum_id: "stratum:rust-boundary-missing-discriminator".to_string(),
                canonical_behavior: "inclusive upper-bound on retry_budget".to_string(),
                existing_proof_state: "missing_discriminator".to_string(),
                fixture_id: "fixtures/strong_boundary_oracle".to_string(),
            },
        ],
        conditions: vec![
            StudyCondition {
                condition_id: ConditionId::Control,
                description: "repository change plus ordinary source, tests, and tooling"
                    .to_string(),
            },
            StudyCondition {
                condition_id: ConditionId::RiprAssisted,
                description: "same task context plus the named RIPR evidence surface".to_string(),
            },
        ],
        assignment: AssignmentLaw {
            selected_before_outcomes: true,
            freeze_rule: AssignmentFreezeRule::BeforeFirstOutcomeOrGraderSignal,
            may_change_after_outcome_or_grader_signal: false,
            pairing: PairingLaw::MatchedStratumCounterbalanced,
            pairs: vec![
                AssignmentPair {
                    pair_id: "pair:boundary-oracle-a".to_string(),
                    task_id: "task:boundary-oracle-a".to_string(),
                    first_condition: ConditionId::Control,
                    second_condition: ConditionId::RiprAssisted,
                },
                AssignmentPair {
                    pair_id: "pair:boundary-oracle-b".to_string(),
                    task_id: "task:boundary-oracle-b".to_string(),
                    first_condition: ConditionId::RiprAssisted,
                    second_condition: ConditionId::Control,
                },
            ],
            assignment_seed: "preregistered:iv01:counterbalanced".to_string(),
        },
        shared_budget: ResourceBudget {
            model_id: "model:study-operator-profile-v1".to_string(),
            operator_profile: "operator:independent-study-agent".to_string(),
            runtime_id: "runtime:isolated-worktree".to_string(),
            tool_ids: vec!["cargo".to_string(), "rustc".to_string(), "git".to_string()],
            wall_clock_ms: 1_800_000,
            token_budget: 50_000,
            retry_limit: 0,
            command_authority: "test_only_repair".to_string(),
        },
        condition_budget_overlays: BTreeMap::from([
            (ConditionId::Control.as_str().to_string(), empty_overlay()),
            (
                ConditionId::RiprAssisted.as_str().to_string(),
                empty_overlay(),
            ),
        ]),
        intervention_surface: InterventionSurface {
            control: ConditionEvidenceSurface {
                allowed_ripr_receipts: Vec::new(),
                allowed_ripr_views: Vec::new(),
                allowed_ripr_commands: Vec::new(),
                may_read_ripr_outputs: false,
                undeclared_repository_context: Vec::new(),
            },
            ripr_assisted: ConditionEvidenceSurface {
                allowed_ripr_receipts: vec!["agent_receipt.v0.5".to_string()],
                allowed_ripr_views: vec!["agent_seam_packet.v0.4".to_string()],
                allowed_ripr_commands: vec![
                    "ripr check --format json".to_string(),
                    "ripr agent packet --json".to_string(),
                ],
                may_read_ripr_outputs: true,
                undeclared_repository_context: Vec::new(),
            },
        },
        leakage_controls: LeakageControls {
            prior_exposure: IsolationPolicy::DiscloseAndExclude,
            branch_reuse: IsolationPolicy::Forbidden,
            cross_condition_artifacts: IsolationPolicy::Forbidden,
            shared_scratch_state: IsolationPolicy::Forbidden,
            operator_leakage: IsolationPolicy::IndependentOperators,
        },
        attempt_policy: AttemptPolicy {
            task_replacement_after_failure: TaskReplacementPolicy::Forbidden,
            retry_policy_equal_across_conditions: true,
        },
        outcome_axes: required_outcome_axes(),
        non_authoritative_summary: None,
        adjudication: AdjudicationPlan {
            grader_identities: vec!["grader:independent-a".to_string()],
            rubric_id: "ripr-intervention-rubric.v1".to_string(),
            rubric_version: "1".to_string(),
            blinded_to_condition: true,
            min_independent_graders: 1,
            disagreement_policy: DisagreementPolicy::RetainPerAxisDoNotAverageCorrectness,
        },
        invalid_attempt_rules: InvalidAttemptRules {
            outcome_cells: vec![
                OutcomeCell::Pass,
                OutcomeCell::Fail,
                OutcomeCell::Partial,
                OutcomeCell::NotProven,
                OutcomeCell::Invalid,
                OutcomeCell::InstrumentFailure,
            ],
            retain_timeouts: true,
            retain_cancellations: true,
            retain_invalid_attempts: true,
            retain_missing_evidence: true,
            retain_instrument_failures: true,
            drop_timeouts: false,
            drop_invalid_attempts: false,
        },
        stopping_rule: StoppingRule {
            kind: StoppingKind::FixedSample,
            planned_pair_count: 2,
            may_depend_on_interim_estimate: false,
            favorable_interim_stop: FavorableInterimStop::Forbidden,
        },
        planned_analysis: PlannedAnalysis {
            unit: "exact repository/head/canonical behavior task".to_string(),
            compares: vec!["control".to_string(), "ripr_assisted".to_string()],
            reports_exact_numerators_and_denominators: true,
        },
        claim_ceiling: ClaimCeiling {
            bounded_to_task_series: true,
            bounded_to_model_operator_profile: true,
            bounded_to_intervention_form: true,
            preregistration_proves_intervention_value: false,
        },
        protocol_lock: ProtocolLock {
            immutable_after_first_attempt: true,
            first_attempt_recorded: false,
            mutation_without_new_study_id: MutationPolicy::Forbidden,
        },
        non_claims: vec![
            "no_agent_execution".to_string(),
            "no_repair_adjudication".to_string(),
            "no_pilot_result".to_string(),
            "no_intervention_value".to_string(),
            "no_generic_ab_platform".to_string(),
        ],
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn empty_overlay() -> BudgetOverlay {
    BudgetOverlay {
        additional_wall_clock_ms: 0,
        additional_token_budget: 0,
        additional_retries: 0,
        additional_tools: Vec::new(),
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn required_outcome_axes() -> Vec<OutcomeAxis> {
    [
        (
            "behavior_alignment",
            "test targets the exact changed behavior, owner, and sink",
        ),
        (
            "discriminating_proof_quality",
            "test would notice if the selected behavior were wrong",
        ),
        (
            "change_scope_and_churn",
            "edit stays inside the allowed test surface",
        ),
        (
            "review_findings_and_repair_burden",
            "independent review findings and follow-up repairs",
        ),
        (
            "completion_terminal_state",
            "typed terminal completeness of the attempt",
        ),
        (
            "time_and_bounded_resource_use",
            "elapsed effort and budget consumption",
        ),
        (
            "maintainability_readability",
            "independent readability and maintenance quality",
        ),
        (
            "false_confidence_or_unsupported_claim_events",
            "unsupported or false-success claims",
        ),
    ]
    .into_iter()
    .map(|(axis_id, description)| OutcomeAxis {
        axis_id: axis_id.to_string(),
        description: description.to_string(),
        compensating: false,
        independently_observable: true,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_code(result: Result<(), InterventionStudyError>, code: &str) -> Result<(), String> {
        match result {
            Ok(()) => Err(format!("expected rejection {code}")),
            Err(error) if error.code == code => Ok(()),
            Err(error) => Err(format!(
                "expected {code}, got {}: {}",
                error.code, error.message
            )),
        }
    }

    #[test]
    fn example_preregistration_satisfies_study_laws() -> Result<(), String> {
        example_preregistered_study()
            .validate()
            .map_err(|error| error.to_string())
    }

    #[test]
    fn assignment_after_outcome_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.assignment.may_change_after_outcome_or_grader_signal = true;
        assert_code(study.validate(), codes::ASSIGNMENT_AFTER_OUTCOME)?;
        study.assignment.may_change_after_outcome_or_grader_signal = false;
        study.assignment.freeze_rule = AssignmentFreezeRule::AfterOutcomeVisible;
        assert_code(study.validate(), codes::ASSIGNMENT_AFTER_OUTCOME)
    }

    #[test]
    fn unequal_condition_budgets_are_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        let overlay = study
            .condition_budget_overlays
            .get_mut(ConditionId::RiprAssisted.as_str())
            .ok_or("assisted overlay")?;
        overlay.additional_token_budget = 8_000;
        assert_code(study.validate(), codes::UNEQUAL_CONDITION_BUDGETS)
    }

    #[test]
    fn control_reading_ripr_outputs_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.intervention_surface.control.may_read_ripr_outputs = true;
        assert_code(study.validate(), codes::CONTROL_CAN_READ_RIPR_OUTPUTS)?;
        study.intervention_surface.control.may_read_ripr_outputs = false;
        study
            .intervention_surface
            .control
            .allowed_ripr_commands
            .push("ripr check --format json".to_string());
        assert_code(study.validate(), codes::CONTROL_CAN_READ_RIPR_OUTPUTS)
    }

    #[test]
    fn assisted_undeclared_context_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study
            .intervention_surface
            .ripr_assisted
            .undeclared_repository_context
            .push("issue analysis not named by the protocol".to_string());
        assert_code(study.validate(), codes::ASSISTED_UNDECLARED_EXTRA_CONTEXT)
    }

    #[test]
    fn task_replacement_after_failure_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.attempt_policy.task_replacement_after_failure =
            TaskReplacementPolicy::AllowedAfterDifficultFailure;
        assert_code(study.validate(), codes::TASK_REPLACEMENT_AFTER_FAILURE)
    }

    #[test]
    fn retry_only_in_weaker_condition_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.attempt_policy.retry_policy_equal_across_conditions = false;
        assert_code(study.validate(), codes::RETRY_ONLY_IN_WEAKER_CONDITION)
    }

    #[test]
    fn dropping_timeouts_or_invalid_attempts_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.invalid_attempt_rules.drop_timeouts = true;
        assert_code(study.validate(), codes::DROP_INVALID_FROM_DENOMINATOR)?;
        study.invalid_attempt_rules.drop_timeouts = false;
        study.invalid_attempt_rules.retain_invalid_attempts = false;
        assert_code(study.validate(), codes::DROP_INVALID_FROM_DENOMINATOR)
    }

    #[test]
    fn stopping_on_favorable_interim_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.stopping_rule.kind = StoppingKind::StopOnFavorableInterim;
        study.stopping_rule.may_depend_on_interim_estimate = true;
        study.stopping_rule.favorable_interim_stop = FavorableInterimStop::Allowed;
        assert_code(study.validate(), codes::STOPPING_ON_FAVORABLE_INTERIM)
    }

    #[test]
    fn missing_grader_or_rubric_is_rejected() -> Result<(), String> {
        let mut study = example_preregistered_study();
        study.adjudication.grader_identities.clear();
        assert_code(study.validate(), codes::GRADER_OR_RUBRIC_ABSENT)?;
        study.adjudication.grader_identities = vec!["grader:independent-a".to_string()];
        study.adjudication.rubric_version.clear();
        assert_code(study.validate(), codes::GRADER_OR_RUBRIC_ABSENT)
    }

    #[test]
    fn protocol_mutation_after_first_attempt_requires_new_study_id() -> Result<(), String> {
        let mut predecessor = example_preregistered_study();
        predecessor.protocol_lock.first_attempt_recorded = true;
        let mut successor = predecessor.clone();
        successor.shared_budget.token_budget = 40_000;
        assert_code(
            predecessor.validate_successor(&successor),
            codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
        )?;
        successor.study_id = "study:ripr-intervention:iv01:successor".to_string();
        predecessor
            .validate_successor(&successor)
            .map_err(|error| error.to_string())
    }

    #[test]
    fn each_missing_required_outcome_cell_is_rejected() -> Result<(), String> {
        for cell in [
            OutcomeCell::Pass,
            OutcomeCell::Fail,
            OutcomeCell::Partial,
            OutcomeCell::NotProven,
            OutcomeCell::Invalid,
            OutcomeCell::InstrumentFailure,
        ] {
            let mut study = example_preregistered_study();
            let before = study.invalid_attempt_rules.outcome_cells.len();
            study
                .invalid_attempt_rules
                .outcome_cells
                .retain(|present| *present != cell);
            if study.invalid_attempt_rules.outcome_cells.len() == before {
                return Err(format!("example study did not carry {cell:?}"));
            }
            assert_code(study.validate(), codes::MISSING_FIELD)?;
        }
        Ok(())
    }

    #[test]
    fn planned_pair_count_mismatch_is_malformed_identity_not_an_interim_stop() -> Result<(), String>
    {
        for planned in [0, 3] {
            let mut study = example_preregistered_study();
            study.stopping_rule.planned_pair_count = planned;
            assert_code(study.validate(), codes::MALFORMED_IDENTITY)?;
        }
        let mut interim = example_preregistered_study();
        interim.stopping_rule.may_depend_on_interim_estimate = true;
        assert_code(interim.validate(), codes::STOPPING_ON_FAVORABLE_INTERIM)
    }

    #[test]
    fn recorded_attempt_lock_cannot_be_cleared_under_the_same_study_id() -> Result<(), String> {
        let mut predecessor = example_preregistered_study();
        predecessor.protocol_lock.first_attempt_recorded = true;
        let mut reset = predecessor.clone();
        reset.protocol_lock.first_attempt_recorded = false;
        assert_code(
            predecessor.validate_successor(&reset),
            codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
        )?;
        // An unchanged recorded successor remains a valid transition.
        predecessor
            .validate_successor(&predecessor.clone())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn mutation_cannot_ride_along_with_recording_the_first_attempt() -> Result<(), String> {
        let predecessor = example_preregistered_study();
        let mut successor = predecessor.clone();
        successor.protocol_lock.first_attempt_recorded = true;
        // Recording the first attempt without mutation is the ordinary start.
        predecessor
            .validate_successor(&successor)
            .map_err(|error| error.to_string())?;
        successor.shared_budget.token_budget = 40_000;
        assert_code(
            predecessor.validate_successor(&successor),
            codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
        )?;
        // Pre-start amendment without a recorded attempt stays permitted.
        successor.protocol_lock.first_attempt_recorded = false;
        predecessor
            .validate_successor(&successor)
            .map_err(|error| error.to_string())
    }

    #[test]
    fn required_axes_stay_non_compensating() -> Result<(), String> {
        let mut study = example_preregistered_study();
        let axis = study
            .outcome_axes
            .get_mut(0)
            .ok_or("missing first outcome axis")?;
        axis.compensating = true;
        assert_code(study.validate(), codes::COMPENSATING_OUTCOME_SCORE)
    }
}

#[cfg(test)]
mod single_field_refusal_tests {
    use super::*;

    type Edit = fn(&mut RiprInterventionStudyV1);

    /// The code and message a study is refused with after one edit to the
    /// valid example, or `None` when the edited study is still accepted.
    fn refusal(edit: Edit) -> Option<(&'static str, String)> {
        let mut study = example_preregistered_study();
        edit(&mut study);
        study
            .validate()
            .err()
            .map(|error| (error.code, error.message))
    }

    fn assert_each_refused(code: &'static str, message: &str, edits: &[(&str, Edit)]) {
        for (name, edit) in edits {
            assert_eq!(refusal(*edit), Some((code, message.to_string())), "{name}");
        }
    }

    #[test]
    fn condition_tokens_and_error_display_are_stable() {
        assert_eq!(ConditionId::Control.as_str(), "control");
        assert_eq!(ConditionId::RiprAssisted.as_str(), "ripr_assisted");
        assert_eq!(
            error(codes::MISSING_FIELD, "study_id must be non-empty").to_string(),
            "missing_field: study_id must be non-empty"
        );
    }

    #[test]
    fn the_envelope_must_name_this_schema_kind_and_state() {
        assert_eq!(
            refusal(|study| study.schema_version = "2".into()),
            Some((
                codes::UNSUPPORTED_SCHEMA,
                format!("schema_version must be {RIPR_INTERVENTION_STUDY_SCHEMA_VERSION}")
            ))
        );
        assert_eq!(
            refusal(|study| study.kind = "study".into()),
            Some((
                codes::UNSUPPORTED_SCHEMA,
                format!("kind must be {RIPR_INTERVENTION_STUDY_KIND}")
            ))
        );
        assert_eq!(
            refusal(|study| study.implementation_state = "pilot_complete".into()),
            Some((
                codes::UNSUPPORTED_SCHEMA,
                format!(
                    "implementation_state must be {RIPR_INTERVENTION_STUDY_IMPLEMENTATION_STATE}"
                )
            ))
        );
        assert_eq!(
            refusal(|study| study.non_claims.clear()),
            Some((
                codes::MISSING_FIELD,
                "non_claims must name at least one excluded claim".to_string()
            ))
        );
    }

    #[test]
    fn each_blank_identity_field_is_named() {
        let blanks: [(&str, Edit); 12] = [
            ("study_id", |s| s.study_id.clear()),
            ("protocol_version", |s| s.protocol_version = " ".into()),
            ("parent_issue", |s| s.parent_issue.clear()),
            ("sequence", |s| s.sequence = "\t".into()),
            ("repository_id", |s| s.repository.repository_id.clear()),
            ("clone_url", |s| s.repository.clone_url.clear()),
            ("task_series_id", |s| s.task_series.task_series_id.clear()),
            ("task_id", |s| s.tasks[0].task_id.clear()),
            ("pair_id", |s| s.assignment.pairs[0].pair_id = " ".into()),
            ("model_id", |s| s.shared_budget.model_id.clear()),
            ("operator_profile", |s| {
                s.shared_budget.operator_profile.clear();
            }),
            ("runtime_id", |s| s.shared_budget.runtime_id = " ".into()),
        ];
        for (field, edit) in blanks {
            assert_eq!(
                refusal(edit),
                Some((codes::MISSING_FIELD, format!("{field} must be non-empty"))),
                "{field}"
            );
        }
        assert_eq!(
            refusal(|study| study.shared_budget.tool_ids.clear()),
            Some((
                codes::MISSING_FIELD,
                "shared tool_ids must be named".to_string()
            ))
        );
    }

    #[test]
    fn the_protocol_digest_and_analyzed_head_must_be_lowercase_hex_of_full_length() {
        assert_eq!(
            refusal(|study| study.protocol_digest = "0".repeat(64)),
            Some((
                codes::MALFORMED_IDENTITY,
                "protocol_digest must use sha256:<64-hex>".to_string()
            ))
        );
        assert_each_refused(
            codes::MALFORMED_IDENTITY,
            "protocol_digest must be 64 lowercase hexadecimal characters",
            &[
                ("short", |s| {
                    s.protocol_digest = format!("sha256:{}", "0".repeat(63));
                }),
                ("long", |s| {
                    s.protocol_digest = format!("sha256:{}", "0".repeat(65));
                }),
                ("not hex", |s| {
                    s.protocol_digest = format!("sha256:{}g", "0".repeat(63));
                }),
                ("uppercase", |s| {
                    s.protocol_digest = format!("sha256:{}A", "0".repeat(63));
                }),
            ],
        );
        assert_each_refused(
            codes::MALFORMED_IDENTITY,
            "analyzed_head_sha must be a 40-character lowercase git SHA",
            &[
                ("short", |s| s.repository.analyzed_head_sha = "a".repeat(39)),
                ("long", |s| s.repository.analyzed_head_sha = "a".repeat(41)),
                ("not hex", |s| {
                    s.repository.analyzed_head_sha = format!("{}g", "a".repeat(39));
                }),
                ("uppercase", |s| {
                    s.repository.analyzed_head_sha = format!("{}A", "a".repeat(39));
                }),
            ],
        );
        assert_eq!(
            refusal(|study| {
                study.protocol_digest = format!("sha256:{}", "0123456789abcdef".repeat(4));
                study.repository.analyzed_head_sha =
                    "0123456789abcdef0123456789abcdef01234567".into();
            }),
            None
        );
    }

    #[test]
    fn strata_tasks_and_pairs_must_be_selected_unique_and_consistent() {
        assert_each_refused(
            codes::MISSING_FIELD,
            "matched strata and tasks must be selected before outcomes",
            &[
                ("no strata", |s| s.strata.clear()),
                ("no tasks", |s| s.tasks.clear()),
            ],
        );
        assert_eq!(
            refusal(|study| {
                let twin = study.strata[0].clone();
                study.strata.push(twin);
            }),
            Some((
                codes::MALFORMED_IDENTITY,
                "stratum_id values must be unique".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| study.tasks[1].task_id = study.tasks[0].task_id.clone()),
            Some((
                codes::MALFORMED_IDENTITY,
                "task_id values must be unique".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| study.tasks[0].stratum_id = "stratum:other".into()),
            Some((
                codes::MALFORMED_IDENTITY,
                "task task:boundary-oracle-a names unknown stratum stratum:other".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| {
                study.assignment.pairs.pop();
            }),
            Some((
                codes::MISSING_FIELD,
                "every selected task must appear in exactly one assignment pair".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| {
                study.assignment.pairs[1].pair_id = "pair:b".into();
                study.assignment.pairs[1].task_id = "task:elsewhere".into();
            }),
            Some((
                codes::MALFORMED_IDENTITY,
                "assignment pair pair:b names unknown task task:elsewhere".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| {
                study.assignment.pairs[1].task_id = study.assignment.pairs[0].task_id.clone();
            }),
            Some((
                codes::MALFORMED_IDENTITY,
                "task task:boundary-oracle-a is assigned more than once".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| study.assignment.selected_before_outcomes = false)
                .map(|(code, _)| code),
            Some(codes::ASSIGNMENT_AFTER_OUTCOME)
        );
    }

    #[test]
    fn every_pair_runs_both_conditions_and_the_order_is_counterbalanced() {
        for condition in [ConditionId::Control, ConditionId::RiprAssisted] {
            let mut study = example_preregistered_study();
            study.assignment.pairs[0].pair_id = "pair:a".into();
            study.assignment.pairs[0].first_condition = condition;
            study.assignment.pairs[0].second_condition = condition;
            assert_eq!(
                study
                    .validate()
                    .err()
                    .map(|error| (error.code, error.message)),
                Some((
                    codes::MALFORMED_IDENTITY,
                    "pair pair:a must counterbalance control and ripr_assisted".to_string()
                )),
                "{condition:?}"
            );
        }
        assert_eq!(
            refusal(|study| {
                for pair in &mut study.assignment.pairs {
                    pair.first_condition = ConditionId::Control;
                    pair.second_condition = ConditionId::RiprAssisted;
                }
            }),
            Some((
                codes::MALFORMED_IDENTITY,
                "matched pairs must counterbalance condition order".to_string()
            ))
        );
    }

    #[test]
    fn exactly_the_two_conditions_are_declared_with_equal_budgets() {
        assert_each_refused(
            codes::MISSING_FIELD,
            "protocol must declare exactly control and ripr_assisted",
            &[
                ("a third condition", |s| {
                    let extra = s.conditions[0].clone();
                    s.conditions.push(extra);
                }),
                ("control twice", |s| {
                    s.conditions[1].condition_id = s.conditions[0].condition_id;
                }),
                ("one condition", |s| {
                    s.conditions.pop();
                }),
            ],
        );
        assert_eq!(
            refusal(|study| {
                study.condition_budget_overlays.remove("ripr_assisted");
            }),
            Some((
                codes::MISSING_FIELD,
                "condition ripr_assisted is missing a budget overlay".to_string()
            ))
        );
        fn overlay(study: &mut RiprInterventionStudyV1) -> Option<&mut BudgetOverlay> {
            study.condition_budget_overlays.get_mut("ripr_assisted")
        }
        assert_each_refused(
            codes::UNEQUAL_CONDITION_BUDGETS,
            "control and ripr_assisted must share model, operator, runtime, tool, time, token, and retry budgets except for the declared intervention",
            &[
                ("wall clock", |s| {
                    if let Some(overlay) = overlay(s) {
                        overlay.additional_wall_clock_ms = 1;
                    }
                }),
                ("tokens", |s| {
                    if let Some(overlay) = overlay(s) {
                        overlay.additional_token_budget = 1;
                    }
                }),
                ("retries", |s| {
                    if let Some(overlay) = overlay(s) {
                        overlay.additional_retries = 1;
                    }
                }),
                ("tools", |s| {
                    if let Some(overlay) = overlay(s) {
                        overlay.additional_tools.push("extra".into());
                    }
                }),
            ],
        );
    }

    #[test]
    fn control_reads_no_ripr_surface_and_assisted_names_at_least_one() {
        assert_each_refused(
            codes::CONTROL_CAN_READ_RIPR_OUTPUTS,
            "control attempts must not read RIPR receipts, views, or commands",
            &[
                ("flag", |s| {
                    s.intervention_surface.control.may_read_ripr_outputs = true;
                }),
                ("receipts", |s| {
                    s.intervention_surface
                        .control
                        .allowed_ripr_receipts
                        .push("receipt".into());
                }),
                ("views", |s| {
                    s.intervention_surface
                        .control
                        .allowed_ripr_views
                        .push("view".into());
                }),
                ("commands", |s| {
                    s.intervention_surface
                        .control
                        .allowed_ripr_commands
                        .push("ripr check".into());
                }),
            ],
        );

        fn only(
            study: &mut RiprInterventionStudyV1,
            receipts: &[&str],
            views: &[&str],
            commands: &[&str],
        ) {
            let owned = |items: &[&str]| items.iter().map(|item| (*item).to_string()).collect();
            let assisted = &mut study.intervention_surface.ripr_assisted;
            assisted.allowed_ripr_receipts = owned(receipts);
            assisted.allowed_ripr_views = owned(views);
            assisted.allowed_ripr_commands = owned(commands);
        }
        assert_eq!(
            refusal(|study| only(study, &[], &[], &[])),
            Some((
                codes::MISSING_FIELD,
                "ripr_assisted must name exact receipts, views, or commands rather than summarizing use RIPR".to_string()
            ))
        );
        // Any one named surface is enough.
        assert_eq!(refusal(|study| only(study, &["receipt"], &[], &[])), None);
        assert_eq!(refusal(|study| only(study, &[], &["view"], &[])), None);
        assert_eq!(
            refusal(|study| only(study, &[], &[], &["ripr check"])),
            None
        );

        assert_each_refused(
            codes::ASSISTED_UNDECLARED_EXTRA_CONTEXT,
            "a RIPR-assisted attempt cannot receive undeclared extra repository context",
            &[
                ("assisted", |s| {
                    s.intervention_surface
                        .ripr_assisted
                        .undeclared_repository_context
                        .push("notes".into());
                }),
                ("control", |s| {
                    s.intervention_surface
                        .control
                        .undeclared_repository_context
                        .push("notes".into());
                }),
            ],
        );
    }

    #[test]
    fn every_kind_of_incomplete_attempt_stays_in_the_denominator() {
        assert_each_refused(
            codes::DROP_INVALID_FROM_DENOMINATOR,
            "timeouts, cancellations, invalid attempts, missing evidence, and instrument failures remain in the denominator",
            &[
                ("drop timeouts", |s| {
                    s.invalid_attempt_rules.drop_timeouts = true
                }),
                ("drop invalid", |s| {
                    s.invalid_attempt_rules.drop_invalid_attempts = true;
                }),
                ("timeouts", |s| {
                    s.invalid_attempt_rules.retain_timeouts = false
                }),
                ("cancellations", |s| {
                    s.invalid_attempt_rules.retain_cancellations = false;
                }),
                ("invalid attempts", |s| {
                    s.invalid_attempt_rules.retain_invalid_attempts = false;
                }),
                ("missing evidence", |s| {
                    s.invalid_attempt_rules.retain_missing_evidence = false;
                }),
                ("instrument failures", |s| {
                    s.invalid_attempt_rules.retain_instrument_failures = false;
                }),
            ],
        );
    }

    #[test]
    fn adjudication_needs_each_of_graders_rubric_and_blinding() {
        assert_each_refused(
            codes::GRADER_OR_RUBRIC_ABSENT,
            "independent adjudication requires grader identity, a rubric version, and condition blinding",
            &[
                ("graders", |s| s.adjudication.grader_identities.clear()),
                ("rubric id", |s| s.adjudication.rubric_id = " ".into()),
                ("rubric version", |s| s.adjudication.rubric_version.clear()),
                ("grader count", |s| {
                    s.adjudication.min_independent_graders = 0
                }),
                ("blinding", |s| s.adjudication.blinded_to_condition = false),
            ],
        );
        assert_eq!(
            refusal(|study| {
                study
                    .outcome_axes
                    .retain(|axis| axis.axis_id != "change_scope_and_churn");
            }),
            Some((
                codes::MISSING_FIELD,
                "outcome axis change_scope_and_churn must be preregistered".to_string()
            ))
        );
        assert_eq!(
            refusal(|study| study.outcome_axes[0].independently_observable = false)
                .map(|(code, _)| code),
            Some(codes::COMPENSATING_OUTCOME_SCORE)
        );
        assert_eq!(
            refusal(|study| {
                study.non_authoritative_summary = Some(NonAuthoritativeSummary {
                    name: "overall".into(),
                    authoritative: true,
                });
            }),
            Some((
                codes::COMPENSATING_OUTCOME_SCORE,
                "a combined score is allowed only when explicitly declared non-authoritative"
                    .to_string()
            ))
        );
        assert_eq!(
            refusal(|study| {
                study.non_authoritative_summary = Some(NonAuthoritativeSummary {
                    name: "overall".into(),
                    authoritative: false,
                });
            }),
            None
        );
    }

    #[test]
    fn stopping_lock_and_claim_ceiling_hold_on_every_field() {
        assert_each_refused(
            codes::STOPPING_ON_FAVORABLE_INTERIM,
            "the stopping rule cannot depend on a favorable interim estimate",
            &[
                ("kind", |s| {
                    s.stopping_rule.kind = StoppingKind::StopOnFavorableInterim;
                }),
                ("interim estimate", |s| {
                    s.stopping_rule.may_depend_on_interim_estimate = true;
                }),
                ("favorable stop", |s| {
                    s.stopping_rule.favorable_interim_stop = FavorableInterimStop::Allowed;
                }),
            ],
        );
        assert_each_refused(
            codes::PROTOCOL_MUTATION_REQUIRES_NEW_STUDY_ID,
            "protocol mutation after the first attempt requires a new study_id",
            &[
                ("immutability", |s| {
                    s.protocol_lock.immutable_after_first_attempt = false;
                }),
                ("mutation policy", |s| {
                    s.protocol_lock.mutation_without_new_study_id = MutationPolicy::Allowed;
                }),
            ],
        );
        assert_each_refused(
            codes::MISSING_FIELD,
            "public conclusions are bounded to the exact task series, model/operator profile, and intervention form",
            &[
                ("proves value", |s| {
                    s.claim_ceiling.preregistration_proves_intervention_value = true;
                }),
                ("task series", |s| {
                    s.claim_ceiling.bounded_to_task_series = false
                }),
                ("model profile", |s| {
                    s.claim_ceiling.bounded_to_model_operator_profile = false;
                }),
                ("intervention form", |s| {
                    s.claim_ceiling.bounded_to_intervention_form = false;
                }),
                ("exact counts", |s| {
                    s.planned_analysis.reports_exact_numerators_and_denominators = false;
                }),
            ],
        );
    }
}
