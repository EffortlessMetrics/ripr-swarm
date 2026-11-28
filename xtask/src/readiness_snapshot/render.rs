use super::model::{ReadinessDimension, ReadinessSnapshot};

pub(crate) fn render_json(snapshot: &ReadinessSnapshot) -> Result<String, String> {
    serde_json::to_string_pretty(snapshot)
        .map_err(|err| format!("failed to encode readiness snapshot JSON: {err}"))
}

pub(crate) fn render_markdown(snapshot: &ReadinessSnapshot) -> String {
    let mut body = String::from("# ripr readiness evidence snapshot\n\n");
    body.push_str(&format!("Source SHA: `{}`\n", snapshot.source_sha));
    body.push_str(&format!("Tree clean: {}\n", snapshot.tree_clean));
    body.push_str(&format!(
        "Semantic identity: `{}`\n\n",
        snapshot.semantic_identity_digest
    ));
    body.push_str("Mode: advisory multidimensional evidence; no ship verdict.\n\n");
    body.push_str("## Dimensions\n\n");
    for (name, dimension) in &snapshot.dimensions {
        body.push_str(&format_dimension(name, dimension));
    }
    if !snapshot.non_claims.is_empty() {
        body.push_str("\n## Non-claims\n\n");
        for claim in &snapshot.non_claims {
            body.push_str(&format!("- {claim}\n"));
        }
    }
    body
}

fn format_dimension(name: &str, dimension: &ReadinessDimension) -> String {
    let mut section = format!("### {name}\n\n");
    section.push_str(&format!("State: `{}`\n", dimension.state.as_str()));
    if !dimension.limitations.is_empty() {
        section.push_str("\nLimitations:\n\n");
        for item in &dimension.limitations {
            section.push_str(&format!("- {item}\n"));
        }
    }
    if !dimension.evidence.is_empty() {
        section.push_str("\nEvidence:\n\n");
        for record in &dimension.evidence {
            section.push_str(&format!(
                "- `{}` via `{}` ({})\n",
                record.producer, record.command, record.artifact
            ));
        }
    }
    if !dimension.next_proof.is_empty() {
        section.push_str("\nNext proof:\n\n");
        for item in &dimension.next_proof {
            section.push_str(&format!("- `{item}`\n"));
        }
    }
    section.push('\n');
    section
}

#[cfg(test)]
pub(crate) fn markdown_matches_json(snapshot: &ReadinessSnapshot, markdown: &str) -> bool {
    snapshot.dimensions.iter().all(|(name, dimension)| {
        markdown.contains(&format!("### {name}"))
            && markdown.contains(&format!("State: `{}`", dimension.state.as_str()))
    })
}
