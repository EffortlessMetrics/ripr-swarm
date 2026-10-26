//! RIPR-SPEC-0197 rule 1 extension: a test-local helper that polls one
//! `Future` to completion and returns its output.
//!
//! `assert_eq!(block_on(owner(..)), e)` observes the `async` owner's return
//! value exactly as `owner(..).await` does, but only when `block_on` is a
//! helper that does nothing else. The helper is read from its own source
//! against a closed set of spellings; a name alone (`block_on`), a helper that
//! transforms the output, or one with a timeout or a second branch is not one,
//! and the call keeps the `rust_indirect_owner_call_unresolved` limit.

use super::owner_pin::{OwnerPinSyntax, polling_helper_call_reaches_definition};
use super::reveal::flattened_use_paths;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionFact;
use crate::analysis::rust_index::{RustIndex, TestSummary};
use crate::analysis::syntax::{
    NameScopes, UseBinding, name_scopes_for_fn, source_binds_value_name,
    supported_targeted_item_context,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

/// The `if let` spelling of the poll loop.
const IF_LET: &str = "fn PH_name < PH_f : Future > ( PH_p : PH_f ) -> PH_f :: Output { \
    let mut PH_fut = pin ! ( PH_p ) ; \
    let mut PH_cx = Context :: from_waker ( Waker :: noop ( ) ) ; \
    loop { if let Poll :: Ready ( PH_v ) = PH_fut . as_mut ( ) . poll ( & mut PH_cx ) { return PH_v ; } } }";

/// The `match` spelling of the same loop.
const MATCH: &str = "fn PH_name < PH_f : Future > ( PH_p : PH_f ) -> PH_f :: Output { \
    let mut PH_fut = pin ! ( PH_p ) ; \
    let mut PH_cx = Context :: from_waker ( Waker :: noop ( ) ) ; \
    loop { match PH_fut . as_mut ( ) . poll ( & mut PH_cx ) { \
    Poll :: Ready ( PH_v ) => return PH_v , Poll :: Pending => { } } } }";

/// Names every definition of which is a poll-to-completion helper. A name
/// with any other definition in the workspace is left out, since the call's
/// callee could be that one.
pub(super) fn verified_helpers(index: &RustIndex) -> BTreeSet<String> {
    let mut by_name: BTreeMap<&str, bool> = BTreeMap::new();
    for function in index.functions() {
        let verdict = polls_to_completion(function, index);
        by_name
            .entry(function.name.as_str())
            .and_modify(|all| *all &= verdict)
            .or_insert(verdict);
    }
    by_name
        .into_iter()
        .filter(|(_, all)| *all)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// `helper(inner)` with a verified helper, as the future expression `inner`.
pub(super) fn strip_helper_call<'a>(
    operand: &'a str,
    helpers: &BTreeSet<String>,
) -> Option<&'a str> {
    if helpers.is_empty() {
        return None;
    }
    let operand = operand.trim();
    let masked = mask_comments_and_strings(operand);
    let name_end = masked.find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))?;
    if !helpers.contains(&operand[..name_end]) {
        return None;
    }
    let rest = &masked[name_end..];
    let open = name_end + (rest.len() - rest.trim_start().len());
    if masked.as_bytes().get(open) != Some(&b'(') || !masked.ends_with(')') {
        return None;
    }
    // The first parenthesis closes at the end: the call has no tail.
    let mut depth = 0usize;
    for (at, ch) in masked.char_indices().skip_while(|(at, _)| *at < open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && at + 1 != masked.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(operand[open + 1..operand.len() - 1].trim())
}

/// Refuse helper names whose actual test-side binding is not the verified
/// function. The ordinary free-call identity owner also refuses local lets,
/// patterns, imports, macros, cfg alternatives and unplaced definitions.
pub(super) fn binding_is_verified(
    test: &TestSummary,
    name: &str,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    index.functions().iter().any(|function| {
        function.name == name
            && polling_helper_call_reaches_definition(test, function, index, syntax)
    })
}

