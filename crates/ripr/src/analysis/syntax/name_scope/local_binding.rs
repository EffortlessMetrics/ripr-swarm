//! Whether a name in an authored function's body only ever means a local of
//! that body (#14552): a `let` binding, a closure or function parameter, or
//! a nested `fn`, read block by block from the parsed tree.
//!
//! Line-based shadow facts treat a `let` as binding to the end of the
//! function, so they cannot tell `{ let dt = |x| x; } dt(1)` (the outer call
//! is not shadowed) or `let dt = |x| dt(x);` (the initializer still sees the
//! outer name) from a real shadow. This reading walks each mention's
//! enclosing blocks instead, and answers yes only when the innermost binding
//! of every mention is one of those locals.

use super::{
    LineIndex, file_imports_name, macro_chain_reach, name_scope_for_items, opaque_macro_call,
    top_level_imports_name, unraw,
};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode, T,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};

/// For the authored function `fn_name` declared at `fn_line`: whether `name`
/// is mentioned in its body and every mention is bound by a local of the body
/// (#14552). The innermost scope decides each mention, as Rust resolves it:
/// a block's own items, then the `let` statements before the mention's
/// statement in that block, then an enclosing closure's or function's
/// parameters. A `let` does not bind inside its own initializer, and a
/// binding does not reach past the end of its block.
///
/// `false` (the caller keeps its prior reading) when there is no mention; a
/// mention is qualified (`a::name`) or sits in a `use`; a block on the way
/// holds an import, glob or non-`fn` item that may bind the name, or an item
/// or `let` of the name under an attribute; a mention reaches a `mod`,
/// `impl` or `trait` boundary, or a nested `fn` boundary, without a binding;
/// the name is bound by any other pattern (a `match` arm, `if let`, `for`);
/// or a mention sits inside a macro whose expansion may add bindings (any
/// macro other than a bare std expression macro, or such a macro when the
/// file chain may redefine it, which `ancestors` settles as for
/// `nested_block_renames_in_tree`, or a macro operand that spells `use`,
/// `mod` or a nested macro call). A method call or field access of the name, a field label
/// (`name: value`) and a path prefix (`name::item`) are not mentions.
pub(crate) fn every_mention_bound_locally_in_tree(
    tree: &ast::SourceFile,
    ancestors: Option<&[ast::SourceFile]>,
    line_index: &LineIndex,
    fn_line: usize,
    fn_name: &str,
    name: &str,
) -> bool {
    let mut functions = tree.syntax().descendants().filter_map(|node| {
        let function = ast::Fn::cast(node)?;
        let token = function.fn_token()?;
        let named = function.name().is_some_and(|ident| ident.text() == fn_name);
        (named && line_index.line(token.text_range().start()) == fn_line).then_some(function)
    });
    let Some(function) = functions.next() else {
        return false;
    };
    if functions.next().is_some() {
        return false;
    }
    every_mention_bound_locally_in_function(tree, ancestors, &function, name)
}

/// [`every_mention_bound_locally_in_tree`] for a macro-generated test, whose
/// expanded body `fragment` (one `fn` item) the file does not spell. Macro
/// scope is still read from the file `tree` and its `ancestors`. The
/// expansion is read as written text, as the line-based shadow facts read
/// it, so a binder and a mention keep no separate macro hygiene.
pub(crate) fn every_mention_bound_locally_in_fragment(
    tree: &ast::SourceFile,
    ancestors: Option<&[ast::SourceFile]>,
    fragment: &ast::SourceFile,
    name: &str,
) -> bool {
    let mut items = fragment.items();
    let (Some(ast::Item::Fn(function)), None) = (items.next(), items.next()) else {
        return false;
    };
    every_mention_bound_locally_in_function(tree, ancestors, &function, name)
}

