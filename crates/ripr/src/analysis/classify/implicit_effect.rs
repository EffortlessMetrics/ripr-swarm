//! Assertion-local authority for the bounded shared-handle trait observers.
//! This is deliberately not an alias resolver: one local shared handle, one
//! record-literal receiver and a prior direct loop/operator/closure dispatch.
//! Unlocated assertions, deferred/nested execution, intervening uses and unknown
//! field transfer refuse the kind-only exception. Reach remains independent.
use super::super::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasArgList, HasAttrs, HasName},
};
use std::collections::HashSet;
use std::sync::OnceLock;

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn known_builtin_attribute(attr: &ast::Attr) -> bool {
    let text = compact(&attr.syntax().text().to_string());
    let Some(inner) = text.strip_prefix("#[").and_then(|s| s.strip_suffix(']')) else {
        return false;
    };
    let key = inner.split(['(', '=']).next().unwrap_or("");
    if key == "derive" {
        let Some((_, list)) = inner.split_once('(') else {
            return false;
        };
        let Some(list) = list.strip_suffix(')') else {
            return false;
        };
        return list.split(',').all(|name| {
            matches!(
                name,
                "Clone"
                    | "Copy"
                    | "Debug"
                    | "Default"
                    | "Eq"
                    | "Hash"
                    | "Ord"
                    | "PartialEq"
                    | "PartialOrd"
            )
        });
    }
    matches!(
        key,
        "allow"
            | "warn"
            | "deny"
            | "forbid"
            | "expect"
            | "doc"
            | "cfg"
            | "test"
            | "ignore"
            | "should_panic"
            | "repr"
            | "inline"
            | "cold"
            | "must_use"
            | "deprecated"
            | "non_exhaustive"
            | "macro_export"
            | "track_caller"
            | "no_mangle"
            | "export_name"
            | "link"
            | "link_section"
            | "target_feature"
    )
}
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct ImplicitExpansionMemo {
    surface: OnceLock<ExpansionSurface>,
}

/// The immutable source index and its run-owned safety memo travel together.
pub(in crate::analysis) struct ImplicitExpansionContext<'a> {
    index: &'a RustIndex,
    memo: &'a ImplicitExpansionMemo,
}

impl<'a> ImplicitExpansionContext<'a> {
    pub(in crate::analysis) fn new(index: &'a RustIndex, memo: &'a ImplicitExpansionMemo) -> Self {
        Self { index, memo }
    }

    fn admits(&self, receiver: &str) -> bool {
        self.memo.admits(self.index, receiver)
    }
}

#[derive(Clone, Debug, Default)]
struct ExpansionSurface {
    bounded: bool,
    excluded_receivers: HashSet<String>,
    scanned_sources: usize,
}

impl ImplicitExpansionMemo {
    pub(in crate::analysis) fn admits(&self, index: &RustIndex, receiver: &str) -> bool {
        let surface = self.surface.get_or_init(|| {
            ExpansionSurface::scan(
                index
                    .files()
                    .values()
                    .map(|facts| std::sync::Arc::clone(&facts.source)),
            )
        });
        surface.bounded
            && !surface
                .excluded_receivers
                .contains(receiver.rsplit("::").next().unwrap_or(receiver))
    }
}

