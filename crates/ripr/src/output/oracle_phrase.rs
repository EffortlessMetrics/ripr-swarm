//! One phrase for "strength + kind" so related-test rows never stutter.
//!
//! The strength `smoke` paired with the kind `smoke only` rendered
//! "uses smoke smoke only oracle". When a multi-word kind already leads with
//! the strength word, the kind alone carries the meaning. The single-word
//! `unknown`/`unknown` pair stays as is: the fixture-contract oracle
//! projection (`unknown/unknown`) reads that two-part form.

/// Render `strength` and the display form of `kind` as one phrase:
/// `("strong", "exact value")` → `"strong exact value"`,
/// `("smoke", "smoke only")` → `"smoke only"`.
pub(crate) fn oracle_phrase(strength: &str, kind_label: &str) -> String {
    if kind_label
        .split_once(' ')
        .is_some_and(|(lead, _)| lead == strength)
    {
        kind_label.to_string()
    } else {
        format!("{strength} {kind_label}")
    }
}

#[cfg(test)]
mod tests {
    use super::oracle_phrase;

    #[test]
    fn distinct_strength_and_kind_are_joined() {
        assert_eq!(oracle_phrase("strong", "exact value"), "strong exact value");
        assert_eq!(oracle_phrase("none", "unknown"), "none unknown");
    }

    #[test]
    fn kind_that_repeats_the_strength_is_not_doubled() {
        assert_eq!(oracle_phrase("smoke", "smoke only"), "smoke only");
        assert_eq!(oracle_phrase("unknown", "unknown"), "unknown unknown");
    }
}
