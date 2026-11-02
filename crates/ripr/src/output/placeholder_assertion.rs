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

pub(crate) fn has_placeholders(text: &str) -> bool {
    text.contains("/*")
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
        FILL_IN_NOTE, fill_in_note_line, has_placeholders, push_fill_in_note,
        push_terminal_assertion,
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
    fn a_lone_block_comment_opener_is_enough_to_warn() {
        assert!(has_placeholders("assert_eq!(f(/* input */), 1)"));
        assert_eq!(
            fill_in_note_line("- ", "assert_eq!(f(/* input */), 1)").as_deref(),
            Some(format!("- ({FILL_IN_NOTE})").as_str())
        );
    }
}
