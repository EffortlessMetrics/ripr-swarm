//! #7753: count-aware noun formatting shared by user-facing surfaces.
//!
//! Hardcoded plural nouns rendered "1 entries" / "1 probes" on the most
//! common first-run path (a single-probe, single-finding diff). This helper
//! is the single owner of the singular/plural decision; new interpolated
//! count strings route through it instead of hand-rolling `== 1` branches.

/// Render "`count` `singular`" for one and "`count` `plural`" otherwise:
/// `count_noun(1, "probe", "probes")` → `"1 probe"`.
pub(crate) fn count_noun(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::count_noun;

    #[test]
    fn one_takes_the_singular() {
        assert_eq!(count_noun(1, "probe", "probes"), "1 probe");
        assert_eq!(count_noun(1, "entry", "entries"), "1 entry");
        assert_eq!(count_noun(1, "finding", "findings"), "1 finding");
    }

    #[test]
    fn zero_and_many_take_the_plural() {
        assert_eq!(count_noun(0, "probe", "probes"), "0 probes");
        assert_eq!(count_noun(2, "attempt", "attempts"), "2 attempts");
    }
}
