//! Shared flow-sink evidence row used by the human explain surface and the
//! machine JSON evidence path.

use crate::domain::FlowSinkFact;

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
}
