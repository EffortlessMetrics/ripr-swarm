//! One canonical next action projected from current first-hour state (#6304).
//!
//! [`select_canonical_next_action`] is the single semantic authority answering:
//!
//! > Given this exact current state, what is the one safest useful thing the
//! > user or agent can do next?
//!
//! Producers (`ripr check` top result, default RepairCard `next_action`,
//! RepairAttempt status, precise `doctor` recovery, and pilot delegation)
//! normalize their state into a [`NextActionInput`] and receive one
//! [`CanonicalNextActionV1`]: at most one primary action, bounded subordinate
//! alternatives, exact subject/currentness/root binding, and a typed
//! prerequisite or stop â€” never a best-effort command string.
//!
//! This module selects meaning and applicability only. [`CommandSpec`] remains
//! the authority for argv/cwd/expected reads/writes and safe rendering: the
//! selector never builds a command, it only references a producer-offered
//! spec, and only when the underlying route admits the selected target on the
//! current platform. Surfaces that only have command *lines* (strings) can
//! never yield [`NextActionClass::RunCommand`]; they yield a typed stop naming
//! the real route instead.
//!
//! [`CommandSpec`]: super::CommandSpec

use super::command_spec::{CommandExecutionMode, CommandPlatform, CommandRole, CommandSpec};

/// Versioned canonical-action schema. Additive changes keep this version and
/// add `#[serde(default)]` fields; breaking shape changes mint a new version.
pub(crate) const CANONICAL_NEXT_ACTION_SCHEMA_VERSION: &str = "canonical_next_action.v1";

/// Alternatives are bounded and subordinate: they can never silently compete
/// as equally preferred routes. Producer refs come first; selector-added refs
/// (receipt, platform alternative) fill remaining room only.
pub(crate) const MAX_NEXT_ACTION_ALTERNATIVES: usize = 3;

/// Candidate identities carried inside a choose stop. The stop always names
/// the total so truncation is explicit, never silent.
pub(crate) const MAX_NEXT_ACTION_STOP_CANDIDATES: usize = 8;

/// What a canonical action never claims. Execution, correctness, and
/// completion stay with the executing route and the attempt/receipt
/// authorities.
pub(crate) const CANONICAL_NEXT_ACTION_NON_CLAIM: &str = "Advisory selection from producer-bound state; it does not execute, complete, or prove the repair, and the display string is never execution authority.";

const TRANSITION_EVIDENCE_CURRENT: &str = "evidence_current";
const TRANSITION_ATTEMPT_RESTARTED: &str = "attempt_restarted";
const TRANSITION_PREREQUISITE_SATISFIED: &str = "prerequisite_satisfied";
const TRANSITION_ROUTE_ADMITTED: &str = "route_admitted";
const TRANSITION_MANUAL_STEP_COMPLETE: &str = "manual_step_complete";
const TRANSITION_RECOVERY_APPLIED: &str = "recovery_applied";
const TRANSITION_SCOPE_PROVIDED: &str = "scope_provided";
const PILOT_INVENTION_LIMITATION: &str =
    "pilot names no action outside its delegated check/repair transaction";
const ROUTE_REFUSAL_DETAIL: &str = "the route refused the selected target";

/// The first-hour producer that normalized the input state. Protocol/editor
/// renderers consume the authority through their existing owners; they never
/// appear here as independent selectors.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NextActionProducer {
    CheckTopResult,
    RepairCard,
    RepairAttemptStatus,
    Doctor,
    PilotDelegation,
    /// The task-first `ripr repair` start decision (#6305): it enumerates
    /// repair-eligible seams through the existing inventory and eligibility
    /// authorities and offers the before-phase command only for a single
    /// bound subject.
    RepairStart,
}

impl NextActionProducer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckTopResult => "check_top_result",
            Self::RepairCard => "repair_card",
            Self::RepairAttemptStatus => "repair_attempt_status",
            Self::Doctor => "doctor",
            Self::PilotDelegation => "pilot_delegation",
            Self::RepairStart => "repair_start",
        }
    }
}

/// The closed action-class vocabulary. Only [`Self::RunCommand`] is
/// executable, and only with a referenced [`CommandSpec`]; every other class
/// carries a typed [`NextActionStop`] and no command.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NextActionClass {
    RunCommand,
    InspectDetails,
    ChooseItem,
    ChooseAttempt,
    SatisfyPrerequisite,
    RetryCurrentSubject,
    TerminalNoAction,
    UnsupportedOrLimited,
}

impl NextActionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunCommand => "run_command",
            Self::InspectDetails => "inspect_details",
            Self::ChooseItem => "choose_item",
            Self::ChooseAttempt => "choose_attempt",
            Self::SatisfyPrerequisite => "satisfy_prerequisite",
            Self::RetryCurrentSubject => "retry_current_subject",
            Self::TerminalNoAction => "terminal_no_action",
            Self::UnsupportedOrLimited => "unsupported_or_limited",
        }
    }

    pub fn is_executable(self) -> bool {
        matches!(self, Self::RunCommand)
    }

    /// Classes whose action moves state forward name their expected
    /// transition; observation/selection/terminal classes carry none.
    fn requires_transition(self) -> bool {
        matches!(
            self,
            Self::RunCommand | Self::SatisfyPrerequisite | Self::RetryCurrentSubject
        )
    }
}

/// The exact diff-source mode the producer analyzed. The mode is always
/// bound; base/head identities ride along when the producer binds them.
/// [`select_canonical_next_action`] copies the mode verbatim and never
/// substitutes one mode for another (control 3).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextActionDiffSource {
    WorkingTree {
        head: Option<String>,
    },
    Committed {
        base: Option<String>,
        head: Option<String>,
    },
}

impl NextActionDiffSource {
    pub fn mode_label(&self) -> &'static str {
        match self {
            Self::WorkingTree { .. } => "working_tree",
            Self::Committed { .. } => "committed",
        }
    }
}

/// The exact subject the action binds: producer-bound root (never the process
/// CWD), exact diff-source mode, and the one bound item â€” or no item exactly
/// when the action is a selection among stop-carried candidates.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NextActionSubject {
    pub root: String,
    pub diff_source: NextActionDiffSource,
    pub item: Option<String>,
}

/// The currentness comparison the action was selected under. `None` is an
/// explicit "this producer does not track this axis", never a silent pass:
/// divergence (both bound, differ) invalidates the action, and an executable
/// action additionally requires bound fresh heads.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NextActionCurrentness {
    pub head_expected: Option<String>,
    pub head_observed: Option<String>,
    pub config_expected: Option<String>,
    pub config_observed: Option<String>,
}

impl NextActionCurrentness {
    fn heads_diverge(&self) -> bool {
        match (&self.head_expected, &self.head_observed) {
            (Some(expected), Some(observed)) => expected != observed,
            _ => false,
        }
    }

    fn configs_diverge(&self) -> bool {
        match (&self.config_expected, &self.config_observed) {
            (Some(expected), Some(observed)) => expected != observed,
            _ => false,
        }
    }

    fn heads_bound_and_fresh(&self) -> bool {
        match (&self.head_expected, &self.head_observed) {
            (Some(expected), Some(observed)) => expected == observed,
            _ => false,
        }
    }
}

/// The coarse `ripr check` triage case behind a check action. The case decides
/// the action class; fine renderer prose stays with the triage renderer keyed
/// off this case, so no renderer can strengthen the class.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NextActionCheckCase {
    TopGap,
    SuppressedByPolicy,
    CandidateFilterHidAll,
    NoDiffFinding,
    StaticLimited,
    PreviewAdvisory,
    ScopeMissing,
}

impl NextActionCheckCase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopGap => "top_gap",
            Self::SuppressedByPolicy => "suppressed_by_policy",
            Self::CandidateFilterHidAll => "candidate_filter_hid_all",
            Self::NoDiffFinding => "no_diff_finding",
            Self::StaticLimited => "static_limited",
            Self::PreviewAdvisory => "preview_advisory",
            Self::ScopeMissing => "scope_missing",
        }
    }

    fn action_class(self) -> NextActionClass {
        match self {
            Self::TopGap => NextActionClass::InspectDetails,
            Self::ScopeMissing => NextActionClass::SatisfyPrerequisite,
            Self::NoDiffFinding => NextActionClass::TerminalNoAction,
            Self::SuppressedByPolicy
            | Self::CandidateFilterHidAll
            | Self::StaticLimited
            | Self::PreviewAdvisory => NextActionClass::UnsupportedOrLimited,
        }
    }
}

/// The typed prerequisite or stop carried by every non-executable action.
/// Each variant names its machine kind plus the exact identities and routes
/// the user or agent needs; human detail rides in the named fields, never as
/// an unqualified command string.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NextActionStop {
    SelectItem {
        candidates: Vec<String>,
        total: usize,
    },
    SelectAttempt {
        candidates: Vec<String>,
        total: usize,
    },
    ResolveDisagreement {
        check_item: String,
        card_item: String,
    },
    RefreshCurrentness {
        observed: String,
        expected: String,
        restart_route: String,
    },
    RefreshConfig {
        observed: String,
        expected: String,
        restart_route: String,
    },
    ProvideInput {
        input: String,
        detail_route: String,
    },
    RestartAttempt {
        attempt_id: String,
        restart_route: String,
    },
    RouteRefused {
        command_id: String,
        reason: String,
    },
    PlatformUnavailable {
        command_id: String,
        supported_platforms: Vec<CommandPlatform>,
        alternative_route: String,
    },
    ManualStep {
        command_id: String,
        instruction: String,
    },
    TerminalComplete {
        receipt_ref: String,
    },
    Unsupported {
        limitation: String,
        detail_route: String,
    },
    InspectTarget {
        detail_route: String,
    },
    CheckTriage {
        case: NextActionCheckCase,
        /// Followable inspect or restart route for this triage case. The
        /// check producer copies `NextActionInput::detail_route` here so a
        /// JSON consumer can replay the printed command without assembling
        /// `--root` / mode flags from the finding id. Empty when there is
        /// no inspect target (for example missing scope) or a fixture
        /// constructs the stop without a route.
        #[serde(default)]
        detail_route: String,
    },
    DoctorRecovery {
        check_name: String,
        recovery_route: String,
    },
    PilotDelegated {
        transaction_ref: String,
        route: String,
    },
}

