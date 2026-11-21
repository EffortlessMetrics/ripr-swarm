//! The arms of an owner whose body is one `match <parameter> { .. }`, read
//! once per owner in a run and indexed for the changed-arm question.
//!
//! Every changed arm of one owner asks whether a test that passes the arm's
//! literal runs that arm. Reading the match again for each arm, and then
//! walking the earlier arms, made a match of `n` literal arms cost `n`
//! squared reads; a generated lookup of a few thousand arms kept `ripr check`
//! busy for minutes after its analysis had finished. The table holds the
//! answer to "which earlier arm takes this input first" by value.

use super::{FunctionSummary, is_plain_literal_pattern, whole_body_match_arms};
use crate::analysis::syntax::run_memo::{self, Product};
use std::collections::HashMap;
use std::rc::Rc;

/// The value an alternative of a plain literal arm matches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum LiteralValue {
    /// A string or char literal, compared by its text.
    Text(String),
    /// An integer, compared by value however it is written (`01`, `-0`).
    Integer(i128),
}

fn is_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

pub(super) struct ArmTable {
    scrutinee: String,
    /// Pattern text and 1-based file line of each top-level arm, in order.
    arms: Vec<(String, usize)>,
    /// Arm indexes by line, in arm order.
    by_line: HashMap<usize, Vec<usize>>,
    /// The first arm that is not plain literals only (a guard, binding,
    /// wildcard, range or path), which could take any input first.
    first_open_arm: Option<usize>,
    /// The first arm holding each value as an alternative.
    first_with_value: HashMap<LiteralValue, usize>,
    /// The first arm holding any integer alternative.
    first_integer: Option<usize>,
    /// The first arm holding an integer too large to read. It counts as equal
    /// to every integer, so the caller refuses.
    first_unread_integer: Option<usize>,
}

impl ArmTable {
    fn new(scrutinee: String, arms: Vec<(String, usize)>) -> Self {
        let mut table = Self {
            scrutinee,
            arms: Vec::new(),
            by_line: HashMap::new(),
            first_open_arm: None,
            first_with_value: HashMap::new(),
            first_integer: None,
            first_unread_integer: None,
        };
        for (index, (pattern, line)) in arms.iter().enumerate() {
            table.by_line.entry(*line).or_default().push(index);
            if table.first_open_arm.is_none() && !pattern.split('|').all(is_plain_literal_pattern) {
                table.first_open_arm = Some(index);
            }
            for alternative in pattern.split('|').map(str::trim) {
                if !is_integer(alternative) {
                    table
                        .first_with_value
                        .entry(LiteralValue::Text(alternative.to_string()))
                        .or_insert(index);
                    continue;
                }
                table.first_integer.get_or_insert(index);
                match alternative.parse::<i128>() {
                    Ok(value) => {
                        table
                            .first_with_value
                            .entry(LiteralValue::Integer(value))
                            .or_insert(index);
                    }
                    Err(_) => {
                        table.first_unread_integer.get_or_insert(index);
                    }
                }
            }
        }
        table.arms = arms;
        table
    }

    pub(super) fn scrutinee(&self) -> &str {
        &self.scrutinee
    }

    /// Whether the arm on `line` whose pattern is exactly the plain literal
    /// `target` is the first arm an input equal to `target` reaches: every
    /// earlier arm is plain literals only and none of them names the same
    /// value (alone or in a `|` alternation).
    pub(super) fn arm_runs_first(&self, line: usize, target: &str) -> bool {
        let Some(arm) = self.by_line.get(&line).and_then(|indexes| {
            indexes
                .iter()
                .copied()
                .find(|index| self.arms[*index].0.trim() == target)
        }) else {
            return false;
        };
        let same_value = if is_integer(target) {
            match target.parse::<i128>() {
                Ok(value) => [
                    self.first_with_value
                        .get(&LiteralValue::Integer(value))
                        .copied(),
                    self.first_unread_integer,
                ]
                .into_iter()
                .flatten()
                .min(),
                Err(_) => self.first_integer,
            }
        } else {
            self.first_with_value
                .get(&LiteralValue::Text(target.to_string()))
                .copied()
        };
        [self.first_open_arm, same_value]
            .into_iter()
            .flatten()
            .all(|taken_at| arm <= taken_at)
    }
}

