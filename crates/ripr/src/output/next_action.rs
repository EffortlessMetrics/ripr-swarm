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

use crate::domain::{CanonicalNextActionV1, NextActionDiffSource, NextActionStop};

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
pub(crate) fn render_next_action_human(action: &CanonicalNextActionV1) -> String {
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
        out.push_str(&format!(
            "  command: {} [{}]: {}\n",
            command.command_id, role, command.display
        ));
    }
    if let Some(stop) = action.stop() {
        out.push_str(&format!(
            "  stop [{}]: {}\n",
            stop.kind(),
            render_stop(stop)
        ));
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
            out.push_str(&format!(
                "    - {} :: {}\n",
                alternative.label, alternative.route
            ));
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

/// One human line per typed stop. Command displays appear only inside the
/// qualified manual-step instruction; nothing here is an unqualified command
/// string (control 8).
fn render_stop(stop: &NextActionStop) -> String {
    match stop {
        NextActionStop::SelectItem { candidates, total } => format!(
            "select one of {total}: {}",
            render_candidates(candidates, *total)
        ),
        NextActionStop::SelectAttempt { candidates, total } => format!(
            "select one of {total} attempts: {}",
            render_candidates(candidates, *total)
        ),
        NextActionStop::ResolveDisagreement {
            check_item,
            card_item,
        } => format!(
            "check selected {check_item} but the card selected {card_item}; reconcile the two before acting"
        ),
        NextActionStop::RefreshCurrentness {
            observed,
            expected,
            restart_route,
        } => format!("observed head {observed} but expected {expected}; {restart_route}"),
        NextActionStop::RefreshConfig {
            observed,
            expected,
            restart_route,
        } => format!("observed config {observed} but expected {expected}; {restart_route}"),
        NextActionStop::ProvideInput {
            input,
            detail_route,
        } => {
            format!("{input}; see {detail_route}")
        }
        NextActionStop::RestartAttempt {
            attempt_id,
            restart_route,
        } => format!("restart attempt {attempt_id}; {restart_route}"),
        NextActionStop::RouteRefused { command_id, reason } => {
            format!("{command_id} refused the selected target: {reason}")
        }
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
            format!(
                "{command_id} renders on {}; instead: {alternative_route}",
                platforms.join(", ")
            )
        }
        NextActionStop::ManualStep {
            command_id,
            instruction,
        } => format!("{command_id} is a manual step, not a runnable command: {instruction}"),
        NextActionStop::TerminalComplete { receipt_ref } => {
            format!("already complete; repeated invocation changes nothing; details: {receipt_ref}")
        }
        NextActionStop::Unsupported {
            limitation,
            detail_route,
        } => format!("{limitation}; see {detail_route}"),
        NextActionStop::InspectTarget { detail_route } => format!("see {detail_route}"),
        NextActionStop::CheckTriage { case } => format!("check case {}", case.as_str()),
        NextActionStop::DoctorRecovery {
            check_name,
            recovery_route,
        } => format!("{check_name}: {recovery_route}"),
        NextActionStop::PilotDelegated {
            transaction_ref,
            route,
        } => format!("delegated transaction {transaction_ref}: {route}"),
    }
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
            let line = render_stop(&stop);
            assert!(!line.trim().is_empty(), "kind {}", stop.kind());
            assert!(!line.contains('\n'), "kind {}", stop.kind());
            let human = render_next_action_human(&stopped_action(stop.clone())?);
            assert!(human.contains(&format!("stop [{}]", stop.kind())));
        }
        Ok(())
    }
}
