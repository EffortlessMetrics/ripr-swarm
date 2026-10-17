//! #7985: whether a test file's macro invocations may define an item named
//! like the owner.
//!
//! A bare call `suit_color('S')` reaches the owner only when nothing in the
//! test file rebinds the name. A same-file `macro_rules!` that spells
//! `fn suit_color(..)` and an item- or statement-position invocation of it
//! put a different `suit_color` in scope, but the macro's expansion is not
//! in the index, so no `fn` twin is recorded. This reads the macro
//! definitions and invocations directly and answers conservatively: the
//! name may be rebound when
//!
//! - an invoked local `macro_rules!` declares an item of the name
//!   (`fn`/`const`/`static`/`struct`/`enum`/`type`/`trait`/`mod`, or
//!   `use .. as name`);
//! - an invoked local `macro_rules!` declares an item through a fragment
//!   binder (`fn $name`) and an invocation mentions the name; or
//! - an item-position invocation of any macro mentions the name in its
//!   arguments (`emit!(suit_color)`).
//!
//! An expression macro that merely calls the name (`assert_eq!(f(..))`) and
//! a macro that defines other names rebind nothing. An unparsable file that
//! holds a `macro_rules!` is undecided and answers that it may rebind.

use super::reveal::contains_as_whole_word;
use crate::analysis::extract::mask_comments_and_strings;
use ra_ap_syntax::{AstNode, ast, ast::HasName};

const ITEM_KEYWORDS: [&str; 8] = [
    "fn", "const", "static", "struct", "enum", "type", "trait", "mod",
];

/// Whether the file's macro invocations may define an item named `name`.
pub(in crate::analysis) fn macro_may_define_item_named(source: &str, name: &str) -> bool {
    let name = name.strip_prefix("r#").unwrap_or(name);
    if name.is_empty() {
        return false;
    }
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
        return mask_comments_and_strings(source).contains("macro_rules!");
    };
    let mut definitions = Vec::new();
    let mut invocations = Vec::new();
    for node in parse.tree().syntax().descendants() {
        if let Some(rules) = ast::MacroRules::cast(node.clone()) {
            if let Some(macro_name) = rules.name() {
                definitions.push((
                    macro_name.text().to_string(),
                    mask_comments_and_strings(&rules.syntax().text().to_string()),
                ));
            }
            continue;
        }
        let Some(call) = ast::MacroCall::cast(node.clone()) else {
            continue;
        };
        let Some(macro_name) = call
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|segment| segment.text().to_string())
        else {
            continue;
        };
        let arguments = call
            .token_tree()
            .map(|tree| mask_comments_and_strings(&tree.syntax().text().to_string()))
            .unwrap_or_default();
        let item_position = node.parent().is_some_and(|parent| {
            ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
        });
        let statement_position = node.parent().is_some_and(|parent| {
            ast::MacroStmts::can_cast(parent.kind())
                || ast::MacroExpr::can_cast(parent.kind())
                    && parent.parent().is_some_and(|holder| {
                        ast::ExprStmt::can_cast(holder.kind())
                            || ast::StmtList::can_cast(holder.kind())
                    })
        });
        if item_position || statement_position {
            invocations.push((
                macro_name,
                contains_as_whole_word(&arguments, name),
                item_position,
            ));
        }
    }
    invocations
        .iter()
        .any(|(macro_name, mentions_name, item_position)| {
            let local = definitions
                .iter()
                .filter(|(defined, _)| defined == macro_name)
                .collect::<Vec<_>>();
            if local.is_empty() {
                // An unread expansion can only receive the name through its
                // arguments, and only an item-position call defines items
                // for the whole file.
                return *item_position && *mentions_name;
            }
            local.iter().any(|(_, definition)| {
                declares_item_named(definition, name)
                    || *mentions_name && declares_item_through_binder(definition)
            })
        })
}

/// Whether masked macro text spells an item declaration of `name`.
fn declares_item_named(masked: &str, name: &str) -> bool {
    let tokens = identifier_tokens(masked);
    tokens.windows(2).enumerate().any(|(index, pair)| {
        let named = |token: &str| token.strip_prefix("r#").unwrap_or(token) == name;
        let keyword_then_name = ITEM_KEYWORDS.contains(&pair[0]) && named(pair[1]);
        let mutable_static =
            pair[0] == "mut" && named(pair[1]) && index > 0 && tokens[index - 1] == "static";
        let renamed_import = pair[0] == "as" && named(pair[1]);
        keyword_then_name || mutable_static || renamed_import
    })
}

/// Whether masked macro text declares an item whose name is a fragment
/// binder (`fn $name`, `static mut $name`, `use .. as $name`), so the
/// invocation chooses the name.
fn declares_item_through_binder(masked: &str) -> bool {
    let squeezed = masked.split_whitespace().collect::<Vec<_>>().join(" ");
    ITEM_KEYWORDS
        .iter()
        .map(|keyword| format!("{keyword} $"))
        .chain(["static mut $".to_string(), " as $".to_string()])
        .any(|needle| {
            squeezed.match_indices(&needle).any(|(at, _)| {
                needle.starts_with(' ')
                    || squeezed[..at]
                        .chars()
                        .next_back()
                        .is_none_or(|before| !(before.is_ascii_alphanumeric() || before == '_'))
            })
        })
}

fn identifier_tokens(masked: &str) -> Vec<&str> {
    masked
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::macro_may_define_item_named;

    const SHADOW: &str = "macro_rules! fake {\n    () => {\n        fn suit_color(_s: char) -> u8 { 1 }\n    };\n}\n";

    #[test]
    fn an_invoked_local_macro_declaring_the_name_may_define_it() {
        assert!(macro_may_define_item_named(
            &format!("{SHADOW}fake!();\n"),
            "suit_color"
        ));
        // Statement position inside a test body.
        assert!(macro_may_define_item_named(
            &format!("{SHADOW}#[test]\nfn t() {{\n    fake!();\n}}\n"),
            "suit_color"
        ));
    }

    #[test]
    fn a_defined_but_uninvoked_or_unrelated_macro_defines_nothing() {
        assert!(!macro_may_define_item_named(SHADOW, "suit_color"));
        assert!(!macro_may_define_item_named(
            &format!("{SHADOW}fake!();\n"),
            "other_name"
        ));
        // A macro that only calls the name defines nothing.
        let calls = "macro_rules! check {\n    ($x:expr) => { assert_eq!(suit_color($x), 1) };\n}\n#[test]\nfn t() {\n    check!('S');\n}\n";
        assert!(!macro_may_define_item_named(calls, "suit_color"));
    }

    #[test]
    fn a_fragment_binder_defines_the_name_the_invocation_passes() {
        let source = "macro_rules! make {\n    ($n:ident) => { fn $n() -> u8 { 1 } };\n}\nmake!(suit_color);\n";
        assert!(macro_may_define_item_named(source, "suit_color"));
        assert!(!macro_may_define_item_named(source, "other_name"));
    }

    #[test]
    fn an_external_item_macro_defines_the_name_only_through_its_arguments() {
        assert!(macro_may_define_item_named(
            "other::emit!(suit_color);\n",
            "suit_color"
        ));
        assert!(!macro_may_define_item_named(
            "proptest::proptest! { fn cases(x in 0..1) {} }\n",
            "suit_color"
        ));
        // Expression-position assertions mention the name without defining it.
        assert!(!macro_may_define_item_named(
            "#[test]\nfn t() {\n    assert_eq!(suit_color('S'), 1);\n}\n",
            "suit_color"
        ));
    }
}