fn polls_to_completion(function: &FunctionFact, index: &RustIndex) -> bool {
    if !function.attrs.is_empty() || !function.impl_attrs.is_empty() {
        return false;
    }
    let body: &str = &function.body;
    if !body.contains("Poll") {
        return false;
    }
    let tokens = drop_std_prefixes(tokenize(&mask_comments_and_strings(body)));
    [IF_LET, MATCH]
        .iter()
        .any(|template| matches_template(&tokens, &tokenize(template)))
        && standard_primitives(function, index)
        && method_dispatch_is_closed(function, index)
}

/// Source spelling alone cannot establish Future/pin!/task primitive identity.
/// Read the helper's own lexical scopes, never imports elsewhere in its file.
/// Unsupported imports or item expansion stay unresolved.
fn standard_primitives(function: &FunctionFact, index: &RustIndex) -> bool {
    let Some(facts) = index.files().get(&function.file) else {
        return false;
    };
    if facts.used_lexical_fallback {
        return false;
    }
    let Some(scopes) = name_scopes_for_fn(&facts.source, function.start_line, &function.name)
    else {
        return false;
    };
    let Some(snapshot) = index.source_syntax.get(index, &function.file) else {
        return false;
    };
    let Some(tree) = snapshot.tree() else {
        return false;
    };
    let mut definitions = tree
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|item| {
            item.name().is_some_and(|name| name.text() == function.name)
                && item.fn_token().is_some_and(|token| {
                    facts.source[..usize::from(token.text_range().start())]
                        .matches('\n')
                        .count()
                        + 1
                        == function.start_line
                })
        });
    let (Some(definition), None) = (definitions.next(), definitions.next()) else {
        return false;
    };
    if !supported_targeted_item_context(definition.syntax()) {
        return false;
    }
    // A module-level invocation can emit unseen bindings. A local pin!
    // definition is also a competing macro even with a canonical use.
    if tree
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .any(|call| {
            call.syntax().parent().is_some_and(|parent| {
                ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
            })
        })
    {
        return false;
    }
    let tokens = tokenize(&mask_comments_and_strings(&function.body));
    let mut at = 0;
    while at < tokens.len() {
        if matches!(tokens[at].as_str(), "std" | "core") {
            let Some(path) = tokens.get(at..at + 7) else {
                return false;
            };
            if path[1] != ":"
                || path[2] != ":"
                || path[4] != ":"
                || path[5] != ":"
                || primitive_module(&path[6]) != Some(path[3].as_str())
                || !standard_root(&path[0], &scopes, function, index)
            {
                return false;
            }
            at += 7;
            continue;
        }
        if primitive_module(&tokens[at]).is_some() {
            let name = &tokens[at];
            if name == "pin"
                && index.files().iter().any(|(file, _)| {
                    index
                        .source_syntax
                        .get(index, file)
                        .and_then(|snapshot| snapshot.tree())
                        .is_none_or(|tree| {
                            tree.syntax()
                                .descendants()
                                .filter_map(|node| {
                                    ast::MacroRules::cast(node.clone())
                                        .and_then(|item| item.name())
                                        .or_else(|| {
                                            ast::MacroDef::cast(node).and_then(|item| item.name())
                                        })
                                })
                                .any(|name| {
                                    Some(name).is_some_and(|name| {
                                        name.text().trim_start_matches("r#") == "pin"
                                    })
                                })
                        })
                })
            {
                return false;
            }
            if !standard_import(name, &scopes, function, index) {
                return false;
            }
        }
        at += 1;
    }
    true
}

fn primitive_module(name: &str) -> Option<&'static str> {
    match name {
        "Future" => Some("future"),
        "pin" => Some("pin"),
        "Context" | "Poll" | "Waker" => Some("task"),
        _ => None,
    }
}

fn standard_import(
    name: &str,
    scopes: &NameScopes,
    function: &FunctionFact,
    index: &RustIndex,
) -> bool {
    standard_import_from(name, scopes, 0, function, index)
}

