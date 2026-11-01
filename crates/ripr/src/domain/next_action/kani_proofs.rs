//! Kani bounded-model-checking proofs for the canonical next-action
//! selector, the load-bearing actionability flip. Compiled only under
//! `cargo kani`; normal builds never see this file.
//!
//! Strings are drawn from a tiny fixed alphabet ("a" / "b" / `None`) chosen by
//! a symbolic index: the selector only compares and clones them, so equality
//! and presence/absence are the properties that matter. Booleans, enum
//! variants and collection sizes are fully symbolic within the stated bounds.
//!
//! Not covered (CBMC runs out of memory on these refusal paths, so they stay
//! guarded by the selector's unit tests): the unsupported/unknown-platform
//! stop (a `format!` alternative label), the terminal-attempt and
//! awaiting-edit attempt stops (`format!`), the restart-recommended stop, and
//! the several-unranked-items stop (candidate `collect`). Each was attempted
//! as its own harness and ran past 12 minutes and 4 GB or failed with an
//! out-of-memory error.

use super::super::command_spec::{
    CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, EnvironmentPolicy,
    ExpectedResultParser, NetworkPolicy, StdinPolicy,
};
use super::*;

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