impl ExpansionSurface {
    fn scan<S: AsRef<str>>(sources: impl IntoIterator<Item = S>) -> Self {
        const BUILTIN_MACROS: &[&str] = &[
            "assert",
            "assert_eq",
            "assert_ne",
            "debug_assert",
            "debug_assert_eq",
            "debug_assert_ne",
            "format",
            "format_args",
            "vec",
            "matches",
            "panic",
            "unreachable",
            "todo",
            "unimplemented",
            "stringify",
            "concat",
            "env",
            "option_env",
            "include_str",
            "include_bytes",
            "file",
            "line",
            "column",
            "module_path",
        ];
        const BUILTIN_DERIVES: &[&str] = &[
            "Clone",
            "Copy",
            "Debug",
            "Default",
            "Eq",
            "Hash",
            "Ord",
            "PartialEq",
            "PartialOrd",
        ];
        const DISPATCH_TRAITS: &[&str] = &[
            "Iterator",
            "std::iter::Iterator",
            "core::iter::Iterator",
            "Add",
            "std::ops::Add",
            "core::ops::Add",
        ];
        let mut surface = Self {
            bounded: true,
            ..Self::default()
        };
        for source in sources {
            let source = source.as_ref();
            surface.scanned_sources += 1;
            let Some(parse) = parse_clean_source_file(source) else {
                surface.bounded = false;
                continue;
            };
            let tree = parse.tree();
            // Store only receiver-independent names, never source copies or syntax trees.
            for node in tree.syntax().descendants() {
                if let Some(alias) = ast::TypeAlias::cast(node.clone())
                    && let Some(ty) = alias.ty()
                {
                    surface.excluded_receivers.extend(
                        compact(&ty.syntax().text().to_string())
                            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                            .filter(|name| !name.is_empty())
                            .map(str::to_owned),
                    );
                }
                if let Some(use_tree) = ast::UseTree::cast(node.clone()) {
                    surface.excluded_receivers.extend(
                        compact(&use_tree.syntax().text().to_string())
                            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                            .filter(|name| !name.is_empty())
                            .map(str::to_owned),
                    );
                }
                if let Some(item) = ast::Impl::cast(node) {
                    let trait_text = item
                        .trait_()
                        .map(|t| compact(&t.syntax().text().to_string()))
                        .map(|path| path.split('<').next().unwrap_or(&path).to_string());
                    if trait_text
                        .as_deref()
                        .is_some_and(|name| !DISPATCH_TRAITS.contains(&name))
                        && let Some(ty) = item.self_ty()
                    {
                        surface.excluded_receivers.insert(
                            compact(&ty.syntax().text().to_string())
                                .rsplit("::")
                                .next()
                                .unwrap_or("")
                                .to_owned(),
                        );
                    }
                }
            }
            let bounded = !tree.syntax().descendants().any(|node| {
            ast::Module::cast(node.clone())
                .and_then(|module| module.name())
                .is_some_and(|name| matches!(name.text(), "std" | "core"))
                || ast::UseTree::cast(node.clone())
                    .and_then(|use_tree| use_tree.rename())
                    .and_then(|rename| rename.name())
                    .is_some_and(|name| matches!(name.text(), "std" | "core"))
                || ast::UseTree::cast(node.clone()).is_some_and(|use_tree| {
                    use_tree.use_tree_list().is_none()
                        && use_tree.rename().is_none()
                        && use_tree.path().is_some_and(|path| {
                            matches!(
                                compact(&path.syntax().text().to_string())
                                    .rsplit("::")
                                    .next()
                                    .unwrap_or(""),
                                "std" | "core"
                            )
                        })
                })
                || ast::ExternCrate::cast(node.clone())
                    .and_then(|krate| krate.rename())
                    .and_then(|rename| rename.name())
                    .is_some_and(|name| matches!(name.text(), "std" | "core"))
        })
            && !tree
            .syntax()
            .descendants()
            .any(|node| ast::MacroRules::can_cast(node.kind()))
            // Preserve the Drop veto through renamed imports. Compacting a
            // whole use item joins `Drop as Alias`, hiding the Drop token.
            && !tree.syntax().descendants().filter_map(ast::UseTree::cast).any(|item| {
                item.path().is_some_and(|path| {
                    compact(&path.syntax().text().to_string()).rsplit("::").next() == Some("Drop")
                })
            })
            && !tree
            .syntax()
            .descendants()
            .filter_map(ast::Impl::cast)
            .any(|item| {
                let trait_text = item
                    .trait_()
                        .map(|t| compact(&t.syntax().text().to_string()))
                        .map(|path| path.split('<').next().unwrap_or(&path).to_string());
                trait_text.as_deref().is_some_and(|name| {
                    name == "Drop" || name.ends_with("::Drop")
                })
            })
            && tree
                .syntax()
                .descendants()
                .filter_map(ast::MacroCall::cast)
                .all(|call| {
                    call.path().is_some_and(|path| {
                        let path_text = path.syntax().text().to_string();
                        let path_text = path_text.trim();
                        !path_text.contains("::") && BUILTIN_MACROS.contains(&path_text)
                    })
                })
            // An imported macro can shadow a prelude macro with the same
            // spelling. Refuse wildcard imports and imports of any name in
            // the small macro allowlist instead of attempting resolution.
            && tree
                .syntax()
                .descendants()
                .filter_map(ast::Use::cast)
                .all(|item| {
                    let use_text = compact(&item.syntax().text().to_string());
                    let canonical_iterator_import = matches!(
                        use_text.as_str(),
                        "usestd::iter::Iterator;" | "usecore::iter::Iterator;"
                    );
                    let canonical_add_import = matches!(
                        use_text.as_str(),
                        "usestd::ops::Add;" | "usecore::ops::Add;"
                    );
                    !use_text.contains('*')
                        && use_text
                            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                            .all(|name| {
                                !BUILTIN_MACROS.contains(&name)
                                    && !BUILTIN_DERIVES.contains(&name)
                                    && name != "Drop"
                                    && (name != "Iterator" || canonical_iterator_import)
                                    && (name != "Add" || canonical_add_import)
                            })
                })
            && tree
                .syntax()
                .descendants()
                .filter_map(ast::Attr::cast)
                .all(|attr| known_builtin_attribute(&attr));
            surface.bounded &= bounded;
        }
        surface
    }
}
fn name(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expr else {
        return None;
    };
    let text = path.syntax().text().to_string();
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then_some(text)
}
fn mentions(node: &ra_ap_syntax::SyntaxNode, binding: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == binding)
}
fn single_self_field_use(function: &FunctionSummary, field: &str) -> bool {
    let Some(parse) = parse_clean_source_file(&function.body) else {
        return false;
    };
    let tree = parse.tree();
    // Macro arguments are token trees, so PathExpr traversal cannot account
    // for hidden reads or writes through `self`.
    if tree
        .syntax()
        .descendants()
        .any(|node| ast::MacroCall::can_cast(node.kind()))
    {
        return false;
    }
    let uses: Vec<_> = tree
        .syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .filter(|p| p.syntax().text() == "self")
        .collect();
    let mut target_uses = 0;
    for receiver in uses {
        let Some(read) = receiver.syntax().parent().and_then(ast::FieldExpr::cast) else {
            return false;
        };
        if read.name_ref().is_some_and(|n| n.text() == field) {
            target_uses += 1;
        }
    }
    target_uses == 1
}
fn single_parameter_use(function: &FunctionSummary, parameter: &str) -> bool {
    let Some(parse) = parse_clean_source_file(&function.body) else {
        return false;
    };
    let tree = parse.tree();
    // Do not grant an assertion based on an incomplete view of macro tokens.
    if tree
        .syntax()
        .descendants()
        .any(|node| ast::MacroCall::can_cast(node.kind()))
    {
        return false;
    }
    tree.syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .filter(|p| p.syntax().text() == parameter)
        .count()
        == 1
        && !tree
            .syntax()
            .descendants()
            .filter_map(ast::LetStmt::cast)
            .any(|s| s.pat().is_some_and(|p| mentions(p.syntax(), parameter)))
}

