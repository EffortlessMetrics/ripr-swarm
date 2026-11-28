//! Kani bounded-model-checking proofs for the canonical next-action
//! selector, the load-bearing actionability flip. Compiled only under
//! `cargo kani`; normal builds never see this file.
//!
//! Strings are drawn from a tiny fixed alphabet ("a" / "b" / `None`) chosen by
//! a symbolic index: the selector only compares and clones them, so equality
//! and presence/absence are the properties that matter. Booleans, enum
//! variants and collection sizes are fully symbolic within the stated bounds.
//!
//! Refusal stops whose rendering formats labels or collects candidates (the
//! unsupported/unknown-platform, terminal-attempt, awaiting-edit,
//! restart-recommended and several-unranked-items stops) are proved on the
//! typed gate decision `decide_next_action` returns, not on the rendered
//! action: CBMC ran past 12 minutes and 4 GB or out of memory on the
//! `format!` / `collect` rendering when these were proved through
//! `select_canonical_next_action` (#13707). Each harness asserts that when the
//! stop condition holds the decision is exactly that typed stop and is not
//! executable, and that the admitted twin decides `Run` (rendering may still
//! fail closed at mint, for example with no bound item).
//!
//! Not covered by Kani: the rendering step `render_decision` itself (labels,
//! candidate lists, selector-added alternatives). Its link to the decision is
//! structural (only `NextActionDecision::Run` reaches `running`; every other
//! variant mints through `stopped`, which references no command) and is
//! checked by the selector's unit tests, including the control-10 battery
//! asserting decision and rendered executability agree for every gate that
//! battery exercises (the CheckTriage, DoctorRecovery, PilotDelegated and
//! InspectTarget tails are covered by their own unit tests).

use super::*;
use ripr_contract::domain::command_spec::{
    CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, EnvironmentPolicy,
    ExpectedResultParser, NetworkPolicy, StdinPolicy,
};

/// `None`, `Some("a")` or `Some("b")`, selected by a symbolic index.
fn any_opt_text() -> Option<String> {
    let index: u8 = kani::any();
    kani::assume(index < 3);
    match index {
        0 => None,
        1 => Some("a".to_string()),
        _ => Some("b".to_string()),
    }
}

fn any_currentness() -> NextActionCurrentness {
    NextActionCurrentness {
        head_expected: any_opt_text(),
        head_observed: any_opt_text(),
        config_expected: any_opt_text(),
        config_observed: any_opt_text(),
    }
}

fn unbound_currentness() -> NextActionCurrentness {
    NextActionCurrentness {
        head_expected: None,
        head_observed: None,
        config_expected: None,
        config_observed: None,
    }
}

fn any_mode() -> CommandExecutionMode {
    let index: u8 = kani::any();
    kani::assume(index < 3);
    match index {
        0 => CommandExecutionMode::Direct,
        1 => CommandExecutionMode::ShellRequired,
        _ => CommandExecutionMode::Manual,
    }
}

fn any_role() -> CommandRole {
    let index: u8 = kani::any();
    kani::assume(index < 6);
    match index {
        0 => CommandRole::Verify,
        1 => CommandRole::Receipt,
        2 => CommandRole::Regeneration,
        3 => CommandRole::Inspection,
        4 => CommandRole::TargetedRerun,
        _ => CommandRole::RepairStart,
    }
}

