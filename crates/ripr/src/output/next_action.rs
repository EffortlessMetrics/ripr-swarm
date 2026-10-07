//! Human and JSON projections of [`CanonicalNextActionV1`].
//!
//! Both projections derive from the same normalized DTO: JSON serializes it,
//! and the human block formats the same fields in the same order. Control 9
//! pins their agreement on subject, class, command identity, reason, and
//! limitations, and control 8 pins that no projection emits a command string
//! for a non-executable action.
//!
//! Producer adapters live with their producers: the check adapter in
//! [`crate::output::human`] triage, the card adapter in
//! [`crate::app::repair_card`], the status adapter in
//! [`crate::app::agent_status`].

use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::domain::{CanonicalNextActionV1, NextActionDiffSource, NextActionStop};
use std::path::Path;

/// The machine projection as a JSON value for envelope embedding. A DTO
/// that cannot serialize degrades to null rather than breaking the envelope.
pub(crate) fn render_next_action_json_value(action: &CanonicalNextActionV1) -> serde_json::Value {
    serde_json::to_value(action).unwrap_or(serde_json::Value::Null)
}

/// Render the machine projection as pretty JSON with one trailing newline,
/// the same normalization every renderer emits. Envelopes embed
/// [`render_next_action_json_value`]; this is the standalone document form.
#[cfg(test)]
fn render_next_action_json(action: &CanonicalNextActionV1) -> Result<String, String> {
    serde_json::to_string_pretty(action)
        .map(|mut rendered| {
            rendered.push('\n');
            rendered
        })
        .map_err(|error| format!("failed to render canonical next action JSON: {error}"))
}

/// Render the human projection: the same DTO fields as a deterministic
/// `next action` block in card-prose style. Executable actions name their
/// referenced command; stopped actions name their typed stop. No branch
/// emits a command display for a non-executable action (control 8).
///
/// Route strings keep their portable DTO spelling here. CLI surfaces that
/// print this block for copy/paste must use
/// [`render_next_action_human_at_root`] so every rendered `ripr` route binds
/// the selected root (#6304 paste-robustness).
pub(crate) fn render_next_action_human(action: &CanonicalNextActionV1) -> String {
    render_next_action_human_inner(action, None)
}

/// [`render_next_action_human`] with every rendered `ripr` route bound to
/// `root` (#3999): a pasted route analyzes the repository that was selected
/// when it was rendered, not the directory it is later pasted into. Already
/// root-bound displays pass through unchanged.
pub(crate) fn render_next_action_human_at_root(
    action: &CanonicalNextActionV1,
    root: &Path,
) -> String {
    let bound = bound_root(&root.to_string_lossy());
    render_next_action_human_inner(action, Some(bound.as_str()))
}

