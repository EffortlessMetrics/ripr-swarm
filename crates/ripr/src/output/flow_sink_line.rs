//! Shared flow-sink evidence row used by the human explain surface and the
//! machine JSON evidence path.

use crate::domain::{Finding, FlowSinkFact, FlowSinkKind, ProbeFamily};
use std::borrow::Cow;

/// Evidence-row line for one flow sink, e.g. `local flow reaches returned
/// value: amount - 10 (line 3)`.
///
/// When the sink text restates the kind label verbatim — the unresolvable
/// `unknown sink` case — the `: text` tail is collapsed so the row does not
/// render tautologically (`unknown sink: unknown sink`, #7798).
pub(crate) fn flow_sink_evidence_line(sink: &FlowSinkFact) -> String {
    let label = sink.kind.label();
    if sink.text == label {
        format!("local flow reaches {label} (line {})", sink.line)
    } else {
        format!(
            "local flow reaches {label}: {} (line {})",
            sink.text, sink.line
        )
    }
}

/// Plain-meaning gloss for a sink kind whose label is producer vocabulary,
/// the same way `stop_reason_meaning` glosses stop reasons (#8744). Only
/// `Unknown` needs one: every classified kind renders as plain words
/// ("returned value", "constructed field", "state write"), while `unknown
/// sink` names the producer's failure to classify, not a property of the
/// code. Human surfaces append this; the JSON evidence strings stay raw so
/// the machine shape does not absorb prose.
pub(crate) fn flow_sink_kind_meaning(kind: &FlowSinkKind) -> Option<&'static str> {
    match kind {
        FlowSinkKind::Unknown => Some(
            "ripr could not classify where the changed value ends up; \
             propagation confidence is limited",
        ),
        _ => None,
    }
}

/// #14420: the arm result the flow model read for a head-only match-arm
/// finding (`Size::Medium =>` gives `450`), so a surface can name the value
/// a test must pin. The probe must be a match arm whose expression is one
/// head (`=>` counted on comment/string-masked text, so `"a=>b" =>` is one
/// head), and the `MatchArm` sink must sit on the probe's line with text
/// other than the head. Anything else is `None`.
pub(crate) fn head_only_arm_result(finding: &Finding) -> Option<&str> {
    let head = finding.probe.expression.trim();
    let is_head = finding.probe.family == ProbeFamily::MatchArm
        && head.ends_with("=>")
        && crate::analysis::mask_comments_and_strings(head)
            .matches("=>")
            .count()
            == 1;
    if !is_head {
        return None;
    }
    finding
        .flow_sinks
        .iter()
        .find(|sink| {
            sink.kind == FlowSinkKind::MatchArm
                && sink.line == finding.probe.location.line
                && !sink.text.trim().is_empty()
                && sink.text.trim() != head
        })
        .map(|sink| sink.text.trim())
}

/// `text` joined to [`head_only_arm_result`] when `text` is the probe's own
/// head (`Size::Medium =>` becomes `Size::Medium => 450`). The result was
/// read for that head, so a different head (an old `before` arm) is never
/// paired with it, and any other text is returned unchanged.
pub(crate) fn with_arm_result<'a>(finding: &Finding, text: &'a str) -> Cow<'a, str> {
    let head = text.trim();
    let compact = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    match head_only_arm_result(finding)
        .filter(|_| compact(head) == compact(&finding.probe.expression))
    {
        Some(result) => Cow::Owned(format!("{head} {result}")),
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FlowSinkFact, FlowSinkKind};

    fn sink(kind: FlowSinkKind, text: &str) -> FlowSinkFact {
        FlowSinkFact {
            kind,
            text: text.to_string(),
            line: 3,
            owner: None,
        }
    }

    #[test]
    fn distinct_text_keeps_label_text_tail() {
        let line = flow_sink_evidence_line(&sink(FlowSinkKind::ReturnValue, "amount - 10"));
        assert_eq!(
            line,
            "local flow reaches returned value: amount - 10 (line 3)"
        );
    }

    #[test]
    fn text_restating_label_collapses_duplication() {
        let line = flow_sink_evidence_line(&sink(FlowSinkKind::Unknown, "unknown sink"));
        assert_eq!(line, "local flow reaches unknown sink (line 3)");
        assert!(!line.contains("unknown sink: unknown sink"));
    }

    #[test]
    fn unknown_kind_carries_a_meaning_gloss() {
        // #8744: the label names the producer's failure to classify, not a
        // property of the code; the human surfaces append this meaning.
        assert_eq!(
            flow_sink_kind_meaning(&FlowSinkKind::Unknown),
            Some(
                "ripr could not classify where the changed value ends up; \
                 propagation confidence is limited"
            )
        );
        assert_eq!(flow_sink_kind_meaning(&FlowSinkKind::ReturnValue), None);
    }
}