/// A spec with symbolic role, execution mode and supported-platform subset.
fn any_spec() -> CommandSpec {
    let mut platforms = Vec::new();
    if kani::any() {
        platforms.push(CommandPlatform::Linux);
    }
    if kani::any() {
        platforms.push(CommandPlatform::Macos);
    }
    if kani::any() {
        platforms.push(CommandPlatform::Windows);
    }
    CommandSpec {
        schema_version: "s".to_string(),
        command_id: "c".to_string(),
        role: any_role(),
        execution_mode: any_mode(),
        program: "p".to_string(),
        args: Vec::new(),
        cwd: ".".to_string(),
        env_set: Vec::new(),
        env_passthrough: Vec::new(),
        environment_policy: EnvironmentPolicy::Inherited,
        stdin: StdinPolicy::Null,
        timeout_ms: 1,
        cancellation: CancellationPolicy::Allowed,
        network_policy: NetworkPolicy::Forbidden,
        expected_result_parser: ExpectedResultParser::DeclaredJson,
        expected_exit_codes: Vec::new(),
        expected_writes: Vec::new(),
        cost_class: CommandCostClass::Check,
        platforms,
        display: "d".to_string(),
        authority_boundary: CommandAuthorityBoundary::InspectionRouteOnly,
    }
}

/// A spec that is direct, supports Linux, and has a symbolic role; used where
/// the platform and mode gates are held open.
fn any_spec_direct_linux() -> CommandSpec {
    let mut spec = any_spec();
    spec.execution_mode = CommandExecutionMode::Direct;
    spec.platforms = vec![CommandPlatform::Linux];
    spec
}

/// A baseline input with every fixed text bound; the callers vary the
/// load-bearing facts symbolically. The producer is `RepairCard`, which has
/// no producer-specific binding requirement, so every `Err` path here is the
/// selector refusing, never a missing-binding artifact.
fn base_input(spec: Option<&CommandSpec>) -> NextActionInput<'_> {
    NextActionInput {
        producer: NextActionProducer::RepairCard,
        root: "root".to_string(),
        diff_source: NextActionDiffSource::WorkingTree { head: None },
        item_id: "item".to_string(),
        check_item: None,
        card_item: None,
        item_candidates: Vec::new(),
        item_candidate_labels: Vec::new(),
        attempts: Vec::new(),
        currentness: unbound_currentness(),
        offered_command: spec,
        route_admitted: true,
        route_refusal: None,
        missing_input: None,
        platform: None,
        limitation: None,
        limitation_route: None,
        detail_route: "detail".to_string(),
        transition_from: "from".to_string(),
        transition_to: Some("to".to_string()),
        restart_route: "restart".to_string(),
        check_case: None,
        check_repair_route: None,
        check_scope_route: None,
        doctor_recovery: None,
        pilot_delegation: None,
        alternatives: Vec::new(),
        limitations: Vec::new(),
    }
}

/// Head/config comparison: `heads_bound_and_fresh` holds only when both heads
/// are bound and equal, and an unbound axis is never divergence and never
/// fresh. Config divergence likewise requires both sides bound.
#[kani::proof]
#[kani::unwind(6)]
fn currentness_fresh_requires_bound_equal_heads() {
    let currentness = any_currentness();
    let fresh = currentness.heads_bound_and_fresh();
    let diverge = currentness.heads_diverge();

    assert_eq!(
        fresh,
        currentness.head_expected.is_some()
            && currentness.head_observed.is_some()
            && currentness.head_expected == currentness.head_observed
    );
    if currentness.head_expected.is_none() || currentness.head_observed.is_none() {
        assert!(!fresh && !diverge);
    } else {
        assert!(fresh != diverge);
    }
    if currentness.config_expected.is_none() || currentness.config_observed.is_none() {
        assert!(!currentness.configs_diverge());
    }
    kani::cover!(fresh);
    kani::cover!(diverge);
    kani::cover!(!fresh && !diverge);
}

