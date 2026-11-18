//! Macro-spelled impls of the owner trait (#11113 remainder).
//!
//! A `macro_rules!` body or macro call is a token tree, so an `impl Trait for
//! Type` it spells is not an indexed impl. The std-receiver demotion used to
//! keep credit for every macro whose text held `impl`, the trait name and
//! `for`, which a bitflags-shaped crate always has: its macro declares the
//! flags type and implements the trait for it, and `Vec::contains` in a test
//! stayed `direct_owner_call` for `Flags::contains`.
//!
//! This reader removes that credit only when every token that could name the
//! trait sits in an impl header whose self type is established:
//!
//! ```text
//! impl [path ::] Trait for Ident ( `{` | `<` | `where` )
//! ```
//!
//! where `Ident` is a plain identifier (never a `$metavariable`, path,
//! reference, slice or `dyn` type), is not the std head, is not a
//! single-letter type parameter, and names a `struct`/`enum`/`union` declared
//! by the same macro or by an indexed file. Anything else (a generic impl, a
//! metavariable self type, trait generics, the trait named in a bound or a
//! nested macro argument, or the trait name inside a literal or a longer
//! identifier that is not one of those self types) keeps the credit, exactly
//! as before.

use ra_ap_syntax::SyntaxNode;

/// Whether the macro at `node` may implement `trait_name` for the std type
/// `head`. `declared` answers whether an indexed file declares an ADT of that
/// name.
pub(super) fn macro_may_implement_for_std(
    node: &SyntaxNode,
    trait_name: &str,
    head: &str,
    declared: &dyn Fn(&str) -> bool,
) -> bool {
    let text = node.text().to_string();
    if !(text.contains("impl") && text.contains(trait_name) && text.contains("for")) {
        return false;
    }
    let tokens = macro_tokens(node);
    tokens_may_implement_for_std(&tokens, trait_name, head, declared)
}

fn tokens_may_implement_for_std(
    tokens: &[String],
    trait_name: &str,
    head: &str,
    declared: &dyn Fn(&str) -> bool,
) -> bool {
    let mut self_types = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        if token != trait_name {
            continue;
        }
        let Some(self_type) = impl_header_self_type(tokens, at) else {
            return true;
        };
        let user_type = self_type != head
            && !(self_type.len() == 1 && self_type.bytes().all(|byte| byte.is_ascii_uppercase()))
            && (macro_declares_adt(tokens, self_type) || declared(self_type));
        if !user_type {
            return true;
        }
        self_types.push(self_type);
    }
    // A longer token holding the trait name (`TestFlags` for `Flags`) is
    // accounted for only as one of those self types; any other could be an
    // alias of the trait or a literal the macro pastes into a name.
    let unaccounted = tokens.iter().any(|token| {
        token != trait_name && token.contains(trait_name) && !self_types.contains(&token.as_str())
    });
    // The text gate matched, so some header must have been read; none means
    // the token walk and the text disagree, which is unresolved.
    unaccounted || self_types.is_empty()
}

/// Non-trivia tokens of the macro, with punctuation split into single
/// characters: a token tree does not glue `::`, and a parsed call may.
fn macro_tokens(node: &SyntaxNode) -> Vec<String> {
    let mut tokens = Vec::new();
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
    {
        let text = token.text();
        if text.starts_with(is_word_start) || text.starts_with(['"', '\'']) {
            tokens.push(text.to_string());
        } else {
            tokens.extend(text.chars().map(String::from));
        }
    }
    tokens
}

fn is_word_start(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn is_plain_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && token.chars().all(is_word_start)
}

/// The self type of `impl [path ::] Trait for Ident` when the trait token at
/// `at` sits in exactly that header.
fn impl_header_self_type(tokens: &[String], at: usize) -> Option<&str> {
    let mut start = at;
    while start >= 2 && tokens[start - 1] == ":" && tokens[start - 2] == ":" {
        start -= 2;
        if start >= 1 && tokens[start - 1] != "impl" && is_plain_ident(&tokens[start - 1]) {
            start -= 1;
            if start >= 1 && tokens[start - 1] == "$" {
                if tokens[start] != "crate" {
                    return None;
                }
                start -= 1;
            }
        }
    }
    if start == 0 || tokens[start - 1] != "impl" {
        return None;
    }
    if tokens.get(at + 1).map(String::as_str) != Some("for") {
        return None;
    }
    let self_type = tokens.get(at + 2)?;
    if !is_plain_ident(self_type) {
        return None;
    }
    matches!(
        tokens.get(at + 3).map(String::as_str),
        Some("{" | "<" | "where")
    )
    .then_some(self_type.as_str())
}

fn macro_declares_adt(tokens: &[String], name: &str) -> bool {
    tokens
        .windows(2)
        .any(|pair| matches!(pair[0].as_str(), "struct" | "enum" | "union") && pair[1] == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Vec<String> {
        source.split_whitespace().map(str::to_string).collect()
    }

    fn may(source: &str, declared: &[&str]) -> bool {
        tokens_may_implement_for_std(&tokens(source), "Flags", "Vec", &|name| {
            declared.contains(&name)
        })
    }

    #[test]
    fn given_macro_declared_self_type_when_reading_impl_then_std_is_ruled_out() {
        assert!(!may(
            "struct TestFlags { } impl $ crate : : Flags for TestFlags { }",
            &[]
        ));
        assert!(!may("impl Flags for Indexed { }", &["Indexed"]));
        assert!(!may(
            "impl : : traits : : Flags for Wrapper < u8 > { }",
            &["Wrapper"]
        ));
    }

    #[test]
    fn given_unestablished_self_type_when_reading_impl_then_credit_is_kept() {
        for source in [
            "impl Flags for Vec < u8 > { }",
            "impl $ crate : : Flags for $ name { }",
            "impl Flags for Undeclared { }",
            "impl < T > Flags for T { }",
            "struct T { } impl Flags for T { }",
            "impl Flags for std : : vec : : Vec < u8 > { }",
            "impl Flags for & TestFlags { } struct TestFlags { }",
            "impl Flags for [ u8 ] { }",
            "impl Flags < u8 > for TestFlags { } struct TestFlags { }",
            "struct TestFlags { } impl Flags for TestFlags { } fn f < T : Flags > ( ) { }",
            "struct TestFlags { } impl Flags for TestFlags { } impl MyFlags for Vec < u8 > { }",
            "struct TestFlags { } impl $ other : : Flags for TestFlags { }",
            "struct TestFlags { } impl Flags for TestFlags ;",
            "struct TestFlags { } impl Flags for TestFlags { } \"Flags\"",
            "struct Unit { } impl Flags for Unit { } use Flags as MyFlags ;",
        ] {
            assert!(may(source, &[]), "{source}");
        }
    }
}