fn every_mention_bound_locally_in_function(
    tree: &ast::SourceFile,
    ancestors: Option<&[ast::SourceFile]>,
    function: &ast::Fn,
    name: &str,
) -> bool {
    let Some(body) = function.body() else {
        return false;
    };
    let mut mentions = 0usize;
    let mut macro_reach: Option<Option<usize>> = None;
    for token in body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if token.kind() != SyntaxKind::IDENT || unraw(token.text()) != name {
            continue;
        }
        let Some(parent) = token.parent() else {
            return false;
        };
        if parent
            .ancestors()
            .any(|node| ast::Use::can_cast(node.kind()) || ast::Rename::can_cast(node.kind()))
        {
            return false;
        }
        if ast::Name::can_cast(parent.kind()) {
            // A binder of the name. A plain pattern or a nested `fn` is
            // judged where it binds; any other declared item may be the
            // mention's target, so it is not settled here.
            let binder = parent.parent();
            if binder.as_ref().is_some_and(|node| {
                ast::IdentPat::can_cast(node.kind()) || ast::Fn::can_cast(node.kind())
            }) {
                continue;
            }
            return false;
        }
        let previous = std::iter::successors(token.prev_token(), |token| token.prev_token())
            .find(|token| !token.kind().is_trivia());
        // Inside a macro's token tree `::` arrives as two adjacent `:`
        // tokens, so `crate::name` there reads like the parsed path.
        let previous_kind = previous.as_ref().map(|token| {
            if token.kind() == T![:]
                && token
                    .prev_token()
                    .is_some_and(|before| before.kind() == T![:])
            {
                T![::]
            } else {
                token.kind()
            }
        });
        match previous_kind {
            // A method call or field access never names a local.
            Some(T![.]) => continue,
            Some(T![::]) => return false,
            _ => {}
        }
        let mut following = std::iter::successors(token.next_token(), |token| token.next_token())
            .filter(|token| !token.kind().is_trivia());
        let next_kind = match following.next() {
            Some(colon)
                if colon.kind() == T![:]
                    && colon
                        .next_token()
                        .is_some_and(|after| after.kind() == T![:]) =>
            {
                following.next();
                Some(T![::])
            }
            other => other.map(|token| token.kind()),
        };
        match next_kind {
            // A path prefix (`name::item`) names a module or type; a
            // turbofish (`name::<T>(..)`) still calls the name.
            Some(T![::]) if following.next().map(|token| token.kind()) != Some(T![<]) => {
                continue;
            }
            // A field label (`Pair { dt: other }`) names a field.
            Some(T![:]) => continue,
            _ => {}
        }
        mentions += 1;
        let mut macro_names = Vec::new();
        for node in parent.ancestors().take_while(|node| node != body.syntax()) {
            if opaque_macro_call(&node) {
                return false;
            }
            let Some(call) = ast::MacroCall::cast(node) else {
                continue;
            };
            // A macro operand is unparsed tokens, so a block inside it
            // (`assert_eq!({ use crate::{dt}; dt(1) }, 0)`) hides its own
            // imports and modules from the walk below, and a nested macro
            // (`assert_eq!(route!(dt(1)), 2)`) may transcribe the name into
            // a path.
            if call
                .token_tree()
                .is_some_and(|tokens| operand_may_rebind(tokens.syntax()))
            {
                return false;
            }
            if let Some(macro_name) = call.path().and_then(|path| path.segment()) {
                macro_names.push(macro_name.to_string());
            }
        }
        if !macro_names.is_empty() {
            let Some(reach) = *macro_reach.get_or_insert_with(|| {
                ancestors.and_then(|ancestors| macro_chain_reach(tree, ancestors))
            }) else {
                return false;
            };
            if macro_names.iter().any(|macro_name| {
                file_imports_name(tree, macro_name)
                    || ancestors
                        .into_iter()
                        .flatten()
                        .take(reach)
                        .any(|file| top_level_imports_name(file, macro_name))
            }) {
                return false;
            }
        }
        if !bound_by_a_local(&parent, name) {
            return false;
        }
    }
    mentions > 0
}

/// Whether a macro operand's tokens spell `use`, `mod` or a nested macro
/// invocation (`name!`, not `name != ..`), any of which may bind or move a
/// name the unparsed operand hides.
fn operand_may_rebind(tokens: &SyntaxNode) -> bool {
    let tokens: Vec<_> = tokens
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens
        .iter()
        .enumerate()
        .any(|(at, token)| match token.kind() {
            T![use] | T![mod] => true,
            SyntaxKind::IDENT => {
                tokens.get(at + 1).is_some_and(|next| next.kind() == T![!])
                    && tokens
                        .get(at + 2)
                        .is_none_or(|after| !matches!(after.kind(), T![=]))
            }
            _ => false,
        })
}