/// The mint law: `CanonicalNextActionV1::new` yields `Ok` only when the
/// action is executable exactly when it references a command, and carries a
/// typed stop exactly when it is not executable. Every action class and every
/// presence combination of item / command / stop / transition is covered.
#[kani::proof]
#[kani::unwind(6)]
fn mint_executable_iff_command_and_never_with_stop() {
    let index: u8 = kani::any();
    kani::assume(index < 8);
    let action_class = match index {
        0 => NextActionClass::RunCommand,
        1 => NextActionClass::InspectDetails,
        2 => NextActionClass::ChooseItem,
        3 => NextActionClass::ChooseAttempt,
        4 => NextActionClass::SatisfyPrerequisite,
        5 => NextActionClass::RetryCurrentSubject,
        6 => NextActionClass::TerminalNoAction,
        _ => NextActionClass::UnsupportedOrLimited,
    };
    let item = if kani::any() {
        Some("item".to_string())
    } else {
        None
    };
    let command = if kani::any() {
        Some(NextActionCommandRef {
            command_id: "c".to_string(),
            role: CommandRole::Inspection,
            display: "d".to_string(),
        })
    } else {
        None
    };
    let stop = if kani::any() {
        Some(NextActionStop::InspectTarget {
            detail_route: "detail".to_string(),
        })
    } else {
        None
    };
    let transition = if kani::any() {
        Some(NextActionTransition {
            from_state: "f".to_string(),
            to_state: "t".to_string(),
        })
    } else {
        None
    };
    let minted = CanonicalNextActionV1::new(
        NextActionProducer::RepairCard,
        NextActionSubject {
            root: "root".to_string(),
            diff_source: NextActionDiffSource::WorkingTree { head: None },
            item,
        },
        any_currentness(),
        action_class,
        command,
        stop,
        transition,
        Vec::new(),
        Vec::new(),
    );
    if let Ok(action) = &minted {
        assert_eq!(action.is_executable(), action.command().is_some());
        assert_eq!(action.is_executable(), action.stop().is_none());
        assert_eq!(
            action.is_executable(),
            action_class == NextActionClass::RunCommand
        );
        assert_eq!(
            action.expected_transition().is_some(),
            action_class.requires_transition()
        );
        kani::cover!(action.is_executable());
        kani::cover!(!action.is_executable());
    }
    kani::cover!(minted.is_err());
}

/// A fully admitted baseline: the offered spec is direct, supports Linux, the
/// platform is Linux, heads are bound and equal, the route is admitted and
/// nothing else objects. The focused harnesses below each open one gate
/// family symbolically against this baseline; with no gate opened the command
/// runs, so every refusal observed is attributable to the opened gate.
fn admitted_baseline(spec: &CommandSpec) -> NextActionInput<'_> {
    let mut input = base_input(Some(spec));
    input.platform = Some(CommandPlatform::Linux);
    input.currentness.head_expected = Some("a".to_string());
    input.currentness.head_observed = Some("a".to_string());
    input
}

fn is_executable(selected: &Result<CanonicalNextActionV1, String>) -> bool {
    matches!(selected, Ok(action) if action.is_executable())
}

/// Route / execution-mode / effect gates. Symbolic: whether the route admits
/// the target, the spec's execution mode and role, and whether the producer
/// named the command's effect. The platform gate is held open here (spec and
/// host both Linux) and proved separately. The selector returns an executable
/// action only when the route is admitted, the mode is not manual and the
/// effect is named; a route the target would be refused on can never run the
/// command, however plausible its syntax.
#[kani::proof]
#[kani::unwind(6)]
fn selector_runs_only_when_route_mode_and_effect_hold() {
    let mut spec = any_spec();
    spec.platforms = vec![CommandPlatform::Linux];
    let mut input = admitted_baseline(&spec);
    input.route_admitted = kani::any();
    input.transition_to = if kani::any() {
        Some("to".to_string())
    } else {
        None
    };

    let selected = select_canonical_next_action(&input);
    let runs = is_executable(&selected);

    let open = input.route_admitted
        && spec.execution_mode != CommandExecutionMode::Manual
        && input.transition_to.is_some();
    // Fail closed and positive twin: the command runs exactly when every gate
    // is open.
    assert_eq!(runs, open);
    kani::cover!(runs);
    kani::cover!(!runs && !input.route_admitted);
    kani::cover!(!runs && spec.execution_mode == CommandExecutionMode::Manual);
    kani::cover!(!runs && input.transition_to.is_none());
}

