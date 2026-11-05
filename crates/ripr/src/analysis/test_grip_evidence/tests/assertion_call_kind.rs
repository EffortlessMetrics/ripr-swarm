use super::*;
use crate::analysis::classifier::classify_probe;
use crate::domain::{DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation};

fn subject(assertion: &str, method_body: Option<&str>) -> String {
    let method_body = method_body.unwrap_or(
        "match value {\n\
             0 => Reduction::Empty,\n\
             _ => Reduction::Over(15),\n\
         }",
    );
    format!(
        "#[derive(Debug, PartialEq)]\n\
         pub enum Reduction {{ Over(u32), Empty }}\n\
         pub fn consume(value: u32) -> Reduction {{\n\
             match value {{\n\
                 0 => Reduction::Empty,\n\
                 _ => Reduction::Over(16),\n\
             }}\n\
         }}\n\
         pub struct Widget;\n\
         impl Widget {{\n\
             pub fn consume(&self, value: u32) -> Reduction {{\n\
                 {method_body}\n\
             }}\n\
         }}\n\
         pub fn bridge(value: u32) -> Reduction {{ consume(value) }}\n\
         pub fn reduce(value: u32) -> Reduction {{ Reduction::Over(value) }}\n\
         #[cfg(test)]\n\
         mod tests {{\n\
             use super::*;\n\
             #[test]\n\
             fn output_contract() {{\n\
                 let widget = Widget;\n\
                 {assertion}\n\
             }}\n\
         }}\n"
    )
}

#[test]
fn same_named_call_kinds_refuse_only_wrong_owner_repo_oracles() -> Result<(), String> {
    for (owner, assertion, method_body, credited) in [
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            None,
            false,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            None,
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(reduce(15), Reduction::Over(15));",
            None,
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            None,
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(bridge(1), Reduction::Over(16));",
            None,
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("consume(value)"),
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("let run = consume; run(value)"),
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("let run = bridge; let forward = || run(value); forward()"),
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            None,
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(Widget::consume(&widget, 1), Reduction::Over(15));",
            None,
            true,
        ),
    ] {
        let source = subject(assertion, method_body);
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let selected: Vec<_> = seams
            .iter()
            .filter(|seam| {
                seam.kind() == SeamKind::MatchArm
                    && seam.owner() == owner
                    && seam.expression() == "_ =>"
            })
            .collect();
        let [arm] = selected.as_slice() else {
            return Err(format!("expected one {owner} arm, got {}", selected.len()));
        };
        let full = evidence_for_seam(arm, &index);
        let context = CompactGripContext::new(&index);
        let compact = compact_evidence_for_seam(arm, &context);
        let expected = if credited {
            StageState::Yes
        } else {
            StageState::No
        };
        assert_eq!(full.related_tests.len(), 1, "{owner}: {assertion}");
        assert_eq!(full.observe.state, expected, "{owner}: {assertion}");
        assert_eq!(full.discriminate.state, expected, "{owner}: {assertion}");
        assert_eq!(compact.observe.state, expected, "{owner}: {assertion}");
        assert_eq!(compact.discriminate.state, expected, "{owner}: {assertion}");
        assert_eq!(
            full.related_tests[0].oracle_strength,
            if credited {
                OracleStrength::Strong
            } else {
                OracleStrength::None
            },
            "published {owner}: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn same_named_call_kinds_keep_diff_owner_observation_scoped() -> Result<(), String> {
    for (owner, assertion, observed) in [
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            false,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            true,
        ),
    ] {
        let source = subject(assertion, None);
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let arm = seams
            .iter()
            .find(|seam| {
                seam.kind() == SeamKind::MatchArm
                    && seam.owner() == owner
                    && seam.expression() == "_ =>"
            })
            .ok_or_else(|| format!("missing parsed match arm for {owner}"))?;
        let probe = Probe {
            id: ProbeId(format!("probe:12234:{owner}")),
            location: SourceLocation::new("src/lib.rs", arm.display_line(), 1),
            owner: Some(SymbolId(owner.to_string())),
            family: ProbeFamily::MatchArm,
            delta: DeltaKind::Control,
            before: None,
            after: Some("_ =>".to_string()),
            expression: "_ =>".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let finding = classify_probe(&probe, &index, true, None);
        assert_eq!(finding.related_tests.len(), 1, "{owner}: {assertion}");
        assert_eq!(
            finding.ripr.reveal.observe.state,
            if observed {
                StageState::Yes
            } else {
                StageState::No
            },
            "diff {owner}: {assertion}"
        );
    }
    Ok(())
}
