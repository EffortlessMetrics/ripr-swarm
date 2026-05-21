use super::{CheckInput, CheckOutput};
use crate::analysis::AnalysisResult;

pub(super) fn check_output_from_analysis(
    input: CheckInput,
    analysis: AnalysisResult,
) -> CheckOutput {
    CheckOutput {
        schema_version: "0.1".to_string(),
        tool: "ripr".to_string(),
        mode: input.mode,
        root: input.root,
        base: input.base,
        summary: analysis.summary,
        findings: analysis.findings,
    }
}
