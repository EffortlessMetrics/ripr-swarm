//! Per-finding verdict lines for `-v`/`--verbose` and `RIPR_VERBOSE`.
//!
//! When a run looks wrong, the first question is why each finding landed in
//! its class. These lines print, for every finding the run reports (after
//! suppression and owner opt-outs), the class, the five static stage states,
//! the first stage that is not `yes` with the reason ripr recorded for it,
//! and any analysis stop reason. They read only fields the finding already
//! carries, and they are printed only when stderr diagnostics are on: a
//! default run neither formats nor prints them.

use crate::domain::{Finding, StageEvidence, StageState};
use crate::terminal_text::terminal_safe;

/// Most findings traced one line each. A larger run names how many more
/// exist and where to read them, so a huge diff cannot flood stderr.
pub(crate) const MAX_VERDICT_LINES: usize = 50;

/// The longest stage reason printed on a verdict line, in characters.
const MAX_REASON_CHARS: usize = 200;

/// One stderr line per finding (bounded by [`MAX_VERDICT_LINES`]), plus an
/// overflow line when findings were left out.
pub(crate) fn verdict_lines(findings: &[&Finding]) -> Vec<String> {
    let mut lines: Vec<String> = findings
        .iter()
        .take(MAX_VERDICT_LINES)
        .copied()
        .map(verdict_line)
        .collect();
    if findings.len() > MAX_VERDICT_LINES {
        lines.push(format!(
            "ripr: verdict: {} more finding(s) not traced here; `ripr explain <file:line>` shows the stage evidence for one",
            findings.len() - MAX_VERDICT_LINES
        ));
    }
    lines
}

fn verdict_line(finding: &Finding) -> String {
    let ripr = &finding.ripr;
    let stages: [(&str, &StageEvidence); 5] = [
        ("reach", &ripr.reach),
        ("infect", &ripr.infect),
        ("propagate", &ripr.propagate),
        ("observe", &ripr.reveal.observe),
        ("discriminate", &ripr.reveal.discriminate),
    ];
    let states = stages
        .iter()
        .map(|(name, stage)| format!("{name}={}", stage.state.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let mut line = format!(
        "ripr: verdict {}:{} {} [{states}]",
        finding.probe.location.file.display(),
        finding.probe.location.line,
        finding.class.as_str(),
    );
    if let Some((name, stage)) = stages
        .iter()
        .find(|(_, stage)| !matches!(stage.state, StageState::Yes | StageState::NotApplicable))
    {
        let reason = one_line_bounded(&stage.summary);
        if !reason.is_empty() {
            line.push_str(&format!("; first stage short of yes: {name}: {reason}"));
        }
    }
    if !finding.stop_reasons.is_empty() {
        let stops = finding
            .stop_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&format!("; stopped: {stops}"));
    }
    terminal_safe(line)
}

/// Collapse a recorded reason onto one line and bound its length, so one
/// finding can never print more than one stderr line.
fn one_line_bounded(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_REASON_CHARS {
        return collapsed;
    }
    let mut bounded: String = collapsed.chars().take(MAX_REASON_CHARS).collect();
    bounded.push('…');
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Confidence, ExposureClass, RevealEvidence, RiprEvidence, StopReason};

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn finding(class: ExposureClass, ripr: RiprEvidence) -> Finding {
        let mut finding = crate::app::tests::sample_finding("src/lib.rs", 7);
        finding.class = class;
        finding.ripr = ripr;
        finding.stop_reasons = Vec::new();
        finding
    }

    fn evidence(reach: StageEvidence, observe: StageEvidence) -> RiprEvidence {
        RiprEvidence {
            reach,
            infect: stage(StageState::Yes, "infected"),
            propagate: stage(StageState::NotApplicable, ""),
            reveal: RevealEvidence {
                observe,
                discriminate: stage(StageState::Unknown, "later reason"),
            },
        }
    }

    #[test]
    fn a_verdict_line_names_class_states_and_the_first_unproven_stage() {
        let f = finding(
            ExposureClass::WeaklyExposed,
            evidence(
                stage(StageState::Yes, "a test calls the owner"),
                stage(StageState::Weak, "the test only checks\n  is_ok()"),
            ),
        );
        assert_eq!(
            verdict_line(&f),
            "ripr: verdict src/lib.rs:7 weakly_exposed [reach=yes infect=yes propagate=not_applicable observe=weak discriminate=unknown]; first stage short of yes: observe: the test only checks is_ok()"
        );
    }

    #[test]
    fn a_verdict_line_appends_stop_reasons_and_skips_an_empty_reason() {
        let mut f = finding(
            ExposureClass::NoStaticPath,
            evidence(stage(StageState::No, ""), stage(StageState::Unknown, "x")),
        );
        f.stop_reasons = vec![StopReason::MaxDepthReached];
        let line = verdict_line(&f);
        assert!(line.ends_with("[reach=no infect=yes propagate=not_applicable observe=unknown discriminate=unknown]; stopped: max_depth_reached"), "{line}");
        assert!(!line.contains("first stage short"), "{line}");
    }

    #[test]
    fn verdict_lines_are_bounded_and_name_the_overflow() {
        let one = finding(
            ExposureClass::Exposed,
            evidence(stage(StageState::Yes, ""), stage(StageState::Yes, "")),
        );
        assert!(verdict_lines(&[]).is_empty());
        let few = vec![&one; 3];
        assert_eq!(verdict_lines(&few).len(), 3);
        let many = vec![&one; MAX_VERDICT_LINES + 4];
        let lines = verdict_lines(&many);
        assert_eq!(lines.len(), MAX_VERDICT_LINES + 1);
        assert!(
            lines[MAX_VERDICT_LINES].starts_with("ripr: verdict: 4 more finding(s) not traced"),
            "{}",
            lines[MAX_VERDICT_LINES]
        );
    }

    #[test]
    fn a_long_or_hostile_reason_stays_one_bounded_terminal_safe_line() {
        let long = "word ".repeat(100);
        let f = finding(
            ExposureClass::ReachableUnrevealed,
            evidence(
                stage(StageState::Yes, ""),
                stage(StageState::No, &format!("{long}\u{1b}[31m")),
            ),
        );
        let line = verdict_line(&f);
        assert!(!line.contains('\n'), "{line}");
        assert!(!line.contains('\u{1b}'), "{line}");
        assert!(line.ends_with('…'), "{line}");
    }
}
