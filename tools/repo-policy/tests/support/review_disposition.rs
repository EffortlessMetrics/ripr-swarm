//! Read the same static workflow-level jq program Actions supplies to each step.
pub fn program(workflow: &str) -> String {
    workflow
        .split_once("  REVIEW_DISPOSITION_JQ: |\n")
        .map(|(_, tail)| {
            tail.lines()
                .take_while(|line| line.is_empty() || line.starts_with("    "))
                .map(|line| line.strip_prefix("    ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

pub fn block_boundary_cases() -> Result<Vec<(String, String, bool)>, String> {
    renderer_cases(
        include_str!(
            "../../../../fixtures/convergence/native-late-admission/block-boundary-rendering.json"
        ),
        42,
    )
}

pub fn block_recognition_cases() -> Result<Vec<(String, String, bool)>, String> {
    renderer_cases(
        include_str!(
            "../../../../fixtures/convergence/native-late-admission/block-recognition-rendering.json"
        ),
        240,
    )
}

fn renderer_cases(contents: &str, count: usize) -> Result<Vec<(String, String, bool)>, String> {
    let fixture: serde_json::Value = serde_json::from_str(contents).map_err(|e| e.to_string())?;
    let cases = fixture["cases"].as_array().ok_or("block cases missing")?;
    if cases.len() != count {
        return Err(format!(
            "expected {count} nonempty renderer-backed block controls"
        ));
    }
    cases
        .iter()
        .map(|case| {
            Ok((
                case["case"]
                    .as_str()
                    .ok_or("case name missing")?
                    .to_string(),
                case["body"]
                    .as_str()
                    .ok_or("Markdown body missing")?
                    .to_string(),
                case["expected"].as_bool().ok_or("expectation missing")?,
            ))
        })
        .collect()
}
