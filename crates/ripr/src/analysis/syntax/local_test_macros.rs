//! Bounded expansion of same-file `macro_rules!` test generators (#5334).
//!
//! A test file can define a macro whose transcriber emits `#[test] fn`
//! and then invoke it once per case:
//!
//! ```text
//! macro_rules! age_case {
//!     ($name:ident, $age:expr, $want:expr) => {
//!         #[test]
//!         fn $name() { assert_eq!(is_adult($age), $want); }
//!     };
//! }
//! age_case!(seventeen_is_minor, 17, false);
//! ```
//!
//! The parser sees only opaque token trees, so these tests were never
//! indexed and the code they call read as unreached (`ungripped`, a false
//! gap). This module is not a macro evaluator. It expands exactly one shape
//! and refuses everything else:
//!
//! - the definition is the only `macro_rules!` of that name in the file,
//!   sits directly in the file or an inline module, and precedes the
//!   invocation, which sits in the same scope or a nested one;
//! - the invocation is at item position;
//! - every arm up to the selected one has a matcher of comma-separated
//!   `$name:fragment` metavariables only, and the selected arm is the first
//!   whose metavariable count matches the invocation's top-level arguments
//!   (an `ident`, `literal`, `block` or `tt` argument must also have that
//!   shape);
//! - the selected transcriber contains `#[test]` and no repetition.
//!
//! Each metavariable is replaced by its argument's source text (an `expr`
//! argument of more than one element is wrapped in parentheses to keep its
//! precedence). The result
//! is plain Rust that the ordinary file-fact producer parses.

use ra_ap_syntax::ast::{HasAttrs, HasName};
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, ast};
use std::collections::{BTreeMap, BTreeSet};

use super::nesting::parse_clean_source_file;
use super::owner_pin::{OwnerPinAssertions, owner_pin_assertions, supported_item_context};
use super::ra::{LineIndex, summarize_file_with_parser};
use crate::analysis::facts::TestFact;
use std::path::Path;

/// One invocation of a same-file test-generating macro, expanded to Rust
/// source text. Lines are 1-based and name the invocation in the original
/// file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpandedLocalTestMacro {
    pub macro_name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

