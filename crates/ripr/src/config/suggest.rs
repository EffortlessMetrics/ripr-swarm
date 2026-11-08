//! Did-you-mean hints for unknown `ripr.toml` keys.
//!
//! Every raw table denies unknown fields, so a typo already fails the load.
//! This module only adds the nearest valid key to that failure. Candidates come
//! from the parser's own expected-field list, so a table can only suggest its
//! own keys, and the hint text is built from fixed key names, never from file
//! content.

/// Every key name `ripr.toml` accepts, in any table. The source-free error
/// summary keeps a hint line only when its key is on this list. A parity test
/// binds the list to the published JSON schema.
pub(super) const KNOWN_KEYS: &[&str] = &[
    "activation_unknown",
    "adapter",
    "analysis",
    "bridge_hints",
    "broad_error_strength",
    "bun_ub",
    "cache_dir",
    "crate",
    "diagnostic_profile",
    "discrimination_unknown",
    "enabled",
    "excluded_file_patterns",
    "executable",
    "exposed",
    "findings",
    "generated_file_patterns",
    "handwritten_files",
    "include_unchanged_tests",
    "infection_unknown",
    "intentional",
    "kind",
    "languages",
    "lsp",
    "marker",
    "max_related_tests",
    "mock_expectation_strength",
    "mode",
    "no_static_path",
    "observation_unknown",
    "opaque",
    "oracles",
    "overrides",
    "path",
    "perl",
    "producer",
    "production_like_targets",
    "profiles",
    "propagation_unknown",
    "python",
    "reachable_unrevealed",
    "registration_id",
    "reports",
    "resolve_tsconfig_paths",
    "rust",
    "seam_diagnostics",
    "seams",
    "severity",
    "snapshot_strength",
    "static_unknown",
    "strongly_gripped",
    "suppressed",
    "suppressions",
    "target",
    "test_harnesses",
    "test_roots",
    "timeout_ms",
    "typescript",
    "ungripped",
    "weakly_exposed",
    "weakly_gripped",
];

/// The field names serde lists after "expected", for either the
/// `expected one of `a`, `b`` or the single `expected `a`` form.
pub(super) fn expected_fields(error: &str) -> Vec<&str> {
    let Some(rest) = error.strip_prefix("unknown field `") else {
        return Vec::new();
    };
    let Some((_, expected)) = rest.rsplit_once("`, expected ") else {
        return Vec::new();
    };
    expected
        .trim_start_matches("one of ")
        .split('`')
        .enumerate()
        .filter_map(|(index, part)| (index % 2 == 1).then_some(part))
        .collect()
}

/// The closest candidate within a small edit distance, or `None` when nothing
/// is close enough to be a believable typo. Ties go to the first candidate.
pub(super) fn did_you_mean<'a>(key: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let limit = (key.chars().count() / 3).clamp(1, 3);
    candidates
        .iter()
        .map(|candidate| (edit_distance(key, candidate), *candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

pub(super) fn suggestion_line(candidate: &str) -> String {
    format!("did you mean `{candidate}`?")
}

/// Whether `line` is exactly a suggestion built from a known key.
pub(super) fn is_known_suggestion_line(line: &str) -> bool {
    KNOWN_KEYS
        .iter()
        .any(|candidate| line == suggestion_line(candidate))
}

/// Optimal string alignment distance: Levenshtein plus adjacent
/// transposition as one edit, so `mdoe` is one edit from `mode`.
fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let width = right.len() + 1;
    let mut table = vec![0usize; (left.len() + 1) * width];
    for (row, cells) in table.chunks_mut(width).enumerate() {
        if let Some(first) = cells.first_mut() {
            *first = row;
        }
    }
    for (column, cell) in table.iter_mut().take(width).enumerate() {
        *cell = column;
    }
    for row in 1..=left.len() {
        for column in 1..=right.len() {
            let cost = usize::from(left[row - 1] != right[column - 1]);
            let mut best = (table[(row - 1) * width + column] + 1)
                .min(table[row * width + column - 1] + 1)
                .min(table[(row - 1) * width + column - 1] + cost);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                best = best.min(table[(row - 2) * width + column - 2] + 1);
            }
            table[row * width + column] = best;
        }
    }
    table[left.len() * width + right.len()]
}
