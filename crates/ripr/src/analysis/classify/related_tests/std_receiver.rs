//! Standard-collection receivers cannot call a trait default method (#11113).
//!
//! A trait default method such as `Flags::contains` has no `impl` self type,
//! so a name match alone used to be `direct_owner_call`: any `x.contains(..)`
//! in a test related it, including `slice::contains` on a local `Vec`.
//!
//! This module recognises the one negative shape that is established from
//! syntax alone and demotes it to a name-only relation:
//!
//! - every spelling of the method in the test body is a method call whose
//!   receiver is a plain local binding (or fn parameter) in live scope;
//! - each such binding has an explicit annotation, or a `vec!`/`Type::new`
//!   style initializer, whose type head is a standard collection or string
//!   that the test file does not rebind and no indexed impl of the owner's
//!   trait names.
//!
//! Anything else (an unannotated binding, a field or call receiver, a path or
//! UFCS spelling, a macro argument ripr cannot parse, a closure or loop
//! binding) keeps the existing credit, so this only ever removes a relation.
//! Every gap in the proof of "not the trait's receiver" must fail open: an
//! unprovable shadow or impl keeps credit, because a missed shadow demotes a
//! genuine relation and a wrong demotion hides a real call.

use super::{
    RustIndex, TestSummary, assertion_arguments_resolve, base_type_name, is_ident_byte,
    mask_comments_and_strings, node_executes_in,
};
use crate::analysis::facts::FunctionContainer;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericParams, HasName},
};
use std::cell::Cell;
use std::path::{Path, PathBuf};

/// Types whose own methods (or deref target's methods) a bare `.contains(`
/// would resolve to; none of them is a user type the trait could be on.
const STD_RECEIVER_TYPES: &[&str] = &[
    "Vec",
    "VecDeque",
    "HashSet",
    "BTreeSet",
    "HashMap",
    "BTreeMap",
    "BinaryHeap",
    "String",
];

/// Whether every spelling of `method` in `test` is a method call on a local
/// bound to a standard collection, so none can be the trait default method of
/// `trait_name`. `false` whenever anything is unresolved.
pub(super) fn method_calls_only_on_std_receivers(
    test: &TestSummary,
    method: &str,
    trait_name: &str,
    index: &RustIndex,
) -> bool {
    if method.is_empty() {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    let spellings = method_spellings(&masked, method);
    if spellings.is_empty() || !spellings.iter().all(|dotted| *dotted) {
        return false;
    }
    let std_calls = Cell::new(0usize);
    let refused = Cell::new(false);
    let resolves = |node: SyntaxNode, scope: Option<&SyntaxNode>| {
        let Some(call) = ast::MethodCallExpr::cast(node) else {
            return false;
        };
        if call.name_ref().is_none_or(|name| name.text() != method) {
            return false;
        }
        let anchor = scope.unwrap_or_else(|| call.syntax());
        match call.receiver().and_then(|receiver| plain_local(&receiver)) {
            Some(name) if local_is_std_collection(&name, anchor, test, trait_name, index) => {
                std_calls.set(std_calls.get() + 1);
            }
            _ => refused.set(true),
        }
        // Visit every site: this predicate tallies, it never short-circuits.
        false
    };
    visit_executable_sites(test, index, &resolves);
    // Each textual spelling must be one resolved std call: a spelling inside
    // a macro the walk does not parse, or a duplicated visit, fails closed.
    !refused.get() && std_calls.get() == spellings.len()
}

/// Visit every site the test executes, including the operands of the standard
/// assertion macros. Unlike the reach authorities this ignores macro-binding
/// provenance: the visit only types a receiver to remove a name-only relation,
/// so an unestablished `assert!` still has its operands read, and a user
/// `macro_rules!` of the same name is still refused by the operand reader.
fn visit_executable_sites(
    test: &TestSummary,
    index: &RustIndex,
    visit: &dyn Fn(SyntaxNode, Option<&SyntaxNode>) -> bool,
) {
    let Some(parse) = index.source_syntax.parse_fragment(&test.body) else {
        return;
    };
    let syntax = parse.tree().syntax().clone();
    let Some(function) = syntax.children().find_map(ast::Fn::cast) else {
        return;
    };
    let executable = |node: &SyntaxNode| node_executes_in(node, function.syntax());
    for node in syntax.descendants().filter(executable) {
        visit(node.clone(), None);
        if let Some(call) = ast::MacroCall::cast(node) {
            assertion_arguments_resolve(&call, Some(index), visit);
        }
    }
}

/// For each identifier-bounded occurrence of `method` in the masked body,
/// whether it is written as a method call (`.method`, whitespace allowed).
fn method_spellings(masked: &str, method: &str) -> Vec<bool> {
    let bytes = masked.as_bytes();
    masked
        .match_indices(method)
        .filter(|(at, _)| {
            let end = at + method.len();
            (*at == 0 || !is_ident_byte(bytes[at - 1]))
                && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte))
        })
        .map(|(at, _)| masked[..at].trim_end().ends_with('.'))
        .collect()
}