fn render_next_action_human_inner(action: &CanonicalNextActionV1, bound: Option<&str>) -> String {
    let mut out = format!("  next action: {}\n", action.action_class().as_str());
    out.push_str(&format!("  producer: {}\n", action.producer().as_str()));
    let subject = action.subject();
    let item = subject.item.as_deref().unwrap_or("<selection required>");
    out.push_str(&format!(
        "  subject: {} @ {} ({})\n",
        item,
        subject.root,
        render_diff_source(&subject.diff_source)
    ));
    if let Some(command) = action.command() {
        let role = serde_json::to_value(command.role)
            .ok()
            .and_then(|role| role.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let display = bind_route_root(&command.display, bound);
        out.push_str(&format!(
            "  command: {} [{}]: {}\n",
            command.command_id, role, display
        ));
        if bound.is_some()
            && let Some(pair) = powershell_pair_line(&display)
        {
            out.push_str(&format!("{pair}\n"));
        }
    }
    if let Some(stop) = action.stop() {
        let mut rendered = render_stop(stop, bound);
        let first = rendered.remove(0);
        out.push_str(&format!("  stop [{}]: {}\n", stop.kind(), first));
        for extra in rendered {
            out.push_str(&format!("{extra}\n"));
        }
    }
    if let Some(transition) = action.expected_transition() {
        out.push_str(&format!(
            "  transition: {} -> {}\n",
            transition.from_state, transition.to_state
        ));
    }
    if !action.alternatives().is_empty() {
        out.push_str("  alternatives:\n");
        for alternative in action.alternatives() {
            let route = bind_route_root(&alternative.route, bound);
            out.push_str(&format!("    - {} :: {}\n", alternative.label, route));
            if bound.is_some()
                && let Some(pair) = powershell_pair_line(&route)
            {
                out.push_str(&format!("{pair}\n"));
            }
        }
    }
    if !action.limitations().is_empty() {
        out.push_str(&format!(
            "  limitations: {}\n",
            action.limitations().join("; ")
        ));
    }
    out.push_str(&format!("  non-claim: {}\n", action.non_claim()));
    out
}

fn render_diff_source(diff_source: &NextActionDiffSource) -> String {
    match diff_source {
        NextActionDiffSource::WorkingTree { head } => match head {
            Some(head) => format!("working tree @ {head}"),
            None => "working tree".to_string(),
        },
        NextActionDiffSource::Committed { base, head } => match (base, head) {
            (Some(base), Some(head)) => format!("committed {base}..{head}"),
            (Some(base), None) => format!("committed base {base}"),
            (None, Some(head)) => format!("committed @ {head}"),
            (None, None) => "committed".to_string(),
        },
    }
}

/// Bind a rendered `ripr` route to the selected root for copy/paste (#3999).
/// Portable DTO routes (`ripr <sub> ...` with no `--root`) gain
/// `--root <bound>` after the subcommand (`ripr agent <sub>` routes bind
/// after the nested subcommand); already-bound routes and non-command
/// strings pass through unchanged. Fail-closed: an unrecognized shape is
/// returned verbatim rather than spliced into a different command.
fn bind_route_root(route: &str, bound: Option<&str>) -> String {
    let Some(bound) = bound else {
        return route.to_string();
    };
    let trimmed = route.trim();
    if !trimmed.starts_with("ripr ") {
        return route.to_string();
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.contains(&"--root") {
        return route.to_string();
    }
    let after_words = if tokens.get(1) == Some(&"agent") {
        3
    } else {
        2
    };
    if tokens.len() < after_words {
        return route.to_string();
    }
    // Splice at a byte offset instead of rejoining tokens: quoted arguments
    // may carry significant interior whitespace that a split/join would
    // collapse.
    let mut offset = 0;
    for _ in 0..after_words {
        while trimmed[offset..].starts_with(char::is_whitespace) {
            offset += 1;
        }
        while offset < trimmed.len() && !trimmed[offset..].starts_with(char::is_whitespace) {
            offset += trimmed[offset..]
                .chars()
                .next()
                .map_or(1, |ch| ch.len_utf8());
        }
    }
    if offset == 0 || offset > trimmed.len() {
        return route.to_string();
    }
    format!(
        "{} --root {}{}",
        &trimmed[..offset],
        shell_arg(bound),
        &trimmed[offset..]
    )
}

/// The paired PowerShell form of one rendered route for plain-text paste
/// surfaces: the same shared translator as every other command surface
/// ([`crate::output::markdown::powershell_text_variant`]). `None` when
/// PowerShell runs the bash spelling unchanged.
fn powershell_pair_line(route: &str) -> Option<String> {
    crate::output::markdown::powershell_text_variant(route)
        .map(|form| format!("  (PowerShell) {form}"))
}

/// One human line per typed stop, plus a paired `(PowerShell)` line when a
/// rendered route needs a different spelling there. Command displays appear
/// only inside the qualified manual-step instruction; nothing here is an
/// unqualified command string (control 8).
fn render_stop(stop: &NextActionStop, bound: Option<&str>) -> Vec<String> {
    let (text, route) = match stop {
        NextActionStop::SelectItem { candidates, total } => (
            format!(
                "select one of {total}: {}",
                render_candidates(candidates, *total)
            ),
            None,
        ),
        NextActionStop::SelectAttempt { candidates, total } => (
            format!(
                "select one of {total} attempts: {}",
                render_candidates(candidates, *total)
            ),
            None,
        ),
        NextActionStop::ResolveDisagreement {
            check_item,
            card_item,
        } => (
            format!(
                "check selected {check_item} but the card selected {card_item}; reconcile the two before acting"
            ),
            None,
        ),
        NextActionStop::RefreshCurrentness {
            observed,
            expected,
            restart_route,
        } => {
            let route = bind_route_root(restart_route, bound);
            (
                format!("observed head {observed} but expected {expected}; {route}"),
                Some(route),
            )
        }
        NextActionStop::RefreshConfig {
            observed,
            expected,
            restart_route,
        } => {
            let route = bind_route_root(restart_route, bound);
            (
                format!("observed config {observed} but expected {expected}; {route}"),
                Some(route),
            )
        }
        NextActionStop::ProvideInput {
            input,
            detail_route,
        } => {
            let route = bind_route_root(detail_route, bound);
            (format!("{input}; see {route}"), Some(route))
        }
        NextActionStop::RestartAttempt {
            attempt_id,
            restart_route,
        } => {
            let route = bind_route_root(restart_route, bound);
            (
                format!("restart attempt {attempt_id}; {route}"),
                Some(route),
            )
        }
        NextActionStop::RouteRefused { command_id, reason } => (
            format!("{command_id} refused the selected target: {reason}"),
            None,
        ),
        NextActionStop::PlatformUnavailable {
            command_id,
            supported_platforms,
            alternative_route,
        } => {
            let platforms: Vec<String> = supported_platforms
                .iter()
                .map(|platform| {
                    serde_json::to_value(platform)
                        .ok()
                        .and_then(|platform| platform.as_str().map(str::to_string))
                        .unwrap_or_else(|| "unknown".to_string())
                })
                .collect();
            let route = bind_route_root(alternative_route, bound);
            (
                format!(
                    "{command_id} renders on {}; instead: {route}",
                    platforms.join(", ")
                ),
                Some(route),
            )
        }
        NextActionStop::ManualStep {
            command_id,
            instruction,
        } => (
            format!("{command_id} is a manual step, not a runnable command: {instruction}"),
            None,
        ),
        NextActionStop::TerminalComplete { receipt_ref } => (
            format!(
                "already complete; repeated invocation changes nothing; details: {receipt_ref}"
            ),
            None,
        ),
        NextActionStop::Unsupported {
            limitation,
            detail_route,
        } => {
            let route = bind_route_root(detail_route, bound);
            (format!("{limitation}; see {route}"), Some(route))
        }
        NextActionStop::InspectTarget { detail_route } => {
            let route = bind_route_root(detail_route, bound);
            (format!("see {route}"), Some(route))
        }
        NextActionStop::CheckTriage { case } => (format!("check case {}", case.as_str()), None),
        NextActionStop::DoctorRecovery {
            check_name,
            recovery_route,
        } => {
            let route = bind_route_root(recovery_route, bound);
            (format!("{check_name}: {route}"), Some(route))
        }
        NextActionStop::PilotDelegated {
            transaction_ref,
            route,
        } => {
            let route = bind_route_root(route, bound);
            (
                format!("delegated transaction {transaction_ref}: {route}"),
                Some(route),
            )
        }
    };
    let mut lines = vec![text];
    // Paste surfaces pair the bash route with its PowerShell spelling on
    // the next line; portable renders carry no paste target.
    if bound.is_some()
        && let Some(route) = route
        && let Some(pair) = powershell_pair_line(&route)
    {
        lines.push(pair);
    }
    lines
}

fn render_candidates(candidates: &[String], total: usize) -> String {
    if candidates.len() >= total {
        candidates.join(", ")
    } else {
        format!(
            "{} (and {} more)",
            candidates.join(", "),
            total - candidates.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CommandPlatform, CommandRole, NextActionAlternative, NextActionCommandRef,
        NextActionCurrentness, NextActionProducer, NextActionSubject, NextActionTransition,
    };

    fn test_subject(item: Option<&str>) -> NextActionSubject {
        NextActionSubject {
            root: "/repo".to_string(),
            diff_source: NextActionDiffSource::Committed {
                base: Some("base1".to_string()),
                head: Some("head1".to_string()),
            },
            item: item.map(str::to_string),
        }
    }

    fn test_currentness() -> NextActionCurrentness {
        NextActionCurrentness {
            head_expected: Some("head1".to_string()),
            head_observed: Some("head1".to_string()),
            config_expected: None,
            config_observed: None,
        }
    }

    fn run_action() -> Result<CanonicalNextActionV1, String> {
        CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(Some("seam:demo")),
            test_currentness(),
            crate::domain::NextActionClass::RunCommand,
            Some(NextActionCommandRef {
                command_id: "ripr:agent:packet".to_string(),
                role: CommandRole::Inspection,
                display: "ripr agent packet --seam-id seam:demo --json".to_string(),
            }),
            None,
            Some(NextActionTransition {
                from_state: "fix_site_ready".to_string(),
                to_state: "packet_inspected".to_string(),
            }),
            Vec::new(),
            vec!["advisory only".to_string()],
        )
    }

    fn stopped_action(stop: NextActionStop) -> Result<CanonicalNextActionV1, String> {
        // Render tests pair each stop with a constructor-valid class; the
        // selector's own pairing is pinned in the domain battery.
        let class = match &stop {
            NextActionStop::SelectItem { .. } => crate::domain::NextActionClass::ChooseItem,
            NextActionStop::SelectAttempt { .. } => crate::domain::NextActionClass::ChooseAttempt,
            _ => crate::domain::NextActionClass::InspectDetails,
        };
        let chooses = matches!(
            class,
            crate::domain::NextActionClass::ChooseItem
                | crate::domain::NextActionClass::ChooseAttempt
        );
        CanonicalNextActionV1::new(
            NextActionProducer::RepairAttemptStatus,
            test_subject(if chooses { None } else { Some("seam:demo") }),
            test_currentness(),
            class,
            None,
            Some(stop),
            None,
            vec![NextActionAlternative {
                label: "receipt".to_string(),
                route: "attempt.json#receipt".to_string(),
            }],
            vec!["head was reread".to_string()],
        )
    }

    #[test]
    fn control_9_human_and_json_agree_from_one_dto() -> Result<(), String> {
        let actions = vec![
            run_action()?,
            stopped_action(NextActionStop::RouteRefused {
                command_id: "ripr:agent:after".to_string(),
                reason: "head moved".to_string(),
            })?,
            stopped_action(NextActionStop::TerminalComplete {
                receipt_ref: "attempt.json#receipt".to_string(),
            })?,
        ];
        for action in &actions {
            let json = render_next_action_json(action)?;
            assert!(json.ends_with('\n') && !json.ends_with("\n\n"));
            let value: serde_json::Value = serde_json::from_str(&json)
                .map_err(|error| format!("canonical JSON must parse: {error}"))?;
            assert_eq!(
                value
                    .get("schema_version")
                    .and_then(|version| version.as_str()),
                Some("canonical_next_action.v1")
            );
            assert_eq!(
                value.get("action_class").and_then(|class| class.as_str()),
                Some(action.action_class().as_str())
            );
            let human = render_next_action_human(action);
            // Subject, class, command identity, reason, and limitations
            // agree across both projections.
            let subject = action.subject();
            assert_eq!(
                value
                    .pointer("/subject/root")
                    .and_then(|root| root.as_str()),
                Some(subject.root.as_str())
            );
            assert!(human.contains(&subject.root));
            if let Some(item) = &subject.item {
                assert_eq!(
                    value
                        .pointer("/subject/item")
                        .and_then(|item| item.as_str()),
                    Some(item.as_str())
                );
                assert!(human.contains(item));
            }
            assert!(human.contains(action.action_class().as_str()));
            match action.command() {
                Some(command) => {
                    assert_eq!(
                        value
                            .pointer("/command/command_id")
                            .and_then(|id| id.as_str()),
                        Some(command.command_id.as_str())
                    );
                    assert!(human.contains(&command.command_id));
                    assert!(human.contains(&command.display));
                }
                None => {
                    assert!(value.get("command").is_none());
                }
            }
            match action.stop() {
                Some(stop) => {
                    assert_eq!(
                        value.pointer("/stop/kind").and_then(|kind| kind.as_str()),
                        Some(stop.kind())
                    );
                    assert!(human.contains(stop.kind()));
                }
                None => {
                    assert!(value.get("stop").is_none());
                }
            }
            for limitation in action.limitations() {
                assert!(
                    value
                        .get("limitations")
                        .and_then(|limitations| limitations.as_array())
                        .is_some_and(|limitations| limitations
                            .iter()
                            .any(|entry| entry.as_str() == Some(limitation.as_str())))
                );
                assert!(human.contains(limitation));
            }
        }
        Ok(())
    }

    #[test]
    fn control_8_stopped_renders_omit_command_strings() -> Result<(), String> {
        let marker = "DO-NOT-EMIT-AS-COMMAND";
        let action = CanonicalNextActionV1::new(
            NextActionProducer::RepairCard,
            test_subject(Some("seam:demo")),
            test_currentness(),
            crate::domain::NextActionClass::UnsupportedOrLimited,
            None,
            Some(NextActionStop::PlatformUnavailable {
                command_id: "ripr:agent:packet".to_string(),
                supported_platforms: vec![CommandPlatform::Windows],
                alternative_route: "ripr agent packet --seam-id seam:demo --json".to_string(),
            }),
            None,
            Vec::new(),
            Vec::new(),
        )?;
        // The offered spec's display carried the marker; the stop must not
        // repeat it anywhere.
        let json = render_next_action_json(&action)?;
        let human = render_next_action_human(&action);
        assert!(!json.contains(marker), "{json}");
        assert!(!human.contains(marker), "{human}");
        assert!(human.contains("platform_unavailable"));
        assert!(human.contains("instead:"));

        // Manual instructions render qualified, never as a runnable command.
        let manual = stopped_action(NextActionStop::ManualStep {
            command_id: "ripr:agent:packet".to_string(),
            instruction: "open the packet and read".to_string(),
        })?;
        let human = render_next_action_human(&manual);
        assert!(human.contains("manual step, not a runnable command"));
        Ok(())
    }

    #[test]
    fn bound_render_binds_rootless_packet_routes_for_paste() -> Result<(), String> {
        // #6304 paste-robustness: a stopped card action embeds the portable
        // packet route in its stop text; the CLI-bound render must splice the
        // selected root in (#3999) so a foreign paste still analyzes the
        // selected repository.
        let rootless = "ripr agent packet --seam-id seam:demo --json".to_string();
        let action = stopped_action(NextActionStop::ProvideInput {
            input: "repair readiness for this seam".to_string(),
            detail_route: rootless.clone(),
        })?;
        let portable = render_next_action_human(&action);
        assert!(portable.contains(&rootless), "{portable}");
        let bound = render_next_action_human_at_root(&action, Path::new("/repo/checkout"));
        let expected = format!(
            "ripr agent packet --root {} --seam-id seam:demo --json",
            bound_root("/repo/checkout")
        );
        assert!(bound.contains(&expected), "{bound}");
        assert!(!bound.contains(&format!("; see {rootless}")), "{bound}");
        // Interior whitespace inside quoted arguments survives the splice
        // byte-for-byte.
        let spaced = stopped_action(NextActionStop::ProvideInput {
            input: "readiness".to_string(),
            detail_route: "ripr agent packet  --seam-id 'a  b' --json".to_string(),
        })?;
        let bound = render_next_action_human_at_root(&spaced, Path::new("/repo/checkout"));
        assert!(bound.contains("--seam-id 'a  b' --json"), "{bound}");
        // A hostile root (apostrophe) pairs the bash route with its
        // PowerShell spelling on the next line, the plain-text convention.
        let hostile = render_next_action_human_at_root(&action, Path::new("/repo/check'out"));
        assert!(hostile.contains("(PowerShell)"), "{hostile}");
        assert!(!render_next_action_human(&action).contains("(PowerShell)"));
        Ok(())
    }

    #[test]
    fn bound_render_leaves_bound_and_non_command_strings_unchanged() -> Result<(), String> {
        let action = stopped_action(NextActionStop::Unsupported {
            limitation: "limited".to_string(),
            detail_route: "ripr agent packet --root /repo --seam-id seam:demo --json".to_string(),
        })?;
        let bound = render_next_action_human_at_root(&action, Path::new("/repo/checkout"));
        assert!(
            bound.contains("ripr agent packet --root /repo --seam-id seam:demo --json"),
            "{bound}"
        );
        // Receipt routes and recovery instructions are not `ripr` commands.
        assert!(bound.contains("attempt.json#receipt"), "{bound}");
        let recovery = stopped_action(NextActionStop::DoctorRecovery {
            check_name: "git".to_string(),
            recovery_route: "install git and rerun ripr doctor".to_string(),
        })?;
        let bound = render_next_action_human_at_root(&recovery, Path::new("/repo/checkout"));
        assert!(
            bound.contains("install git and rerun ripr doctor"),
            "{bound}"
        );
        Ok(())
    }

    #[test]
    fn human_block_shape_is_stable() -> Result<(), String> {
        let human = render_next_action_human(&run_action()?);
        let lines: Vec<&str> = human.lines().collect();
        assert_eq!(lines[0], "  next action: run_command");
        assert_eq!(lines[1], "  producer: repair_card");
        assert_eq!(
            lines[2],
            "  subject: seam:demo @ /repo (committed base1..head1)"
        );
        assert_eq!(
            lines[3],
            "  command: ripr:agent:packet [inspection]: ripr agent packet --seam-id seam:demo --json"
        );
        assert_eq!(lines[4], "  transition: fix_site_ready -> packet_inspected");
        assert_eq!(lines[5], "  limitations: advisory only");
        assert!(lines[6].starts_with("  non-claim: "));
        Ok(())
    }

    #[test]
    fn every_stop_renders_one_nonempty_line() -> Result<(), String> {
        let stops = vec![
            NextActionStop::SelectItem {
                candidates: vec!["a".to_string(), "b".to_string()],
                total: 3,
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
                case: crate::domain::NextActionCheckCase::TopGap,
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
        for stop in stops {
            let lines = render_stop(&stop, None);
            assert_eq!(lines.len(), 1, "kind {}", stop.kind());
            let line = &lines[0];
            assert!(!line.trim().is_empty(), "kind {}", stop.kind());
            assert!(!line.contains('\n'), "kind {}", stop.kind());
            let human = render_next_action_human(&stopped_action(stop.clone())?);
            assert!(human.contains(&format!("stop [{}]", stop.kind())));
        }
        Ok(())
    }
}