/// Expands every item-position invocation of a same-file test-generating
/// `macro_rules!` that fits the bounded shape in the module docs. A source
/// that does not parse cleanly yields nothing.
pub(crate) fn expand_local_test_macros(source: &str) -> Vec<ExpandedLocalTestMacro> {
    if !source.contains("macro_rules") || !source.contains("test") {
        return Vec::new();
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let root = parse.tree();
    let (definitions, local_macros) = local_test_macro_definitions(root.syntax());
    if definitions.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(source);
    let mut expanded = Vec::new();
    for call in root.syntax().descendants().filter_map(ast::MacroCall::cast) {
        if !is_item_position(call.syntax()) {
            continue;
        }
        let Some(name) = call.path().map(|path| path.syntax().text().to_string()) else {
            continue;
        };
        let Some(definition) = definitions.get(&name) else {
            continue;
        };
        // An attribute on the call (`#[cfg(..)]`) or a cfg on an enclosing
        // module or the file decides whether rustc compiles the generated
        // test at all; refuse rather than index a test that may not exist.
        if !in_definition_scope(&call, definition)
            || call.attrs().next().is_some()
            || !supported_item_context(call.syntax())
        {
            continue;
        }
        let Some(arguments) = call
            .token_tree()
            .and_then(|tree| top_level_arguments(&tree))
        else {
            continue;
        };
        let Some(text) = expand(definition, &arguments, &local_macros) else {
            continue;
        };
        let range = call.syntax().text_range();
        expanded.push(ExpandedLocalTestMacro {
            macro_name: name,
            start_line: lines.line(range.start()),
            end_line: lines.line_for_range_end(range.end()),
            text,
        });
    }
    expanded
}

/// Assertion-admission facts for the tests one invocation generates, in
/// the expansion's own line coordinates. The index pins a generated test's
/// lines to its invocation; admission reads the expanded text, where the
/// test is an ordinary `#[test] fn`.
#[derive(Clone, Debug)]
pub(crate) struct GeneratedTestPins {
    pub start_line: usize,
    pub end_line: usize,
    pub pins: OwnerPinAssertions,
    pub tests: Vec<TestFact>,
}

/// [`GeneratedTestPins`] for every expandable invocation in one file.
pub(crate) fn generated_test_pins(
    path: &Path,
    source: &str,
    trusted: &[&str],
) -> Vec<GeneratedTestPins> {
    expand_local_test_macros(source)
        .into_iter()
        .filter_map(|expansion| {
            let tests = summarize_file_with_parser(path, &expansion.text)
                .ok()?
                .tests;
            Some(GeneratedTestPins {
                start_line: expansion.start_line,
                end_line: expansion.end_line,
                pins: owner_pin_assertions(&expansion.text, trusted),
                tests,
            })
        })
        .collect()
}

/// Unique same-file definitions whose body mentions a `#[test]` attribute,
/// and the name of every macro the file defines.
fn local_test_macro_definitions(
    root: &SyntaxNode,
) -> (BTreeMap<String, ast::MacroRules>, BTreeSet<String>) {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut candidates = BTreeMap::new();
    for node in root.descendants() {
        let rule = ast::MacroRules::cast(node.clone());
        let name = rule
            .as_ref()
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        let Some(name) = name else {
            continue;
        };
        let name = name.text().to_string();
        *counts.entry(name.clone()).or_default() += 1;
        // `supported_item_context` also refuses a definition behind a cfg:
        // another `case!` may be in scope when it is off.
        if let Some(rule) = rule
            && !name.starts_with("r#")
            && is_item_position(rule.syntax())
            && supported_item_context(rule.syntax())
            && rule
                .token_tree()
                .is_some_and(|tree| has_test_attribute(&tree))
        {
            candidates.insert(name, rule);
        }
    }
    candidates.retain(|name, _| counts.get(name) == Some(&1));
    (candidates, counts.into_keys().collect())
}

fn is_item_position(node: &SyntaxNode) -> bool {
    node.parent().is_some_and(|parent| {
        ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
    })
}

fn in_definition_scope(call: &ast::MacroCall, definition: &ast::MacroRules) -> bool {
    let Some(scope) = definition.syntax().parent() else {
        return false;
    };
    definition.syntax().text_range().end() <= call.syntax().text_range().start()
        && call.syntax().ancestors().any(|ancestor| ancestor == scope)
}

fn has_test_attribute(tree: &ast::TokenTree) -> bool {
    let tokens = code_tokens(tree.syntax());
    tokens.windows(4).any(|window| {
        window[0].kind() == SyntaxKind::POUND
            && window[1].kind() == SyntaxKind::L_BRACK
            && window[2].text() == "test"
            && window[3].kind() == SyntaxKind::R_BRACK
    })
}

fn code_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect()
}

/// A token tree's direct children without its delimiters or trivia.
fn inner_elements(tree: &SyntaxNode) -> Vec<NodeOrToken<SyntaxNode, SyntaxToken>> {
    let mut elements: Vec<_> = tree
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect();
    if elements.len() < 2 {
        return Vec::new();
    }
    elements.remove(0);
    elements.pop();
    elements
}

/// One invocation argument: its source text and the kinds of its
/// top-level elements.
struct Argument {
    text: String,
    kinds: Vec<SyntaxKind>,
}

impl Argument {
    fn single(&self) -> Option<SyntaxKind> {
        match self.kinds.as_slice() {
            [kind] => Some(*kind),
            _ => None,
        }
    }
}

/// Splits an invocation's token tree at top-level commas. A trailing comma
/// is dropped; an empty argument between commas refuses.
fn top_level_arguments(tree: &ast::TokenTree) -> Option<Vec<Argument>> {
    let elements = inner_elements(tree.syntax());
    let mut arguments = Vec::new();
    let mut current: Vec<NodeOrToken<SyntaxNode, SyntaxToken>> = Vec::new();
    for element in elements {
        if element.kind() == SyntaxKind::COMMA {
            arguments.push(argument(&current)?);
            current.clear();
        } else {
            current.push(element);
        }
    }
    if !current.is_empty() {
        arguments.push(argument(&current)?);
    }
    Some(arguments)
}

fn argument(elements: &[NodeOrToken<SyntaxNode, SyntaxToken>]) -> Option<Argument> {
    let first = elements.first()?;
    let last = elements.last()?;
    let start = first.text_range().start();
    let end = last.text_range().end();
    let parent = match first {
        NodeOrToken::Node(node) => node.parent()?,
        NodeOrToken::Token(token) => token.parent()?,
    };
    let base = parent.text_range().start();
    let parent_text = parent.text().to_string();
    let text = parent_text
        .get(usize::from(start - base)..usize::from(end - base))?
        .to_string();
    Some(Argument {
        text,
        kinds: elements.iter().map(|element| element.kind()).collect(),
    })
}