fn plain_local(receiver: &ast::Expr) -> Option<String> {
    let mut receiver = receiver.clone();
    for _ in 0..8 {
        let inner = match &receiver {
            ast::Expr::ParenExpr(expression) => expression.expr(),
            ast::Expr::RefExpr(expression) => expression.expr(),
            _ => break,
        };
        receiver = inner?;
    }
    let ast::Expr::PathExpr(_) = &receiver else {
        return None;
    };
    let text = receiver.syntax().text().to_string();
    let text = text.trim();
    (!text.is_empty() && text.bytes().all(is_ident_byte)).then(|| text.to_string())
}

/// Nearest live `let`/parameter binding of `name` at `call`, standard
/// collection-typed. Closures, loops, match arms, `if let` bindings, scope
/// items, `use`s and non-plain patterns are unresolved.
fn local_is_std_collection(
    name: &str,
    call: &SyntaxNode,
    test: &TestSummary,
    trait_name: &str,
    index: &RustIndex,
) -> bool {
    let std_head = |head: &str| {
        STD_RECEIVER_TYPES.contains(&head)
            && !std_name_rebound(index, &test.file, head)
            && !trait_implemented_for(index, trait_name, head)
    };
    for ancestor in call.ancestors().skip(1) {
        // A closure parameter, loop pattern or match arm shadows `name` only
        // when its own pattern binds it; any other loop or arm leaves the
        // enclosing `let` visible.
        let shadows = ast::ClosureExpr::cast(ancestor.clone()).is_some_and(|closure| {
            closure
                .param_list()
                .is_some_and(|list| list.params().any(|param| pat_binds(param.pat(), name)))
        }) || ast::ForExpr::cast(ancestor.clone())
            .is_some_and(|for_loop| pat_binds(for_loop.pat(), name))
            || ast::MatchArm::cast(ancestor.clone()).is_some_and(|arm| pat_binds(arm.pat(), name));
        if shadows {
            return false;
        }
        let condition = ast::IfExpr::cast(ancestor.clone())
            .and_then(|node| node.condition())
            .or_else(|| ast::WhileExpr::cast(ancestor.clone()).and_then(|node| node.condition()));
        if condition.is_some_and(|condition| {
            condition
                .syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .any(|pattern| pattern.name().is_some_and(|bound| bound.text() == name))
        }) {
            return false;
        }
        if let Some(list) = ast::StmtList::cast(ancestor.clone()) {
            let rebound = list.syntax().children().any(|node| {
                (ast::Item::can_cast(node.kind())
                    && ast::AnyHasName::cast(node.clone())
                        .and_then(|item| item.name())
                        .is_some_and(|bound| bound.text() == name))
                    || ast::Use::cast(node)
                        .is_some_and(|item| item.syntax().text().to_string().contains(name))
            });
            if rebound {
                return false;
            }
            let binding = list
                .syntax()
                .children()
                .filter_map(ast::LetStmt::cast)
                .filter(|binding| binding.syntax().text_range().end() <= call.text_range().start())
                .filter(|binding| {
                    binding.pat().is_some_and(|pattern| {
                        pattern
                            .syntax()
                            .descendants()
                            .filter_map(ast::IdentPat::cast)
                            .any(|pattern| pattern.name().is_some_and(|bound| bound.text() == name))
                    })
                })
                .last();
            if let Some(binding) = binding {
                let Some(ast::Pat::IdentPat(pattern)) = binding.pat() else {
                    return false;
                };
                if pattern.at_token().is_some() || binding.let_else().is_some() {
                    return false;
                }
                if let Some(annotation) = binding.ty() {
                    return annotation_head(&annotation.syntax().text().to_string())
                        .is_some_and(|head| std_head(&head));
                }
                return binding.initializer().is_some_and(|initializer| {
                    initializer_head(&initializer).is_some_and(|head| std_head(&head))
                });
            }
        }
        if let Some(function) = ast::Fn::cast(ancestor) {
            return function.param_list().is_some_and(|list| {
                list.params().any(|param| {
                    matches!(param.pat(), Some(ast::Pat::IdentPat(pattern))
                        if pattern.at_token().is_none()
                            && pattern.name().is_some_and(|bound| bound.text() == name))
                        && param.ty().is_some_and(|ty| {
                            annotation_head(&ty.syntax().text().to_string())
                                .is_some_and(|head| std_head(&head))
                        })
                })
            });
        }
    }
    false
}

