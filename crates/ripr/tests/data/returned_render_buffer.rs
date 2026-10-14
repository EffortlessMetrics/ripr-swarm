//! Reduced output-buffer path from ub-review b97b2edefe0b41a23e0b5e081a39a60849071b0f.
//! This fixture preserves writes before/after the helper and the returned local.
//! It deliberately omits receipt activation, loops and cap_review_body.

fn render_pr_signal(text: &mut String, value: &str) {
    let sentence = value.trim();
    text.push_str(&format!("- {sentence}\n"));
}

fn append_diagnostic(text: &mut String) {
    render_pr_signal(text, "Focused proof evidence remains incomplete.");
}

fn rendered_summary() -> String {
    let mut text = String::new();
    text.push_str("before\n");
    append_diagnostic(&mut text);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_returned_buffer() {
        let rendered = rendered_summary();
        assert!(rendered.contains("Focused proof evidence remains incomplete."));
        assert!(rendered.starts_with("before\n- "));
    }

    #[test]
    fn observes_returned_buffer_after_later_write() {
        fn rendered_with_suffix() -> String {
            let mut text = String::new();
            text.push_str("before\n");
            append_diagnostic(&mut text);
            text.push_str("after\n");
            text
        }
        let rendered = rendered_with_suffix();
        assert!(rendered.contains("Focused proof evidence remains incomplete."));
        assert!(rendered.ends_with("after\n"));
    }

    #[test]
    fn sibling_only() {
        let _actual = rendered_summary();
        let sibling = String::from("Focused proof evidence remains incomplete.");
        assert!(sibling.contains("Focused proof evidence remains incomplete."));
    }

    #[test]
    fn ignored_return() {
        let _ = rendered_summary();
        assert!("unchanged".contains("unchanged"));
    }

    #[test]
    fn shadowed_buffer() {
        #[allow(
            clippy::let_and_return,
            reason = "returning the shadowed binding is the negative witness under test"
        )]
        fn shadowed_summary() -> String {
            let mut text = String::new();
            text.push_str("before\n");
            append_diagnostic(&mut text);
            let text = String::from("Focused proof evidence remains incomplete.");
            text
        }
        let rendered = shadowed_summary();
        assert!(rendered.contains("Focused proof evidence remains incomplete."));
    }

    #[test]
    fn wrong_helper() {
        fn other_render(text: &mut String) {
            text.push_str("Focused proof evidence remains incomplete.");
        }
        let mut text = String::new();
        other_render(&mut text);
        let rendered = text;
        assert!(rendered.contains("Focused proof evidence remains incomplete."));
    }

    #[test]
    fn overwritten_buffer() {
        let mut text = rendered_summary();
        text.clear();
        text.push_str("Focused proof evidence remains incomplete.");
        assert!(text.contains("Focused proof evidence remains incomplete."));
    }
}
