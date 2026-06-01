use serde_json::Value;

pub(super) fn normalize_suggested_assertion(value: &str) -> String {
    let prefix = "Add a focused test where ";
    let middle = " and assert the exact ";
    if let Some(rest) = value.strip_prefix(prefix)
        && let Some((condition, target)) = rest.split_once(middle)
    {
        return format!(
            "Assert the exact {} at {}.",
            trim_period(target),
            trim_period(condition)
        );
    }
    value.to_string()
}

pub(super) fn classification_from_sources(sources: &[(Option<&Value>, &[&str])]) -> Option<String> {
    string_from_sources(sources).map(|value| match value.as_str() {
        "weakly_gripped" => "weakly_exposed".to_string(),
        "strongly_gripped" => "exposed".to_string(),
        other => other.to_string(),
    })
}

pub(super) fn current_evidence_strength_from_sources(sources: &[Option<&Value>]) -> Option<String> {
    sources.iter().find_map(|source| {
        let source = (*source)?;
        string_from_sources(&[
            (Some(source), &["current_evidence_strength"]),
            (Some(source), &["evidence", "current_evidence_strength"]),
            (Some(source), &["selected", "current_evidence_strength"]),
        ])
    })
}

pub(super) fn current_evidence_strength_for_selection(
    repair_route: Option<&str>,
    classification: Option<&str>,
    seam_kind: Option<&str>,
) -> Option<String> {
    match repair_route.or(seam_kind) {
        Some("MissingOutputContract" | "AddOutputGolden" | "RegenerateArtifact") => Some(
            "Static evidence found changed user-facing output, but no checked output or golden proof is attached."
                .to_string(),
        ),
        Some(
            "MissingBoundaryAssertion" | "MissingValueAssertion" | "MissingErrorDiscriminator"
            | "AddBoundaryAssertion" | "AddTargetedAssertion" | "predicate_boundary",
        ) => Some(
            "Static evidence found related test context, but the current check is weak because the discriminator is missing."
                .to_string(),
        ),
        _ => match classification {
            Some("weakly_exposed") => Some(
                "Static evidence found related test context, but the current check is weak because the discriminator is missing."
                    .to_string(),
            ),
            Some("reachable_unrevealed") => Some(
                "Static evidence found reachable changed behavior, but no current check observes the changed result."
                    .to_string(),
            ),
            Some("no_static_path") => Some(
                "Static analysis did not find a current test path to the changed behavior."
                    .to_string(),
            ),
            Some("exposed") => Some(
                "Static evidence found a current check that appears to observe the changed behavior."
                    .to_string(),
            ),
            Some(kind @ ("static_unknown" | "infection_unknown" | "propagation_unknown")) => {
                Some(format!(
                    "Static evidence is `{kind}`; no runtime proof is claimed."
                ))
            }
            Some(other) => Some(format!(
                "Static evidence reported `{other}`; no runtime proof is claimed."
            )),
            None => None,
        },
    }
}

pub(super) fn string_from_sources(sources: &[(Option<&Value>, &[&str])]) -> Option<String> {
    sources
        .iter()
        .find_map(|(value, path)| value.and_then(|value| string_path(value, path)))
}

pub(super) fn u64_from_sources(sources: &[(Option<&Value>, &[&str])]) -> Option<u64> {
    sources
        .iter()
        .find_map(|(value, path)| value.and_then(|value| u64_path(value, path)))
}

pub(super) fn bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    path_value(value, path).and_then(Value::as_bool)
}

pub(super) fn string_path(value: &Value, path: &[&str]) -> Option<String> {
    path_value(value, path).and_then(value_as_string)
}

pub(super) fn u64_path(value: &Value, path: &[&str]) -> Option<u64> {
    path_value(value, path).and_then(Value::as_u64)
}

pub(super) fn path_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        if let Ok(index) = key.parse::<usize>() {
            current = current.get(index)?;
        } else {
            current = current.get(*key)?;
        }
    }
    Some(current)
}

pub(super) fn value_as_string(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(number) = value.as_i64() {
        return Some(number.to_string());
    }
    value.as_u64().map(|number| number.to_string())
}

pub(super) fn with_period(value: &str) -> String {
    if value.ends_with('.') {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

pub(super) fn str_or<'a>(value: Option<&'a str>, fallback: &'static str) -> &'a str {
    match value {
        Some(value) => value,
        None => fallback,
    }
}

pub(super) fn trim_period(value: &str) -> &str {
    value.trim_end_matches('.')
}