fn pat_binds(pattern: Option<ast::Pat>, name: &str) -> bool {
    pattern.is_some_and(|pattern| {
        pattern
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .any(|bound| bound.name().is_some_and(|bound| bound.text() == name))
    })
}

/// The unqualified type name of an annotation, behind any `&`/`&mut`:
/// `Vec<(&str, T)>` and `&mut HashSet<u8>` name `Vec` and `HashSet`. A
/// qualified path, `impl`/`dyn` type or tuple is not a name.
fn annotation_head(annotation: &str) -> Option<String> {
    let mut text = annotation.trim();
    loop {
        let stripped = text
            .strip_prefix('&')
            .map(|rest| {
                let rest = rest.trim_start();
                // Skip a lifetime such as `'a`.
                let rest = match rest.strip_prefix('\'') {
                    Some(lifetime) => lifetime.trim_start_matches(is_ident_char).trim_start(),
                    None => rest,
                };
                rest.strip_prefix("mut ").unwrap_or(rest).trim_start()
            })
            .or_else(|| text.strip_prefix("mut ").map(str::trim_start));
        match stripped {
            Some(rest) => text = rest,
            None => break,
        }
    }
    let head: String = text.chars().take_while(|c| is_ident_char(*c)).collect();
    let rest = text[head.len()..].trim_start();
    (!head.is_empty() && (rest.is_empty() || rest.starts_with('<'))).then_some(head)
}

