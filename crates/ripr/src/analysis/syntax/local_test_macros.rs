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
//! - the selected transcriber contains `#[test]`; its only repetition is
//!   the table form below;
//! - no `use` in the file imports the generator's name, and the expansion
//!   carries no `cfg` (other than `cfg(test)`) or `cfg_attr` attribute.
//!
//! The table form is a whole-matcher repetition `$( row ) sep? *` (or `+`)
//! whose row is `$name:ident`, `literal`, `tt` or `block` variables,
//! literal `,`, `;` or `:` tokens, and delimited groups of comma-separated
//! metavariables (an `expr` outside a group needs a row separator after it),
//! paired with a transcriber that is exactly one separator-free
//! `$( body )*`. The invocation must split into rows with nothing left
//! over; the body is emitted once per row. This is itoa's
//! `test! { name(value, expected) ... }` table (#6925).
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
    // Pure in the source text; every overlapping index of a diff run expands
    // the same files again (#5363).
    super::run_memo::memoize(super::run_memo::Product::LocalTestMacros, 0, source, || {
        expand_local_test_macros_uncached(source)
    })
}

fn expand_local_test_macros_uncached(source: &str) -> Vec<ExpandedLocalTestMacro> {
    let Some(parse) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let root = parse.tree();
    let (definitions, local_macros) = local_test_macro_definitions(root.syntax());
    if definitions.is_empty() {
        return Vec::new();
    }
    let imported = imported_names(root.syntax());
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
            || imported.contains(&name)
            || call.attrs().next().is_some()
            || !supported_item_context(call.syntax())
        {
            continue;
        }
        let Some(tree) = call.token_tree() else {
            continue;
        };
        // A repetition arm reads the whole token tree, so a comma layout
        // `top_level_arguments` refuses is not yet a refusal.
        let arguments = top_level_arguments(&tree);
        let Some(text) = expand(definition, &tree, arguments.as_deref(), &local_macros) else {
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

pub(super) fn is_item_position(node: &SyntaxNode) -> bool {
    node.parent().is_some_and(|parent| {
        ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
    })
}

pub(super) fn in_definition_scope(call: &ast::MacroCall, definition: &ast::MacroRules) -> bool {
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
pub(super) fn inner_elements(tree: &SyntaxNode) -> Vec<NodeOrToken<SyntaxNode, SyntaxToken>> {
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
    // Sliced, not copied: a row's arguments share the invocation as parent,
    // and copying it per argument made a table's cost rows times its size.
    let base = parent.text_range().start();
    let text = parent
        .text()
        .slice(ra_ap_syntax::TextRange::new(start - base, end - base))
        .to_string();
    Some(Argument {
        text,
        kinds: elements.iter().map(|element| element.kind()).collect(),
    })
}

fn invocation_ends_with_comma(invocation: &ast::TokenTree) -> bool {
    inner_elements(invocation.syntax())
        .last()
        .is_some_and(|element| element.kind() == SyntaxKind::COMMA)
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
            // A comma after the last metavariable is a literal the
            // invocation must carry, which this reader does not model.
            Some(separator) if separator.kind() == SyntaxKind::COMMA => {
                index += 1;
                if index == elements.len() {
                    return None;
                }
            }
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
    invocation: &ast::TokenTree,
    arguments: Option<&[Argument]>,
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
        if let Some(repetition) = repetition_matcher(&matcher) {
            // The arm is the one rustc picks only when every row fits; a
            // row that does not could mean a later arm, which this
            // expander does not guess.
            let rows = match_rows(&repetition, invocation)?;
            return expand_rows(&transcriber, &rows, local_macros);
        }
        let variables = simple_matcher(&matcher)?;
        // `top_level_arguments` drops a trailing comma, which a matcher
        // without one would not accept: rustc moves to the next arm.
        if invocation_ends_with_comma(invocation) {
            return None;
        }
        let arguments = arguments?;
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
        let text = substitute(&transcriber, |name| {
            let position = variables
                .iter()
                .position(|(variable, _)| variable == name)?;
            Some((variables[position].1.as_str(), arguments.get(position)?))
        })?;
        return (!text_invokes_local_macro(&text, local_macros)
            && !text_has_conditional_attribute(&text))
        .then_some(text);
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

/// Whether the expansion carries a `cfg` or `cfg_attr` attribute other
/// than `cfg(test)`. Whether a `#[cfg(any())] #[test] fn` or a cfg'd
/// generated module exists in a test build is a configuration question
/// this expander does not answer, so such an expansion indexes nothing.
fn text_has_conditional_attribute(text: &str) -> bool {
    let Some(parse) = parse_clean_source_file(text) else {
        return true;
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Attr::cast)
        .any(|attr| {
            let text: String = attr
                .syntax()
                .text()
                .to_string()
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            let inner = text
                .trim_start_matches("#!")
                .trim_start_matches('#')
                .trim_start_matches('[')
                .trim_end_matches(']');
            inner.starts_with("cfg_attr") || (inner.starts_with("cfg") && inner != "cfg(test)")
        })
}

/// How many macros of each name a file defines, and the names a `use`
/// brings into scope.
pub(super) fn local_macro_names(root: &SyntaxNode) -> (BTreeMap<String, usize>, BTreeSet<String>) {
    let mut counts = BTreeMap::<String, usize>::new();
    for node in root.descendants() {
        let name = ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node).and_then(|item| item.name()));
        if let Some(name) = name {
            *counts.entry(name.text().to_string()).or_default() += 1;
        }
    }
    (counts, imported_names(root))
}

/// Names a `use` item in the file brings into scope. A named import of a
/// generator's name makes the invocation ambiguous for rustc (E0659), so
/// the expander does not guess which macro runs.
fn imported_names(root: &SyntaxNode) -> BTreeSet<String> {
    root.descendants()
        .filter_map(ast::UseTree::cast)
        .filter_map(|tree| {
            if let Some(rename) = tree.rename() {
                return rename.name().map(|name| name.text().to_string());
            }
            if tree.use_tree_list().is_some() || tree.star_token().is_some() {
                return None;
            }
            tree.path()
                .and_then(|path| path.segment())
                .map(|segment| segment.syntax().text().to_string())
        })
        .collect()
}

/// Replaces each `$name` in a transcriber's body (its delimiters
/// stripped) with the argument `lookup` binds it to, with that argument's
/// fragment kind.
fn substitute<'a>(
    transcriber: &SyntaxNode,
    lookup: impl Fn(&str) -> Option<(&'a str, &'a Argument)>,
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
            let (fragment, argument) = lookup(name.text())?;
            // A one-token argument needs no grouping; wrapping it would
            // hide a literal from the value readers.
            if matches!(fragment, "expr" | "literal") && argument.single().is_none() {
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

/// The most rows one repetition invocation may expand to.
const MAX_REPETITION_ROWS: usize = 512;

/// The most expansion text one table invocation may produce. Rows are
/// bounded, but rows times a large body is not: every generated test is
/// parsed and kept, so a big table of big bodies is refused whole.
const MAX_EXPANSION_BYTES: usize = 128 * 1024;

/// One element of a repetition matcher's row.
enum RowItem {
    /// `$name:fragment` outside any delimiter.
    Variable { name: String, fragment: String },
    /// A literal `,`, `;` or `:` between row elements.
    Separator(SyntaxKind),
    /// A delimited group of comma-separated metavariables, as in
    /// `name($value:expr, $want:expr)`.
    Group {
        open: char,
        variables: Vec<(String, String)>,
    },
}

/// `$( row ) sep? *` or `+` as a whole matcher, the shape of a test table.
struct RepetitionMatcher {
    row: Vec<RowItem>,
    separator: Option<SyntaxKind>,
}

/// What each row binds: variable name to (fragment kind, argument).
type RowBindings = BTreeMap<String, (String, Argument)>;

fn repetition_matcher(matcher: &SyntaxNode) -> Option<RepetitionMatcher> {
    let elements = inner_elements(matcher);
    let [dollar, row, rest @ ..] = elements.as_slice() else {
        return None;
    };
    if dollar.kind() != SyntaxKind::DOLLAR
        || row.kind() != SyntaxKind::TOKEN_TREE
        || !element_text(row).starts_with('(')
    {
        return None;
    }
    let (separator, repeat) = match rest {
        [repeat] => (None, repeat),
        [separator, repeat]
            if matches!(separator.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON) =>
        {
            (Some(separator.kind()), repeat)
        }
        _ => return None,
    };
    if !matches!(repeat.kind(), SyntaxKind::STAR | SyntaxKind::PLUS) {
        return None;
    }
    let items = row_items(row.as_node()?)?;
    Some(RepetitionMatcher {
        row: items,
        separator,
    })
}

fn row_items(row: &SyntaxNode) -> Option<Vec<RowItem>> {
    let elements = inner_elements(row);
    let mut items = Vec::new();
    let mut names = BTreeSet::new();
    let mut index = 0;
    while index < elements.len() {
        let element = elements.get(index)?;
        match element.kind() {
            SyntaxKind::DOLLAR => {
                let name = elements.get(index + 1)?;
                let colon = elements.get(index + 2)?;
                let fragment = elements.get(index + 3)?;
                if !is_identifier(name.kind())
                    || colon.kind() != SyntaxKind::COLON
                    || !is_identifier(fragment.kind())
                    || !names.insert(element_text(name))
                {
                    return None;
                }
                items.push(RowItem::Variable {
                    name: element_text(name),
                    fragment: element_text(fragment),
                });
                index += 4;
            }
            SyntaxKind::TOKEN_TREE => {
                let variables = simple_matcher(element.as_node()?)?;
                if variables.is_empty()
                    || !variables.iter().all(|(name, _)| names.insert(name.clone()))
                {
                    return None;
                }
                items.push(RowItem::Group {
                    open: element_text(element).chars().next()?,
                    variables,
                });
                index += 1;
            }
            kind @ (SyntaxKind::COMMA | SyntaxKind::SEMICOLON | SyntaxKind::COLON) => {
                items.push(RowItem::Separator(kind));
                index += 1;
            }
            _ => return None,
        }
    }
    (!items.is_empty()).then_some(items)
}

/// Splits the invocation into the rows the matcher describes. `None` when
/// any element is left over or any fragment is uncertain: a table that
/// rustc might read differently registers nothing.
fn match_rows(
    matcher: &RepetitionMatcher,
    invocation: &ast::TokenTree,
) -> Option<Vec<RowBindings>> {
    let elements = inner_elements(invocation.syntax());
    let mut rows = Vec::new();
    let mut position = 0;
    while position < elements.len() {
        if !rows.is_empty()
            && let Some(separator) = matcher.separator
        {
            if elements.get(position)?.kind() != separator {
                return None;
            }
            position += 1;
            // `$(..),*` takes no trailing separator.
            if position >= elements.len() {
                return None;
            }
        }
        if rows.len() == MAX_REPETITION_ROWS {
            return None;
        }
        let mut bindings = RowBindings::new();
        for (index, item) in matcher.row.iter().enumerate() {
            match item {
                RowItem::Separator(kind) => {
                    if elements.get(position)?.kind() != *kind {
                        return None;
                    }
                    position += 1;
                }
                RowItem::Variable { name, fragment } => {
                    let width = match fragment.as_str() {
                        "ident" | "tt" | "block" => 1,
                        "literal" => {
                            if elements.get(position)?.kind() == SyntaxKind::MINUS {
                                2
                            } else {
                                1
                            }
                        }
                        // An expression ends where rustc's follow set
                        // does; only an explicit row separator after it
                        // makes that visible.
                        "expr" => {
                            let end = match matcher.row.get(index + 1) {
                                Some(RowItem::Separator(end)) => *end,
                                None => matcher.separator?,
                                Some(_) => return None,
                            };
                            elements[position..]
                                .iter()
                                .position(|element| element.kind() == end)
                                .unwrap_or(elements.len() - position)
                        }
                        _ => return None,
                    };
                    let argument = argument(elements.get(position..position + width)?)?;
                    if !argument_fits(fragment, &argument)? {
                        return None;
                    }
                    bindings.insert(name.clone(), (fragment.clone(), argument));
                    position += width;
                }
                RowItem::Group { open, variables } => {
                    let element = elements.get(position)?;
                    if element.kind() != SyntaxKind::TOKEN_TREE
                        || !element_text(element).starts_with(*open)
                    {
                        return None;
                    }
                    let tree = ast::TokenTree::cast(element.as_node()?.clone())?;
                    // A group matcher has no trailing comma, so `(10,)` is
                    // not `($v:expr)`; rustc would try a later arm.
                    if invocation_ends_with_comma(&tree) {
                        return None;
                    }
                    let arguments = top_level_arguments(&tree)?;
                    if arguments.len() != variables.len() {
                        return None;
                    }
                    for ((name, fragment), argument) in variables.iter().zip(arguments) {
                        if !argument_fits(fragment, &argument)? {
                            return None;
                        }
                        bindings.insert(name.clone(), (fragment.clone(), argument));
                    }
                    position += 1;
                }
            }
        }
        rows.push(bindings);
    }
    (!rows.is_empty()).then_some(rows)
}

/// A transcriber that is exactly one separator-free repetition
/// `$( body )*` over the matcher's variables, emitted once per row.
fn expand_rows(
    transcriber: &SyntaxNode,
    rows: &[RowBindings],
    local_macros: &BTreeSet<String>,
) -> Option<String> {
    let elements = inner_elements(transcriber);
    let [dollar, body, repeat] = elements.as_slice() else {
        return None;
    };
    if dollar.kind() != SyntaxKind::DOLLAR
        || body.kind() != SyntaxKind::TOKEN_TREE
        || !element_text(body).starts_with('(')
        || !matches!(repeat.kind(), SyntaxKind::STAR | SyntaxKind::PLUS)
    {
        return None;
    }
    let body = ast::TokenTree::cast(body.as_node()?.clone())?;
    if !has_test_attribute(&body) || invokes_local_macro(&body, local_macros) {
        return None;
    }
    let mut text = String::new();
    for row in rows {
        text.push_str(&substitute(body.syntax(), |name| {
            row.get(name)
                .map(|(fragment, argument)| (fragment.as_str(), argument))
        })?);
        text.push('\n');
        if text.len() > MAX_EXPANSION_BYTES {
            return None;
        }
    }
    (!text_invokes_local_macro(&text, local_macros) && !text_has_conditional_attribute(&text))
        .then_some(text)
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

    #[test]
    fn conditional_attributes_in_the_expansion_and_imported_names_refuse() {
        let emits = |body: &str| {
            format!("macro_rules! case {{\n    ($name:ident) => {{ {body} }};\n}}\ncase!(one);\n")
        };
        assert_eq!(
            expand_local_test_macros(&emits("#[cfg(test)] #[test] fn $name() { f(); }")).len(),
            1,
            "cfg(test) is the test build itself"
        );
        for body in [
            "#[cfg(any())] #[test] fn $name() { f(); }",
            "#[cfg(any())] mod $name { #[test] fn t() { f(); } }",
            "#[cfg_attr(miri, ignore)] #[test] fn $name() { f(); }",
        ] {
            assert!(expand_local_test_macros(&emits(body)).is_empty(), "{body}");
        }
        let generator =
            "macro_rules! case {\n    ($name:ident) => { #[test] fn $name() { f(); } };\n}\n";
        for import in [
            "mod m {\n    use crate::foreign::case;\n    case!(one);\n}\n",
            "use crate::foreign::other as case;\ncase!(one);\n",
        ] {
            assert!(
                expand_local_test_macros(&format!("{generator}{import}")).is_empty(),
                "{import}"
            );
        }
        let glob = format!("{generator}mod m {{\n    use super::*;\n    case!(one);\n}}\n");
        assert_eq!(
            expand_local_test_macros(&glob).len(),
            1,
            "a glob import keeps the generator"
        );
    }

    const ITOA_TABLE: &str = "macro_rules! test {\n    ($($name:ident($value:expr, $expected:expr))*) => {\n        $(\n            #[test]\n            fn $name() {\n                assert_eq!(format($value), $expected);\n            }\n        )*\n    };\n}\n\ntest! {\n    zero(0u64, \"0\")\n    max(u64::MAX, \"18446744073709551615\")\n    minus(-1i64 + 0, \"-1\")\n}\n";

    #[test]
    fn a_table_invocation_expands_one_test_per_row() {
        let expanded = expand_local_test_macros(ITOA_TABLE);
        assert_eq!(expanded.len(), 1, "{expanded:?}");
        assert_eq!(expanded[0].macro_name, "test");
        assert_eq!((expanded[0].start_line, expanded[0].end_line), (12, 16));
        let text = &expanded[0].text;
        for call in [
            "fn zero() {\n                assert_eq!(format(0u64), \"0\");",
            "fn max() {\n                assert_eq!(format((u64::MAX)), \"18446744073709551615\");",
            "assert_eq!(format((-1i64 + 0)), \"-1\");",
        ] {
            assert!(text.contains(call), "{call}\n{text}");
        }
        assert_eq!(text.matches("#[test]").count(), 3, "{text}");
        let facts = summarize_file_with_parser(Path::new("tests/t.rs"), text)
            .map(|facts| facts.tests.len());
        assert_eq!(facts, Ok(3), "premise: the expansion parses as three tests");
    }

    #[test]
    fn separators_groups_and_plus_repetition_expand() {
        let comma = "macro_rules! cases {\n    ($($name:ident: $input:expr, $want:expr);+) => {\n        $( #[test] fn $name() { assert_eq!(gate($input), $want); } )*\n    };\n}\ncases! { nine: 9, false; ten: 10 + 0, true }\n";
        let expanded = expand_local_test_macros(comma);
        assert_eq!(expanded.len(), 1, "{expanded:?}");
        assert!(
            expanded[0]
                .text
                .contains("fn nine() { assert_eq!(gate(9), false); }")
        );
        assert!(
            expanded[0]
                .text
                .contains("fn ten() { assert_eq!(gate((10 + 0)), true); }")
        );

        let square = "macro_rules! cases {\n    ($($name:ident[$a:literal, $b:tt])*) => {\n        $( #[test] fn $name() { assert_eq!(gate($a, $b), 1); } )*\n    };\n}\ncases! { one[1, 2] two[-3, 4] }\n";
        let expanded = expand_local_test_macros(square);
        assert_eq!(expanded.len(), 1, "{expanded:?}");
        assert!(
            expanded[0].text.contains("gate((-3), 4)"),
            "{}",
            expanded[0].text
        );
    }

    #[test]
    fn tables_the_expander_cannot_split_exactly_refuse() {
        let table = |matcher: &str, body: &str, invocation: &str| {
            format!(
                "macro_rules! t {{\n    ({matcher}) => {{ {body} }};\n}}\nt! {{ {invocation} }}\n"
            )
        };
        let body = "$( #[test] fn $name() { assert_eq!(gate($v), $w); } )*";
        let ok = table("$($name:ident($v:expr, $w:expr))*", body, "a(1, 2) b(3, 4)");
        assert_eq!(expand_local_test_macros(&ok).len(), 1, "premise: {ok}");

        for (label, matcher, body, invocation) in [
            (
                "a trailing separator",
                "$($name:ident($v:expr, $w:expr)),*",
                body,
                "a(1, 2), b(3, 4),",
            ),
            (
                "leftover tokens",
                "$($name:ident($v:expr, $w:expr))*",
                body,
                "a(1, 2) b(3, 4) +",
            ),
            (
                "a wrong group arity",
                "$($name:ident($v:expr, $w:expr))*",
                body,
                "a(1, 2) b(3)",
            ),
            (
                "a wrong delimiter",
                "$($name:ident($v:expr, $w:expr))*",
                body,
                "a[1, 2]",
            ),
            (
                "an ident that is a literal",
                "$($name:ident($v:expr, $w:expr))*",
                body,
                "7(1, 2)",
            ),
            ("no rows", "$($name:ident($v:expr, $w:expr))*", body, ""),
            (
                "a nested repetition",
                "$($name:ident($($v:expr),*))*",
                body,
                "a(1, 2)",
            ),
            (
                "an expr with no row separator after it",
                "$($name:ident $v:expr)*",
                "$( #[test] fn $name() { gate($v); } )*",
                "a 1 b 2",
            ),
            (
                "an unsupported fragment",
                "$($name:ident($v:ty, $w:expr))*",
                body,
                "a(u8, 2)",
            ),
            (
                "a repeated variable name",
                "$($name:ident($name:expr, $w:expr))*",
                body,
                "a(1, 2)",
            ),
            (
                "a row item that is not a separator",
                "$($name:ident => ($v:expr, $w:expr))*",
                body,
                "a => (1, 2)",
            ),
            (
                "a separator inside the transcriber",
                "$($name:ident($v:expr, $w:expr))*",
                "$( #[test] fn $name() { assert_eq!(gate($v), $w); } ),*",
                "a(1, 2)",
            ),
            (
                "a transcriber with more than the repetition",
                "$($name:ident($v:expr, $w:expr))*",
                "fn helper() {} $( #[test] fn $name() { assert_eq!(gate($v), $w); } )*",
                "a(1, 2)",
            ),
            (
                "a variable the matcher never bound",
                "$($name:ident($v:expr, $w:expr))*",
                "$( #[test] fn $name() { assert_eq!(gate($v), $other); } )*",
                "a(1, 2)",
            ),
            (
                "a body with no test attribute",
                "$($name:ident($v:expr, $w:expr))*",
                "$( fn $name() { assert_eq!(gate($v), $w); } )*",
                "a(1, 2)",
            ),
        ] {
            assert!(
                expand_local_test_macros(&table(matcher, body, invocation)).is_empty(),
                "{label}"
            );
        }
    }

    #[test]
    fn a_local_macro_in_a_table_row_or_body_refuses() {
        let skip = "macro_rules! skip {\n    ($($t:tt)*) => {};\n}\n";
        let rows = format!(
            "{skip}macro_rules! t {{\n    ($($name:ident($v:expr))*) => {{ $( #[test] fn $name() {{ assert!(gate($v)); }} )* }};\n}}\nt! {{ a(skip!(1)) }}\n"
        );
        assert!(expand_local_test_macros(&rows).is_empty());
        let body = format!(
            "{skip}macro_rules! t {{\n    ($($name:ident($v:expr))*) => {{ $( #[test] fn $name() {{ skip!(assert!(gate($v))); }} )* }};\n}}\nt! {{ a(1) }}\n"
        );
        assert!(expand_local_test_macros(&body).is_empty());
        let conditional = "macro_rules! t {\n    ($($name:ident($v:expr))*) => { $( #[cfg(any())] #[test] fn $name() { assert!(gate($v)); } )* };\n}\nt! { a(1) }\n";
        assert!(expand_local_test_macros(conditional).is_empty());
    }

    #[test]
    fn a_table_arm_follows_simple_arms_that_do_not_fit() {
        let source = "macro_rules! t {\n    ($name:ident, $v:expr) => { #[test] fn $name() { assert!(simple($v)); } };\n    ($($name:ident($v:expr))*) => { $( #[test] fn $name() { assert!(table($v)); } )* };\n}\nt! { a(1) b(2) }\nt!(c, 3);\n";
        let expanded = expand_local_test_macros(source);
        assert_eq!(expanded.len(), 2, "{expanded:?}");
        assert!(expanded[0].text.contains("table(1)") && expanded[0].text.contains("table(2)"));
        assert!(expanded[1].text.contains("simple(3)"));
    }

    #[test]
    fn a_trailing_comma_must_agree_between_matcher_and_invocation() {
        let strict_first = "macro_rules! c {\n    ($n:ident, $v:expr,) => { #[test] fn $n() { assert!(strict($v)); } };\n    ($n:ident, $v:expr) => { #[test] fn $n() { assert!(loose($v)); } };\n}\nc!(a, 1);\n";
        assert!(
            expand_local_test_macros(strict_first).is_empty(),
            "a matcher's trailing comma is required, so arm 2 runs and is not guessed"
        );
        let loose_first = "macro_rules! c {\n    ($n:ident, $v:expr) => { #[test] fn $n() { assert!(first($v)); } };\n    ($n:ident, $v:expr,) => { #[test] fn $n() { assert!(second($v)); } };\n}\nc!(a, 1,);\n";
        assert!(
            expand_local_test_macros(loose_first).is_empty(),
            "an invocation's trailing comma does not fit a matcher without one"
        );
        let table = "macro_rules! t {\n    ($($n:ident($v:expr, $w:expr,))*) => { $( #[test] fn $n() { assert_eq!(f($v), $w); } )* };\n}\nt! { a(1, 2) }\n";
        assert!(expand_local_test_macros(table).is_empty());
        let plain = "macro_rules! c {\n    ($n:ident, $v:expr) => { #[test] fn $n() { assert!(f($v)); } };\n}\nc!(a, 1);\n";
        assert_eq!(expand_local_test_macros(plain).len(), 1, "premise");
    }

    #[test]
    fn a_row_group_ending_in_a_comma_does_not_fit_a_group_without_one() {
        let table = |row: &str| {
            format!(
                "macro_rules! t {{\n    ($($n:ident($v:expr))*) => {{ $( #[test] fn $n() {{ assert!(gate($v)); }} )* }};\n    ($($tt:tt)*) => {{}};\n}}\nt! {{ {row} }}\n"
            )
        };
        assert_eq!(
            expand_local_test_macros(&table("edge(10)")).len(),
            1,
            "premise"
        );
        assert!(
            expand_local_test_macros(&table("edge(10,)")).is_empty(),
            "`(10,)` is not `($v:expr)`; the catch-all arm runs and defines no test"
        );
    }

    #[test]
    fn row_and_expansion_size_are_bounded() {
        let table = |rows: usize, body: &str| {
            let invocation: String = (0..rows).map(|row| format!("r{row}({row}) ")).collect();
            format!(
                "macro_rules! t {{\n    ($($n:ident($v:expr))*) => {{ $( #[test] fn $n() {{ {body} }} )* }};\n}}\nt! {{ {invocation} }}\n"
            )
        };
        let small = "assert!(f($v));";
        let at_cap = expand_local_test_macros(&table(MAX_REPETITION_ROWS, small));
        assert_eq!(at_cap.len(), 1, "premise: the cap row count expands");
        assert!(expand_local_test_macros(&table(MAX_REPETITION_ROWS + 1, small)).is_empty());

        let large = format!("{} assert!(f($v));", "let _x = 1;".repeat(400));
        assert!(
            expand_local_test_macros(&table(64, &large)).is_empty(),
            "64 rows of a 4 KB body exceed the expansion budget"
        );
        assert_eq!(
            expand_local_test_macros(&table(4, &large)).len(),
            1,
            "a few rows of the same body fit"
        );
    }
}