fn standard_import_from(
    name: &str,
    scopes: &NameScopes,
    start: usize,
    function: &FunctionFact,
    index: &RustIndex,
) -> bool {
    for (at, scope) in scopes.scopes.iter().enumerate().skip(start) {
        if scope
            .defines
            .iter()
            .any(|bound| bound.trim_start_matches("r#") == name)
            || scope
                .child_modules
                .iter()
                .any(|bound| bound.trim_start_matches("r#") == name)
            || scope.block_binds(name)
        {
            return false;
        }
        let imports: Vec<_> = scope
            .imports
            .iter()
            .filter(|import| import.bound.trim_start_matches("r#") == name)
            .map(|import| &import.binding)
            .collect();
        if let [UseBinding::Path(path)] = imports.as_slice() {
            return path.len() == 3
                && matches!(path[0].as_str(), "std" | "core")
                && primitive_module(name) == Some(path[1].as_str())
                && path[2] == name
                && standard_root(&path[0], scopes, function, index);
        }
        if !imports.is_empty() {
            return false;
        }
        if !scope.globs.is_empty() {
            // Only an exact super::* into a parent inline module whose
            // complete source/scopes are in this file can lend a primitive.
            if scope.fn_body
                || scope.modules.is_empty()
                || !scope.globs.iter().all(|glob| {
                    glob.as_ref()
                        .is_some_and(|path| path.as_slice() == ["super"])
                })
            {
                return false;
            }
            let parent = &scope.modules[..scope.modules.len() - 1];
            return scopes
                .scopes
                .iter()
                .enumerate()
                .skip(at + 1)
                .find(|(_, outer)| !outer.fn_body && outer.modules == parent)
                .is_some_and(|(next, _)| {
                    standard_import_from(name, scopes, next, function, index)
                });
        }
        if !scope.fn_body {
            return false;
        }
    }
    false
}

/// Standard types do not close method lookup: an extension trait can take
/// .as_mut() or .poll() before the standard implementation. Refuse competing
/// definitions and unverified imports/expansions in the source ancestry.
fn method_dispatch_is_closed(function: &FunctionFact, index: &RustIndex) -> bool {
    if index
        .functions()
        .iter()
        .any(|item| matches!(item.name.trim_start_matches("r#"), "as_mut" | "poll"))
    {
        return false;
    }
    let Some(context) = index.execution_context(&function.file) else {
        return false;
    };
    std::iter::once(&function.file).chain(context.edges.iter().map(|edge| &edge.parent)).all(|file| {
        let Some(snapshot) = index.source_syntax.get(index, file) else { return false };
        let Some(tree) = snapshot.tree() else { return false };
        for node in tree.syntax().descendants() {
            if let Some(item) = ast::Use::cast(node.clone()) {
                let imports = flattened_use_paths(&item.syntax().text().to_string());
                if imports.is_empty() || !imports.iter().all(|import| {
                    import.path.starts_with("std::") || import.path.starts_with("core::")
                        || (import.path == "super::*" && item.syntax().ancestors().any(|ancestor| ast::Module::can_cast(ancestor.kind())))
                }) { return false; }
            }
            if ast::ExternCrate::can_cast(node.kind()) { return false; }
            if let Some(attr) = ast::Attr::cast(node.clone()) {
                let text = attr.syntax().text().to_string();
                if text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>() != "#[test]"
                    && !crate::analysis::facts::cfg_predicates::attribute_preserves_source_items(&text)
                { return false; }
            }
            if let Some(call) = ast::MacroCall::cast(node) {
                if call.syntax().parent().is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())) {
                    return false;
                }
            }
        }
        true
    })
}

