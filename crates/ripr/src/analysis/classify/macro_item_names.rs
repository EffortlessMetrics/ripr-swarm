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
//! - an item-position invocation of a non-local macro passes the name as a
//!   bare argument (`emit!(suit_color)`, `emit!(inner!(suit_color))`) or
//!   spells `fn suit_color(..)` there. A call, path or method
//!   use, or a mention nested in a braced body (`proptest! { fn t() {
//!   f(suit_color(1)); } }`), binds nothing.
//!
//! The answer rides the shared same-name gate: both
//! `ProbeContext::test_file_imports_foreign_callee_name` (memoized) and
//! `reveal::file_imports_foreign_callee_name` (helper-hop, owner-pin and
//! activation callers) consult it.
//!
//! Known limit: an invocation nested inside another `macro_rules!`
//! definition's body (a macro that expands to `fake!();`) is not read as an
//! invocation, so a definition reached only that way is not seen. The
//! defining macro's own file is the only scope read; a macro defined in
//! another file or crate is read only through an invocation's arguments.
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
                item_position && passes_name_as_bare_argument(&arguments, name),
                declares_item_named(&arguments, name),
            ));
        }
    }
    invocations.iter().any(
        |(macro_name, mentions_name, bare_item_argument, declares_in_arguments)| {
            let local = definitions
                .iter()
                .filter(|(defined, _)| defined == macro_name)
                .collect::<Vec<_>>();
            if local.is_empty() {
                // An unread expansion can only receive the name through its
                // arguments, and only an item-position call defines items
                // for the whole file.
                // A std expression macro defines no item; a body fragment
                // (the helper-hop fallback source) parses its statement
                // macros at "item position".
                return *bare_item_argument
                    && !STD_EXPRESSION_MACROS.contains(&macro_name.as_str());
            }
            local.iter().any(|(_, definition)| {
                declares_item_named(definition, name)
                    || *mentions_name && declares_item_through_binder(definition)
                    || *declares_in_arguments && forwards_arguments_verbatim(definition)
            })
        },
    )
}

/// Whether the invocation's top-level arguments pass `name` as a bare
/// identifier (`emit!(name)`, `emit!(name, u8)`), not as a call, path
/// segment or method receiver, and not nested inside a braced, bracketed or
/// parenthesized group such as a `proptest! { .. }` body that merely calls it.
fn passes_name_as_bare_argument(masked_arguments: &str, name: &str) -> bool {
    let trimmed = masked_arguments.trim();
    let inner = trimmed
        .strip_prefix(['(', '[', '{'])
        .and_then(|rest| rest.strip_suffix([')', ']', '}']))
        .unwrap_or(trimmed);
    // Keep top-level characters; a nested group collapses to one `(` marker.
    // A group opened by a macro call (`inner!(..)`) is transparent: its
    // arguments are read as if they were top-level, because the inner macro
    // may forward them as items.
    let mut top_level = String::new();
    let mut open_groups: Vec<bool> = Vec::new();
    for character in inner.chars() {
        let collecting = open_groups.iter().all(|transparent| *transparent);
        match character {
            '(' | '[' | '{' => {
                let transparent = collecting && top_level.trim_end().ends_with('!');
                if collecting {
                    top_level.push('(');
                }
                open_groups.push(transparent);
            }
            ')' | ']' | '}' => {
                open_groups.pop();
            }
            _ if collecting => top_level.push(character),
            _ => {}
        }
    }
    let word_edge = |c: Option<char>| c.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
    top_level.match_indices(name).any(|(at, _)| {
        let end = at + name.len();
        let before = top_level[..at].trim_end();
        let after = top_level[end..].trim_start();
        // `fn name(..)` inside an argument declares the name; it is not a call.
        let declared = ITEM_KEYWORDS.contains(&trailing_identifier(before));
        word_edge(top_level[..at].chars().next_back())
            && word_edge(top_level[end..].chars().next())
            && !before.ends_with("::")
            && !before.ends_with('.')
            && (declared || !after.starts_with('('))
            && !after.starts_with("::")
            && !after.starts_with('.')
    })
}

/// The identifier that ends `text`, or `""` when it ends in anything else.
fn trailing_identifier(text: &str) -> &str {
    let start = text
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |at| at + 1);
    &text[start..]
}

/// Whether a local macro's transcriber can re-emit caller tokens untouched
/// (`$($t:tt)*` / `$i:item` forwarded as `$($t)*` / `$i`), so an item the
/// caller spells in the arguments is defined for the file.
fn forwards_arguments_verbatim(masked_definition: &str) -> bool {
    let squeezed = masked_definition
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("");
    squeezed.contains(":tt") || squeezed.contains(":item")
}

/// Standard-library macros that expand to expressions or statements and
/// never define an item. A file-local `macro_rules!` of the same name is
/// handled by the local-definition branch before this list is consulted.
const STD_EXPRESSION_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "matches",
    "format",
    "format_args",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "vec",
    "dbg",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
];

/// Whether masked macro text spells an item declaration of `name`.
fn declares_item_named(masked: &str, name: &str) -> bool {
    let tokens = identifier_tokens(masked);
    imports_name(masked, name)
        || tokens.windows(2).enumerate().any(|(index, pair)| {
            let named = |token: &str| token.strip_prefix("r#").unwrap_or(token) == name;
            let keyword_then_name = ITEM_KEYWORDS.contains(&pair[0]) && named(pair[1]);
            let mutable_static =
                pair[0] == "mut" && named(pair[1]) && index > 0 && tokens[index - 1] == "static";
            let renamed_import = pair[0] == "as" && named(pair[1]);
            keyword_then_name || mutable_static || renamed_import
        })
}

