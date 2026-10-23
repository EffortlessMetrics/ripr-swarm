//! `#[test]` functions written inside a verbatim pass-through item macro
//! (#8999 item 5, RIPR-SPEC-0117).
//!
//! A test file can wrap hand-written tests in a same-file macro that only
//! re-emits its input:
//!
//! ```text
//! macro_rules! suite {
//!     ($($t:tt)*) => { $($t)* };
//! }
//! suite! {
//!     #[test]
//!     fn adults() { assert!(is_adult(18)); }
//! }
//! ```
//!
//! The parser sees an opaque token tree, so the wrapped tests were never
//! indexed. An arbitrary item-position macro may drop, rename, gate or
//! wrap its input, so this pass checks the macro is the identity before it
//! reads the invocation's own tokens as source, and refuses everything
//! else:
//!
//! - the macro is defined once in the file, directly in the file or an
//!   inline module, before the invocation and in an enclosing scope, with
//!   no `use` importing its name and no `cfg` on the definition;
//! - the definition has exactly one arm, whose matcher is
//!   `$($name:tt)*` or `$($name:item)*` and whose transcriber is exactly
//!   `$($name)*`;
//! - the invocation is at item position, has no attribute, and carries no
//!   `$` metavariable, raw identifier, `include!`, file-local macro call,
//!   `cfg`/`cfg_attr` (other than `cfg(test)`) or macro definition;
//! - the tokens parse cleanly as complete `fn` items, at least one with a
//!   literal `#[test]`.
//!
//! The expansion is the invocation's own text, so the ordinary file-fact
//! producer extracts the calls and assertions of the literal bodies.

use ra_ap_syntax::ast::{HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, TextSize, ast};
use std::collections::{BTreeMap, BTreeSet};

use super::local_test_macros::{
    ExpandedLocalTestMacro, code_tokens, imported_names, in_definition_scope, inner_elements,
    is_item_position, local_macro_names, text_has_conditional_attribute, text_invokes_local_macro,
};
use super::nesting::parse_clean_source_file;
use super::owner_pin::supported_item_context;
use super::ra::LineIndex;

pub(super) fn passthrough_item_expansions(
    root: &SyntaxNode,
    lines: &LineIndex,
    local_macros: &BTreeSet<String>,
) -> Vec<ExpandedLocalTestMacro> {
    identity_invocations(root, local_macros)
        .into_iter()
        .map(|(call, macro_name, text)| {
            let range = call.syntax().text_range();
            ExpandedLocalTestMacro {
                macro_name,
                start_line: lines.line(range.start()),
                end_line: lines.line_for_range_end(range.end()),
                text,
            }
        })
        .collect()
}

/// Start offsets of the invocations this pass accepts as the identity over
/// literal `fn` items. Such a call cannot define, import or rebind a macro
/// name, so the workspace macro-binding scan need not treat its mention of
/// `assert_eq` as a possible definition; the expansion is scanned as
/// ordinary source instead.
pub(super) fn passthrough_invocation_starts(root: &SyntaxNode) -> BTreeSet<TextSize> {
    let (_, local_macros) = local_macro_names(root);
    identity_invocations(root, &local_macros)
        .into_iter()
        .filter(|(call, _, _)| {
            // A function-local `use`/`extern crate` can alias or rebind a
            // macro name (`use crate::weak::weak as assert_eq;`), and a
            // macro-scope attribute can change what resolves. The expansion
            // is still indexed, but the call keeps counting as a possible
            // binding site so the mention stays ambiguous.
            !call.syntax().descendants_with_tokens().any(|element| {
                element.as_token().is_some_and(|token| {
                    matches!(
                        token.text(),
                        "use" | "extern" | "macro_use" | "no_implicit_prelude"
                    )
                })
            })
        })
        .map(|(call, _, _)| call.syntax().text_range().start())
        .collect()
}