impl NextActionStop {
    /// The wire spelling of the stop kind, exactly as it appears in the
    /// DTO's `stop.kind` field.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SelectItem { .. } => "select_item",
            Self::SelectAttempt { .. } => "select_attempt",
            Self::ResolveDisagreement { .. } => "resolve_disagreement",
            Self::RefreshCurrentness { .. } => "refresh_currentness",
            Self::RefreshConfig { .. } => "refresh_config",
            Self::ProvideInput { .. } => "provide_input",
            Self::RestartAttempt { .. } => "restart_attempt",
            Self::RouteRefused { .. } => "route_refused",
            Self::PlatformUnavailable { .. } => "platform_unavailable",
            Self::ManualStep { .. } => "manual_step",
            Self::TerminalComplete { .. } => "terminal_complete",
            Self::Unsupported { .. } => "unsupported",
            Self::InspectTarget { .. } => "inspect_target",
            Self::CheckTriage { .. } => "check_triage",
            Self::DoctorRecovery { .. } => "doctor_recovery",
            Self::PilotDelegated { .. } => "pilot_delegated",
        }
    }
}

/// A reference to the producer-offered [`CommandSpec`]: identity plus the
/// spec's own verbatim display. argv/cwd/expected reads/writes stay with the
/// spec; this reference never reconstructs them.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NextActionCommandRef {
    pub command_id: String,
    pub role: CommandRole,
    pub display: String,
}

/// The state transition the action is expected to produce. Producer-owned
/// `from` labels name the current state; canonical `to` labels name the
/// outcome vocabulary, except [`NextActionClass::RunCommand`] whose effect the
/// offering producer names.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NextActionTransition {
    pub from_state: String,
    pub to_state: String,
}

/// One bounded subordinate alternative: a detail reference, never a competing
/// command. Labels and routes are producer-owned; empties fail closed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NextActionAlternative {
    pub label: String,
    pub route: String,
}

/// One repair attempt as the status/inventory producer sees it. Terminality,
/// edit obligation, and restart recommendation are producer-owned facts the
/// selector branches on; it never re-derives attempt lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NextActionAttemptView {
    pub(crate) id: String,
    pub(crate) terminal: bool,
    pub(crate) awaits_edit: bool,
    pub(crate) restart_recommended: bool,
    pub(crate) restart_route: String,
    pub(crate) receipt_ref: Option<String>,
}

/// A pilot delegation handle: the exact check/repair transaction pilot acts
/// inside. Pilot names an action only through this handle, never by
/// inventing a separate route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NextActionPilotDelegation {
    pub(crate) transaction_ref: String,
    pub(crate) route: String,
}

/// One precise doctor setup/recovery action: the failing check plus its exact
/// recovery route. Doctor states without a precise recovery declare a
/// limitation instead; the selector never invents a recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NextActionDoctorRecovery {
    pub(crate) check_name: String,
    pub(crate) recovery_route: String,
}

/// Normalized producer state for [`select_canonical_next_action`]. Every
/// field is producer-owned: the selector branches on these facts and never
/// re-derives lifecycle, readiness, ranking, or command authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NextActionInput<'a> {
    pub(crate) producer: NextActionProducer,
    /// Producer-bound root. The selector copies it verbatim and never reads
    /// the process working directory (control 4).
    pub(crate) root: String,
    pub(crate) diff_source: NextActionDiffSource,
    /// The single bound subject item (seam, finding, gap, check name, or
    /// attempt id). Empty exactly when the producer binds no single item.
    pub(crate) item_id: String,
    /// The check-selected item, when the producer knows the check's pick.
    pub(crate) check_item: Option<String>,
    /// The card-selected item, when the producer knows the card's pick.
    pub(crate) card_item: Option<String>,
    /// Unranked item candidates. Non-empty exactly when the producer cannot
    /// bind one item; producers with their own selection law (check triage
    /// rank) bind the winner and leave this empty.
    pub(crate) item_candidates: Vec<String>,
    /// Visible attempts, oldest first. Empty for producers without attempt
    /// state; more than one forces an explicit selection.
    pub(crate) attempts: Vec<NextActionAttemptView>,
    pub(crate) currentness: NextActionCurrentness,
    /// The one command the producer offers as the primary action, if any.
    pub(crate) offered_command: Option<&'a CommandSpec>,
    /// Whether the underlying route would admit the selected target. A
    /// syntactically valid command the route would refuse can never be the
    /// primary action (control 2).
    pub(crate) route_admitted: bool,
    /// Producer-owned refusal text, carried verbatim when the route is
    /// closed and a command was offered.
    pub(crate) route_refusal: Option<String>,
    /// Producer-owned missing prerequisite that blocks the route when no
    /// command is offered (for example the card's missing evidence). The
    /// selector names it as the input to satisfy; an admitted offered
    /// command always wins over it.
    pub(crate) missing_input: Option<String>,
    /// The platform commands must render on. `None` fails closed: no command
    /// renders against an unknown platform.
    pub(crate) platform: Option<CommandPlatform>,
    /// Producer-declared limitation. A limitation is a valid product result
    /// but never an executable repair.
    pub(crate) limitation: Option<String>,
    pub(crate) limitation_route: Option<String>,
    /// Primary detail route for inspect-class outcomes.
    pub(crate) detail_route: String,
    /// Producer-owned current-state label for the expected transition.
    pub(crate) transition_from: String,
    /// Producer-named effect of the offered command. Used only when the
    /// decision is [`NextActionClass::RunCommand`].
    pub(crate) transition_to: Option<String>,
    /// Recompute route named when currentness invalidates the action.
    pub(crate) restart_route: String,
    /// The check triage case, bound by the check producer only.
    pub(crate) check_case: Option<NextActionCheckCase>,
    /// The precise doctor recovery, bound by the doctor producer only.
    pub(crate) doctor_recovery: Option<NextActionDoctorRecovery>,
    /// The delegated check/repair transaction, bound by pilot only.
    pub(crate) pilot_delegation: Option<NextActionPilotDelegation>,
    /// Producer-supplied subordinate detail refs, bounded by
    /// [`MAX_NEXT_ACTION_ALTERNATIVES`].
    pub(crate) alternatives: Vec<NextActionAlternative>,
    pub(crate) limitations: Vec<String>,
}

/// The one canonical next action for the bound state. Fields are private so
/// the selection law is structural: [`select_canonical_next_action`] and
/// [`Self::new`] are the only constructors, and renderers cannot strengthen
/// a non-executable state by rebuilding one.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanonicalNextActionV1 {
    schema_version: String,
    producer: NextActionProducer,
    subject: NextActionSubject,
    currentness: NextActionCurrentness,
    action_class: NextActionClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<NextActionCommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stop: Option<NextActionStop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_transition: Option<NextActionTransition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    alternatives: Vec<NextActionAlternative>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    limitations: Vec<String>,
    non_claim: String,
}

impl CanonicalNextActionV1 {
    /// Mint one action, enforcing the selection law. Each refusal names the
    /// exact violated check so removing one check fails its focused negative
    /// (control 10).
    #[allow(
        clippy::too_many_arguments,
        reason = "one mint call carries the full selection-law check surface"
    )]
    pub(crate) fn new(
        producer: NextActionProducer,
        subject: NextActionSubject,
        currentness: NextActionCurrentness,
        action_class: NextActionClass,
        command: Option<NextActionCommandRef>,
        stop: Option<NextActionStop>,
        expected_transition: Option<NextActionTransition>,
        alternatives: Vec<NextActionAlternative>,
        limitations: Vec<String>,
    ) -> Result<Self, String> {
        if subject.root.trim().is_empty() {
            return Err("canonical next action requires a bound root".to_string());
        }
        let chooses = matches!(
            action_class,
            NextActionClass::ChooseItem | NextActionClass::ChooseAttempt
        );
        if chooses == subject.item.is_some() {
            return Err(
                "canonical next action binds one item exactly when it is not a selection"
                    .to_string(),
            );
        }
        if let Some(item) = &subject.item
            && item.trim().is_empty()
        {
            return Err("canonical next action item must not be blank".to_string());
        }
        if action_class.is_executable() != command.is_some() {
            return Err(
                "canonical next action is executable exactly when it references a command"
                    .to_string(),
            );
        }
        if action_class.is_executable() == stop.is_some() {
            return Err(
                "canonical next action carries a typed stop exactly when it is not executable"
                    .to_string(),
            );
        }
        if action_class.requires_transition() != expected_transition.is_some() {
            return Err(
                "canonical next action names its expected transition exactly when it moves state"
                    .to_string(),
            );
        }
        if let Some(command) = &command {
            if command.command_id.trim().is_empty() {
                return Err("canonical next action command id must not be blank".to_string());
            }
            if command.display.trim().is_empty() {
                return Err("canonical next action command display must not be blank".to_string());
            }
        }
        if alternatives.len() > MAX_NEXT_ACTION_ALTERNATIVES {
            return Err(format!(
                "canonical next action carries at most {MAX_NEXT_ACTION_ALTERNATIVES} alternatives"
            ));
        }
        for alternative in &alternatives {
            if alternative.label.trim().is_empty() || alternative.route.trim().is_empty() {
                return Err(
                    "canonical next action alternatives must name a label and route".to_string(),
                );
            }
        }
        Ok(Self {
            schema_version: CANONICAL_NEXT_ACTION_SCHEMA_VERSION.to_string(),
            producer,
            subject,
            currentness,
            action_class,
            command,
            stop,
            expected_transition,
            alternatives,
            limitations,
            non_claim: CANONICAL_NEXT_ACTION_NON_CLAIM.to_string(),
        })
    }

    pub fn producer(&self) -> NextActionProducer {
        self.producer
    }

    pub fn subject(&self) -> &NextActionSubject {
        &self.subject
    }

    pub fn currentness(&self) -> &NextActionCurrentness {
        &self.currentness
    }

    pub fn action_class(&self) -> NextActionClass {
        self.action_class
    }

    pub fn is_executable(&self) -> bool {
        self.action_class.is_executable()
    }

    pub fn command(&self) -> Option<&NextActionCommandRef> {
        self.command.as_ref()
    }

    pub fn stop(&self) -> Option<&NextActionStop> {
        self.stop.as_ref()
    }

    pub fn expected_transition(&self) -> Option<&NextActionTransition> {
        self.expected_transition.as_ref()
    }

    pub fn alternatives(&self) -> &[NextActionAlternative] {
        &self.alternatives
    }

    pub fn limitations(&self) -> &[String] {
        &self.limitations
    }

    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    pub fn non_claim(&self) -> &str {
        &self.non_claim
    }
}