/// Whether masked text holds a plain import that binds `name`
/// (`use other::name;`, `use other::{a, name};`). A rename (`name as x`) binds
/// the new name and a mid-path segment (`use name::x;`) binds `x`.
fn imports_name(masked: &str, name: &str) -> bool {
    let word_edge = |c: Option<char>| c.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
    masked.match_indices("use").any(|(at, _)| {
        if !word_edge(masked[..at].chars().next_back())
            || !word_edge(masked[at + 3..].chars().next())
        {
            return false;
        }
        let statement = masked[at + 3..].split(';').next().unwrap_or_default();
        statement.match_indices(name).any(|(found, _)| {
            let end = found + name.len();
            word_edge(statement[..found].chars().next_back())
                && word_edge(statement[end..].chars().next())
                && statement[end..]
                    .trim_start()
                    .chars()
                    .next()
                    .is_none_or(|next| next == ',' || next == '}')
        })
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
    fn a_std_expression_macro_mentioning_the_name_defines_nothing() {
        // A body fragment parses its statement macros at item position.
        assert!(!macro_may_define_item_named(
            "assert!(entry(9));\n",
            "entry"
        ));
        assert!(macro_may_define_item_named("emit!(entry);\n", "entry"));
        assert!(macro_may_define_item_named("emit!(entry, u8);\n", "entry"));
        // Item-position property macros that call the owner inside their
        // body bind nothing.
        for source in [
            "proptest! {\n    #[test]\n    fn t(x in 0..9u32) { assert_eq!(entry(x), 1); }\n}\n",
            "quickcheck! { fn t(x: u32) -> bool { entry(x) == 1 } }\n",
            "thread_local! { static CACHE: u32 = entry(1); }\n",
            "emit!(entry(1));\n",
        ] {
            assert!(!macro_may_define_item_named(source, "entry"), "{source}");
        }
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

    #[test]
    fn an_item_spelled_in_external_macro_arguments_defines_the_name() {
        for source in [
            "emit!(fn name() {});\n",
            "emit!(fn name(x: u8) -> u8 { x });\n",
            "emit!(#[inline] fn name() {});\n",
            "emit!(pub const fn name() {});\n",
        ] {
            assert!(macro_may_define_item_named(source, "name"), "{source}");
        }
        // Controls: a call, a method call and another item's name bind nothing.
        for source in [
            "emit!(name());\n",
            "emit!(other.name());\n",
            "emit!(fn other() { name() });\n",
            "emit!(x::name());\n",
        ] {
            assert!(!macro_may_define_item_named(source, "name"), "{source}");
        }
    }

    #[test]
    fn a_passthrough_local_macro_defines_the_item_its_caller_spells() {
        let tt = "macro_rules! emit {\n    ($($t:tt)*) => { $($t)* };\n}\n";
        assert!(macro_may_define_item_named(
            &format!("{tt}emit!(fn name() {{}});\n"),
            "name"
        ));
        let item = "macro_rules! emit {\n    ($i:item) => { $i };\n}\n";
        assert!(macro_may_define_item_named(
            &format!("{item}emit!(fn name() {{}});\n"),
            "name"
        ));
        // Controls: the caller spells another item, or only calls the name, or
        // the macro does not forward its arguments.
        assert!(!macro_may_define_item_named(
            &format!("{tt}emit!(fn other() {{}});\n"),
            "name"
        ));
        assert!(!macro_may_define_item_named(
            &format!("{tt}emit!(fn other() {{ name() }});\n"),
            "name"
        ));
        let fixed = "macro_rules! emit {\n    ($e:expr) => { let _ = $e; };\n}\n";
        assert!(!macro_may_define_item_named(
            &format!("{fixed}emit!(fn name() {{}});\n"),
            "name"
        ));
    }

    #[test]
    fn a_plain_import_inside_an_invoked_macro_defines_the_name() {
        let body = |import: &str| {
            format!("macro_rules! fake {{\n    () => {{ {import} }};\n}}\nfake!();\n")
        };
        for import in ["use other::name;", "use other::{a, name};", "use name;"] {
            assert!(
                macro_may_define_item_named(&body(import), "name"),
                "{import}"
            );
        }
        // Controls: a rename binds the new name; a mid-path segment binds the
        // leaf; a differently named import binds nothing.
        for import in [
            "use other::name as x;",
            "use name::leaf;",
            "use other::thing;",
        ] {
            assert!(
                !macro_may_define_item_named(&body(import), "name"),
                "{import}"
            );
        }
    }

    #[test]
    fn a_name_nested_in_an_inner_macro_call_is_still_a_bare_argument() {
        for source in [
            "outer!(inner!(name));\n",
            "outer!(a, inner![name, u8]);\n",
            "outer!(inner!(deeper!(name)));\n",
            "outer!(inner!{ name });\n",
            "outer!(path::inner!(name));\n",
        ] {
            assert!(macro_may_define_item_named(source, "name"), "{source}");
        }
        // Controls: a call or a plain braced body nested in the inner macro
        // still binds nothing.
        for source in [
            "outer!(inner!(name()));\n",
            "outer!(inner!({ name(1) }));\n",
            "outer!((name));\n",
        ] {
            assert!(!macro_may_define_item_named(source, "name"), "{source}");
        }
    }
}
