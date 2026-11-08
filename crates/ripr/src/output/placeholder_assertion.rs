//! Human wording for suggested assertions that still carry `/* ... */`
//! placeholders (#9244 item 3).
//!
//! Labelled as a ready assertion they read like unfilled template text. A
//! shape with a placeholder names itself a shape and says what to do with
//! it; any other assertion text keeps its existing label and gets no note.

/// Instruction printed under a placeholder assertion so a newcomer does not
/// paste the comments as if they were values.
pub(crate) const FILL_IN_NOTE: &str =
    "replace each /* ... */ comment with a real value from your test before using it";

/// Note printed under a drafted concrete assertion: the values come from
/// ripr's static model of the current behavior, not from a run, so the
/// draft must not read as a verified assertion. The phrases are the honesty
/// contract the repair surfaces keep verbatim.
pub(crate) const DRAFTED_ASSERTION_NOTE: &str = "drafted from the current behavior; ripr has not run or verified anything, and the developer confirms the expected value before relying on this test";

pub(crate) fn has_placeholders(text: &str) -> bool {
    text.contains("/*") && text.contains("*/")
}

/// Pilot-terminal form: `assertion shape:` plus the fill-in note when the
/// text carries placeholders; otherwise `assertion:` with no note.
pub(crate) fn push_terminal_assertion(out: &mut String, indent: &str, shape: &str) {
    if has_placeholders(shape) {
        out.push_str(indent);
        out.push_str("assertion shape: ");
        out.push_str(shape);
        out.push('\n');
        out.push_str(indent);
        out.push_str("  (");
        out.push_str(FILL_IN_NOTE);
        out.push_str(")\n");
    } else {
        out.push_str(indent);
        out.push_str("assertion: ");
        out.push_str(shape);
        out.push('\n');
    }
}

/// Terminal assertion block with drafted honesty: a drafted assertion keeps
/// the `assertion shape:` label (it is not a verified assertion) and adds
/// the drafted note; when it still carries a placeholder the fill-in note
/// precedes it. Non-drafted shapes keep [`push_terminal_assertion`].
pub(crate) fn push_drafted_terminal_assertion(
    out: &mut String,
    indent: &str,
    shape: &str,
    drafted: bool,
) {
    if !drafted {
        push_terminal_assertion(out, indent, shape);
        return;
    }
    out.push_str(indent);
    out.push_str("assertion shape: ");
    out.push_str(shape);
    out.push('\n');
    if has_placeholders(shape) {
        out.push_str(indent);
        out.push_str("  (");
        out.push_str(FILL_IN_NOTE);
        out.push_str(")\n");
    }
    out.push_str(indent);
    out.push_str("  (");
    out.push_str(DRAFTED_ASSERTION_NOTE);
    out.push_str(")\n");
}

/// Append a parenthetical fill-in note when `text` carries `/* ... */`.
pub(crate) fn push_fill_in_note(out: &mut String, indent: &str, text: &str) {
    if let Some(line) = fill_in_note_line(indent, text) {
        out.push_str(&line);
        out.push('\n');
    }
}

/// Same note as a vector line for Markdown and CLI summaries that already
/// printed the assertion on the previous line.
pub(crate) fn fill_in_note_line(indent: &str, text: &str) -> Option<String> {
    has_placeholders(text).then(|| format!("{indent}({FILL_IN_NOTE})"))
}

#[cfg(test)]
mod tests {
    use super::{
        DRAFTED_ASSERTION_NOTE, FILL_IN_NOTE, fill_in_note_line, has_placeholders,
        push_drafted_terminal_assertion, push_fill_in_note, push_terminal_assertion,
    };

    #[test]
    fn placeholder_shape_is_labelled_a_shape_with_a_fill_in_note() {
        let mut out = String::new();
        push_terminal_assertion(
            &mut out,
            "  ",
            "assert_eq!(total(/* boundary input where a >= b */), /* expected */)",
        );
        assert_eq!(
            out,
            "  assertion shape: assert_eq!(total(/* boundary input where a >= b */), /* expected */)\n    (replace each /* ... */ comment with a real value from your test before using it)\n"
        );
    }

    #[test]
    fn drafted_concrete_assertion_keeps_the_shape_label_and_the_drafted_note() {
        let mut out = String::new();
        push_drafted_terminal_assertion(&mut out, "  ", "assert_eq!(total(100), 90)", true);
        assert_eq!(
            out,
            "  assertion shape: assert_eq!(total(100), 90)\n    (drafted from the current behavior; ripr has not run or verified anything, and the developer confirms the expected value before relying on this test)\n"
        );
    }

    #[test]
    fn drafted_placeholder_shape_keeps_both_notes() {
        let mut out = String::new();
        push_drafted_terminal_assertion(
            &mut out,
            "  ",
            "assert_eq!(total(100), /* expected */)",
            true,
        );
        assert!(out.contains(FILL_IN_NOTE), "{out}");
        assert!(out.contains(DRAFTED_ASSERTION_NOTE), "{out}");
    }

    #[test]
    fn non_drafted_shape_keeps_the_plain_terminal_form() {
        let mut out = String::new();
        push_drafted_terminal_assertion(
            &mut out,
            "  ",
            "assert!(quote.total > Money::zero())",
            false,
        );
        assert_eq!(
            out,
            "  assertion: assert!(quote.total > Money::zero())
"
        );
    }

    #[test]
    fn concrete_assertion_keeps_its_label_and_gets_no_note() {
        let mut out = String::new();
        push_terminal_assertion(&mut out, "  ", "assert!(quote.total > Money::zero())");
        assert_eq!(out, "  assertion: assert!(quote.total > Money::zero())\n");
        assert!(!has_placeholders("assert!(quote.total > Money::zero())"));
        assert!(fill_in_note_line("  ", "assert_eq!(total(1), 2)").is_none());
    }

    #[test]
    fn empty_and_not_applicable_text_are_not_placeholders() {
        assert!(!has_placeholders(""));
        assert!(!has_placeholders("not_applicable"));
        assert!(!has_placeholders("assert the exact returned value"));
        let mut out = String::new();
        push_fill_in_note(&mut out, "  ", "not_applicable");
        assert!(out.is_empty(), "{out}");
    }

    #[test]
    fn block_comment_pair_triggers_placeholder_wording() {
        assert!(has_placeholders("assert_eq!(f(/* input */), 1)"));
        assert_eq!(
            fill_in_note_line("- ", "assert_eq!(f(/* input */), 1)").as_deref(),
            Some(format!("- ({FILL_IN_NOTE})").as_str())
        );
    }

    #[test]
    fn division_by_dereference_is_not_a_placeholder_shape() {
        assert!(!has_placeholders("assert_eq!(numerator / *denominator, 1)"));
        assert!(!has_placeholders("x / *y"));
    }
}