/// A matcher of comma-separated `$name:fragment` metavariables, as
/// `(name, fragment)` pairs; `None` for any other matcher.
fn simple_matcher(matcher: &SyntaxNode) -> Option<Vec<(String, String)>> {
    let elements = inner_elements(matcher);
    let mut variables = Vec::new();
    let mut index = 0;
    while index < elements.len() {
        let dollar = elements.get(index)?;
        let name = elements.get(index + 1)?;
        let colon = elements.get(index + 2)?;
        let fragment = elements.get(index + 3)?;
        if dollar.kind() != SyntaxKind::DOLLAR
            || !is_identifier(name.kind())
            || colon.kind() != SyntaxKind::COLON
            || !is_identifier(fragment.kind())
        {
            return None;
        }
        variables.push((element_text(name), element_text(fragment)));
        index += 4;
        match elements.get(index) {
            None => break,
            Some(separator) if separator.kind() == SyntaxKind::COMMA => index += 1,
            Some(_) => return None,
        }
    }
    Some(variables)
}

fn is_identifier(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::IDENT || kind.is_keyword(ra_ap_syntax::Edition::CURRENT)
}

fn element_text(element: &NodeOrToken<SyntaxNode, SyntaxToken>) -> String {
    match element {
        NodeOrToken::Node(node) => node.text().to_string(),
        NodeOrToken::Token(token) => token.text().to_string(),
    }
}

/// Whether rustc would match `argument` against `fragment`: `Some(false)`
/// moves on to the next arm, as rustc does; `None` means ripr cannot tell,
/// and the invocation is refused rather than guessing an arm.
fn argument_fits(fragment: &str, argument: &Argument) -> Option<bool> {
    match fragment {
        "ident" => match argument.single() {
            Some(SyntaxKind::IDENT) => Some(true),
            Some(kind) if kind.is_keyword(ra_ap_syntax::Edition::CURRENT) => None,
            _ => Some(false),
        },
        "literal" => Some(match argument.kinds.as_slice() {
            [kind] => is_literal_fragment(*kind),
            [SyntaxKind::MINUS, kind] => kind.is_literal(),
            _ => false,
        }),
        "block" => Some(
            argument.single() == Some(SyntaxKind::TOKEN_TREE) && argument.text.starts_with('{'),
        ),
        // The token-tree parser splits joint punctuation (`..`, `::`, `->`)
        // that rustc reads as one `tt`, so several elements are uncertain.
        "tt" => argument.single().is_some().then_some(true),
        "expr" => Some(is_expression(&argument.text)),
        _ => None,
    }
}

fn is_literal_fragment(kind: SyntaxKind) -> bool {
    kind.is_literal() || matches!(kind, SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW)
}

fn is_expression(text: &str) -> bool {
    parse_clean_source_file(&format!("fn __ripr_fragment() {{ let _ = {text}; }}")).is_some()
}

/// Selects the first arm the arguments fit, refusing when an earlier arm
/// is outside the bounded shape, then substitutes into its transcriber.
fn expand(
    definition: &ast::MacroRules,
    arguments: &[Argument],
    local_macros: &BTreeSet<String>,
) -> Option<String> {
    let body = definition.token_tree()?;
    let elements = inner_elements(body.syntax());
    let mut index = 0;
    while index < elements.len() {
        let matcher = elements.get(index)?.as_node()?.clone();
        let eq = elements.get(index + 1)?;
        let gt = elements.get(index + 2)?;
        let transcriber = elements.get(index + 3)?.as_node()?.clone();
        if eq.kind() != SyntaxKind::EQ || gt.kind() != SyntaxKind::R_ANGLE {
            return None;
        }
        index += 4;
        if elements
            .get(index)
            .is_some_and(|separator| separator.kind() == SyntaxKind::SEMICOLON)
        {
            index += 1;
        }
        let variables = simple_matcher(&matcher)?;
        if variables.len() != arguments.len() {
            continue;
        }
        let mut fits = true;
        for ((_, fragment), argument) in variables.iter().zip(arguments) {
            fits &= argument_fits(fragment, argument)?;
        }
        if !fits {
            continue;
        }
        let tree = ast::TokenTree::cast(transcriber.clone())?;
        // A file-local macro in the transcriber (a discarding catch-all,
        // a helper, the generator itself) changes what the expansion runs,
        // and the expansion is parsed without the file's definitions.
        if !has_test_attribute(&tree) || invokes_local_macro(&tree, local_macros) {
            return None;
        }
        // Arguments are spliced in too, so a local macro named or passed
        // in an argument is refused the same way.
        let text = substitute(&transcriber, &variables, arguments)?;
        return (!text_invokes_local_macro(&text, local_macros)).then_some(text);
    }
    None
}