/// The host platform commands must render on, observed once per selection.
/// Unknown hosts fail closed: [`select_canonical_next_action`] renders no
/// command against them.
pub(crate) fn current_command_platform() -> Option<CommandPlatform> {
    match std::env::consts::OS {
        "linux" => Some(CommandPlatform::Linux),
        "macos" => Some(CommandPlatform::Macos),
        "windows" => Some(CommandPlatform::Windows),
        _ => None,
    }
}

/// Project the one canonical next action for normalized producer state.
///
/// Gate order is load-bearing and pinned by the control battery below:
/// invention, divergence, disagreement, limitation, attempt fan-out, single
/// attempt obligations, item fan-out, route admission, platform rendering,
/// manual execution, missing prerequisite, then the producer-specific and
/// run/inspect tail. Each gate names its typed stop; removing one gate fails
/// its focused negative (control 10).
pub(crate) fn select_canonical_next_action(
    input: &NextActionInput<'_>,
) -> Result<CanonicalNextActionV1, String> {
    if input.root.trim().is_empty() {
        return Err("next-action producer must bind a root".to_string());
    }
    if input.transition_from.trim().is_empty() {
        return Err("next-action producer must name the current state".to_string());
    }
    match input.producer {
        NextActionProducer::CheckTopResult if input.check_case.is_none() => {
            return Err("check producer must bind its triage case".to_string());
        }
        NextActionProducer::Doctor
            if input.doctor_recovery.is_none() && input.limitation.is_none() =>
        {
            return Err(
                "doctor producer must bind a precise recovery or declare its limitation"
                    .to_string(),
            );
        }
        _ => {}
    }
    if !input.item_id.trim().is_empty() && !input.item_candidates.is_empty() {
        return Err(
            "next-action producer must bind one item or name candidates, not both".to_string(),
        );
    }

    // Pilot without a delegation handle invented a route outside its
    // transaction: a typed limitation, never an action.
    if input.producer == NextActionProducer::PilotDelegation && input.pilot_delegation.is_none() {
        return stopped(
            input,
            NextActionClass::UnsupportedOrLimited,
            NextActionStop::Unsupported {
                limitation: PILOT_INVENTION_LIMITATION.to_string(),
                detail_route: input.detail_route.clone(),
            },
            None,
            Vec::new(),
        );
    }
    // A stale or moved head invalidates the action and names the restart
    // route (control 5). Unbound axes are explicit unknowns, handled at the
    // tail; only bound divergence refuses here.
    if input.currentness.heads_diverge() {
        let observed = input
            .currentness
            .head_observed
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let expected = input
            .currentness
            .head_expected
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        return stopped(
            input,
            NextActionClass::RetryCurrentSubject,
            NextActionStop::RefreshCurrentness {
                observed,
                expected,
                restart_route: input.restart_route.clone(),
            },
            Some(TRANSITION_EVIDENCE_CURRENT),
            Vec::new(),
        );
    }
    if input.currentness.configs_diverge() {
        let observed = input
            .currentness
            .config_observed
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let expected = input
            .currentness
            .config_expected
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        return stopped(
            input,
            NextActionClass::RetryCurrentSubject,
            NextActionStop::RefreshConfig {
                observed,
                expected,
                restart_route: input.restart_route.clone(),
            },
            Some(TRANSITION_EVIDENCE_CURRENT),
            Vec::new(),
        );
    }
    // Check and card disagree on the selected item: refuse rather than
    // choosing one silently (control 1).
    if let (Some(check_item), Some(card_item)) = (&input.check_item, &input.card_item)
        && check_item != card_item
    {
        return stopped(
            input,
            NextActionClass::ChooseItem,
            NextActionStop::ResolveDisagreement {
                check_item: check_item.clone(),
                card_item: card_item.clone(),
            },
            None,
            Vec::new(),
        );
    }
    // A producer-declared limitation is a valid result, never an executable
    // repair.
    if let Some(limitation) = &input.limitation {
        return stopped(
            input,
            NextActionClass::UnsupportedOrLimited,
            NextActionStop::Unsupported {
                limitation: limitation.clone(),
                detail_route: input
                    .limitation_route
                    .clone()
                    .unwrap_or_else(|| input.detail_route.clone()),
            },
            None,
            Vec::new(),
        );
    }
    // Several visible attempts require selection rather than a
    // newest/first/mtime fallback (control 6).
    if input.attempts.len() > 1 {
        let candidates: Vec<String> = input
            .attempts
            .iter()
            .take(MAX_NEXT_ACTION_STOP_CANDIDATES)
            .map(|attempt| attempt.id.clone())
            .collect();
        return stopped(
            input,
            NextActionClass::ChooseAttempt,
            NextActionStop::SelectAttempt {
                candidates,
                total: input.attempts.len(),
            },
            None,
            Vec::new(),
        );
    }
    if let Some(attempt) = input.attempts.first() {
        // Read-only specs stay available on terminal results: inspection and
        // receipt reads are not continuation. Anything else on a terminal
        // attempt is already-complete status plus details, never another
        // continue command (control 7).
        let read_only_offer = input.offered_command.is_some_and(|spec| {
            matches!(spec.role, CommandRole::Inspection | CommandRole::Receipt)
        });
        if attempt.terminal && !read_only_offer {
            let receipt_ref = attempt
                .receipt_ref
                .clone()
                .unwrap_or_else(|| input.detail_route.clone());
            return stopped(
                input,
                NextActionClass::TerminalNoAction,
                NextActionStop::TerminalComplete {
                    receipt_ref: receipt_ref.clone(),
                },
                None,
                vec![NextActionAlternative {
                    label: format!("terminal details for attempt {}", attempt.id),
                    route: receipt_ref,
                }],
            );
        }
        if attempt.awaits_edit {
            return stopped(
                input,
                NextActionClass::SatisfyPrerequisite,
                NextActionStop::ProvideInput {
                    input: format!("focused test edit for attempt {}", attempt.id),
                    detail_route: attempt
                        .receipt_ref
                        .clone()
                        .unwrap_or_else(|| input.detail_route.clone()),
                },
                Some(TRANSITION_PREREQUISITE_SATISFIED),
                Vec::new(),
            );
        }
        if attempt.restart_recommended {
            return stopped(
                input,
                NextActionClass::RetryCurrentSubject,
                NextActionStop::RestartAttempt {
                    attempt_id: attempt.id.clone(),
                    restart_route: attempt.restart_route.clone(),
                },
                Some(TRANSITION_ATTEMPT_RESTARTED),
                Vec::new(),
            );
        }
    }
    // Several unranked items require selection (control 6 covers attempts;
    // this is the item twin). One candidate normalizes to the bound item.
    if input.item_candidates.len() > 1 {
        let candidates: Vec<String> = input
            .item_candidates
            .iter()
            .take(MAX_NEXT_ACTION_STOP_CANDIDATES)
            .cloned()
            .collect();
        return stopped(
            input,
            NextActionClass::ChooseItem,
            NextActionStop::SelectItem {
                candidates,
                total: input.item_candidates.len(),
            },
            None,
            Vec::new(),
        );
    }
    // A command the route would refuse can never be the primary action,
    // however plausible its syntax (control 2).
    if let Some(spec) = input.offered_command
        && !input.route_admitted
    {
        return stopped(
            input,
            NextActionClass::SatisfyPrerequisite,
            NextActionStop::RouteRefused {
                command_id: spec.command_id.clone(),
                reason: input
                    .route_refusal
                    .clone()
                    .unwrap_or_else(|| ROUTE_REFUSAL_DETAIL.to_string()),
            },
            Some(TRANSITION_ROUTE_ADMITTED),
            Vec::new(),
        );
    }
    // Missing platform rendering stays unavailable with an explicit
    // alternative; it never emits an unqualified string (control 8).
    if let Some(spec) = input.offered_command
        && input
            .platform
            .is_none_or(|platform| !spec.platforms.contains(&platform))
    {
        return stopped(
            input,
            NextActionClass::UnsupportedOrLimited,
            NextActionStop::PlatformUnavailable {
                command_id: spec.command_id.clone(),
                supported_platforms: spec.platforms.clone(),
                alternative_route: input.detail_route.clone(),
            },
            None,
            vec![NextActionAlternative {
                label: format!("alternative to {}", spec.command_id),
                route: input.detail_route.clone(),
            }],
        );
    }
    if let Some(spec) = input.offered_command
        && spec.execution_mode == CommandExecutionMode::Manual
    {
        return stopped(
            input,
            NextActionClass::SatisfyPrerequisite,
            NextActionStop::ManualStep {
                command_id: spec.command_id.clone(),
                instruction: spec.display.clone(),
            },
            Some(TRANSITION_MANUAL_STEP_COMPLETE),
            Vec::new(),
        );
    }
    // No offered command but a named missing prerequisite: satisfy it
    // before anything can run.
    if input.offered_command.is_none()
        && let Some(missing) = &input.missing_input
    {
        return stopped(
            input,
            NextActionClass::SatisfyPrerequisite,
            NextActionStop::ProvideInput {
                input: missing.clone(),
                detail_route: input.detail_route.clone(),
            },
            Some(TRANSITION_PREREQUISITE_SATISFIED),
            Vec::new(),
        );
    }

    // Tail: an admitted executable command wins; otherwise the most specific
    // producer stop explains what to do instead of running.
    if let Some(spec) = input.offered_command {
        if !input.currentness.heads_bound_and_fresh() {
            let observed = input
                .currentness
                .head_observed
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let expected = input
                .currentness
                .head_expected
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            return stopped(
                input,
                NextActionClass::RetryCurrentSubject,
                NextActionStop::RefreshCurrentness {
                    observed,
                    expected,
                    restart_route: input.restart_route.clone(),
                },
                Some(TRANSITION_EVIDENCE_CURRENT),
                Vec::new(),
            );
        }
        let Some(transition_to) = input.transition_to.clone() else {
            return Err("next-action producer must name the offered command's effect".to_string());
        };
        return running(input, spec, transition_to);
    }
    if let Some(case) = input.check_case {
        let class = case.action_class();
        let transition = match class {
            NextActionClass::SatisfyPrerequisite => Some(TRANSITION_SCOPE_PROVIDED),
            _ => None,
        };
        return stopped(
            input,
            class,
            NextActionStop::CheckTriage {
                case,
                detail_route: input.detail_route.clone(),
            },
            transition,
            Vec::new(),
        );
    }
    if let Some(recovery) = &input.doctor_recovery {
        return stopped(
            input,
            NextActionClass::SatisfyPrerequisite,
            NextActionStop::DoctorRecovery {
                check_name: recovery.check_name.clone(),
                recovery_route: recovery.recovery_route.clone(),
            },
            Some(TRANSITION_RECOVERY_APPLIED),
            Vec::new(),
        );
    }
    if let Some(delegation) = &input.pilot_delegation {
        return stopped(
            input,
            NextActionClass::InspectDetails,
            NextActionStop::PilotDelegated {
                transaction_ref: delegation.transaction_ref.clone(),
                route: delegation.route.clone(),
            },
            None,
            Vec::new(),
        );
    }
    stopped(
        input,
        NextActionClass::InspectDetails,
        NextActionStop::InspectTarget {
            detail_route: input.detail_route.clone(),
        },
        None,
        Vec::new(),
    )
}