/// Limitation gate. Symbolic: a producer-declared limitation. A limitation is
/// a valid product result but never an executable repair: with it declared the
/// command never runs, and without it (everything else admitted) it does.
#[kani::proof]
#[kani::unwind(6)]
fn selector_never_runs_a_command_when_a_limitation_is_declared() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.limitation = if kani::any() {
        Some("lim".to_string())
    } else {
        None
    };

    let selected = select_canonical_next_action(&input);
    let runs = is_executable(&selected);

    assert_eq!(runs, input.limitation.is_none());
    kani::cover!(runs);
    kani::cover!(!runs);
}

/// Config-currentness gate. Symbolic: expected/observed config (each side
/// None / "a" / "b"), with the heads held bound and equal. A bound config
/// divergence refuses; an unbound side is an explicit unknown that does not by
/// itself refuse (the head freshness requirement is the executable gate and is
/// proved separately), so the command runs exactly when the configs do not
/// diverge.
#[kani::proof]
#[kani::unwind(6)]
fn selector_never_runs_a_command_on_diverged_config() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.currentness.config_expected = any_opt_text();
    input.currentness.config_observed = any_opt_text();

    let selected = select_canonical_next_action(&input);
    let runs = is_executable(&selected);

    assert_eq!(runs, !input.currentness.configs_diverge());
    kani::cover!(runs);
    kani::cover!(input.currentness.configs_diverge());
}

/// Check/card agreement gate. Symbolic: the check-selected and card-selected
/// item (each None / "a" / "b"). When both are bound and differ the selector
/// refuses rather than choosing one silently; otherwise the command runs.
#[kani::proof]
#[kani::unwind(6)]
fn selector_never_runs_a_command_when_check_and_card_disagree() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.check_item = any_opt_text();
    input.card_item = any_opt_text();

    let selected = select_canonical_next_action(&input);
    let runs = is_executable(&selected);

    let disagree = matches!(
        (&input.check_item, &input.card_item),
        (Some(check), Some(card)) if check != card
    );
    assert_eq!(runs, !disagree);
    kani::cover!(runs);
    kani::cover!(disagree);
}

/// Unknown or missing inputs fail closed: with an offered command that is
/// otherwise fully admitted (route admitted, platform supported, direct mode,
/// no limitation), an unbound or mismatched head can never produce an
/// executable action, and fresh heads do run it (positive twin).
#[kani::proof]
#[kani::unwind(6)]
fn unknown_or_stale_head_never_runs_an_admitted_command() {
    let mut spec = any_spec();
    spec.execution_mode = CommandExecutionMode::Direct;
    spec.platforms = vec![CommandPlatform::Linux];
    let mut input = base_input(Some(&spec));
    input.platform = Some(CommandPlatform::Linux);
    input.currentness.head_expected = any_opt_text();
    input.currentness.head_observed = any_opt_text();

    let selected = select_canonical_next_action(&input);
    let fresh = input.currentness.heads_bound_and_fresh();
    if fresh {
        assert!(matches!(&selected, Ok(action) if action.is_executable()));
    } else if let Ok(action) = &selected {
        assert!(!action.is_executable());
        assert_eq!(action.action_class(), NextActionClass::RetryCurrentSubject);
    }
    kani::cover!(fresh);
    kani::cover!(!fresh && input.currentness.head_expected.is_none());
    kani::cover!(!fresh && input.currentness.head_expected.is_some());
}

/// `None`, Linux, macOS or Windows, selected by a symbolic index.
fn any_platform() -> Option<CommandPlatform> {
    let index: u8 = kani::any();
    kani::assume(index < 4);
    match index {
        0 => None,
        1 => Some(CommandPlatform::Linux),
        2 => Some(CommandPlatform::Macos),
        _ => Some(CommandPlatform::Windows),
    }
}

/// `"a"` or `"b"`, selected by a symbolic bool.
fn any_text() -> String {
    if kani::any() {
        "a".to_string()
    } else {
        "b".to_string()
    }
}

