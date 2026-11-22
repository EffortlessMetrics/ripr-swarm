//! RIPR-SPEC-0240 / #13843: string-literal text outside every function, with
//! no test seen reading it, is not a gap.
//!
//! A help-table row such as `summary: "Print a shell completion script.",`
//! sits in a `static` and has no owning function. Nothing calls it, so ripr
//! has no name to look for in a test; the only tests it relates are the ones
//! that share the file or a name token. Reach then reads `weak`, a
//! neighbour's oracle is graded as if it observed the row, and the finding
//! read `weakly_exposed` with a next step that points at assertions unrelated
//! to the text. The text is usually pinned somewhere ripr does not follow:
//! a golden file rendered through the binary, a snapshot of `--help`.
//!
//! The rule needs every link, read from the finding and the index:
//!
//! - the probe has no owner and no indexed function spans its line;
//! - both sides of the change are present and differ only inside string
//!   literals, as [`string_literal_spans`] reads them (a changed identifier,
//!   number, operator or added line keeps today's reading);
//! - reach is `weak` and every related test is a proximity relation
//!   ([`RelationReason::establishes_reach`] is false for each).
//!
//! A test whose assertion quotes the changed literal whole relates through
//! `assertion_target_affinity`, which establishes reach for a probe with no
//! function owner, so that finding keeps its own reading. Like every
//! RIPR-SPEC-0240 rule this only withholds: nothing is credited and the
//! actionability flip stays fail-closed.

use super::gap_admission::withhold_gap;
use crate::analysis::extract::string_literal_spans;
use crate::analysis::rust_index::RustIndex;
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustUnownedLiteralReaderUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "which code reads the changed string literal, and whether a test compares that text";

/// Withhold a gap on string-literal text that no function owns and no test is
/// seen reading. Returns `true` when the finding was rewritten to
/// `static_unknown`.
pub(in crate::analysis) fn withhold_unowned_literal_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
        || finding.ripr.reach.state != StageState::Weak
        || finding.probe.owner.is_some()
    {
        return false;
    }
    let (Some(before), Some(after)) = (
        finding.probe.before.as_deref(),
        finding.probe.after.as_deref(),
    ) else {
        return false;
    };
    if !differs_only_inside_string_literals(before, after) {
        return false;
    }
    let line = finding.probe.location.line;
    let file = &finding.probe.location.file;
    // The probe path may be absolute and the function's root-relative.
    let inside_a_function = index.functions().iter().any(|function| {
        file.ends_with(&function.file) && function.start_line <= line && line <= function.end_line
    });
    if inside_a_function {
        return false;
    }
    let proximity_only = !finding.related_tests.is_empty()
        && finding.related_tests.iter().all(|test| {
            test.relation_reason
                .is_some_and(|reason| !reason.establishes_reach(false))
        });
    if !proximity_only {
        return false;
    }
    let next_step = "The change is string-literal text outside every function, and no test is seen reading it: the related tests only share its file or a name token, so ripr does not report a gap. Check whether a test compares this text, for example a golden file or rendered help output; add one only if none does.".to_string();
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &[]);
    true
}

/// Whether `before` and `after` differ, each holds a terminated string
/// literal, and the text outside their string literals is the same. Leading
/// and trailing whitespace is not compared: the diff side keeps the line's
/// indentation and the probe side may not.
fn differs_only_inside_string_literals(before: &str, after: &str) -> bool {
    let (Some(code_before), Some(code_after)) = (
        outside_string_literals(before),
        outside_string_literals(after),
    ) else {
        return false;
    };
    before.trim() != after.trim() && code_before.trim() == code_after.trim()
}

/// `text` with the contents of every terminated string literal removed, or
/// `None` when it holds no literal or a quote the shared scan left unpaired
/// (a line that begins or ends inside a multi-line string).
fn outside_string_literals(text: &str) -> Option<String> {
    let spans = string_literal_spans(text);
    if spans.is_empty() {
        return None;
    }
    let mut kept = String::new();
    let mut at = 0;
    for (open, close, _) in spans {
        kept.push_str(text.get(at..=open)?);
        at = close;
    }
    kept.push_str(text.get(at..)?);
    let quotes_kept = kept.matches('"').count();
    (quotes_kept % 2 == 0).then_some(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_change_confined_to_string_literals_counts() {
        for (before, after, expected) in [
            // The help-table row of #13843.
            (
                "summary: \"Print a script.\",",
                "summary: \"Print a shell script.\",",
                true,
            ),
            // Indentation differs between the diff side and the probe side.
            ("    name: \"a\",", "name: \"b\",", true),
            // Raw strings and several literals on one line.
            ("(r#\"a \"x\"\"#, \"b\")", "(r#\"c\"#, \"d\")", true),
            // A changed field name, number or call is code.
            ("summary: \"a\",", "title: \"b\",", false),
            ("row(\"a\", 1),", "row(\"b\", 2),", false),
            ("row(\"a\"),", "other(\"a\"),", false),
            // No change, no literal, and an unpaired quote keep the reading.
            ("summary: \"a\",", "summary: \"a\",", false),
            ("weight: 1,", "weight: 2,", false),
            ("continued text\",", "other text\",", false),
            // A quote inside a comment opens no literal.
            ("x: 1, // \"a\"", "x: 2, // \"a\"", false),
        ] {
            assert_eq!(
                differs_only_inside_string_literals(before, after),
                expected,
                "{before:?} -> {after:?}"
            );
        }
    }
}
