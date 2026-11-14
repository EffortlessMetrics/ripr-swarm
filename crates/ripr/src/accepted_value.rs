//! Shared near-miss wording for unrecognized enum-style values and flags.
//!
//! Owned below `cli` so inward layers (`output`, `app`) report an unknown
//! value with the same suggestion logic without importing a CLI adapter
//! (#5193). `cli::suggest` re-uses this authority for flag suggestions; it is
//! not forked.

/// Build the error for an unrecognized enum-style *value* (`--format yaml`).
///
/// Enumerates the accepted values and, when one is close enough, proposes it
/// as a near-miss suggestion, so the same mistake names the fix on every
/// command instead of only where a parser happens to inline the list.
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

/// Pick the best candidate for `input`, or nothing when none is close enough.
///
/// Leading dashes are stripped before comparing. Every flag shares the `--`
/// prefix, so leaving it in inflates similarity between flags that have
/// nothing else in common — that is how `--wat` came out as "did you mean
/// `--base`?", which is worse than no suggestion at all.
///
/// A candidate the input is a prefix of always beats an edit-distance match,
/// because a typed prefix is almost always an abbreviation rather than a typo
/// (`--sea` means `--seam-id`, not `--base`). Among prefix matches the
/// shortest completion wins; among typo matches the smallest distance wins.
/// Ties break on the candidate name so the choice is deterministic.
pub(crate) fn closest<'a>(
    input: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let needle = input.trim_start_matches('-');
    if needle.is_empty() {
        return None;
    }
    let budget = typo_budget(needle);
    let mut best: Option<(u8, usize, &'a str)> = None;
    for candidate in candidates {
        let hay = candidate.trim_start_matches('-');
        let ranked = if hay.starts_with(needle) {
            // Tier 0: abbreviation. Rank by how much is left to type.
            (0u8, hay.chars().count() - needle.chars().count(), candidate)
        } else {
            let distance = edit_distance(needle, hay);
            if distance > budget {
                continue;
            }
            // Compound enum values such as `gitlab-ci` are not near misses for
            // the shorter accepted token `gitlab`; flag abbreviations like
            // `--out-m` → `--out` must keep working.
            if !input.starts_with('-')
                && needle.len() > hay.len()
                && needle.starts_with(hay)
                && needle.as_bytes().get(hay.len()) == Some(&b'-')
            {
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

/// How many edits to tolerate, scaled to the length actually being compared.
///
/// This runs on the dash-stripped name, so a three-character name like `wat`
/// gets one edit rather than the three a `--`-inclusive length would have
/// bought it.
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