/// One attempt with the given lifecycle flags and fixed identities.
fn attempt_view(
    terminal: bool,
    awaits_edit: bool,
    restart_recommended: bool,
) -> NextActionAttemptView {
    NextActionAttemptView {
        id: "a".to_string(),
        terminal,
        awaits_edit,
        restart_recommended,
        restart_route: "r".to_string(),
        receipt_ref: None,
    }
}

fn decided_executable(decision: &Result<NextActionDecision<'_>, &'static str>) -> bool {
    matches!(decision, Ok(decision) if decision.is_executable())
}

/// Platform stop. Symbolic: the host platform (unknown / Linux / macOS /
/// Windows), the spec's supported-platform subset and its role; the mode is
/// held direct and every other gate admitted. Whenever the host is unknown or
/// not in the spec's platforms the decision is exactly `PlatformUnavailable`
/// for the offered spec and is not executable; otherwise the command runs.
#[kani::proof]
#[kani::unwind(6)]
fn platform_stop_decides_unavailable_and_never_runs() {
    let mut spec = any_spec();
    spec.execution_mode = CommandExecutionMode::Direct;
    let mut input = admitted_baseline(&spec);
    input.platform = any_platform();

    let decision = decide_next_action(&input);
    let unavailable = input
        .platform
        .is_none_or(|platform| !spec.platforms.contains(&platform));

    if unavailable {
        assert!(matches!(
            decision,
            Ok(NextActionDecision::PlatformUnavailable { spec: chosen })
                if core::ptr::eq(chosen, &spec)
        ));
    } else {
        assert!(matches!(decision, Ok(NextActionDecision::Run { .. })));
    }
    assert_eq!(decided_executable(&decision), !unavailable);
    kani::cover!(unavailable && input.platform.is_none());
    kani::cover!(unavailable && input.platform.is_some());
    kani::cover!(!unavailable);
}

/// The single-attempt gate facts the three attempt-stop harnesses share.
struct AttemptGates {
    /// Terminal attempt with a non-read-only offer.
    terminal_stop: bool,
    awaits_edit: bool,
    restart_recommended: bool,
}

impl AttemptGates {
    fn of(spec: &CommandSpec, attempt: &NextActionAttemptView) -> Self {
        let read_only = matches!(spec.role, CommandRole::Inspection | CommandRole::Receipt);
        Self {
            terminal_stop: attempt.terminal && !read_only,
            awaits_edit: attempt.awaits_edit,
            restart_recommended: attempt.restart_recommended,
        }
    }

    fn obligated(&self) -> bool {
        self.terminal_stop || self.awaits_edit || self.restart_recommended
    }
}

/// Whether `attempt` is the input's own single attempt (not a copy).
fn is_input_attempt(input: &NextActionInput<'_>, attempt: &NextActionAttemptView) -> bool {
    input
        .attempts
        .first()
        .is_some_and(|first| core::ptr::eq(first, attempt))
}

/// Terminal-attempt stop. Symbolic: the attempt's terminal / awaits-edit /
/// restart flags and the offered spec's role, against an otherwise admitted
/// direct Linux offer. A terminal attempt with a non-read-only offer decides
/// exactly `TerminalAttempt` for that attempt and never runs; a read-only
/// (inspection / receipt) offer on a terminal attempt is not this stop. With
/// no attempt obligation at all the command runs.
#[kani::proof]
#[kani::unwind(6)]
fn terminal_attempt_stop_decides_terminal_and_never_runs() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.attempts = vec![attempt_view(kani::any(), kani::any(), kani::any())];
    let Some(attempt) = input.attempts.first() else {
        return;
    };
    let gates = AttemptGates::of(&spec, attempt);

    let decision = decide_next_action(&input);

    if gates.terminal_stop {
        assert!(matches!(
            decision,
            Ok(NextActionDecision::TerminalAttempt { attempt: chosen })
                if is_input_attempt(&input, chosen)
        ));
    } else {
        assert!(!matches!(
            decision,
            Ok(NextActionDecision::TerminalAttempt { .. })
        ));
    }
    assert_eq!(decided_executable(&decision), !gates.obligated());
    kani::cover!(gates.terminal_stop);
    kani::cover!(attempt.terminal && !gates.terminal_stop);
    kani::cover!(!gates.obligated());
}