fn identity_invocations(
    root: &SyntaxNode,
    local_macros: &BTreeSet<String>,
) -> Vec<(ast::MacroCall, String, String)> {
    let (counts, _) = local_macro_names(root);
    let definitions = passthrough_definitions(root, &counts);
    if definitions.is_empty() {
        return Vec::new();
    }
    let imported = imported_names(root);
    let mut accepted = Vec::new();
    for call in root.descendants().filter_map(ast::MacroCall::cast) {
        if !is_item_position(call.syntax()) {
            continue;
        }
        let Some(name) = call.path().map(|path| path.syntax().text().to_string()) else {
            continue;
        };
        let Some(definition) = definitions.get(&name) else {
            continue;
        };
        if !in_definition_scope(&call, definition)
            || imported.contains(&name)
            || call.attrs().next().is_some()
            || !supported_item_context(call.syntax())
            || shadowed_between(root, definition, &call, &name)
        {
            continue;
        }
        let Some(text) = call
            .token_tree()
            .and_then(|tree| literal_test_items(tree.syntax(), local_macros))
        else {
            continue;
        };
        accepted.push((call, name, text));
    }
    accepted
}

/// Whether another item-position macro call sits between the definition and
/// this call. Its expansion is unseen and could define a macro of the same
/// name, so the identity proof does not extend past it. Calls of the same
/// identity macro do not count.
fn shadowed_between(
    root: &SyntaxNode,
    definition: &ast::MacroRules,
    call: &ast::MacroCall,
    name: &str,
) -> bool {
    let from = definition.syntax().text_range().end();
    let to = call.syntax().text_range().start();
    // An out-of-line `mod x;` (a `#[macro_use]` one in particular) pulls in
    // an unseen file that could redefine the name.
    let out_of_line_mod = root
        .descendants()
        .filter_map(ast::Module::cast)
        .any(|module| {
            let start = module.syntax().text_range().start();
            module.item_list().is_none() && start >= from && start < to
        });
    out_of_line_mod
        || root
            .descendants()
            .filter_map(ast::MacroCall::cast)
            .filter(|other| is_item_position(other.syntax()))
            .filter(|other| {
                let start = other.syntax().text_range().start();
                start >= from && start < to
            })
            .any(|other| {
                other
                    .path()
                    .map(|path| path.syntax().text().to_string())
                    .as_deref()
                    != Some(name)
            })
}

/// Unique same-file definitions that re-emit their input unchanged.
fn passthrough_definitions(
    root: &SyntaxNode,
    counts: &BTreeMap<String, usize>,
) -> BTreeMap<String, ast::MacroRules> {
    let mut definitions = BTreeMap::new();
    for rule in root.descendants().filter_map(ast::MacroRules::cast) {
        let Some(name) = rule.name().map(|name| name.text().to_string()) else {
            continue;
        };
        if name.starts_with("r#")
            || counts.get(&name) != Some(&1)
            || !is_item_position(rule.syntax())
            || !supported_item_context(rule.syntax())
            || !rule
                .token_tree()
                .is_some_and(|tree| is_identity_arm(tree.syntax()))
        {
            continue;
        }
        definitions.insert(name, rule);
    }
    definitions
}

/// Whether the macro body is exactly one `($($x:tt)*) => { $($x)* }` arm
/// (the fragment may also be `item`), with an optional trailing `;`.
fn is_identity_arm(body: &SyntaxNode) -> bool {
    let elements = inner_elements(body);
    let (matcher, eq, gt, transcriber) = match elements.as_slice() {
        [matcher, eq, gt, transcriber] => (matcher, eq, gt, transcriber),
        [matcher, eq, gt, transcriber, separator] if separator.kind() == SyntaxKind::SEMICOLON => {
            (matcher, eq, gt, transcriber)
        }
        _ => return false,
    };
    if eq.kind() != SyntaxKind::EQ || gt.kind() != SyntaxKind::R_ANGLE {
        return false;
    }
    let (Some(matcher), Some(transcriber)) = (matcher.as_node(), transcriber.as_node()) else {
        return false;
    };
    let Some(matcher) = delimited_text(matcher) else {
        return false;
    };
    let Some(transcriber) = delimited_text(transcriber) else {
        return false;
    };
    // Matcher `$($name:tt)*`, transcriber `$($name)*`, same `name`.
    let Some(rest) = matcher.strip_prefix("$($") else {
        return false;
    };
    let Some((name, fragment)) = rest
        .strip_suffix(")*")
        .and_then(|inner| inner.split_once(':'))
    else {
        return false;
    };
    is_plain_identifier(name)
        && matches!(fragment, "tt" | "item")
        && transcriber == format!("$(${name})*")
}