/// Mint a non-executable action: the bound subject plus the typed stop.
/// `transition_to` names the canonical outcome label when the class moves
/// state; selector-added alternatives fill room the producer left.
fn stopped(
    input: &NextActionInput<'_>,
    action_class: NextActionClass,
    stop: NextActionStop,
    transition_to: Option<&'static str>,
    added_alternatives: Vec<NextActionAlternative>,
) -> Result<CanonicalNextActionV1, String> {
    debug_assert!(
        !action_class.is_executable(),
        "stopped actions are never executable"
    );
    let subject = NextActionSubject {
        root: input.root.clone(),
        diff_source: input.diff_source.clone(),
        item: bound_item(input, action_class),
    };
    let expected_transition = transition_to.map(|to| NextActionTransition {
        from_state: input.transition_from.clone(),
        to_state: to.to_string(),
    });
    let mut alternatives = input.alternatives.clone();
    for added in added_alternatives {
        if alternatives.len() >= MAX_NEXT_ACTION_ALTERNATIVES {
            break;
        }
        alternatives.push(added);
    }
    CanonicalNextActionV1::new(
        input.producer,
        subject,
        input.currentness.clone(),
        action_class,
        None,
        Some(stop),
        expected_transition,
        alternatives,
        input.limitations.clone(),
    )
}

/// Mint the executable action: the offered spec referenced verbatim, with the
/// producer-named effect as its expected transition.
fn running(
    input: &NextActionInput<'_>,
    spec: &CommandSpec,
    transition_to: String,
) -> Result<CanonicalNextActionV1, String> {
    let subject = NextActionSubject {
        root: input.root.clone(),
        diff_source: input.diff_source.clone(),
        item: bound_item(input, NextActionClass::RunCommand),
    };
    CanonicalNextActionV1::new(
        input.producer,
        subject,
        input.currentness.clone(),
        NextActionClass::RunCommand,
        Some(NextActionCommandRef {
            command_id: spec.command_id.clone(),
            role: spec.role,
            display: spec.display.clone(),
        }),
        None,
        Some(NextActionTransition {
            from_state: input.transition_from.clone(),
            to_state: transition_to,
        }),
        input.alternatives.clone(),
        input.limitations.clone(),
    )
}