/// Awaiting-edit stop. Same symbolic space as the terminal harness. When the
/// terminal stop does not fire and the attempt awaits a focused test edit,
/// the decision is exactly `AwaitingEdit` for that attempt and never runs.
#[kani::proof]
#[kani::unwind(6)]
fn awaiting_edit_stop_decides_awaiting_and_never_runs() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.attempts = vec![attempt_view(kani::any(), kani::any(), kani::any())];
    let Some(attempt) = input.attempts.first() else {
        return;
    };
    let gates = AttemptGates::of(&spec, attempt);

    let decision = decide_next_action(&input);

    let awaiting = !gates.terminal_stop && gates.awaits_edit;
    if awaiting {
        assert!(matches!(
            decision,
            Ok(NextActionDecision::AwaitingEdit { attempt: chosen })
                if is_input_attempt(&input, chosen)
        ));
    } else {
        assert!(!matches!(
            decision,
            Ok(NextActionDecision::AwaitingEdit { .. })
        ));
    }
    assert_eq!(decided_executable(&decision), !gates.obligated());
    kani::cover!(awaiting);
    kani::cover!(gates.awaits_edit && !awaiting);
    kani::cover!(!gates.obligated());
}

/// Restart-recommended stop. Same symbolic space as the terminal harness.
/// When neither the terminal nor the awaiting-edit stop fires and the attempt
/// recommends a restart, the decision is exactly `RestartRecommended` for
/// that attempt and never runs.
#[kani::proof]
#[kani::unwind(6)]
fn restart_recommended_stop_decides_restart_and_never_runs() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.attempts = vec![attempt_view(kani::any(), kani::any(), kani::any())];
    let Some(attempt) = input.attempts.first() else {
        return;
    };
    let gates = AttemptGates::of(&spec, attempt);

    let decision = decide_next_action(&input);

    let restart = !gates.terminal_stop && !gates.awaits_edit && gates.restart_recommended;
    if restart {
        assert!(matches!(
            decision,
            Ok(NextActionDecision::RestartRecommended { attempt: chosen })
                if is_input_attempt(&input, chosen)
        ));
    } else {
        assert!(!matches!(
            decision,
            Ok(NextActionDecision::RestartRecommended { .. })
        ));
    }
    assert_eq!(decided_executable(&decision), !gates.obligated());
    kani::cover!(restart);
    kani::cover!(gates.restart_recommended && !restart);
    kani::cover!(!gates.obligated());
}

/// Several-unranked-items stop. Symbolic: zero to three item candidates, each
/// "a" or "b", with no single bound item and an otherwise admitted offer.
/// More than one candidate decides exactly `SeveralItems` and never runs;
/// zero or one candidate (one normalizes to the bound item) decides `Run`;
/// with zero candidates and no item the render then fails closed at mint.
#[kani::proof]
#[kani::unwind(6)]
fn several_items_stop_decides_selection_and_never_runs() {
    let spec = any_spec_direct_linux();
    let mut input = admitted_baseline(&spec);
    input.item_id = String::new();
    let count: u8 = kani::any();
    kani::assume(count <= 3);
    for _ in 0..count {
        input.item_candidates.push(any_text());
    }

    let decision = decide_next_action(&input);
    let several = input.item_candidates.len() > 1;

    if several {
        assert!(matches!(decision, Ok(NextActionDecision::SeveralItems)));
    } else {
        assert!(matches!(decision, Ok(NextActionDecision::Run { .. })));
    }
    assert_eq!(decided_executable(&decision), !several);
    kani::cover!(several);
    kani::cover!(input.item_candidates.len() == 1);
    kani::cover!(input.item_candidates.is_empty());
}