fn is_ident_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// `vec![..]`, `Vec::new()`, `HashSet::with_capacity(n)`, `String::from(..)`
/// and the like: a single constructor expression with no trailing method
/// chain, whose type head is returned.
fn initializer_head(initializer: &ast::Expr) -> Option<String> {
    match initializer {
        ast::Expr::MacroExpr(expression) => {
            let call = expression.macro_call()?;
            let path = call.path()?.syntax().text().to_string();
            (path.trim() == "vec").then(|| "Vec".to_string())
        }
        ast::Expr::CallExpr(call) => {
            let ast::Expr::PathExpr(callee) = call.expr()? else {
                return None;
            };
            let text = callee.syntax().text().to_string();
            let segments: Vec<_> = text.split("::").map(str::trim).collect();
            match segments.as_slice() {
                [ty, "new" | "with_capacity" | "from" | "default"] => Some((*ty).to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether the bare standard-library name `head` may mean another type at
/// the test: the test file, or its parent module's file, uses or declares an
/// item of that name, or imports through a glob other than `self::*`/`super::*`.
/// Unlike `file_rebinds_type`, a `use super::*;` at the top of a module file
/// is admitted when the parent module file shows no such binding, because
/// every test module of a crate starts that way. When a parent module only
/// inherits through `use super::*;`, its parent module files are read too
/// (#11236 grandparent glob chain).
fn std_name_rebound(index: &RustIndex, test_file: &Path, head: &str) -> bool {
    rebound_scope_files(index, test_file, head)
        .iter()
        .any(|file| {
            if file != test_file && index.files().get(file).is_none() {
                return false;
            }
            file_binds_std_name(index, file, head)
        })
}

/// The test file and every module file whose `use super::*;` chain can name
/// `head` without a rebinding stop in between.
fn rebound_scope_files(index: &RustIndex, test_file: &Path, head: &str) -> Vec<PathBuf> {
    let mut files = vec![test_file.to_path_buf()];
    let mut pending = parent_module_files(test_file);
    files.extend(pending.iter().cloned());
    while let Some(module) = pending.pop() {
        if index.files().get(&module).is_none() {
            continue;
        }
        if !files.contains(&module) {
            files.push(module.clone());
        }
        if module_inherits_via_super_glob(index, &module)
            && !file_binds_std_name(index, &module, head)
        {
            pending.extend(parent_module_files(&module));
        }
    }
    files
}

fn module_inherits_via_super_glob(index: &RustIndex, file: &Path) -> bool {
    let Some(tree) = index.source_syntax.get(index, file).and_then(|s| s.tree()) else {
        return false;
    };
    tree.syntax()
        .descendants()
        .filter_map(ast::Use::cast)
        .any(|item| {
            let compact: String = item
                .syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect();
            compact == "usesuper::*;"
        })
}

#[cfg(test)]
pub(super) fn std_name_rebound_for_tests(index: &RustIndex, test_file: &str, head: &str) -> bool {
    std_name_rebound(index, Path::new(test_file), head)
}

/// `dir/mod.rs` and `dir.rs` for a test file `dir/stem.rs`: the files a
/// `super::*` in it can name. When `file` is itself `mod.rs`, only the
/// enclosing inline module file `dir.rs` is returned.
fn parent_module_files(test_file: &Path) -> Vec<PathBuf> {
    let Some(dir) = test_file.parent() else {
        return Vec::new();
    };
    if test_file.file_name().is_some_and(|name| name == "mod.rs") {
        return vec![dir.with_extension("rs")];
    }
    let mut files = vec![dir.join("mod.rs")];
    if test_file.file_name().is_some_and(|name| name != "mod.rs") {
        files.push(dir.with_extension("rs"));
    }
    files
}

fn file_binds_std_name(index: &RustIndex, file: &Path, head: &str) -> bool {
    let Some(tree) = index.source_syntax.get(index, file).and_then(|s| s.tree()) else {
        return true;
    };
    let syntax = tree.syntax();
    let names = |node: &SyntaxNode| {
        node.descendants().any(|node| {
            ast::Name::cast(node.clone()).is_some_and(|bound| bound.text() == head)
                || ast::NameRef::cast(node).is_some_and(|bound| bound.text() == head)
        })
    };
    for item in syntax.descendants().filter_map(ast::Use::cast) {
        let compact: String = item
            .syntax()
            .text()
            .to_string()
            .split_whitespace()
            .collect();
        let glob = item
            .syntax()
            .descendants()
            .filter_map(ast::UseTree::cast)
            .any(|tree| tree.star_token().is_some());
        if names(item.syntax())
            || (glob && !matches!(compact.as_str(), "usesuper::*;" | "useself::*;"))
        {
            return true;
        }
    }
    syntax.descendants().any(|node| {
        let declared = ast::Adt::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::TypeAlias::cast(node.clone()).and_then(|item| item.name()))
            .or_else(|| ast::Trait::cast(node.clone()).and_then(|item| item.name()))
            .or_else(|| ast::Module::cast(node).and_then(|item| item.name()));
        declared.is_some_and(|name| name.text() == head)
    })
}

/// Whether an `impl <owner trait> for <self type>` could make the std type
/// `head` a receiver of the trait's default method. Fails open: an impl whose
/// self type is not provably a user-declared type keeps the credit. Indexed
/// impl fns are read first; the source scan then covers what no fn indexes
/// (an empty `impl Trait for Vec<u32> {}`), qualified spellings, aliases and
/// macro-generated impls (#11236).
fn trait_implemented_for(index: &RustIndex, trait_name: &str, head: &str) -> bool {
    let indexed = index.functions().iter().any(|function| {
        matches!(&function.item.container,
            FunctionContainer::TraitImpl { trait_path, self_ty }
                if impl_trait_name_matches(index, &function.file, trait_path, trait_name)
                    && (last_segment_name(strip_reference_prefix(self_ty)) == head
                        || impl_self_reaches_std(self_ty)))
    });
    indexed || source_may_implement_for_std(index, trait_name, head)
}

/// `std::vec::Vec<u32>` -> `Vec`: the final path segment before any generics.
fn last_segment_name(self_ty: &str) -> &str {
    let path = self_ty.split('<').next().unwrap_or(self_ty);
    path.rsplit("::").next().map(str::trim).unwrap_or(path)
}

/// Scan every indexed file for an impl of `trait_name` (including empty ones)
/// whose self type is std-headed or cannot be ruled out, or a macro that
/// spells such an impl. A file that does not parse is treated as a match.
fn source_may_implement_for_std(index: &RustIndex, trait_name: &str, head: &str) -> bool {
    // This runs per resolved receiver of every related test, so keep it off
    // the whole-workspace AST: only a file that spells the trait name can hold
    // an impl or macro for it, and the declared-type set (every file) is built
    // lazily, once, and only when such an impl needs it.
    let mut trees = Vec::new();
    let mut mentioning = Vec::new();
    for (path, file) in index.files().iter() {
        let Some(tree) = index.source_syntax.get(index, path).and_then(|s| s.tree()) else {
            return true;
        };
        let root = tree.syntax().clone();
        if file.source.contains(trait_name) {
            mentioning.push((path.clone(), root.clone()));
        }
        trees.push(root);
    }
    let declared: std::cell::OnceCell<std::collections::BTreeSet<String>> =
        std::cell::OnceCell::new();
    let is_declared = |base: &str| {
        declared
            .get_or_init(|| {
                trees
                    .iter()
                    .flat_map(|root| root.descendants())
                    .filter_map(|node| ast::Adt::cast(node).and_then(|item| item.name()))
                    .map(|name| name.text().to_string())
                    .collect()
            })
            .contains(base)
    };
    mentioning
        .iter()
        .flat_map(|(path, root)| {
            root.descendants()
                .map(|node| (path.clone(), node))
                .collect::<Vec<_>>()
        })
        .any(|(path, node)| {
            if let Some(item) = ast::Impl::cast(node.clone()) {
                let Some(trait_ty) = item.trait_() else {
                    return false;
                };
                let trait_text = trait_ty.syntax().text().to_string();
                if !impl_trait_name_matches(index, &path, &trait_text, trait_name) {
                    return false;
                }
                let Some(self_ty) = item.self_ty() else {
                    return true;
                };
                let text = self_ty.syntax().text().to_string();
                let text = text.trim();
                let base = last_segment_name(strip_reference_prefix(text));
                let generic = item.generic_param_list().is_some_and(|list| {
                    list.generic_params().any(|param| {
                        matches!(&param, ast::GenericParam::TypeParam(ty)
                        if ty.name().is_some_and(|name| name.text() == base))
                    })
                });
                return base == head
                    || generic
                    || impl_self_reaches_std(text)
                    || !is_declared(base);
            }
            // Tokens a macro expands into an impl are not parsed as an item.
            let macro_text = (ast::MacroRules::can_cast(node.kind())
                || ast::MacroCall::can_cast(node.kind()))
            .then(|| node.text().to_string());
            macro_text.is_some_and(|text| {
                text.contains("impl") && text.contains(trait_name) && text.contains("for")
            })
        })
}

/// `&Vec<u8>` / `&mut Vec<u8>` -> `Vec<u8>`: only leading `&` and `mut `, so a
/// qualified path such as `other::Vec` keeps its identity.
fn strip_reference_prefix(self_ty: &str) -> &str {
    let mut text = self_ty.trim();
    while let Some(rest) = text.strip_prefix('&') {
        text = rest.trim_start();
        text = text.strip_prefix("mut ").unwrap_or(text).trim_start();
    }
    text
}

/// Whether an `impl <trait> for <self_ty>` can apply to a std collection
/// receiver without naming it: a slice, array, `str` or `dyn` type is reached
/// by deref, and a bare single-letter type parameter is a blanket impl.
fn impl_self_reaches_std(self_ty: &str) -> bool {
    let text = strip_reference_prefix(self_ty);
    text.starts_with('[')
        || text.starts_with("dyn ")
        || base_type_name(text) == "str"
        || (text.len() == 1 && text.bytes().all(|byte| byte.is_ascii_uppercase()))
}

fn impl_trait_name_matches(
    index: &RustIndex,
    file: &Path,
    trait_path: &str,
    trait_name: &str,
) -> bool {
    let path = trait_path.split('<').next().unwrap_or(trait_path);
    let last = path.rsplit("::").next().map(str::trim);
    if last == Some(trait_name) {
        return true;
    }
    last.is_some_and(|alias| use_rename_imports_trait(index, file, alias, trait_name))
}

/// Whether a `use … as alias` in `file` binds `alias` to the owner trait name.
fn use_rename_imports_trait(index: &RustIndex, file: &Path, alias: &str, trait_name: &str) -> bool {
    let Some(tree) = index.source_syntax.get(index, file).and_then(|s| s.tree()) else {
        return false;
    };
    tree.syntax()
        .descendants()
        .filter_map(ast::UseTree::cast)
        .any(|tree| {
            tree.rename()
                .and_then(|rename| rename.name())
                .is_some_and(|name| name.text() == alias)
                && tree.path().is_some_and(|path| {
                    path.segments().last().is_some_and(|segment| {
                        segment
                            .name_ref()
                            .is_some_and(|name| name.text() == trait_name)
                    })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_annotations_when_reading_head_then_std_name_is_unqualified_only() {
        assert_eq!(
            annotation_head("Vec<(&'static str, TestFlags)>").as_deref(),
            Some("Vec")
        );
        assert_eq!(
            annotation_head("&mut HashSet<u8>").as_deref(),
            Some("HashSet")
        );
        assert_eq!(annotation_head("&'a String").as_deref(), Some("String"));
        assert_eq!(annotation_head("std::vec::Vec<u8>"), None);
        assert_eq!(annotation_head("impl Iterator<Item = u8>"), None);
        assert_eq!(annotation_head("(Vec<u8>, u8)"), None);
    }

    #[test]
    fn given_masked_body_when_scanning_spellings_then_dotted_and_path_forms_differ() {
        assert_eq!(
            method_spellings("a.contains(x); b . contains (y);", "contains"),
            vec![true, true]
        );
        assert_eq!(
            method_spellings("case(T::contains); v.contains(x);", "contains"),
            vec![false, true]
        );
        assert!(method_spellings("not_contains(x); contains_all(y);", "contains").is_empty());
    }

    #[test]
    fn given_reference_self_types_when_stripping_then_only_prefix_goes() {
        assert_eq!(strip_reference_prefix("&Vec<u32>"), "Vec<u32>");
        assert_eq!(strip_reference_prefix("& mut Vec<u32>"), "Vec<u32>");
        assert_eq!(strip_reference_prefix("&mut Vec<u32>"), "Vec<u32>");
        assert_eq!(strip_reference_prefix("other::Vec<u32>"), "other::Vec<u32>");
    }

    #[test]
    fn given_impl_self_types_when_checking_reach_then_deref_and_blanket_block() {
        for reaches in [
            "[u8]",
            "&[T]",
            "&mut [u8; 4]",
            "str",
            "&str",
            "dyn Any",
            "T",
        ] {
            assert!(impl_self_reaches_std(reaches), "{reaches}");
        }
        for named in ["Unit", "Vec<u8>", "Wrapper<T>", "Tx"] {
            assert!(!impl_self_reaches_std(named), "{named}");
        }
    }
}