/// Resolve the bound subject item. Selections bind none (candidates ride in
/// the stop); one candidate normalizes to the bound item; anything else with
/// no item fails closed at construction.
fn bound_item(input: &NextActionInput<'_>, action_class: NextActionClass) -> Option<String> {
    if matches!(
        action_class,
        NextActionClass::ChooseItem | NextActionClass::ChooseAttempt
    ) {
        return None;
    }
    if !input.item_id.trim().is_empty() {
        return Some(input.item_id.clone());
    }
    if input.item_candidates.len() == 1 {
        return input.item_candidates.first().cloned();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::command_spec::{
        CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, EnvironmentPolicy,
        ExpectedResultParser, NetworkPolicy, StdinPolicy,
    };
    use super::*;

    fn test_spec() -> CommandSpec {
        CommandSpec {
            schema_version: CommandSpec::SCHEMA_VERSION.to_string(),
            command_id: "ripr:agent:packet".to_string(),
            role: CommandRole::Inspection,
            execution_mode: CommandExecutionMode::Direct,
            program: "ripr".to_string(),
            args: vec!["agent".to_string(), "packet".to_string()],
            cwd: ".".to_string(),
            env_set: Vec::new(),
            env_passthrough: Vec::new(),
            environment_policy: EnvironmentPolicy::Inherited,
            stdin: StdinPolicy::Null,
            timeout_ms: 60_000,
            cancellation: CancellationPolicy::Allowed,
            network_policy: NetworkPolicy::Forbidden,
            expected_result_parser: ExpectedResultParser::DeclaredJson,
            expected_exit_codes: vec![0],
            expected_writes: Vec::new(),
            cost_class: CommandCostClass::Check,
            platforms: vec![
                CommandPlatform::Linux,
                CommandPlatform::Macos,
                CommandPlatform::Windows,
            ],
            display: "ripr agent packet --seam-id seam:demo --json".to_string(),
            authority_boundary: CommandAuthorityBoundary::InspectionRouteOnly,
        }
    }

    fn fresh_currentness() -> NextActionCurrentness {
        NextActionCurrentness {
            head_expected: Some("head1".to_string()),
            head_observed: Some("head1".to_string()),
            config_expected: None,
            config_observed: None,
        }
    }

    fn base_input() -> NextActionInput<'static> {
        NextActionInput {
            producer: NextActionProducer::RepairCard,
            root: "/repo".to_string(),
            diff_source: NextActionDiffSource::Committed {
                base: Some("base1".to_string()),
                head: Some("head1".to_string()),
            },
            item_id: "seam:demo".to_string(),
            check_item: None,
            card_item: Some("seam:demo".to_string()),
            item_candidates: Vec::new(),
            attempts: Vec::new(),
            currentness: fresh_currentness(),
            offered_command: None,
            route_admitted: true,
            route_refusal: None,
            missing_input: None,
            platform: Some(CommandPlatform::Linux),
            limitation: None,
            limitation_route: None,
            detail_route: "ripr agent packet --seam-id seam:demo --json".to_string(),
            transition_from: "fix_site_ready".to_string(),
            transition_to: Some("packet_inspected".to_string()),
            restart_route: "ripr check --root .".to_string(),
            check_case: None,
            doctor_recovery: None,
            pilot_delegation: None,
            alternatives: Vec::new(),
            limitations: Vec::new(),
        }
    }

    fn attempt_view(id: &str) -> NextActionAttemptView {
        NextActionAttemptView {
            id: id.to_string(),
            terminal: false,
            awaits_edit: false,
            restart_recommended: false,
            restart_route: String::new(),
            receipt_ref: None,
        }
    }

    fn test_subject() -> NextActionSubject {
        NextActionSubject {
            root: "/repo".to_string(),
            diff_source: NextActionDiffSource::Committed {
                base: Some("base1".to_string()),
                head: Some("head1".to_string()),
            },
            item: Some("seam:demo".to_string()),
        }
    }

    fn test_command_ref() -> NextActionCommandRef {
        NextActionCommandRef {
            command_id: "ripr:agent:packet".to_string(),
            role: CommandRole::Inspection,
            display: "ripr agent packet --seam-id seam:demo --json".to_string(),
        }
    }

    fn test_transition() -> NextActionTransition {
        NextActionTransition {
            from_state: "fix_site_ready".to_string(),
            to_state: "packet_inspected".to_string(),
        }
    }

    fn stop_kind(action: &CanonicalNextActionV1) -> &'static str {
        action.stop().map(NextActionStop::kind).unwrap_or("none")
    }

    #[test]
    fn run_command_requires_command_and_forbids_stop() -> Result<(), String> {
        let minted = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::RunCommand,
            Some(test_command_ref()),
            None,
            Some(test_transition()),
            Vec::new(),
            Vec::new(),
        );
        let Ok(minted) = minted else {
            return Err("run-command mint with a command must succeed".to_string());
        };
        assert!(minted.is_executable());

        let missing_command = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::RunCommand,
            None,
            None,
            Some(test_transition()),
            Vec::new(),
            Vec::new(),
        );
        let Err(_) = missing_command else {
            return Err("run-command mint without a command must fail".to_string());
        };

        let with_stop = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::RunCommand,
            Some(test_command_ref()),
            Some(NextActionStop::InspectTarget {
                detail_route: "route".to_string(),
            }),
            Some(test_transition()),
            Vec::new(),
            Vec::new(),
        );
        let Err(_) = with_stop else {
            return Err("run-command mint with a stop must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn non_executable_classes_require_stop_and_forbid_command() -> Result<(), String> {
        let classes = [
            NextActionClass::InspectDetails,
            NextActionClass::ChooseItem,
            NextActionClass::ChooseAttempt,
            NextActionClass::SatisfyPrerequisite,
            NextActionClass::RetryCurrentSubject,
            NextActionClass::TerminalNoAction,
            NextActionClass::UnsupportedOrLimited,
        ];
        for class in classes {
            let mut subject = test_subject();
            if matches!(
                class,
                NextActionClass::ChooseItem | NextActionClass::ChooseAttempt
            ) {
                subject.item = None;
            }
            let transition = class.requires_transition().then(|| NextActionTransition {
                from_state: "from".to_string(),
                to_state: "to".to_string(),
            });
            let stop = NextActionStop::InspectTarget {
                detail_route: "route".to_string(),
            };
            let minted = CanonicalNextActionV1::new(
                NextActionProducer::RepairCard,
                subject.clone(),
                fresh_currentness(),
                class,
                None,
                Some(stop),
                transition.clone(),
                Vec::new(),
                Vec::new(),
            );
            let Ok(minted) = minted else {
                return Err(format!(
                    "class {} mint with a stop must succeed",
                    class.as_str()
                ));
            };
            assert!(!minted.is_executable(), "class {}", class.as_str());

            let missing_stop = CanonicalNextActionV1::new(
                NextActionProducer::RepairCard,
                subject.clone(),
                fresh_currentness(),
                class,
                None,
                None,
                transition,
                Vec::new(),
                Vec::new(),
            );
            let Err(_) = missing_stop else {
                return Err(format!(
                    "class {} mint without a stop must fail",
                    class.as_str()
                ));
            };

            let with_command = CanonicalNextActionV1::new(
                NextActionProducer::RepairCard,
                subject,
                fresh_currentness(),
                class,
                Some(test_command_ref()),
                Some(NextActionStop::InspectTarget {
                    detail_route: "route".to_string(),
                }),
                None,
                Vec::new(),
                Vec::new(),
            );
            let Err(_) = with_command else {
                return Err(format!(
                    "class {} mint with a command must fail",
                    class.as_str()
                ));
            };
        }
        Ok(())
    }

    #[test]
    fn transition_required_exactly_when_class_moves_state() -> Result<(), String> {
        for class in [
            NextActionClass::RunCommand,
            NextActionClass::SatisfyPrerequisite,
            NextActionClass::RetryCurrentSubject,
        ] {
            assert!(class.requires_transition(), "{}", class.as_str());
        }
        for class in [
            NextActionClass::InspectDetails,
            NextActionClass::ChooseItem,
            NextActionClass::ChooseAttempt,
            NextActionClass::TerminalNoAction,
            NextActionClass::UnsupportedOrLimited,
        ] {
            assert!(!class.requires_transition(), "{}", class.as_str());
        }
        // A terminal action naming a transition is a stronger claim than the
        // class allows.
        let terminal = CanonicalNextActionV1::new(
            NextActionProducer::RepairAttemptStatus,
            test_subject(),
            fresh_currentness(),
            NextActionClass::TerminalNoAction,
            None,
            Some(NextActionStop::TerminalComplete {
                receipt_ref: "receipt".to_string(),
            }),
            Some(test_transition()),
            Vec::new(),
            Vec::new(),
        );
        let Err(_) = terminal else {
            return Err("terminal mint with a transition must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn alternatives_bounded_and_nonempty() -> Result<(), String> {
        let full: Vec<NextActionAlternative> = (0..MAX_NEXT_ACTION_ALTERNATIVES)
            .map(|index| NextActionAlternative {
                label: format!("label-{index}"),
                route: format!("route-{index}"),
            })
            .collect();
        let minted = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::InspectDetails,
            None,
            Some(NextActionStop::InspectTarget {
                detail_route: "route".to_string(),
            }),
            None,
            full,
            Vec::new(),
        );
        let Ok(_) = minted else {
            return Err("full-capacity alternatives mint must succeed".to_string());
        };

        let mut overflowing: Vec<NextActionAlternative> = (0..=MAX_NEXT_ACTION_ALTERNATIVES)
            .map(|index| NextActionAlternative {
                label: format!("label-{index}"),
                route: format!("route-{index}"),
            })
            .collect();
        overflowing.push(NextActionAlternative {
            label: "extra".to_string(),
            route: "extra".to_string(),
        });
        let refused = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::InspectDetails,
            None,
            Some(NextActionStop::InspectTarget {
                detail_route: "route".to_string(),
            }),
            None,
            overflowing,
            Vec::new(),
        );
        let Err(_) = refused else {
            return Err("overflowing alternatives mint must fail".to_string());
        };

        let blank = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(),
            fresh_currentness(),
            NextActionClass::InspectDetails,
            None,
            Some(NextActionStop::InspectTarget {
                detail_route: "route".to_string(),
            }),
            None,
            vec![NextActionAlternative {
                label: "  ".to_string(),
                route: "route".to_string(),
            }],
            Vec::new(),
        );
        let Err(_) = blank else {
            return Err("blank-label alternatives mint must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn control_1_disagreement_refuses_rather_than_choosing() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.check_item = Some("finding:top".to_string());
        input.card_item = Some("seam:other".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("refusal is a valid result: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::ChooseItem);
        assert!(!action.is_executable());
        assert!(action.command().is_none());
        assert_eq!(stop_kind(&action), "resolve_disagreement");
        assert!(action.subject().item.is_none());
        match action.stop() {
            Some(NextActionStop::ResolveDisagreement {
                check_item,
                card_item,
            }) => {
                assert_eq!(check_item, "finding:top");
                assert_eq!(card_item, "seam:other");
            }
            other => return Err(format!("expected disagreement stop, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn control_1_agreement_proceeds_to_the_offered_command() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.check_item = Some("seam:demo".to_string());
        input.card_item = Some("seam:demo".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("agreed selection runs: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);
        assert_eq!(
            action.command().map(|command| command.command_id.as_str()),
            Some("ripr:agent:packet")
        );
        assert_eq!(action.subject().item.as_deref(), Some("seam:demo"));
        Ok(())
    }

    #[test]
    fn control_2_refused_route_cannot_be_the_primary_action() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.route_admitted = false;
        input.route_refusal = Some("edit cage forbids the production file".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("refusal is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::SatisfyPrerequisite);
        assert!(!action.is_executable());
        assert!(action.command().is_none());
        match action.stop() {
            Some(NextActionStop::RouteRefused { command_id, reason }) => {
                assert_eq!(command_id, "ripr:agent:packet");
                assert_eq!(reason, "edit cage forbids the production file");
            }
            other => return Err(format!("expected route-refused stop, got {other:?}")),
        }
        assert_eq!(
            action
                .expected_transition()
                .map(|transition| transition.to_state.as_str()),
            Some(TRANSITION_ROUTE_ADMITTED)
        );
        Ok(())
    }

    #[test]
    fn control_2_admitted_route_runs() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.route_admitted = true;
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("admitted route runs: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);
        Ok(())
    }

    #[test]
    fn control_3_diff_source_mode_preserved_verbatim() -> Result<(), String> {
        let spec = test_spec();
        for diff_source in [
            NextActionDiffSource::WorkingTree {
                head: Some("dirty-head".to_string()),
            },
            NextActionDiffSource::Committed {
                base: Some("base9".to_string()),
                head: Some("head9".to_string()),
            },
            NextActionDiffSource::Committed {
                base: None,
                head: None,
            },
        ] {
            let mut input = base_input();
            input.offered_command = Some(&spec);
            input.diff_source = diff_source.clone();
            let action = select_canonical_next_action(&input)
                .map_err(|error| format!("mode preserved: {error}"))?;
            assert_eq!(action.subject().diff_source, diff_source);
            assert_eq!(
                action.subject().diff_source.mode_label(),
                diff_source.mode_label()
            );
        }
        Ok(())
    }

    #[test]
    fn control_4_root_bound_verbatim_never_from_cwd() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        // A foreign-looking root and a decoy item from elsewhere: the
        // selector binds exactly what the producer bound.
        input.root = "/decoy/checkout".to_string();
        input.item_id = "seam:elsewhere".to_string();
        input.card_item = Some("seam:elsewhere".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("foreign root binds: {error}"))?;
        assert_eq!(action.subject().root, "/decoy/checkout");
        assert_eq!(action.subject().item.as_deref(), Some("seam:elsewhere"));

        let mut blank = base_input();
        blank.root = "   ".to_string();
        let Err(_) = select_canonical_next_action(&blank) else {
            return Err("blank-root selection must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn control_5_stale_head_invalidates_and_names_restart() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.currentness.head_observed = Some("moved-head".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("stale is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RetryCurrentSubject);
        assert!(!action.is_executable());
        match action.stop() {
            Some(NextActionStop::RefreshCurrentness {
                observed,
                expected,
                restart_route,
            }) => {
                assert_eq!(observed, "moved-head");
                assert_eq!(expected, "head1");
                assert_eq!(restart_route, "ripr check --root .");
            }
            other => return Err(format!("expected refresh stop, got {other:?}")),
        }
        // The stale subject and the divergence survive on the DTO.
        assert_eq!(
            action.currentness().head_observed.as_deref(),
            Some("moved-head")
        );
        assert_eq!(
            action
                .expected_transition()
                .map(|transition| transition.to_state.as_str()),
            Some(TRANSITION_EVIDENCE_CURRENT)
        );

        // Without an offered command the early gate still names the retry:
        // staleness never degrades into a plain inspect.
        let mut unoffered = base_input();
        unoffered.currentness.head_observed = Some("moved-head".to_string());
        let action = select_canonical_next_action(&unoffered)
            .map_err(|error| format!("stale is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RetryCurrentSubject);
        assert_eq!(stop_kind(&action), "refresh_currentness");
        Ok(())
    }

    #[test]
    fn control_5_config_mismatch_invalidates() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.currentness.config_expected = Some("config-a".to_string());
        input.currentness.config_observed = Some("config-b".to_string());
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("config drift is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RetryCurrentSubject);
        assert_eq!(stop_kind(&action), "refresh_config");
        match action.stop() {
            Some(NextActionStop::RefreshConfig { restart_route, .. }) => {
                assert_eq!(restart_route, "ripr check --root .");
            }
            other => return Err(format!("expected config refresh stop, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn control_5_fresh_currentness_proceeds() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.currentness.config_expected = Some("config-a".to_string());
        input.currentness.config_observed = Some("config-a".to_string());
        let action =
            select_canonical_next_action(&input).map_err(|error| format!("fresh runs: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);
        Ok(())
    }

    #[test]
    fn control_6_zero_attempts_starts_or_inspects() -> Result<(), String> {
        let spec = test_spec();
        let mut running = base_input();
        running.producer = NextActionProducer::RepairAttemptStatus;
        running.offered_command = Some(&spec);
        running.transition_from = "no_attempt".to_string();
        let action = select_canonical_next_action(&running)
            .map_err(|error| format!("start runs: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);

        let mut inspecting = base_input();
        inspecting.producer = NextActionProducer::RepairAttemptStatus;
        inspecting.transition_from = "no_attempt".to_string();
        let action = select_canonical_next_action(&inspecting)
            .map_err(|error| format!("start inspects: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::InspectDetails);
        assert_eq!(stop_kind(&action), "inspect_target");
        Ok(())
    }

    #[test]
    fn control_6_several_attempts_force_choose_attempt() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::RepairAttemptStatus;
        input.attempts = vec![
            attempt_view("repair-attempt-01"),
            attempt_view("repair-attempt-02"),
        ];
        input.item_id = String::new();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("ambiguity is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::ChooseAttempt);
        assert!(action.subject().item.is_none());
        match action.stop() {
            Some(NextActionStop::SelectAttempt { candidates, total }) => {
                assert_eq!(
                    candidates,
                    &vec![
                        "repair-attempt-01".to_string(),
                        "repair-attempt-02".to_string()
                    ]
                );
                assert_eq!(*total, 2);
            }
            other => return Err(format!("expected select-attempt stop, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn control_6_candidate_names_truncate_with_explicit_total() -> Result<(), String> {
        let mut input = base_input();
        input.attempts = (0..10)
            .map(|index| attempt_view(&format!("repair-attempt-{index:02}")))
            .collect();
        input.item_id = String::new();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("ambiguity is typed: {error}"))?;
        match action.stop() {
            Some(NextActionStop::SelectAttempt { candidates, total }) => {
                assert_eq!(candidates.len(), MAX_NEXT_ACTION_STOP_CANDIDATES);
                assert_eq!(*total, 10);
            }
            other => return Err(format!("expected select-attempt stop, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn control_7_terminal_attempt_yields_no_continue_command() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::RepairAttemptStatus;
        let mut finished = attempt_view("repair-attempt-09");
        finished.terminal = true;
        finished.receipt_ref = Some("attempt.json#receipt".to_string());
        input.attempts = vec![finished];
        input.transition_from = "finished_current".to_string();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("terminal is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::TerminalNoAction);
        assert!(action.command().is_none());
        assert!(action.expected_transition().is_none());
        match action.stop() {
            Some(NextActionStop::TerminalComplete { receipt_ref }) => {
                assert_eq!(receipt_ref, "attempt.json#receipt");
            }
            other => return Err(format!("expected terminal stop, got {other:?}")),
        }
        assert_eq!(action.alternatives().len(), 1);
        assert_eq!(action.alternatives()[0].route, "attempt.json#receipt");
        Ok(())
    }

    #[test]
    fn control_7_terminal_blocks_continue_specs_but_keeps_inspection() -> Result<(), String> {
        let mut verify_spec = test_spec();
        verify_spec.command_id = "ripr:agent:verify".to_string();
        verify_spec.role = CommandRole::Verify;
        let mut blocked = base_input();
        blocked.producer = NextActionProducer::RepairAttemptStatus;
        let mut finished = attempt_view("repair-attempt-09");
        finished.terminal = true;
        blocked.attempts = vec![finished];
        blocked.offered_command = Some(&verify_spec);
        blocked.transition_from = "finished_current".to_string();
        let action = select_canonical_next_action(&blocked)
            .map_err(|error| format!("terminal blocks continue: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::TerminalNoAction);
        assert!(action.command().is_none());

        let inspect_spec = test_spec();
        let mut reading = base_input();
        reading.producer = NextActionProducer::RepairAttemptStatus;
        let mut finished = attempt_view("repair-attempt-09");
        finished.terminal = true;
        reading.attempts = vec![finished];
        reading.offered_command = Some(&inspect_spec);
        reading.transition_from = "finished_current".to_string();
        let action = select_canonical_next_action(&reading)
            .map_err(|error| format!("inspection reads results: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);
        Ok(())
    }

    #[test]
    fn control_8_missing_platform_rendering_stays_unavailable() -> Result<(), String> {
        let mut spec = test_spec();
        spec.platforms = vec![CommandPlatform::Windows];
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.platform = Some(CommandPlatform::Linux);
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("platform gap is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::UnsupportedOrLimited);
        assert!(action.command().is_none());
        match action.stop() {
            Some(NextActionStop::PlatformUnavailable {
                command_id,
                supported_platforms,
                alternative_route,
            }) => {
                assert_eq!(command_id, "ripr:agent:packet");
                assert_eq!(supported_platforms, &vec![CommandPlatform::Windows]);
                assert_eq!(
                    alternative_route,
                    "ripr agent packet --seam-id seam:demo --json"
                );
            }
            other => return Err(format!("expected platform stop, got {other:?}")),
        }
        // The explicit alternative survives even though no command renders.
        assert_eq!(action.alternatives().len(), 1);
        Ok(())
    }

    #[test]
    fn control_8_unknown_platform_fails_closed() -> Result<(), String> {
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        input.platform = None;
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("unknown platform is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::UnsupportedOrLimited);
        assert_eq!(stop_kind(&action), "platform_unavailable");
        assert!(action.command().is_none());
        Ok(())
    }

    #[test]
    fn control_8_manual_steps_stay_qualified_prerequisites() -> Result<(), String> {
        let mut spec = test_spec();
        spec.execution_mode = CommandExecutionMode::Manual;
        spec.display = "open the packet and read the fix site".to_string();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("manual is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::SatisfyPrerequisite);
        assert!(action.command().is_none());
        match action.stop() {
            Some(NextActionStop::ManualStep {
                command_id,
                instruction,
            }) => {
                assert_eq!(command_id, "ripr:agent:packet");
                assert_eq!(instruction, "open the packet and read the fix site");
            }
            other => return Err(format!("expected manual stop, got {other:?}")),
        }
        Ok(())
    }

    /// Control 10: every gate owns one focused negative that violates exactly
    /// that gate. Removing one prerequisite/currentness check changes that
    /// case's decision and fails this battery.
    #[test]
    fn control_10_each_gate_owns_its_negative() -> Result<(), String> {
        struct Case {
            name: &'static str,
            mutate: fn(&mut NextActionInput<'_>),
            class: NextActionClass,
            kind: &'static str,
        }
        let spec = test_spec();
        // The platform case runs against a spec that omits macOS, the manual
        // case against a manual-mode spec; every other case runs against the
        // all-platform spec.
        let mut without_macos = test_spec();
        without_macos.platforms = vec![CommandPlatform::Linux, CommandPlatform::Windows];
        let mut manual_spec = test_spec();
        manual_spec.execution_mode = CommandExecutionMode::Manual;
        let cases = [
            Case {
                name: "stale head",
                mutate: |input| {
                    // No offered command: only the early divergence gate
                    // names the retry; the executable tail cannot backstop
                    // it. Removing that gate yields inspect_target here.
                    input.offered_command = None;
                    input.currentness.head_observed = Some("moved".to_string());
                },
                class: NextActionClass::RetryCurrentSubject,
                kind: "refresh_currentness",
            },
            Case {
                name: "config drift",
                mutate: |input| {
                    input.currentness.config_expected = Some("a".to_string());
                    input.currentness.config_observed = Some("b".to_string());
                },
                class: NextActionClass::RetryCurrentSubject,
                kind: "refresh_config",
            },
            Case {
                name: "check/card disagreement",
                mutate: |input| {
                    input.check_item = Some("finding:a".to_string());
                    input.card_item = Some("seam:b".to_string());
                },
                class: NextActionClass::ChooseItem,
                kind: "resolve_disagreement",
            },
            Case {
                name: "declared limitation",
                mutate: |input| {
                    input.limitation = Some("preview only".to_string());
                },
                class: NextActionClass::UnsupportedOrLimited,
                kind: "unsupported",
            },
            Case {
                name: "several attempts",
                mutate: |input| {
                    input.attempts = vec![
                        attempt_view("repair-attempt-01"),
                        attempt_view("repair-attempt-02"),
                    ];
                    input.item_id = String::new();
                },
                class: NextActionClass::ChooseAttempt,
                kind: "select_attempt",
            },
            Case {
                name: "terminal attempt",
                mutate: |input| {
                    let mut finished = attempt_view("repair-attempt-09");
                    finished.terminal = true;
                    input.attempts = vec![finished];
                    input.offered_command = None;
                },
                class: NextActionClass::TerminalNoAction,
                kind: "terminal_complete",
            },
            Case {
                name: "awaiting edit",
                mutate: |input| {
                    let mut awaiting = attempt_view("repair-attempt-03");
                    awaiting.awaits_edit = true;
                    input.attempts = vec![awaiting];
                    input.offered_command = None;
                },
                class: NextActionClass::SatisfyPrerequisite,
                kind: "provide_input",
            },
            Case {
                name: "restart recommended",
                mutate: |input| {
                    let mut stale = attempt_view("repair-attempt-04");
                    stale.restart_recommended = true;
                    stale.restart_route = "ripr agent repair --root .".to_string();
                    input.attempts = vec![stale];
                    input.offered_command = None;
                },
                class: NextActionClass::RetryCurrentSubject,
                kind: "restart_attempt",
            },
            Case {
                name: "several items",
                mutate: |input| {
                    input.item_id = String::new();
                    input.card_item = None;
                    input.item_candidates = vec![
                        "item:a".to_string(),
                        "item:b".to_string(),
                        "item:c".to_string(),
                    ];
                },
                class: NextActionClass::ChooseItem,
                kind: "select_item",
            },
            Case {
                name: "route refused",
                mutate: |input| {
                    input.route_admitted = false;
                },
                class: NextActionClass::SatisfyPrerequisite,
                kind: "route_refused",
            },
            Case {
                name: "platform gap",
                mutate: |input| {
                    input.platform = Some(CommandPlatform::Macos);
                },
                class: NextActionClass::UnsupportedOrLimited,
                kind: "platform_unavailable",
            },
            Case {
                name: "manual execution",
                mutate: |_| {},
                class: NextActionClass::SatisfyPrerequisite,
                kind: "manual_step",
            },
            Case {
                name: "pilot invention",
                mutate: |input| {
                    input.producer = NextActionProducer::PilotDelegation;
                },
                class: NextActionClass::UnsupportedOrLimited,
                kind: "unsupported",
            },
            Case {
                name: "unbound head with offered command",
                mutate: |input| {
                    input.currentness.head_observed = None;
                },
                class: NextActionClass::RetryCurrentSubject,
                kind: "refresh_currentness",
            },
            Case {
                name: "missing prerequisite input",
                mutate: |input| {
                    input.offered_command = None;
                    input.missing_input = Some("owner evidence".to_string());
                },
                class: NextActionClass::SatisfyPrerequisite,
                kind: "provide_input",
            },
            Case {
                name: "offered command wins over missing input",
                mutate: |input| {
                    input.missing_input = Some("owner evidence".to_string());
                },
                class: NextActionClass::RunCommand,
                kind: "none",
            },
        ];
        for case in cases {
            let offered = match case.name {
                "platform gap" => &without_macos,
                "manual execution" => &manual_spec,
                _ => &spec,
            };
            let mut input = base_input();
            input.offered_command = Some(offered);
            (case.mutate)(&mut input);
            let action = select_canonical_next_action(&input)
                .map_err(|error| format!("{} must stay a valid result: {error}", case.name))?;
            assert_eq!(action.action_class(), case.class, "{}", case.name);
            assert_eq!(stop_kind(&action), case.kind, "{}", case.name);
            if case.class.is_executable() {
                assert!(action.command().is_some(), "{}", case.name);
            } else {
                assert!(!action.is_executable(), "{}", case.name);
                assert!(action.command().is_none(), "{}", case.name);
            }
        }
        // The unviolated twin of every case above stays executable: the
        // battery proves each gate fires, not that nothing ever runs.
        let mut clean = base_input();
        clean.offered_command = Some(&spec);
        let action = select_canonical_next_action(&clean)
            .map_err(|error| format!("clean input runs: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RunCommand);
        assert_eq!(
            action
                .expected_transition()
                .map(|transition| transition.to_state.as_str()),
            Some("packet_inspected")
        );
        Ok(())
    }

    #[test]
    fn check_cases_map_to_their_closed_classes() -> Result<(), String> {
        let rows = [
            (
                NextActionCheckCase::TopGap,
                NextActionClass::InspectDetails,
                None,
            ),
            (
                NextActionCheckCase::SuppressedByPolicy,
                NextActionClass::UnsupportedOrLimited,
                None,
            ),
            (
                NextActionCheckCase::CandidateFilterHidAll,
                NextActionClass::UnsupportedOrLimited,
                None,
            ),
            (
                NextActionCheckCase::NoDiffFinding,
                NextActionClass::TerminalNoAction,
                None,
            ),
            (
                NextActionCheckCase::StaticLimited,
                NextActionClass::UnsupportedOrLimited,
                None,
            ),
            (
                NextActionCheckCase::PreviewAdvisory,
                NextActionClass::UnsupportedOrLimited,
                None,
            ),
            (
                NextActionCheckCase::ScopeMissing,
                NextActionClass::SatisfyPrerequisite,
                Some(TRANSITION_SCOPE_PROVIDED),
            ),
        ];
        for (case, class, transition_to) in rows {
            let mut input = base_input();
            input.producer = NextActionProducer::CheckTopResult;
            input.check_case = Some(case);
            input.currentness = NextActionCurrentness {
                head_expected: None,
                head_observed: None,
                config_expected: None,
                config_observed: None,
            };
            input.transition_from = case.as_str().to_string();
            let action = select_canonical_next_action(&input)
                .map_err(|error| format!("case {} must select: {error}", case.as_str()))?;
            assert_eq!(action.action_class(), class, "case {}", case.as_str());
            assert!(!action.is_executable(), "case {}", case.as_str());
            assert_eq!(
                action
                    .expected_transition()
                    .map(|transition| transition.to_state.as_str()),
                transition_to,
                "case {}",
                case.as_str()
            );
            match action.stop() {
                Some(NextActionStop::CheckTriage {
                    case: bound,
                    detail_route,
                }) => {
                    assert_eq!(*bound, case);
                    assert_eq!(
                        detail_route,
                        &input.detail_route,
                        "check case {} must carry its inspect route",
                        case.as_str()
                    );
                }
                other => {
                    return Err(format!(
                        "case {} must stop typed, got {other:?}",
                        case.as_str()
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn check_producer_must_bind_its_triage_case() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::CheckTopResult;
        let Err(_) = select_canonical_next_action(&input) else {
            return Err("unbound check-case selection must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn doctor_precise_recovery_is_a_satisfiable_prerequisite() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::Doctor;
        input.item_id = "tool_git".to_string();
        input.card_item = None;
        input.doctor_recovery = Some(NextActionDoctorRecovery {
            check_name: "tool_git".to_string(),
            recovery_route: "install git and rerun ripr doctor".to_string(),
        });
        input.transition_from = "doctor_check_failed".to_string();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("precise recovery is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::SatisfyPrerequisite);
        match action.stop() {
            Some(NextActionStop::DoctorRecovery {
                check_name,
                recovery_route,
            }) => {
                assert_eq!(check_name, "tool_git");
                assert_eq!(recovery_route, "install git and rerun ripr doctor");
            }
            other => return Err(format!("expected doctor stop, got {other:?}")),
        }
        assert_eq!(
            action
                .expected_transition()
                .map(|transition| transition.to_state.as_str()),
            Some(TRANSITION_RECOVERY_APPLIED)
        );
        Ok(())
    }

    #[test]
    fn doctor_without_recovery_declares_its_limitation() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::Doctor;
        input.item_id = "root_directory".to_string();
        input.card_item = None;
        input.limitation = Some("no precise recovery is available".to_string());
        input.transition_from = "doctor_check_failed".to_string();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("imprecise doctor is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::UnsupportedOrLimited);
        assert_eq!(stop_kind(&action), "unsupported");

        let mut bare = base_input();
        bare.producer = NextActionProducer::Doctor;
        let Err(_) = select_canonical_next_action(&bare) else {
            return Err("bare doctor selection must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn pilot_inspects_only_through_its_delegated_transaction() -> Result<(), String> {
        let mut input = base_input();
        input.producer = NextActionProducer::PilotDelegation;
        input.pilot_delegation = Some(NextActionPilotDelegation {
            transaction_ref: "check-artifact-1".to_string(),
            route: "ripr check --root . --json".to_string(),
        });
        input.transition_from = "pilot_top_seam".to_string();
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("delegation is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::InspectDetails);
        match action.stop() {
            Some(NextActionStop::PilotDelegated {
                transaction_ref,
                route,
            }) => {
                assert_eq!(transaction_ref, "check-artifact-1");
                assert_eq!(route, "ripr check --root . --json");
            }
            other => return Err(format!("expected pilot stop, got {other:?}")),
        }

        let mut inventing = base_input();
        inventing.producer = NextActionProducer::PilotDelegation;
        inventing.transition_from = "pilot_top_seam".to_string();
        let action = select_canonical_next_action(&inventing)
            .map_err(|error| format!("invention is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::UnsupportedOrLimited);
        assert_eq!(stop_kind(&action), "unsupported");
        Ok(())
    }

    #[test]
    fn status_attempt_states_cover_the_lifecycle() -> Result<(), String> {
        // Awaiting edit: the focused test edit is the prerequisite.
        let mut awaiting = base_input();
        awaiting.producer = NextActionProducer::RepairAttemptStatus;
        let mut view = attempt_view("repair-attempt-03");
        view.awaits_edit = true;
        awaiting.attempts = vec![view];
        awaiting.transition_from = "awaiting_edit".to_string();
        let action = select_canonical_next_action(&awaiting)
            .map_err(|error| format!("awaiting edit is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::SatisfyPrerequisite);
        assert_eq!(stop_kind(&action), "provide_input");

        // Ready to continue with no spec: inspect the recorded after phase.
        let mut ready = base_input();
        ready.producer = NextActionProducer::RepairAttemptStatus;
        ready.attempts = vec![attempt_view("repair-attempt-03")];
        ready.transition_from = "awaiting_edit".to_string();
        let action = select_canonical_next_action(&ready)
            .map_err(|error| format!("ready is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::InspectDetails);
        assert_eq!(stop_kind(&action), "inspect_target");

        // Failed/stale/incomparable/corrupt producers recommend a restart.
        let mut failed = base_input();
        failed.producer = NextActionProducer::RepairAttemptStatus;
        let mut view = attempt_view("repair-attempt-04");
        view.restart_recommended = true;
        view.restart_route = "ripr agent repair --root . --seam-id seam:demo".to_string();
        failed.attempts = vec![view];
        failed.transition_from = "failed".to_string();
        let action = select_canonical_next_action(&failed)
            .map_err(|error| format!("failed is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::RetryCurrentSubject);
        match action.stop() {
            Some(NextActionStop::RestartAttempt {
                attempt_id,
                restart_route,
            }) => {
                assert_eq!(attempt_id, "repair-attempt-04");
                assert!(restart_route.contains("agent repair"));
            }
            other => return Err(format!("expected restart stop, got {other:?}")),
        }

        // Limited producers declare the bound.
        let mut limited = base_input();
        limited.producer = NextActionProducer::RepairAttemptStatus;
        limited.limitation = Some("head unreadable".to_string());
        limited.transition_from = "limited".to_string();
        let action = select_canonical_next_action(&limited)
            .map_err(|error| format!("limited is typed: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::UnsupportedOrLimited);
        Ok(())
    }

    #[test]
    fn single_candidate_normalizes_to_the_bound_item() -> Result<(), String> {
        let mut input = base_input();
        input.item_id = String::new();
        input.card_item = None;
        input.item_candidates = vec!["item:only".to_string()];
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("one candidate binds: {error}"))?;
        assert_eq!(action.action_class(), NextActionClass::InspectDetails);
        assert_eq!(action.subject().item.as_deref(), Some("item:only"));
        Ok(())
    }

    #[test]
    fn producer_input_validation_fails_closed() -> Result<(), String> {
        let spec = test_spec();
        let mut both = base_input();
        both.item_candidates = vec!["item:a".to_string()];
        let Err(_) = select_canonical_next_action(&both) else {
            return Err("bound-plus-candidates selection must fail".to_string());
        };

        let mut blank_state = base_input();
        blank_state.transition_from = String::new();
        let Err(_) = select_canonical_next_action(&blank_state) else {
            return Err("blank-state selection must fail".to_string());
        };

        let mut missing_effect = base_input();
        missing_effect.offered_command = Some(&spec);
        missing_effect.transition_to = None;
        let Err(_) = select_canonical_next_action(&missing_effect) else {
            return Err("command-without-effect selection must fail".to_string());
        };

        let mut bound_nothing = base_input();
        bound_nothing.item_id = String::new();
        bound_nothing.card_item = None;
        let Err(_) = select_canonical_next_action(&bound_nothing) else {
            return Err("unbound-item selection must fail".to_string());
        };
        Ok(())
    }

    #[test]
    fn selector_added_alternatives_fill_room_only() -> Result<(), String> {
        let mut full = base_input();
        full.producer = NextActionProducer::RepairAttemptStatus;
        let mut finished = attempt_view("repair-attempt-09");
        finished.terminal = true;
        finished.receipt_ref = Some("receipt".to_string());
        full.attempts = vec![finished];
        full.transition_from = "finished_current".to_string();
        full.alternatives = (0..MAX_NEXT_ACTION_ALTERNATIVES)
            .map(|index| NextActionAlternative {
                label: format!("label-{index}"),
                route: format!("route-{index}"),
            })
            .collect();
        let action = select_canonical_next_action(&full)
            .map_err(|error| format!("full stays bounded: {error}"))?;
        // The receipt alternative is dropped but the stop keeps the ref, so
        // no identity is lost.
        assert_eq!(action.alternatives().len(), MAX_NEXT_ACTION_ALTERNATIVES);
        match action.stop() {
            Some(NextActionStop::TerminalComplete { receipt_ref }) => {
                assert_eq!(receipt_ref, "receipt");
            }
            other => return Err(format!("expected terminal stop, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn wire_spellings_match_their_accessors() -> Result<(), String> {
        for (class, spelling) in [
            (NextActionClass::RunCommand, "run_command"),
            (NextActionClass::InspectDetails, "inspect_details"),
            (NextActionClass::ChooseItem, "choose_item"),
            (NextActionClass::ChooseAttempt, "choose_attempt"),
            (NextActionClass::SatisfyPrerequisite, "satisfy_prerequisite"),
            (
                NextActionClass::RetryCurrentSubject,
                "retry_current_subject",
            ),
            (NextActionClass::TerminalNoAction, "terminal_no_action"),
            (
                NextActionClass::UnsupportedOrLimited,
                "unsupported_or_limited",
            ),
        ] {
            assert_eq!(class.as_str(), spelling);
            let rendered = serde_json::to_value(class)
                .map_err(|error| format!("class serializes: {error}"))?;
            assert_eq!(rendered.as_str(), Some(spelling));
        }
        for (producer, spelling) in [
            (NextActionProducer::CheckTopResult, "check_top_result"),
            (NextActionProducer::RepairCard, "repair_card"),
            (
                NextActionProducer::RepairAttemptStatus,
                "repair_attempt_status",
            ),
            (NextActionProducer::Doctor, "doctor"),
            (NextActionProducer::PilotDelegation, "pilot_delegation"),
        ] {
            assert_eq!(producer.as_str(), spelling);
            let rendered = serde_json::to_value(producer)
                .map_err(|error| format!("producer serializes: {error}"))?;
            assert_eq!(rendered.as_str(), Some(spelling));
        }
        for (case, spelling) in [
            (NextActionCheckCase::TopGap, "top_gap"),
            (
                NextActionCheckCase::SuppressedByPolicy,
                "suppressed_by_policy",
            ),
            (
                NextActionCheckCase::CandidateFilterHidAll,
                "candidate_filter_hid_all",
            ),
            (NextActionCheckCase::NoDiffFinding, "no_diff_finding"),
            (NextActionCheckCase::StaticLimited, "static_limited"),
            (NextActionCheckCase::PreviewAdvisory, "preview_advisory"),
            (NextActionCheckCase::ScopeMissing, "scope_missing"),
        ] {
            assert_eq!(case.as_str(), spelling);
            let rendered =
                serde_json::to_value(case).map_err(|error| format!("case serializes: {error}"))?;
            assert_eq!(rendered.as_str(), Some(spelling));
        }
        Ok(())
    }

    #[test]
    fn stop_kinds_match_their_serde_tags() -> Result<(), String> {
        let stops = vec![
            NextActionStop::SelectItem {
                candidates: vec!["a".to_string()],
                total: 1,
            },
            NextActionStop::SelectAttempt {
                candidates: vec!["a".to_string()],
                total: 1,
            },
            NextActionStop::ResolveDisagreement {
                check_item: "a".to_string(),
                card_item: "b".to_string(),
            },
            NextActionStop::RefreshCurrentness {
                observed: "a".to_string(),
                expected: "b".to_string(),
                restart_route: "r".to_string(),
            },
            NextActionStop::RefreshConfig {
                observed: "a".to_string(),
                expected: "b".to_string(),
                restart_route: "r".to_string(),
            },
            NextActionStop::ProvideInput {
                input: "i".to_string(),
                detail_route: "r".to_string(),
            },
            NextActionStop::RestartAttempt {
                attempt_id: "a".to_string(),
                restart_route: "r".to_string(),
            },
            NextActionStop::RouteRefused {
                command_id: "c".to_string(),
                reason: "r".to_string(),
            },
            NextActionStop::PlatformUnavailable {
                command_id: "c".to_string(),
                supported_platforms: vec![CommandPlatform::Linux],
                alternative_route: "r".to_string(),
            },
            NextActionStop::ManualStep {
                command_id: "c".to_string(),
                instruction: "i".to_string(),
            },
            NextActionStop::TerminalComplete {
                receipt_ref: "r".to_string(),
            },
            NextActionStop::Unsupported {
                limitation: "l".to_string(),
                detail_route: "r".to_string(),
            },
            NextActionStop::InspectTarget {
                detail_route: "r".to_string(),
            },
            NextActionStop::CheckTriage {
                case: NextActionCheckCase::TopGap,
                detail_route: "ripr explain finding:top".to_string(),
            },
            NextActionStop::DoctorRecovery {
                check_name: "c".to_string(),
                recovery_route: "r".to_string(),
            },
            NextActionStop::PilotDelegated {
                transaction_ref: "t".to_string(),
                route: "r".to_string(),
            },
        ];
        assert_eq!(stops.len(), 16);
        for stop in &stops {
            let rendered =
                serde_json::to_value(stop).map_err(|error| format!("stop serializes: {error}"))?;
            assert_eq!(
                rendered.get("kind").and_then(|kind| kind.as_str()),
                Some(stop.kind())
            );
        }
        // Round trip: the DTO survives the wire unchanged.
        let spec = test_spec();
        let mut input = base_input();
        input.offered_command = Some(&spec);
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("clean input runs: {error}"))?;
        let rendered =
            serde_json::to_value(&action).map_err(|error| format!("action serializes: {error}"))?;
        assert_eq!(
            rendered
                .get("schema_version")
                .and_then(|version| version.as_str()),
            Some(CANONICAL_NEXT_ACTION_SCHEMA_VERSION)
        );
        let back: CanonicalNextActionV1 = serde_json::from_value(rendered)
            .map_err(|error| format!("action deserializes: {error}"))?;
        assert_eq!(back, action);
        Ok(())
    }

    fn repair_start_input<'a>(
        item_id: &str,
        candidates: Vec<String>,
        offered: Option<&'a CommandSpec>,
    ) -> NextActionInput<'a> {
        NextActionInput {
            producer: NextActionProducer::RepairStart,
            root: "/repo".to_string(),
            diff_source: NextActionDiffSource::WorkingTree {
                head: Some("head1".to_string()),
            },
            item_id: item_id.to_string(),
            check_item: None,
            card_item: None,
            item_candidates: candidates,
            attempts: Vec::new(),
            currentness: fresh_currentness(),
            offered_command: offered,
            route_admitted: true,
            route_refusal: None,
            missing_input: None,
            platform: Some(CommandPlatform::Linux),
            limitation: None,
            limitation_route: None,
            detail_route: "ripr pilot --root .".to_string(),
            transition_from: "one_eligible_seam".to_string(),
            transition_to: offered.map(|_| "repair_started".to_string()),
            restart_route: "ripr repair --root .".to_string(),
            check_case: None,
            doctor_recovery: None,
            pilot_delegation: None,
            alternatives: Vec::new(),
            limitations: Vec::new(),
        }
    }

    /// #6305: the repair-start producer offers the before-phase command
    /// only for one bound subject with fresh heads; the selector runs it.
    #[test]
    fn repair_start_single_admitted_candidate_runs() -> Result<(), String> {
        let spec = crate::agent::command_specs::repair_start_command_spec(".", "seam:demo");
        spec.validate()
            .map_err(|error| format!("repair-start spec validates: {error:?}"))?;
        let input = repair_start_input("seam:demo", Vec::new(), Some(&spec));
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("single candidate runs: {error}"))?;
        if action.action_class() != NextActionClass::RunCommand {
            return Err(format!(
                "single candidate must run, got {}",
                action.action_class().as_str()
            ));
        }
        assert!(action.is_executable());
        let command = action
            .command()
            .ok_or_else(|| "run action carries no command".to_string())?;
        if command.command_id != "ripr:repair:start" {
            return Err(format!("unexpected command id {}", command.command_id));
        }
        if action.producer() != NextActionProducer::RepairStart {
            return Err("producer must survive the decision".to_string());
        }
        Ok(())
    }

    /// #6305: the repair-start producer never selects implicitly —
    /// several candidates choose. (Zero candidates never reach the
    /// selector: with no suitable subject the anti-invention law leaves
    /// nothing to bind, so the producer owns the honest outcome.)
    #[test]
    fn repair_start_several_candidates_choose() -> Result<(), String> {
        let several = repair_start_input(
            "",
            vec!["seam:one".to_string(), "seam:two".to_string()],
            None,
        );
        let action = select_canonical_next_action(&several)
            .map_err(|error| format!("several candidates decide: {error}"))?;
        if action.action_class() != NextActionClass::ChooseItem
            || stop_kind(&action) != "select_item"
        {
            return Err(format!(
                "several candidates must choose, got {}",
                action.action_class().as_str()
            ));
        }
        assert!(!action.is_executable());
        Ok(())
    }

    /// #6305: an unreadable HEAD unbinds currentness, so one candidate
    /// cannot execute until the evidence is recomputed.
    #[test]
    fn repair_start_unreadable_head_cannot_execute() -> Result<(), String> {
        let spec = crate::agent::command_specs::repair_start_command_spec(".", "seam:demo");
        let mut input = repair_start_input("seam:demo", Vec::new(), Some(&spec));
        input.currentness.head_expected = None;
        input.currentness.head_observed = None;
        let action = select_canonical_next_action(&input)
            .map_err(|error| format!("unbound heads decide: {error}"))?;
        if action.action_class() != NextActionClass::RetryCurrentSubject
            || stop_kind(&action) != "refresh_currentness"
        {
            return Err(format!(
                "unbound heads must retry currentness, got {}",
                action.action_class().as_str()
            ));
        }
        assert!(!action.is_executable());
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn current_platform_names_linux() {
        assert_eq!(current_command_platform(), Some(CommandPlatform::Linux));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn current_platform_names_windows() {
        assert_eq!(current_command_platform(), Some(CommandPlatform::Windows));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn current_platform_names_macos() {
        assert_eq!(current_command_platform(), Some(CommandPlatform::Macos));
    }
}