/// Whether the innermost binding of `name` seen from `start` is a local:
/// walks outward one enclosing node at a time until a scope decides.
fn bound_by_a_local(start: &SyntaxNode, name: &str) -> bool {
    let mut current = start.clone();
    while let Some(up) = current.parent() {
        if let Some(list) = ast::StmtList::cast(up.clone()) {
            if let Some(local) = block_binding(&list, &current, name) {
                return local;
            }
        } else if let Some(closure) = ast::ClosureExpr::cast(up.clone()) {
            let in_body = closure.body().is_some_and(|body| body.syntax() == &current);
            if in_body
                && closure
                    .param_list()
                    .is_some_and(|params| params_bind(&params, name))
            {
                return true;
            }
        } else if let Some(function) = ast::Fn::cast(up.clone()) {
            // A function body sees only its own parameters and items: the
            // blocks around a nested `fn` are not in its scope.
            let in_body = function
                .body()
                .is_some_and(|body| body.syntax() == &current);
            return in_body
                && function
                    .param_list()
                    .is_some_and(|params| params_bind(&params, name));
        } else if ast::Module::can_cast(up.kind())
            || ast::Impl::can_cast(up.kind())
            || ast::Trait::can_cast(up.kind())
            || ast::SourceFile::can_cast(up.kind())
        {
            return false;
        }
        current = up;
    }
    false
}

/// How the block of `list` binds `name` for a mention inside its child
/// `child`: `Some(true)` for a local (a nested `fn`, or a `let` before
/// `child`), `Some(false)` for a binding that is not settled as a local, and
/// `None` when the block does not bind the name.
fn block_binding(list: &ast::StmtList, child: &SyntaxNode, name: &str) -> Option<bool> {
    let mut items = Vec::new();
    let mut bound = None;
    let mut before_child = true;
    for statement in list.statements() {
        if statement.syntax() == child {
            before_child = false;
        }
        match statement {
            ast::Stmt::Item(item) => items.push(item),
            ast::Stmt::LetStmt(binding)
                if before_child
                    && binding
                        .pat()
                        .is_some_and(|pattern| pattern_binds(pattern.syntax(), name)) =>
            {
                bound = Some(binding.attrs().next().is_none());
            }
            _ => {}
        }
    }
    if !items.is_empty() {
        let scope = name_scope_for_items(&items);
        if scope.imports_of(name).next().is_some() || !scope.globs.is_empty() {
            return Some(false);
        }
        let mut local_fn = false;
        for item in items.iter().filter(|item| item_has_name(item, name)) {
            match item {
                ast::Item::Fn(function) if function.attrs().next().is_none() => local_fn = true,
                _ => return Some(false),
            }
        }
        if local_fn {
            return Some(true);
        }
    }
    bound
}

/// Whether the item declares `name`.
fn item_has_name(item: &ast::Item, name: &str) -> bool {
    let declared = match item {
        ast::Item::Fn(item) => item.name(),
        ast::Item::Const(item) => item.name(),
        ast::Item::Static(item) => item.name(),
        ast::Item::Struct(item) => item.name(),
        ast::Item::Enum(item) => item.name(),
        ast::Item::Union(item) => item.name(),
        ast::Item::Module(item) => item.name(),
        ast::Item::MacroRules(item) => item.name(),
        ast::Item::Trait(item) => item.name(),
        ast::Item::TypeAlias(item) => item.name(),
        _ => None,
    };
    declared.is_some_and(|declared| declared.text().trim_start_matches("r#") == name)
}

/// Whether a parameter of `params` binds `name` by a plain pattern.
fn params_bind(params: &ast::ParamList, name: &str) -> bool {
    params.params().any(|param| {
        param
            .pat()
            .is_some_and(|pattern| pattern_binds(pattern.syntax(), name))
    })
}

/// Whether the pattern `pattern` binds `name` through an identifier pattern
/// (`dt`, `mut dt`, `(a, dt)`, `Some(dt)`).
fn pattern_binds(pattern: &SyntaxNode, name: &str) -> bool {
    pattern
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|binding| {
            binding
                .name()
                .is_some_and(|ident| unraw(ident.text()) == name)
        })
}

#[cfg(test)]
mod tests {
    use super::every_mention_bound_locally_in_tree;
    use crate::analysis::syntax::ra::LineIndex;
    use ra_ap_syntax::{Edition, SourceFile};

    /// Reads `source` (whose test `t` is declared on line 2) for `dt`.
    fn bound(source: &str) -> bool {
        let parse = SourceFile::parse(source, Edition::Edition2024);
        let tree = parse.tree();
        let lines = LineIndex::new(source);
        every_mention_bound_locally_in_tree(&tree, Some(&[]), &lines, 2, "t", "dt")
    }