fn standard_root(
    root: &str,
    scopes: &NameScopes,
    function: &FunctionFact,
    index: &RustIndex,
) -> bool {
    if scopes.scopes.iter().any(|scope| {
        scope
            .defines
            .iter()
            .any(|bound| bound.trim_start_matches("r#") == root)
            || scope
                .child_modules
                .iter()
                .any(|bound| bound.trim_start_matches("r#") == root)
            || scope
                .imports
                .iter()
                .any(|import| import.bound.trim_start_matches("r#") == root)
            || scope.block_binds(root)
    }) || !index
        .drop_in_manifests
        .unbound_standard_root(&function.file, root)
    {
        return false;
    }
    let Some(context) = index.execution_context(&function.file) else {
        return false;
    };
    std::iter::once(&function.file)
        .chain(context.edges.iter().map(|edge| &edge.parent))
        .all(|file| {
            let Some(facts) = index.files().get(file) else {
                return false;
            };
            let Some(snapshot) = index.source_syntax.get(index, file) else {
                return false;
            };
            let Some(tree) = snapshot.tree() else {
                return false;
            };
            !source_binds_value_name(&facts.source, root)
                && !tree
                    .syntax()
                    .descendants()
                    .filter_map(ast::Name::cast)
                    .any(|name| name.text().trim_start_matches("r#") == root)
        })
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !ch.is_whitespace() {
            tokens.push(ch.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

/// `std::pin::pin!` and `core::task::Waker` read as `pin!` and `Waker`.
fn drop_std_prefixes(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut at = 0;
    while at < tokens.len() {
        if matches!(tokens[at].as_str(), "std" | "core")
            && tokens.get(at + 1).is_some_and(|t| t == ":")
            && tokens.get(at + 2).is_some_and(|t| t == ":")
            && tokens
                .get(at + 3)
                .is_some_and(|t| matches!(t.as_str(), "future" | "pin" | "task"))
            && tokens.get(at + 4).is_some_and(|t| t == ":")
            && tokens.get(at + 5).is_some_and(|t| t == ":")
        {
            at += 6;
            continue;
        }
        out.push(tokens[at].clone());
        at += 1;
    }
    out
}

/// Token equality where each `PH_` word binds one identifier, the same one
/// at every use.
fn matches_template(tokens: &[String], template: &[String]) -> bool {
    if tokens.len() != template.len() {
        return false;
    }
    let mut bound: BTreeMap<&str, &str> = BTreeMap::new();
    for (token, expected) in tokens.iter().zip(template) {
        if expected.starts_with("PH_") {
            if token.starts_with(|ch: char| ch.is_ascii_digit())
                || !token.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            {
                return false;
            }
            if *bound.entry(expected).or_insert(token) != token {
                return false;
            }
        } else if token != expected {
            return false;
        }
    }
    bound.get("PH_fut") != bound.get("PH_cx")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polls(source: &str) -> bool {
        let tokens = drop_std_prefixes(tokenize(&mask_comments_and_strings(source)));
        [IF_LET, MATCH]
            .iter()
            .any(|template| matches_template(&tokens, &tokenize(template)))
    }

    const HELPER: &str = "fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }";

    #[test]
    fn the_polling_loop_is_a_helper() {
        assert!(polls(HELPER));
    }

    #[test]
    fn the_match_spelling_and_std_paths_are_a_helper() {
        assert!(polls(
            "fn run<T: Future>(f: T) -> T::Output {
                let mut g = std::pin::pin!(f);
                let mut c = std::task::Context::from_waker(std::task::Waker::noop());
                loop {
                    match g.as_mut().poll(&mut c) {
                        Poll::Ready(out) => return out,
                        Poll::Pending => {}
                    }
                }
            }"
        ));
    }

    #[test]
    fn a_helper_that_changes_the_output_is_not_one() {
        assert!(!polls(&HELPER.replace("return v;", "return v + 1;")));
    }

    #[test]
    fn a_helper_with_a_second_exit_is_not_one() {
        assert!(!polls(
            &HELPER.replace("loop {", "if false { return todo!(); } loop {")
        ));
    }

    #[test]
    fn the_context_must_not_shadow_the_pinned_future() {
        assert!(!polls(
            &HELPER
                .replace("let mut cx =", "let mut fut =")
                .replace("&mut cx", "&mut fut")
        ));
    }

    #[test]
    fn polling_a_different_binding_is_not_a_helper() {
        assert!(!polls(&HELPER.replace("fut.as_mut()", "other.as_mut()")));
    }

    fn strip(operand: &str) -> Option<&str> {
        let helpers = BTreeSet::from(["block_on".to_string()]);
        strip_helper_call(operand, &helpers)
    }

    #[test]
    fn a_helper_call_reads_as_its_argument() {
        assert_eq!(strip("block_on(add_tax(100))"), Some("add_tax(100)"));
        assert_eq!(strip(" block_on (f(&[1, 2]))"), Some("f(&[1, 2])"));
    }

    #[test]
    fn only_a_complete_call_of_a_verified_name_is_stripped() {
        assert_eq!(strip("block_on(a(1)).unwrap()"), None);
        assert_eq!(strip("block_on(a(1)) + 1"), None);
        assert_eq!(strip("other(a(1))"), None);
        assert_eq!(strip("rt.block_on(a(1))"), None);
        assert_eq!(strip("block_on"), None);
    }
}