fn invokes_local_macro(tree: &ast::TokenTree, local_macros: &BTreeSet<String>) -> bool {
    let tokens = code_tokens(tree.syntax());
    tokens.windows(2).any(|window| {
        window[1].kind() == SyntaxKind::BANG && local_macros.contains(window[0].text())
    })
}

fn text_invokes_local_macro(text: &str, local_macros: &BTreeSet<String>) -> bool {
    let Some(parse) = parse_clean_source_file(text) else {
        return true;
    };
    let tokens = code_tokens(parse.tree().syntax());
    tokens.windows(2).any(|window| {
        window[1].kind() == SyntaxKind::BANG && local_macros.contains(window[0].text())
    })
}

fn substitute(
    transcriber: &SyntaxNode,
    variables: &[(String, String)],
    arguments: &[Argument],
) -> Option<String> {
    let mut tokens: Vec<SyntaxToken> = transcriber
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .collect();
    if tokens.len() < 2 {
        return None;
    }
    tokens.remove(0);
    tokens.pop();
    let mut text = String::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind() != SyntaxKind::DOLLAR {
            text.push_str(token.text());
            index += 1;
            continue;
        }
        let name = tokens.get(index + 1)?;
        if name.text() == "crate" {
            text.push_str("crate");
        } else {
            let position = variables
                .iter()
                .position(|(variable, _)| variable == name.text())?;
            let argument = arguments.get(position)?;
            // A one-token argument needs no grouping; wrapping it would
            // hide a literal from the value readers.
            if matches!(variables[position].1.as_str(), "expr" | "literal")
                && argument.single().is_none()
            {
                text.push('(');
                text.push_str(&argument.text);
                text.push(')');
            } else {
                text.push_str(&argument.text);
            }
        }
        index += 2;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGE_CASES: &str = "macro_rules! age_case {\n    ($name:ident, $age:expr, $want:expr) => {\n        #[test]\n        fn $name() {\n            assert_eq!(macrocase::is_adult($age), $want);\n        }\n    };\n}\n\nage_case!(seventeen_is_minor, 17, false);\nage_case!(eighteen_is_adult, 18 + 0, true);\n";

    #[test]
    fn expands_each_invocation_with_its_arguments_and_lines() {
        let expanded = expand_local_test_macros(AGE_CASES);
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].macro_name, "age_case");
        assert_eq!((expanded[0].start_line, expanded[0].end_line), (10, 10));
        assert!(
            expanded[0].text.contains(
                "fn seventeen_is_minor() {\n            assert_eq!(macrocase::is_adult(17), false);"
            ),
            "{}",
            expanded[0].text
        );
        assert_eq!(expanded[1].start_line, 11);
        assert!(expanded[1].text.contains("is_adult((18 + 0))"));
    }

    #[test]
    fn a_macro_without_a_test_attribute_is_not_expanded() {
        let source = "macro_rules! helper {\n    ($x:expr) => { fn made() -> i32 { $x } };\n}\nhelper!(1);\n";
        assert!(expand_local_test_macros(source).is_empty());
    }

    #[test]
    fn repetition_shadowing_scope_and_order_refuse() {
        let repetition = "macro_rules! cases {\n    ($($name:ident),*) => { $( #[test] fn $name() {} )* };\n}\ncases!(a, b);\n";
        assert!(expand_local_test_macros(repetition).is_empty());

        let duplicate =
            format!("{AGE_CASES}mod other {{ macro_rules! age_case {{ () => {{}}; }} }}\n");
        assert!(expand_local_test_macros(&duplicate).is_empty());

        let before = "age_case!(early, 1, false);\nmacro_rules! age_case {\n    ($name:ident, $age:expr, $want:expr) => { #[test] fn $name() { assert_eq!(f($age), $want); } };\n}\n";
        assert!(expand_local_test_macros(before).is_empty());

        let sibling_module = "mod a {\n    macro_rules! case {\n        ($name:ident) => { #[test] fn $name() { f(); } };\n    }\n}\nmod b {\n    case!(outside);\n}\n";
        assert!(expand_local_test_macros(sibling_module).is_empty());
    }

    #[test]
    fn arm_selection_follows_count_and_fragment_shape() {
        let source = "macro_rules! case {\n    ($name:ident, $v:literal) => { #[test] fn $name() { assert!(lit($v)); } };\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(expr($v)); } };\n}\ncase!(one, 1);\ncase!(two, 1 + 1);\ncase!(3, 3);\n";
        let expanded = expand_local_test_macros(source);
        assert_eq!(expanded.len(), 2, "{expanded:?}");
        assert!(expanded[0].text.contains("lit(1)"));
        assert!(expanded[1].text.contains("expr((1 + 1))"));
    }

    #[test]
    fn cfg_context_local_macros_and_uncertain_fragments_refuse() {
        let generator = "macro_rules! case {\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(gate($v)); } };\n}\n";
        assert_eq!(
            expand_local_test_macros(&format!("{generator}case!(one, 1);\n")).len(),
            1
        );
        for invocation in [
            "#[cfg(any())]\ncase!(one, 1);\n",
            "#[cfg(feature = \"never\")]\nmod m {\n    case!(one, 1);\n}\n",
        ] {
            assert!(
                expand_local_test_macros(&format!("{generator}{invocation}")).is_empty(),
                "{invocation}"
            );
        }
        let file_cfg = format!("#![cfg(feature = \"slow\")]\n{generator}case!(one, 1);\n");
        assert!(expand_local_test_macros(&file_cfg).is_empty());

        let discarding = "macro_rules! skip {\n    ($($t:tt)*) => {};\n}\nmacro_rules! case {\n    ($name:ident, $v:expr) => { #[test] fn $name() { skip!(assert!(gate($v))); } };\n}\ncase!(one, 1);\n";
        assert!(expand_local_test_macros(discarding).is_empty());

        let negative_literal = "macro_rules! case {\n    ($name:ident, $v:literal) => { #[test] fn $name() { let _ = gate($v); } };\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(gate($v)); } };\n}\ncase!(one, -1);\n";
        let expanded = expand_local_test_macros(negative_literal);
        assert_eq!(expanded.len(), 1);
        assert!(
            expanded[0].text.contains("let _ = gate((-1))"),
            "{}",
            expanded[0].text
        );

        let boolean_literal = "macro_rules! case {\n    ($name:ident, $v:literal) => { #[test] fn $name() { let _ = gate($v); } };\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(gate($v)); } };\n}\ncase!(one, true);\n";
        let expanded = expand_local_test_macros(boolean_literal);
        assert_eq!(expanded.len(), 1);
        assert!(
            expanded[0].text.contains("let _ = gate(true)"),
            "{}",
            expanded[0].text
        );

        let joint_punctuation = "macro_rules! case {\n    ($name:ident, $v:tt) => { #[test] fn $name() { let _ = gate($v); } };\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(gate($v)); } };\n}\ncase!(one, ..);\n";
        assert!(expand_local_test_macros(joint_punctuation).is_empty());

        let argument_macro = "macro_rules! skip {\n    ($($t:tt)*) => {};\n}\nmacro_rules! case {\n    ($name:ident, $b:block) => { #[test] fn $name() $b };\n}\ncase!(one, { skip!(let _ = gate(10);); assert!(!gate(9)); });\n";
        assert!(expand_local_test_macros(argument_macro).is_empty());

        let gated_definition = "#[cfg(feature = \"x\")]\nmacro_rules! case {\n    ($name:ident) => { #[test] fn $name() { assert!(gate(1)); } };\n}\ncase!(one);\n";
        assert!(expand_local_test_macros(gated_definition).is_empty());

        let unknown_fragment = "macro_rules! case {\n    ($name:ident, $v:ty) => { #[test] fn $name() { assert!(gate::<$v>()); } };\n}\ncase!(one, u8);\n";
        assert!(expand_local_test_macros(unknown_fragment).is_empty());

        let keyword_ident = "macro_rules! case {\n    ($name:ident) => { #[test] fn $name() { assert!(gate(1)); } };\n}\ncase!(r#match);\ncase!(self);\n";
        assert_eq!(expand_local_test_macros(keyword_ident).len(), 1);
    }

    #[test]
    fn an_earlier_arm_outside_the_shape_refuses() {
        let source = "macro_rules! case {\n    (@inner $name:ident) => { fn $name() {} };\n    ($name:ident) => { #[test] fn $name() { f(); } };\n}\ncase!(one);\n";
        assert!(expand_local_test_macros(source).is_empty());
    }
}