fn shared_field(
    dispatch: &FunctionSummary,
    owner: &FunctionSummary,
    expression: &str,
) -> Option<String> {
    let root = expression.split('.').next()?;
    if dispatch.id == owner.id {
        let field = expression.strip_prefix("self.")?.split('.').next()?;
        return single_self_field_use(dispatch, field).then(|| field.to_string());
    }
    // Only a direct, one-argument transfer into the changed owner's sole
    // parameter is established. Longer reverse paths stay unknown.
    if super::activation::signature_parameters(owner) != [root]
        || !single_parameter_use(owner, root)
    {
        return None;
    }
    let parse = parse_clean_source_file(&dispatch.body)?;
    let calls: Vec<_> = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| {
            call.expr()
                .is_some_and(|e| name(&e).as_deref() == Some(owner.name.as_str()))
        })
        .collect();
    if calls.len() != 1
        || calls[0]
            .syntax()
            .ancestors()
            .skip(1)
            .any(|node| matches!(node.kind(), SyntaxKind::CLOSURE_EXPR | SyntaxKind::IF_EXPR))
        || calls[0]
            .syntax()
            .ancestors()
            .filter(|node| ast::BlockExpr::can_cast(node.kind()))
            .count()
            != 1
    {
        return None;
    }
    let args: Vec<_> = calls[0].arg_list()?.args().collect();
    let [ast::Expr::RefExpr(reference)] = args.as_slice() else {
        return None;
    };
    let ast::Expr::FieldExpr(field) = reference.expr()? else {
        return None;
    };
    let field_name = field.name_ref()?.text().to_string();
    (name(&field.expr()?).as_deref() == Some("self")
        && single_self_field_use(dispatch, &field_name))
    .then_some(field_name)
}
fn standard_handles(tree: &ra_ap_syntax::SourceFile) -> bool {
    const HANDLES: &[&str] = &["Rc", "Cell", "RefCell", "Vec"];
    // A same-spelled local type or type alias defeats the lexical std form.
    if tree.syntax().descendants().any(|node| {
        let declared = ast::Struct::cast(node.clone())
            .and_then(|s| s.name())
            .or_else(|| ast::Enum::cast(node.clone()).and_then(|s| s.name()))
            .or_else(|| ast::TypeAlias::cast(node).and_then(|s| s.name()));
        declared.is_some_and(|name| HANDLES.contains(&name.text()))
    }) {
        return false;
    }
    let imports: Vec<_> = tree
        .syntax()
        .descendants()
        .filter_map(ast::UseTree::cast)
        .filter(|u| u.use_tree_list().is_none())
        .collect();
    ["Rc", "Cell", "RefCell", "Vec"].into_iter().all(|handle| {
        let named: Vec<_> = imports
            .iter()
            .filter(|u| {
                u.path().is_some_and(|p| {
                    p.syntax().text().to_string().rsplit("::").next() == Some(handle)
                }) || u
                    .rename()
                    .is_some_and(|r| r.syntax().text().to_string().trim_end().ends_with(handle))
            })
            .collect();
        if named.is_empty() {
            return handle != "Rc";
        }
        if named.len() != 1 || named[0].rename().is_some() {
            return false;
        }
        let mut paths: Vec<_> = named[0]
            .syntax()
            .ancestors()
            .filter_map(ast::UseTree::cast)
            .filter_map(|u| u.path())
            .map(|p| compact(&p.syntax().text().to_string()))
            .collect();
        paths.reverse();
        let expected = if handle == "Rc" {
            "std::rc::Rc"
        } else if handle == "Cell" {
            "std::cell::Cell"
        } else if handle == "RefCell" {
            "std::cell::RefCell"
        } else {
            "std::vec::Vec"
        };
        paths.join("::") == expected
    })
}

fn observer_local(assertion: &ast::MacroCall, source: &str) -> Option<(String, String)> {
    if assertion.path()?.syntax().text() != "assert_eq" {
        return None;
    }
    let args_source = format!(
        "fn __ripr_arguments() {{ let _ = {}; }}",
        assertion.token_tree()?.syntax().text()
    );
    let parse = parse_clean_source_file(&args_source)?;
    let tuple = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::TupleExpr::cast)?;
    let args: Vec<_> = tuple.fields().collect();
    let [
        ast::Expr::RecordExpr(actual),
        ast::Expr::RecordExpr(expected),
    ] = args.as_slice()
    else {
        return None;
    };
    let wrapper = compact(&actual.path()?.syntax().text().to_string());
    if compact(&expected.path()?.syntax().text().to_string()) != wrapper {
        return None;
    }
    if actual.record_expr_field_list()?.spread().is_some()
        || expected.record_expr_field_list()?.spread().is_some()
    {
        return None;
    }
    let actual_fields: Vec<_> = actual.record_expr_field_list()?.fields().collect();
    let expected_fields: Vec<_> = expected.record_expr_field_list()?.fields().collect();
    let [actual_field] = actual_fields.as_slice() else {
        return None;
    };
    let [expected_field] = expected_fields.as_slice() else {
        return None;
    };
    if actual_field.field_name()?.text() != expected_field.field_name()?.text() {
        return None;
    }
    let local = actual_field.expr().and_then(|e| name(&e)).or_else(|| {
        actual_field
            .expr()
            .is_none()
            .then(|| actual_field.field_name().map(|n| n.text().to_string()))
            .flatten()
    })?;
    if expected_field
        .expr()
        .is_some_and(|e| mentions(e.syntax(), &local))
    {
        return None;
    }
    // Only a one-field, explicitly derived equality wrapper in this source
    // file is supported; custom equality and unknown containers refuse.
    let definitions = parse_clean_source_file(source)?;
    if !standard_handles(&definitions.tree()) {
        return None;
    }
    let structs: Vec<_> = definitions
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter(|s| s.name().is_some_and(|n| n.text() == wrapper))
        .collect();
    let [definition] = structs.as_slice() else {
        return None;
    };
    if !definition.attrs().any(|a| {
        a.path().is_some_and(|p| p.syntax().text() == "derive")
            && a.syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == "PartialEq")
    }) {
        return None;
    }
    let ast::FieldList::RecordFieldList(fields) = definition.field_list()? else {
        return None;
    };
    let fields: Vec<_> = fields.fields().collect();
    let [field] = fields.as_slice() else {
        return None;
    };
    if field.name()?.text() != actual_field.field_name()?.text() {
        return None;
    }
    let ty = compact(&field.ty()?.syntax().text().to_string());
    if !(ty.starts_with("Rc<Cell<") || ty.starts_with("Rc<RefCell<Vec<")) {
        return None;
    }
    let expected_value = compact(&expected_field.expr()?.syntax().text().to_string());
    Some((local, expected_value))
}
fn dispatch_statement(statement: &ast::Stmt, receiver: &str, method: &str) -> bool {
    let text = compact(&statement.syntax().text().to_string());
    match method {
        "next" | "into_iter" => {
            text == format!("for_in{receiver}{{}}")
                || text == format!("{receiver}.for_each(|_|{{}});")
                || text == format!("while{receiver}.next().is_some(){{}}")
        }
        "add" => text == format!("let_={receiver}+();"),
        _ => false,
    }
}

