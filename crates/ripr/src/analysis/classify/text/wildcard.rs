/// Whitespace-stable wildcard-discard binding predicate (#3233).
///
/// Recognizes `let` + the wildcard `_` + a `:`/`=` binding token across any
/// legal whitespace between the tokens: `let _ = expr`, `let _: Ty = expr`,
/// `let _ : Ty = expr`, and `let _=expr` are the same discard. The predicate
/// stays narrow to exactly that token grammar — an identifier continuation
/// (`let _value`, `let __x`) is a named binding, possibly used, and never a
/// discard; `let _;` has no discarded initializer.
///
/// Single authority for both classification stages: the infection stage's
/// discard detection and the flow stage's `value_is_swallowed` consume this
/// predicate so their contracts cannot drift apart again (#2401, #3233).
pub(in crate::analysis) fn is_wildcard_discard_binding(text: &str) -> bool {
    let Some(after_let) = text.trim_start().strip_prefix("let") else {
        return false;
    };
    // `let` must be a keyword, not the head of a longer identifier.
    if !after_let.starts_with(char::is_whitespace) {
        return false;
    }
    let Some(after_wildcard) = after_let.trim_start().strip_prefix('_') else {
        return false;
    };
    // The character after `_` decides: whitespace or a binding token keeps
    // the wildcard interpretation; anything else is an identifier body.
    let binding = after_wildcard.trim_start();
    binding.starts_with('=') || binding.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_wildcard_discards_across_whitespace_shapes() {
        for text in [
            "let _ = expr;",
            "let _: Ty = expr;",
            "let _ : Ty = expr;",
            "let _=expr;",
            "  let  _  =  expr;",
        ] {
            assert!(
                is_wildcard_discard_binding(text),
                "`{text}` must be a wildcard discard"
            );
        }
    }

    #[test]
    fn rejects_named_bindings_and_non_binding_wildcards() {
        for text in [
            "let _value = expr;",
            "let __x = expr;",
            "let _;",
            "let_thing = expr;",
            "let mut _ = expr;",
            "_ = expr;",
        ] {
            assert!(
                !is_wildcard_discard_binding(text),
                "`{text}` must not be a wildcard discard"
            );
        }
    }
}