/// A token tree's code tokens between its delimiters, concatenated without
/// trivia. `None` unless the tree is a delimited group.
fn delimited_text(tree: &SyntaxNode) -> Option<String> {
    let tokens = code_tokens(tree);
    let (first, rest) = tokens.split_first()?;
    let (last, middle) = rest.split_last()?;
    let matched = matches!(
        (first.kind(), last.kind()),
        (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN)
            | (SyntaxKind::L_BRACK, SyntaxKind::R_BRACK)
            | (SyntaxKind::L_CURLY, SyntaxKind::R_CURLY)
    );
    matched.then(|| middle.iter().map(|token| token.text()).collect())
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && name != "crate"
}

/// The invocation's tokens as source text when they are complete `fn`
/// items, at least one carrying a literal `#[test]`, and nothing in them
/// could generate, gate or rename an item.
fn literal_test_items(tree: &SyntaxNode, local_macros: &BTreeSet<String>) -> Option<String> {
    let tokens = code_tokens(tree);
    if tokens.iter().any(|token| {
        token.kind() == SyntaxKind::DOLLAR
            || token.text().starts_with("r#")
            || token.text().starts_with("br#")
    }) {
        return None;
    }
    // `include!` splices unseen items; a file-local macro changes what the
    // body runs and the text is parsed without the file's definitions.
    let spliced = tokens.windows(2).any(|window| {
        window[1].kind() == SyntaxKind::BANG
            && matches!(window[0].text(), "include" | "macro_rules")
    });
    if spliced {
        return None;
    }
    let full = tree.text().to_string();
    let text = full.get(1..full.len().checked_sub(1)?)?.to_string();
    if text_invokes_local_macro(&text, local_macros) || text_has_conditional_attribute(&text) {
        return None;
    }
    let parse = parse_clean_source_file(&text)?;
    let file = parse.tree();
    let mut has_test = false;
    let mut items = 0;
    for item in file.items() {
        let ast::Item::Fn(function) = item else {
            return None;
        };
        items += 1;
        has_test |= function.attrs().any(|attr| {
            attr.syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect::<String>()
                == "#[test]"
        });
    }
    (items > 0 && has_test).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::super::expand_local_test_macros;
    use super::super::nesting::parse_clean_source_file;
    use super::passthrough_invocation_starts;

    const SUITE: &str = "macro_rules! suite {\n    ($($t:tt)*) => { $($t)* };\n}\n\n";

    fn expand(invocation: &str) -> Vec<String> {
        expand_local_test_macros(&format!("{SUITE}{invocation}"))
            .into_iter()
            .map(|expansion| expansion.text)
            .collect()
    }

    #[test]
    fn literal_tests_in_an_identity_macro_are_expanded_to_their_own_text() {
        let source = format!(
            "{SUITE}suite! {{\n    #[test]\n    fn adults() {{\n        assert!(is_adult(18));\n    }}\n}}\n"
        );
        let expanded = expand_local_test_macros(&source);
        assert_eq!(expanded.len(), 1, "{expanded:?}");
        assert_eq!(expanded[0].macro_name, "suite");
        assert_eq!((expanded[0].start_line, expanded[0].end_line), (5, 10));
        assert!(
            expanded[0].text.contains("assert!(is_adult(18));"),
            "{}",
            expanded[0].text
        );
        // The `item` fragment and a parenthesized invocation are the same
        // identity.
        let item_fragment = "macro_rules! suite {\n    ($($i:item)*) => { $($i)* }\n}\nsuite!(#[test] fn a() { f(); });\n";
        assert_eq!(expand_local_test_macros(item_fragment).len(), 1);
    }

    #[test]
    fn an_unseen_item_macro_between_definition_and_call_blocks_the_proof() {
        let call = "suite! { #[test] fn a() { f(); } }\n";
        let unseen = format!("{SUITE}unseen_generator! {{}}\n{call}");
        assert!(expand_local_test_macros(&unseen).is_empty());
        // Positive control: another identity call in between is harmless.
        let sibling = format!("{SUITE}{call}{call}");
        assert_eq!(expand_local_test_macros(&sibling).len(), 2);
    }

    #[test]
    fn a_macro_that_is_not_the_identity_expands_nothing() {
        let body = "suite! { #[test] fn a() { f(); } }\n";
        for definition in [
            // Drops its input.
            "macro_rules! suite {\n    ($($t:tt)*) => {};\n}\n",
            // Gates its input.
            "macro_rules! suite {\n    ($($t:tt)*) => { #[cfg(feature = \"x\")] mod m { $($t)* } };\n}\n",
            // Wraps its input.
            "macro_rules! suite {\n    ($($t:tt)*) => { mod m { $($t)* } };\n}\n",
            // Different name in the transcriber.
            "macro_rules! suite {\n    ($($t:tt)*) => { $($u)* };\n}\n",
            // Two arms.
            "macro_rules! suite {\n    (skip) => {};\n    ($($t:tt)*) => { $($t)* };\n}\n",
            // Separator or fragment that is not the identity.
            "macro_rules! suite {\n    ($($t:tt),*) => { $($t)* };\n}\n",
            "macro_rules! suite {\n    ($($t:expr)*) => { $($t)* };\n}\n",
            // Behind a cfg.
            "#[cfg(feature = \"x\")]\nmacro_rules! suite {\n    ($($t:tt)*) => { $($t)* };\n}\n",
        ] {
            assert!(
                expand_local_test_macros(&format!("{definition}{body}")).is_empty(),
                "{definition}"
            );
        }
        // Control: the identity definition accepts the same body.
        assert_eq!(expand(body).len(), 1);
    }

    #[test]
    fn unknown_imported_duplicate_late_and_pathed_macros_expand_nothing() {
        let body = "#[test] fn a() { f(); }";
        // Not defined in this file (an arbitrary macro may gate or drop).
        assert!(
            expand_local_test_macros(&format!(
                "macro_rules! unused {{ () => {{}}; }}\nother! {{ {body} }}\n"
            ))
            .is_empty()
        );
        // Invoked before its definition.
        assert!(expand_local_test_macros(&format!("suite! {{ {body} }}\n{SUITE}")).is_empty());
        // Defined twice.
        assert!(expand(&format!("mod m {{ {SUITE} }}\nsuite! {{ {body} }}\n")).is_empty());
        // Imported name may select a different macro.
        assert!(
            expand_local_test_macros(&format!("use other::suite;\n{SUITE}suite! {{ {body} }}\n"))
                .is_empty()
        );
        // Pathed invocation names no local definition.
        assert!(expand(&format!("crate::suite! {{ {body} }}\n")).is_empty());
        // Attribute on the invocation.
        assert!(expand(&format!("#[cfg(any())]\nsuite! {{ {body} }}\n")).is_empty());
        // Statement position is not item position.
        assert!(expand(&format!("fn host() {{ suite! {{ {body} }} }}\n")).is_empty());
        // Control.
        assert_eq!(expand(&format!("suite! {{ {body} }}\n")).len(), 1);
    }

    #[test]
    fn rebinding_and_nested_module_variants_expand_nothing() {
        let body = "#[test] fn a() { f(); }";
        // A second local definition shadowing the name inside an inline module.
        assert!(
            expand(&format!(
                "mod m {{ macro_rules! suite {{ ($($t:tt)*) => {{}}; }} suite! {{ {body} }} }}\n"
            ))
            .is_empty()
        );
        // A rebinding import alias of the same name.
        assert!(
            expand_local_test_macros(&format!(
                "{SUITE}use other::real as suite;\nsuite! {{ {body} }}\n"
            ))
            .is_empty()
        );
        // An alias in a nested inline module.
        assert!(
            expand_local_test_macros(&format!(
                "{SUITE}mod m {{ use other::real as suite; suite! {{ {body} }} }}\n"
            ))
            .is_empty()
        );
        // Textual scope reaches nested inline modules, so an unshadowed
        // invocation there is still the identity (supported, not a gap).
        assert_eq!(
            expand(&format!("mod m {{ mod n {{ suite! {{ {body} }} }} }}\n")).len(),
            1
        );
        // Definition nested in a module, invocation outside its scope.
        assert!(
            expand_local_test_macros(&format!("mod m {{ {SUITE} }}\nsuite! {{ {body} }}\n"))
                .is_empty()
        );
        // An out-of-line module between definition and call may redefine it.
        assert!(expand(&format!("#[macro_use]\nmod other;\nsuite! {{ {body} }}\n")).is_empty());
        // Control.
        assert_eq!(expand(&format!("suite! {{ {body} }}\n")).len(), 1);
    }

    #[test]
    fn binding_capable_tokens_keep_the_call_a_possible_binding_site() {
        let starts = |body: &str| {
            let source = format!("{SUITE}suite! {{ {body} }}\n");
            let parse = parse_clean_source_file(&source).map(|parse| parse.syntax_node());
            parse.map(|root| passthrough_invocation_starts(&root).len())
        };
        assert_eq!(starts("#[test] fn a() { f(); }"), Some(1));
        for body in [
            "#[test] fn a() { use crate::weak::weak as assert_eq; assert_eq!(f(), 1); }",
            "#[test] fn a() { extern crate weak as assert_eq; f(); }",
            "#[macro_use] #[test] fn a() { f(); }",
            "#[no_implicit_prelude] #[test] fn a() { f(); }",
        ] {
            assert_eq!(starts(body), Some(0), "{body}");
            // Still expanded: the tests stay indexed.
            assert_eq!(expand(&format!("suite! {{ {body} }}\n")).len(), 1, "{body}");
        }
    }

    #[test]
    fn invocation_tokens_that_could_generate_gate_or_rename_items_expand_nothing() {
        for tokens in [
            // Not a `#[test]` function.
            "fn helper() { f(); }",
            "#[test] fn a() { f(); } struct S;",
            // Nested generated names and macro items.
            "#[test] fn a() { f(); } other_tests! {}",
            "#[test] fn a() { f(); } macro_rules! inner { () => {}; }",
            "#[test] fn a() { f(); } include!(\"more.rs\");",
            "#[test] fn a() { include!(\"body.rs\"); }",
            // Metavariable, raw identifier, cfg.
            "#[test] fn a() { f($x); }",
            "#[test] fn r#match() { f(); }",
            "#[test] fn a() { r#fn(); }",
            "#[cfg(feature = \"x\")] #[test] fn a() { f(); }",
            "#[cfg_attr(feature = \"x\", ignore)] #[test] fn a() { f(); }",
            // The wrapper (or another file-local macro) in a body.
            "#[test] fn a() { suite!(f()); }",
            // Incomplete or unparsable items.
            "#[test] fn a() { f(); ",
            "#[test] fn a(",
        ] {
            assert!(
                expand(&format!("suite! {{ {tokens} }}\n")).is_empty(),
                "{tokens}"
            );
        }
        // Controls: cfg(test), should_panic and several tests are accepted.
        let accepted = expand(
            "suite! {\n#[cfg(test)] #[test] fn a() { f(); }\n#[test] #[should_panic] fn b() { g(); }\n}\n",
        );
        assert_eq!(accepted.len(), 1);
        assert!(accepted[0].contains("fn b()"));
    }
}