    #[test]
    fn local_bindings_of_the_name_settle_every_mention() {
        let cases = [
            (
                "closure binding",
                "let dt = |x: i32| x + 1;\n    assert_eq!(dt(100), 101);",
                true,
            ),
            (
                "let alias of another function",
                "let dt = other;\n    dt(1);",
                true,
            ),
            (
                "nested fn",
                "fn dt(x: i32) -> i32 { x }\n    assert!(dt(1) > 0);",
                true,
            ),
            (
                "closure parameter",
                "let apply = |dt: fn(i32) -> i32| dt(1);\n    apply(other);",
                true,
            ),
            (
                "tuple pattern",
                "let (a, dt) = (1, other);\n    dt(a);",
                true,
            ),
            (
                "value mention after the binding",
                "let dt = other;\n    let f = dt;\n    f(1);",
                true,
            ),
            (
                "call before the binding",
                "dt(1);\n    let dt = other;\n    dt(2);",
                false,
            ),
            (
                "call in the binding's own initializer",
                "let dt = |x: i32| dt(x) + 1;\n    dt(1);",
                false,
            ),
            (
                "call after the binding's block",
                "{ let dt = other; dt(1); }\n    dt(2);",
                false,
            ),
            (
                "qualified call",
                "let dt = other;\n    super::dt(1);",
                false,
            ),
            (
                "qualified call inside a std macro",
                "let dt = other;\n    assert_eq!(crate::dt(1), 2);",
                false,
            ),
            (
                "path prefix inside a std macro",
                "let dt = other;\n    assert_eq!(dt(1), dt::ONE);",
                true,
            ),
            (
                "block import beside the binding",
                "let dt = other;\n    { use super::dt; dt(1); }",
                false,
            ),
            (
                "block glob",
                "let dt = other;\n    { use super::*; dt(1); }",
                false,
            ),
            (
                "match arm binder",
                "match Some(other) { Some(dt) => dt(1), None => 0 };",
                false,
            ),
            (
                "non-std macro",
                "let dt = other;\n    check!(dt(1));",
                false,
            ),
            ("no mention", "assert!(true);", false),
            (
                "cfg-gated binding",
                "#[cfg(test)]\n    let dt = other;\n    dt(1);",
                false,
            ),
            (
                "nested fn body sees no outer let",
                "let dt = other;\n    fn inner() -> i32 { dt(1) }",
                false,
            ),
            ("method call only", "let s = S;\n    s.dt();", false),
            (
                "import inside a macro operand block",
                "let dt = |x: i32| x + 1;\n    assert_eq!({ use super::{dt}; dt(100) }, 90);",
                false,
            ),
            (
                "nested macro in an assertion operand",
                "let dt = |x: i32| x;\n    assert_eq!(route!(dt(1)), 2);",
                false,
            ),
            (
                "inequality in an assertion operand",
                "let dt = |x: i32| x;\n    assert!(dt(1) != 2);",
                true,
            ),
            (
                "named field pattern",
                "let Pair { callback: dt } = Pair { callback: other };\n    assert_eq!(dt(1), 1);",
                true,
            ),
            ("reference pattern", "let &dt = &other;\n    dt(1);", true),
            ("slice pattern", "let [dt] = [other];\n    dt(1);", true),
            (
                "shorthand field pattern",
                "let Pair { dt } = Pair { dt: other };\n    dt(1);",
                true,
            ),
        ];
        for (label, body, expected) in cases {
            let source = format!("fn other(x: i32) -> i32 {{ x }}\nfn t() {{\n    {body}\n}}\n");
            assert_eq!(bound(&source), expected, "{label}");
        }
    }

    #[test]
    fn a_generated_body_is_read_from_its_expanded_fragment() {
        let file =
            SourceFile::parse("macro_rules! case { () => {}; }\n", Edition::Edition2024).tree();
        let read = |body: &str| {
            let fragment = SourceFile::parse(body, Edition::Edition2024).tree();
            super::every_mention_bound_locally_in_fragment(&file, Some(&[]), &fragment, "dt")
        };
        assert!(read(
            "#[test]\nfn t() {\n    let dt = |x: u32| x;\n    assert_eq!(dt(9), 9);\n}\n"
        ));
        assert!(!read("#[test]\nfn t() {\n    assert_eq!(dt(9), 9);\n}\n"));
        assert!(!read("fn t() { let dt = other; dt(1); }\nfn u() {}\n"));
    }

    #[test]
    fn a_std_macro_redefined_in_the_file_is_not_settled() {
        let source = "fn other(x: i32) -> i32 { x }\nfn t() {\n    let dt = other;\n    assert!(dt(1) > 0);\n}\nmacro_rules! assert { ($e:expr) => {}; }\n";
        assert!(!bound(source));
        let unknown_chain = SourceFile::parse(source, ra_ap_syntax::Edition::Edition2024).tree();
        let lines = LineIndex::new(source);
        assert!(!every_mention_bound_locally_in_tree(
            &unknown_chain,
            None,
            &lines,
            2,
            "t",
            "dt"
        ));
    }
}
