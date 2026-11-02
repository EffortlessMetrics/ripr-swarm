//! Shared near-miss wording for unrecognized enum-style values. CLI and
//! output parsers both use this module so inward layers never import CLI
//! adapters (#5193).

pub(crate) fn unknown_value(label: &str, value: &str, accepted: &[&str]) -> String {
    match closest(value, accepted.iter().copied()) {
        Some(suggestion) => format!(
            "unknown {label} {value:?}. Did you mean `{suggestion}`? Accepted: {}.",
            accepted.join(", ")
        ),
        None => format!(
            "unknown {label} {value:?}. Accepted: {}.",
            accepted.join(", ")
        ),
    }
}

fn closest<'a>(input: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let needle = input.trim_start_matches('-');
    if needle.is_empty() {
        return None;
    }
    let budget = typo_budget(needle);
    let mut best: Option<(u8, usize, &'a str)> = None;
    for candidate in candidates {
        let hay = candidate.trim_start_matches('-');
        let ranked = if hay.starts_with(needle) {
            (0u8, hay.chars().count() - needle.chars().count(), candidate)
        } else {
            let distance = edit_distance(needle, hay);
            if distance > budget {
                continue;
            }
            (1u8, distance, candidate)
        };
        if best.is_none_or(|current| ranked < current) {
            best = Some(ranked);
        }
    }
    best.map(|(_, _, candidate)| candidate)
}

fn typo_budget(name: &str) -> usize {
    match name.chars().count() {
        0..=3 => 1,
        4..=7 => 2,
        _ => 3,
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right_chars: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right_chars.len()).collect();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_idx, left_char) in left.chars().enumerate() {
        current[0] = left_idx + 1;
        for (right_idx, right_char) in right_chars.iter().enumerate() {
            let substitution_cost = usize::from(left_char != *right_char);
            let deletion = previous[right_idx + 1] + 1;
            let insertion = current[right_idx] + 1;
            let substitution = previous[right_idx] + substitution_cost;
            current[right_idx + 1] = deletion.min(insertion).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right_chars.len()]
}
