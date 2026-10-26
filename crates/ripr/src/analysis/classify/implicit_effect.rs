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
}