fn local_dispatch_type(
    source: &str,
    self_ty: &str,
    expansion: &ImplicitExpansionContext<'_>,
) -> bool {
    if self_ty.is_empty()
        || !self_ty
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    // Keep the bounded observer proof inside source forms whose expansions
    // cannot add hidden Drop implementations.
    if !expansion.admits(self_ty) {
        return false;
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let tree = parse.tree();
    // An item macro can emit a trait impl (including Drop) without producing
    // an `ast::Impl` in the unexpanded tree. Refuse module-level invocations.
    if tree
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .any(|call| {
            !call
                .syntax()
                .ancestors()
                .any(|node| ast::Fn::can_cast(node.kind()))
        })
    {
        return false;
    }
    // A destructor runs when the consumed receiver leaves scope and may write
    // the same sink after the observed dispatch. Its spelling may be
    // qualified, so any same-file Drop impl makes this bounded proof unknown.
    let receiver_types: Vec<_> = tree
        .syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter(|s| s.name().is_some_and(|n| n.text() == self_ty))
        .collect();
    receiver_types.len() == 1
        && receiver_types[0].attrs().next().is_none()
        && !tree.syntax().descendants().any(|node| {
            ast::TypeAlias::cast(node.clone())
                .is_some_and(|a| a.name().is_some_and(|n| n.text() == self_ty))
                || ast::UseTree::cast(node).is_some_and(|u| {
                    u.rename().is_some()
                        || u.path().is_some_and(|p| {
                            p.syntax().text().to_string().rsplit("::").next() == Some(self_ty)
                        })
                })
        })
}

pub(super) fn assertion_observes_dispatch(
    dispatch: &FunctionSummary,
    owner: &FunctionSummary,
    expression: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    source: &str,
    expansion: &ImplicitExpansionContext<'_>,
) -> bool {
    let FunctionContainer::TraitImpl { self_ty, .. } = &dispatch.item.container else {
        return false;
    };
    if !local_dispatch_type(source, self_ty, expansion) {
        return false;
    }
    let Some(field) = shared_field(dispatch, owner, expression) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file(&test.body) else {
        return false;
    };
    let tree = parse.tree();
    let Some(function) = tree.syntax().children().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(statements) = function.body().and_then(|b| b.stmt_list()) else {
        return false;
    };
    let Some(relative_line) = assertion.line.checked_sub(test.start_line) else {
        return false;
    };
    let calls: Vec<_> = tree
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| {
            let at = usize::from(call.syntax().text_range().start());
            test.body
                .get(..at)
                .is_some_and(|p| p.matches('\n').count() == relative_line)
        })
        .collect();
    let [call] = calls.as_slice() else {
        return false;
    };
    let Some((observed_local, expected_value)) = observer_local(call, source) else {
        return false;
    };
    let visible: Vec<_> = statements
        .statements()
        .take_while(|s| s.syntax().text_range().end() <= call.syntax().text_range().start())
        .collect();
    // The assertion must be a top-level statement, not a deferred closure,
    // nested function or condition whose execution/order is unestablished.
    if !statements.statements().any(|s| matches!(s,
        ast::Stmt::ExprStmt(ref e) if matches!(e.expr(), Some(ast::Expr::MacroExpr(ref m)) if m.macro_call().is_some_and(|c| c.syntax() == call.syntax())))) { return false; }
    for (at, statement) in visible.iter().enumerate() {
        let ast::Stmt::LetStmt(binding) = statement else {
            continue;
        };
        let Some(ast::Pat::IdentPat(pattern)) = binding.pat() else {
            continue;
        };
        let Some(receiver) = pattern.name().map(|n| n.text().to_string()) else {
            continue;
        };
        let Some(ast::Expr::RecordExpr(record)) = binding.initializer() else {
            continue;
        };
        if record
            .path()
            .is_none_or(|p| compact(&p.syntax().text().to_string()) != compact(self_ty))
        {
            continue;
        }
        // The existing receiver/trait resolver is still required by the caller;
        // here we bind the exact literal instance's written field.
        let Some(fields) = record.record_expr_field_list() else {
            continue;
        };
        if fields.spread().is_some()
            || fields.fields().any(|f| {
                f.field_name().is_none_or(|name| name.text() != field)
                    && !matches!(f.expr(), Some(ast::Expr::Literal(_)))
            })
        {
            continue;
        }
        let Some(shared) = fields
            .fields()
            .find(|f| f.field_name().is_some_and(|n| n.text() == field))
        else {
            continue;
        };
        let Some(ast::Expr::MethodCallExpr(clone)) = shared.expr() else {
            continue;
        };
        if clone.name_ref().is_none_or(|n| n.text() != "clone")
            || clone.arg_list().is_none_or(|a| a.args().next().is_some())
        {
            continue;
        }
        let Some(local) = clone.receiver().and_then(|e| name(&e)) else {
            continue;
        };
        if observed_local != local {
            continue;
        }
        // Exactly one local introduction and no intervening alias, mutation,
        // replacement or opaque use of either handle is admitted.
        if visible[..at]
            .iter()
            .filter(|s| mentions(s.syntax(), &receiver))
            .count()
            != 0
        {
            continue;
        }
        let introductions: Vec<_> = visible[..at]
            .iter()
            .filter(|s| mentions(s.syntax(), &local))
            .collect();
        if introductions.len() != 1 {
            continue;
        }
        let ast::Stmt::LetStmt(introduction) = introductions[0] else {
            continue;
        };
        if introduction
            .pat()
            .is_none_or(|p| compact(&p.syntax().text().to_string()) != local)
        {
            continue;
        }
        let Some(initializer) = introduction.initializer() else {
            continue;
        };
        let init = compact(&initializer.syntax().text().to_string());
        let Some(value_parse) =
            parse_clean_source_file(&format!("fn __ripr_value() {{ let _ = {expression}; }}"))
        else {
            continue;
        };
        let Some(write) = value_parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::LetStmt::cast)
            .and_then(|s| s.initializer())
        else {
            continue;
        };
        let ast::Expr::MethodCallExpr(write) = write else {
            continue;
        };
        let Some(arguments) = write.arg_list() else {
            continue;
        };
        let arguments: Vec<_> = arguments.args().collect();
        let [ast::Expr::Literal(value)] = arguments.as_slice() else {
            continue;
        };
        let value = compact(&value.syntax().text().to_string());
        let expected = match write.name_ref().map(|n| n.text().to_string()).as_deref() {
            Some("set") => format!("Rc::new(Cell::new({value}))"),
            Some("push") => format!("Rc::new(RefCell::new(vec![{value}]))"),
            _ => continue,
        };
        if expected_value != expected {
            continue;
        }
        // The exact constructors and literal/empty initial states exclude
        // initializer transformations and computed expected values.
        let initial_state = init
            .strip_prefix("Rc::new(Cell::new(")
            .and_then(|s| s.strip_suffix("))"));
        let initial_cell = initial_state
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s != value);
        let initial_vec =
            init == "Rc::new(RefCell::new(Vec::new()))" || init == "Rc::new(RefCell::new(vec![]))";
        if !(initial_cell || initial_vec) {
            continue;
        }
        let mut dispatched = false;
        let mut defeated = false;
        for next in &visible[at + 1..] {
            if mentions(next.syntax(), &local) {
                defeated = true;
                break;
            }
            if mentions(next.syntax(), &receiver) {
                if !dispatched && dispatch_statement(next, &receiver, &dispatch.name) {
                    dispatched = true;
                } else {
                    defeated = true;
                    break;
                }
            }
        }
        if dispatched && !defeated {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::OracleFact;
    use crate::analysis::rust_index::{RustIndex, summarize_file};
    use std::path::PathBuf;

    #[test]
    fn workspace_expansion_scan_is_once_per_immutable_run() {
        let mut index = RustIndex::default();
        for slot in 0..64 {
            let path = PathBuf::from(format!("src/file_{slot}.rs"));
            index.insert_file_only(
                path.clone(),
                summarize_file(path, "struct Counter;".to_owned()),
            );
        }
        let memo = ImplicitExpansionMemo::default();
        for _probe in 0..32 {
            for receiver in ["Counter", "Other", "module::Counter"] {
                for _assertion in 0..8 {
                    assert!(memo.admits(&index, receiver));
                }
            }
        }
        assert_eq!(memo.surface.get().map(|s| s.scanned_sources), Some(64));
        // A successor source index receives a fresh run memo. Cross-file Drop,
        // aliases and unknown expansion remain workspace-wide vetoes.
        for source in [
            "impl Drop for Elsewhere { fn drop(&mut self) {} }",
            "use std::ops::Drop as Destructor; impl Destructor for Elsewhere { fn drop(&mut self) {} }",
            "macro_rules! hidden { () => {} }",
            "fn broken( {",
        ] {
            let path = PathBuf::from("src/unknown.rs");
            let mut successor = index.clone();
            successor.insert_file_only(path.clone(), summarize_file(path, source.to_owned()));
            let successor_memo = ImplicitExpansionMemo::default();
            assert!(!successor_memo.admits(&successor, "Counter"), "{source}");
            assert!(memo.admits(&index, "Counter"));
        }
    }

    #[test]
    fn cached_receiver_exclusions_remain_receiver_specific() {
        let surface = ExpansionSurface::scan([
            "struct Counter; struct Other; type Alias = Counter; impl Custom for Other {}",
        ]);
        assert!(surface.bounded);
        assert!(surface.excluded_receivers.contains("Counter"));
        assert!(surface.excluded_receivers.contains("Other"));
        assert!(!surface.excluded_receivers.contains("Unrelated"));
    }

    const CELL_OBSERVER: &str = r#"use std::{cell::Cell, rc::Rc};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl Iterator for Counter {
    type Item = ();
    fn next(&mut self) -> Option<()> {
        if self.finished { return None; }
        self.finished = true;
        self.sink.set(1);
        Some(())
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn implicit_observer() {
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    for _ in counter {}
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
"#;

    const VEC_OBSERVER: &str = r#"use std::{cell::RefCell, rc::Rc};
struct Counter { sink: Rc<RefCell<Vec<usize>>>, finished: bool }
impl Iterator for Counter {
    type Item = ();
    fn next(&mut self) -> Option<()> {
        if self.finished { return None; }
        self.finished = true;
        record(&self.sink);
        Some(())
    }
}
fn record(sink: &Rc<RefCell<Vec<usize>>>) {
    sink.borrow_mut().push(1);
}
#[derive(Debug, PartialEq)] struct Observation { sink: Rc<RefCell<Vec<usize>>> }
#[test]
fn implicit_observer() {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let counter = Counter { sink: sink.clone(), finished: false };
    for _ in counter {}
    assert_eq!(Observation { sink }, Observation { sink: Rc::new(RefCell::new(vec![1])) });
}
"#;

    const ADD_OBSERVER: &str = r#"use std::ops::Add;
use std::{cell::Cell, rc::Rc};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl std::ops::Add<()> for Counter {
    type Output = ();
    fn add(self, _: ()) {
        self.sink.set(1);
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn implicit_observer() {
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    let _ = counter + ();
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
"#;

    const DIRECT: &str = "self.sink.set(1)";
    const REVERSE: &str = "sink.borrow_mut().push(1)";
    const CELL_ASSERTION: &str =
        "assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });";

    fn edit(source: &str, from: &str, to: &str) -> Result<String, String> {
        if !source.contains(from) {
            return Err(format!("fixture drifted: missing {from:?}"));
        }
        Ok(source.replacen(from, to, 1))
    }

    /// Runs the production predicate over parsed facts of `source`.
    fn observes(
        source: &str,
        dispatch: &str,
        owner: &str,
        expression: &str,
    ) -> Result<bool, String> {
        observes_with(source, source, dispatch, owner, expression, |_| {})
    }

    fn observes_with(
        facts_source: &str,
        predicate_source: &str,
        dispatch: &str,
        owner: &str,
        expression: &str,
        adjust: impl FnOnce(&mut OracleFact),
    ) -> Result<bool, String> {
        let path = PathBuf::from("src/lib.rs");
        let facts = summarize_file(path.clone(), facts_source.to_owned());
        let find = |name: &str| {
            facts
                .functions
                .iter()
                .find(|function| function.name == name)
                .ok_or_else(|| format!("fixture lacks function {name}"))
        };
        let dispatch = find(dispatch)?;
        let owner = find(owner)?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "implicit_observer")
            .ok_or("fixture lacks the test")?;
        let mut assertion = test
            .assertions
            .first()
            .cloned()
            .ok_or("fixture lacks an assertion")?;
        adjust(&mut assertion);
        let mut index = RustIndex::default();
        index.insert_file_only(path.clone(), summarize_file(path, facts_source.to_owned()));
        let memo = ImplicitExpansionMemo::default();
        let context = ImplicitExpansionContext::new(&index, &memo);
        Ok(assertion_observes_dispatch(
            dispatch,
            owner,
            expression,
            test,
            &assertion,
            predicate_source,
            &context,
        ))
    }

    #[test]
    fn controls_admit_each_bounded_observer_form() -> Result<(), String> {
        assert!(observes(CELL_OBSERVER, "next", "next", DIRECT)?);
        assert!(observes(ADD_OBSERVER, "add", "add", DIRECT)?);
        assert!(observes(VEC_OBSERVER, "next", "record", REVERSE)?);
        for form in [
            "counter.for_each(|_| {});",
            "while counter.next().is_some() {}",
        ] {
            let source = edit(CELL_OBSERVER, "for _ in counter {}", form)?;
            assert!(observes(&source, "next", "next", DIRECT)?, "{form}");
        }
        Ok(())
    }

    #[test]
    fn non_trait_impl_dispatch_is_refused() -> Result<(), String> {
        let source = edit(
            CELL_OBSERVER,
            "impl Iterator for Counter {\n    type Item = ();\n    fn next(&mut self) -> Option<()> {",
            "impl Counter {\n    fn next(&mut self) -> Option<()> {",
        )?;
        assert!(!observes(&source, "next", "next", DIRECT)?);
        Ok(())
    }

    #[test]
    fn hidden_destructor_or_expansion_vetoes_the_proof() -> Result<(), String> {
        for extra in [
            "impl Drop for Counter { fn drop(&mut self) {} }",
            "impl std::ops::Drop for Counter { fn drop(&mut self) {} }",
            "macro_rules! hidden { () => {} }",
            "hidden_item!{}",
            "impl Custom for Counter {}",
            "type Alias = Counter;",
            "use other::Counter;",
        ] {
            let source = format!("{CELL_OBSERVER}{extra}\n");
            assert!(!observes(&source, "next", "next", DIRECT)?, "{extra}");
        }
        // Any attribute on the receiver struct, even a derive, is unproven.
        let source = edit(
            CELL_OBSERVER,
            "struct Counter",
            "#[derive(Debug)]\nstruct Counter",
        )?;
        assert!(!observes(&source, "next", "next", DIRECT)?);
        // A duplicate receiver definition is ambiguous.
        let source = format!("{CELL_OBSERVER}mod other {{ struct Counter; }}\n");
        assert!(!observes(&source, "next", "next", DIRECT)?);
        Ok(())
    }

    #[test]
    fn unparseable_predicate_source_is_refused() -> Result<(), String> {
        let admitted = observes_with(
            CELL_OBSERVER,
            "fn broken( {",
            "next",
            "next",
            DIRECT,
            |_| {},
        )?;
        assert!(!admitted);
        Ok(())
    }

    #[test]
    fn unlocated_or_misplaced_assertion_is_refused() -> Result<(), String> {
        for shift in [-1_i64, 1, 500] {
            let admitted =
                observes_with(CELL_OBSERVER, CELL_OBSERVER, "next", "next", DIRECT, |a| {
                    a.line = a.line.saturating_add_signed(shift as isize);
                })?;
            assert!(!admitted, "shift {shift}");
        }
        let before_test =
            observes_with(CELL_OBSERVER, CELL_OBSERVER, "next", "next", DIRECT, |a| {
                a.line = 0;
            })?;
        assert!(!before_test);
        Ok(())
    }

    #[test]
    fn deferred_or_conditional_assertions_are_refused() -> Result<(), String> {
        for wrapped in [
            format!("if true {{ {CELL_ASSERTION} }}"),
            format!("let check = || {{ {CELL_ASSERTION} }};\n    check();"),
            format!("{{ {CELL_ASSERTION} }}"),
        ] {
            let source = edit(CELL_OBSERVER, CELL_ASSERTION, &wrapped)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{wrapped}");
        }
        Ok(())
    }

    #[test]
    fn assertion_shapes_other_than_equal_record_pair_are_refused() -> Result<(), String> {
        for replacement in [
            "assert_ne!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });",
            "assert!(true);",
            "assert_eq!(state, state);",
            "assert_eq!(Observation { cell: state }, Other { cell: Rc::new(Cell::new(1)) });",
            "assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)), ..base() });",
            "assert_eq!(Observation { cell: state, extra: 1 }, Observation { cell: Rc::new(Cell::new(1)), extra: 1 });",
            "assert_eq!(Observation { cell: state }, Observation { other: Rc::new(Cell::new(1)) });",
            "assert_eq!(Observation { cell: state }, Observation { cell: state });",
            "assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(state.get())) });",
        ] {
            let source = edit(CELL_OBSERVER, CELL_ASSERTION, replacement)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{replacement}");
        }
        Ok(())
    }

    #[test]
    fn expected_value_must_match_the_observed_write() -> Result<(), String> {
        let source = edit(CELL_OBSERVER, "Cell::new(1)) });", "Cell::new(2)) });")?;
        assert!(!observes(&source, "next", "next", DIRECT)?);
        let source = edit(VEC_OBSERVER, "vec![1]", "vec![2]")?;
        assert!(!observes(&source, "next", "record", REVERSE)?);
        for expression in [
            "self.sink.replace(1)",
            "self.sink.set(1, 2)",
            "self.sink.set(compute())",
            "self.sink",
        ] {
            assert!(
                !observes(CELL_OBSERVER, "next", "next", expression)?,
                "{expression}"
            );
        }
        Ok(())
    }

    #[test]
    fn initial_state_must_be_a_distinct_literal_or_empty_vec() -> Result<(), String> {
        for initial in [
            "Rc::new(Cell::new(1))",
            "Rc::new(Cell::new(seed))",
            "Rc::new(Cell::new(0)).clone()",
            "make()",
        ] {
            let source = edit(CELL_OBSERVER, "Rc::new(Cell::new(0))", initial)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{initial}");
        }
        for initial in [
            "Rc::new(RefCell::new(vec![5]))",
            "Rc::new(RefCell::new(Vec::with_capacity(1)))",
        ] {
            let source = edit(VEC_OBSERVER, "Rc::new(RefCell::new(Vec::new()))", initial)?;
            assert!(!observes(&source, "next", "record", REVERSE)?, "{initial}");
        }
        let source = edit(
            VEC_OBSERVER,
            "Rc::new(RefCell::new(Vec::new()))",
            "Rc::new(RefCell::new(vec![]))",
        )?;
        assert!(observes(&source, "next", "record", REVERSE)?);
        Ok(())
    }

    #[test]
    fn wrapper_must_be_a_derived_single_shared_handle() -> Result<(), String> {
        let decl = "struct Observation { cell: Rc<Cell<usize>> }";
        for (from, to) in [
            (
                "#[derive(Debug, PartialEq)] struct Observation",
                "#[derive(Debug)] struct Observation",
            ),
            (
                "#[derive(Debug, PartialEq)] struct Observation",
                "struct Observation",
            ),
            (decl, "struct Observation { cell: Cell<usize> }"),
            (decl, "struct Observation(Rc<Cell<usize>>);"),
            (
                decl,
                "struct Observation { cell: Rc<Cell<usize>>, extra: u8 }",
            ),
            (decl, "struct Observation { sink: Rc<Cell<usize>> }"),
        ] {
            let source = edit(CELL_OBSERVER, from, to)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{to}");
        }
        let source = format!("{CELL_OBSERVER}struct Observation;\n");
        assert!(!observes(&source, "next", "next", DIRECT)?);
        Ok(())
    }

    #[test]
    fn shadowed_or_renamed_standard_handles_are_refused() -> Result<(), String> {
        for extra in [
            "struct Cell;",
            "enum Vec {}",
            "type RefCell = u8;",
            "struct Rc;",
        ] {
            let source = format!("{CELL_OBSERVER}{extra}\n");
            assert!(!observes(&source, "next", "next", DIRECT)?, "{extra}");
        }
        let imports = "use std::{cell::Cell, rc::Rc};";
        for to in [
            "use std::{cell::Cell, rc::Rc as Shared};",
            "use std::cell::Cell;",
            "use other::rc::Rc;\nuse std::cell::Cell;",
            "use std::{cell::Cell, rc::Rc};\nuse std::rc::Rc;",
            "use std::{cell::Cell, rc::Rc};\nuse other::Vec;",
        ] {
            let source = edit(CELL_OBSERVER, imports, to)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{to}");
        }
        Ok(())
    }

    #[test]
    fn receiver_literal_must_be_exact_and_bind_the_shared_local() -> Result<(), String> {
        let literal = "let counter = Counter { sink: state.clone(), finished: false };";
        for replacement in [
            "let counter = Counter { sink: state.clone(), finished: flag };",
            "let counter = Counter { sink: state.clone(), ..base() };",
            "let counter = Counter { sink: other.clone(), finished: false };",
            "let counter = Counter { sink: state, finished: false };",
            "let counter = Counter { sink: state.clone(1), finished: false };",
            "let counter = Counter { sink: state.to_owned(), finished: false };",
            "let counter = Counter { sink: make(), finished: false };",
            "let counter = Other { sink: state.clone(), finished: false };",
            "let counter = build(state.clone());",
            "let counter;",
        ] {
            let source = edit(CELL_OBSERVER, literal, replacement)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{replacement}");
        }
        Ok(())
    }

    #[test]
    fn intervening_uses_or_unproven_dispatch_defeat_the_exception() -> Result<(), String> {
        for replacement in [
            "counter.next();",
            "let _ = counter;",
            "for _ in counter {}\n    let _again = counter;",
            "for _ in counter {}\n    for _ in counter {}",
            "let alias = state.clone();\n    for _ in counter {}",
            "for _ in counter {}\n    state.set(9);",
            "touch(&counter);\n    for _ in counter {}",
            "",
        ] {
            let source = edit(CELL_OBSERVER, "for _ in counter {}", replacement)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{replacement}");
        }
        Ok(())
    }

    #[test]
    fn shared_local_must_have_exactly_one_plain_introduction() -> Result<(), String> {
        let introduction = "let state = Rc::new(Cell::new(0));";
        for replacement in [
            "let state = Rc::new(Cell::new(0));\n    let state = Rc::new(Cell::new(0));",
            "let state = Rc::new(Cell::new(0));\n    state.set(0);",
            "let mut state = Rc::new(Cell::new(0));",
            "let (state, _) = (Rc::new(Cell::new(0)), 0);",
            "",
        ] {
            let source = edit(CELL_OBSERVER, introduction, replacement)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{replacement}");
        }
        // A type annotation does not change the bound pattern; it stays admitted.
        let annotated = edit(
            CELL_OBSERVER,
            introduction,
            "let state: Rc<Cell<usize>> = Rc::new(Cell::new(0));",
        )?;
        assert!(observes(&annotated, "next", "next", DIRECT)?);
        Ok(())
    }

    #[test]
    fn dispatch_body_must_use_the_shared_field_exactly_once() -> Result<(), String> {
        for replacement in [
            "self.sink.set(1);\n        self.sink.set(1);",
            "assert!(self.finished);\n        self.sink.set(1);",
            "let me = self;\n        me.sink.set(1);",
            "self.sink.set(1);\n        let _ = self;",
        ] {
            let source = edit(CELL_OBSERVER, "self.sink.set(1);", replacement)?;
            assert!(!observes(&source, "next", "next", DIRECT)?, "{replacement}");
        }
        for expression in ["other.sink.set(1)", "sink.set(1)", ""] {
            assert!(
                !observes(CELL_OBSERVER, "next", "next", expression)?,
                "{expression}"
            );
        }
        Ok(())
    }

    #[test]
    fn reverse_transfer_admits_only_one_direct_unconditional_argument() -> Result<(), String> {
        for replacement in [
            "if self.finished { record(&self.sink); }",
            "let run = || record(&self.sink);\n        run();",
            "{ record(&self.sink); }",
            "record(&self.sink);\n        record(&self.sink);",
            "record(self.sink.clone());",
            "record(&self.sink.clone());",
            "record(&self.other);",
            "record(&other.sink);",
            "record(&self);",
            "record();",
            "other(&self.sink);",
            "let _ = self.sink.clone();\n        record(&self.sink);",
        ] {
            let source = edit(VEC_OBSERVER, "record(&self.sink);", replacement)?;
            assert!(
                !observes(&source, "next", "record", REVERSE)?,
                "{replacement}"
            );
        }
        Ok(())
    }

    #[test]
    fn reverse_owner_must_have_one_unique_parameter_use() -> Result<(), String> {
        let signature = "fn record(sink: &Rc<RefCell<Vec<usize>>>) {";
        let write = "    sink.borrow_mut().push(1);\n}";
        for (from, to) in [
            (
                signature,
                "fn record(sink: &Rc<RefCell<Vec<usize>>>, extra: usize) {",
            ),
            (
                write,
                "    sink.borrow_mut().push(1);\n    sink.borrow_mut().push(1);\n}",
            ),
            (
                write,
                "    let _alias = sink;\n    sink.borrow_mut().push(1);\n}",
            ),
            (write, "    log!(sink);\n    sink.borrow_mut().push(1);\n}"),
        ] {
            let source = edit(VEC_OBSERVER, from, to)?;
            assert!(!observes(&source, "next", "record", REVERSE)?, "{to}");
        }
        assert!(!observes(
            VEC_OBSERVER,
            "next",
            "record",
            "other.borrow_mut().push(1)"
        )?);
        Ok(())
    }

    #[test]
    fn expansion_surface_scan_vetoes_unknown_forms() {
        let bounded = |sources: &[&str]| ExpansionSurface::scan(sources.iter().copied()).bounded;
        assert!(bounded(&[
            "struct Counter; #[derive(Debug, Clone)] struct Other; #[cfg(test)] mod t { fn f() { assert!(true); let _ = vec![1]; } }"
        ]));
        assert!(bounded(&[
            "use std::iter::Iterator; use std::ops::Add; impl Iterator for Counter {}"
        ]));
        assert!(bounded(&["use core::iter::Iterator; use core::ops::Add;"]));
        for source in [
            "#[tokio::test] fn t() {}",
            "#[derive(Serialize)] struct S;",
            "#[derive] struct S;",
            "#[custom_attr] struct S;",
            "fn f() { custom!(); }",
            "fn f() { module::vec![1]; }",
            "use other::*;",
            "use other::vec;",
            "use other::assert_eq;",
            "use other::Clone;",
            "use other::Iterator;",
            "use other::ops::Add;",
            "use std::ops::Drop;",
            "use other::Drop as Gone;",
            "mod std {}",
            "mod core {}",
            "use other as std;",
            "use other as core;",
            "extern crate other as std;",
            "extern crate other as core;",
            "use std;",
            "use core;",
            "macro_rules! m { () => {} }",
            "impl Drop for S { fn drop(&mut self) {} }",
            "impl other::Drop for S { fn drop(&mut self) {} }",
            "fn broken( {",
        ] {
            assert!(!bounded(&[source]), "{source}");
        }
        // A renamed dispatch trait stays bounded, but an impl through the
        // rename is a non-dispatch trait impl and excludes its receiver.
        let renamed = ExpansionSurface::scan([
            "use std::iter::Iterator as It; impl It for Counter {}",
            "use std::ops::Add as Plus; impl Plus<()> for Summed {}",
        ]);
        assert!(renamed.excluded_receivers.contains("Counter"));
        assert!(renamed.excluded_receivers.contains("Summed"));
        // One vetoing source anywhere in the workspace vetoes every receiver.
        assert!(!bounded(&[
            "struct Counter;",
            "macro_rules! m { () => {} }"
        ]));
    }

    #[test]
    fn expansion_surface_excludes_only_names_the_sources_rename_or_reimplement() {
        let surface = ExpansionSurface::scan([
            "use other::{Imported, nested::Deep};",
            "type Wrapped = Vec<Hidden>;",
            "impl From<u8> for module::Converted {}",
            "impl Iterator for Plain {}",
            "impl std::ops::Add<()> for Summed {}",
            "impl core::iter::Iterator for Spelled {}",
        ]);
        for excluded in ["Imported", "nested", "Deep", "Hidden", "Converted"] {
            assert!(surface.excluded_receivers.contains(excluded), "{excluded}");
        }
        // An alias excludes the names it points at, not its own spelling.
        for admitted in ["Wrapped", "Plain", "Summed", "Spelled", "Unrelated"] {
            assert!(!surface.excluded_receivers.contains(admitted), "{admitted}");
        }
        let memo = ImplicitExpansionMemo::default();
        let mut index = RustIndex::default();
        let path = PathBuf::from("src/lib.rs");
        index.insert_file_only(
            path.clone(),
            summarize_file(path, "use other::Imported; struct Plain;".to_owned()),
        );
        assert!(!memo.admits(&index, "Imported"));
        assert!(!memo.admits(&index, "module::Imported"));
        assert!(memo.admits(&index, "Plain"));
        assert!(memo.admits(&index, "module::Plain"));
    }

    #[test]
    fn known_builtin_attribute_rejects_malformed_and_unknown_attributes() {
        let attr = |text: &str| -> Option<bool> {
            let parse = parse_clean_source_file(&format!("{text} fn f() {{}}"))?;
            let found = parse
                .tree()
                .syntax()
                .descendants()
                .find_map(ast::Attr::cast)?;
            Some(known_builtin_attribute(&found))
        };
        for accepted in [
            "#[test]",
            "#[inline(always)]",
            "#[doc = \"text\"]",
            "#[cfg(unix)]",
            "#[warn(unused)]",
            "#[derive(Debug, Clone, PartialEq)]",
            "#[must_use]",
        ] {
            assert_eq!(attr(accepted), Some(true), "{accepted}");
        }
        for refused in [
            "#[tokio::test]",
            "#[rstest]",
            "#[derive(Debug, Serialize)]",
            "#[derive(Debug, other::Clone)]",
            "#[derive]",
        ] {
            assert_eq!(attr(refused), Some(false), "{refused}");
        }
    }
}