/// The arm table of `owner`, read once per owner text and start line in a
/// run scope; `None` when the body is not one whole match on an identifier.
pub(super) fn owner_arm_table(owner: &FunctionSummary) -> Rc<Option<ArmTable>> {
    run_memo::memoize(
        Product::OwnerMatchArms,
        owner.start_line,
        owner.body.as_str(),
        || {
            Rc::new(
                whole_body_match_arms(owner)
                    .map(|(scrutinee, arms)| ArmTable::new(scrutinee, arms)),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::test_grip_evidence::same_literal_value;
    use proptest::prelude::*;

    /// The pairwise walk the table replaces: the first arm on `line` whose
    /// pattern is `target`, unless an earlier arm is not plain literals only
    /// or names the same value.
    fn walk(arms: &[(String, usize)], line: usize, target: &str) -> bool {
        for (pattern, arm_line) in arms {
            if *arm_line == line && pattern.trim() == target {
                return true;
            }
            if !pattern.split('|').all(is_plain_literal_pattern)
                || pattern
                    .split('|')
                    .any(|alternative| same_literal_value(alternative, target))
            {
                return false;
            }
        }
        false
    }

    /// Plain literals in several spellings of one value, an integer too large
    /// to read, and the open shapes (wildcard, binding, guard, range, a
    /// string holding `|`) that take any input first.
    const PATTERNS: &[&str] = &[
        "0",
        "-0",
        "00",
        "1",
        "01",
        " 1 ",
        "-1",
        "7",
        "340282366920938463463374607431768211456",
        "\"a\"",
        "\"b\"",
        "'a'",
        "1 | 2",
        "\"a\" | \"c\"",
        "2 | 01",
        "_",
        "n",
        "n if n > 3",
        "1..=3",
        "\"a|b\"",
        "-",
    ];

    fn arms() -> impl Strategy<Value = Vec<(String, usize)>> {
        prop::collection::vec((0..PATTERNS.len(), 0usize..3), 0..9).prop_map(|picks| {
            let mut line = 10;
            picks
                .into_iter()
                .map(|(pattern, advance)| {
                    // `advance == 0` keeps two arms on one line.
                    line += advance;
                    (PATTERNS[pattern].to_string(), line)
                })
                .collect()
        })
    }

    proptest! {
        /// The indexed answer equals the pairwise walk for every plain
        /// literal target on every line the match spans.
        #[test]
        fn table_agrees_with_the_pairwise_walk(arms in arms()) {
            let table = ArmTable::new("n".to_string(), arms.clone());
            for target in PATTERNS.iter().map(|pattern| pattern.trim()) {
                if !is_plain_literal_pattern(target) {
                    continue;
                }
                for line in 9..30 {
                    prop_assert_eq!(
                        table.arm_runs_first(line, target),
                        walk(&arms, line, target),
                        "target {} line {} arms {:?}", target, line, &arms
                    );
                }
            }
        }
    }

    /// The generator must reach both answers and the refusing shapes, or the
    /// agreement above compares nothing.
    #[test]
    fn walk_reference_tells_the_cases_apart() {
        let arms = |patterns: &[&str]| -> Vec<(String, usize)> {
            patterns
                .iter()
                .enumerate()
                .map(|(at, pattern)| ((*pattern).to_string(), 10 + at))
                .collect()
        };
        let plain = arms(&["0", "1", "\"a\""]);
        assert!(walk(&plain, 11, "1"));
        assert!(ArmTable::new("n".to_string(), plain.clone()).arm_runs_first(11, "1"));
        // Wrong line, an earlier spelling of the same value, an earlier
        // wildcard, and an earlier unreadable integer each refuse.
        assert!(!ArmTable::new("n".to_string(), plain).arm_runs_first(12, "1"));
        for earlier in [
            "01",
            "_",
            "340282366920938463463374607431768211456",
            "2 | 1",
        ] {
            let shadowed = arms(&[earlier, "1"]);
            assert!(!walk(&shadowed, 11, "1"), "{earlier}");
            assert!(
                !ArmTable::new("n".to_string(), shadowed).arm_runs_first(11, "1"),
                "{earlier}"
            );
        }
    }
}
