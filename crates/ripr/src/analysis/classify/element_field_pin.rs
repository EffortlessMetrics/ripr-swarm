//! #7858: a field constructed inside an array the owner returns is observed
//! by a test that pins that element's field, by position or by membership.
//!
//! `fn rows(a, b) -> [Row; 2] { [Row { total: 0 }, Row { total: a + b }] }`
//! constructs the changed `total` in element 1. Two test shapes observe it:
//!
//! - `assert_eq!(rows[1].total, 33)` reads element 1 by its literal index;
//! - `assert!(rows.iter().any(|r| r.total == 33))` can only hold through
//!   element 1 when every other element builds `total` from an integer
//!   literal other than 33. When another element already holds 33, the test
//!   passes whatever element 1 holds, so it observes nothing.
//!
//! Both shapes need element identity, never a field-name coincidence. The
//! owner's body is the array and nothing else, the receiver is one immutable
//! binding of a direct owner call, and the assertion is a top-level statement
//! of a plain `#[test]` that cannot leave before it.

use super::super::rust_index::{FunctionSummary, RustIndex, TestSummary};
use super::owner_pin::{OwnerPinSyntax, field_builtin_integer, targeted_free_call_reaches_owner};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::Probe;
use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasName},
};

/// Whether `test` pins the field `probe` constructs inside the array `owner`
/// returns, through that element alone. The shared owner-pin authorities
/// decide what this syntax reading cannot: the assertion's context, what the
/// owner call binds, and the field's equality.
pub(in crate::analysis) fn observes_returned_element_field(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    observes_with(
        probe,
        owner,
        test,
        &SharedAuthority {
            owner,
            test,
            index,
            syntax,
        },
    )
}

/// The questions the element pin defers to a shared authority.
trait PinAuthority {
    /// The shared assertion-context validator (macro rebinding, escapes,
    /// module composition) admits the assertion on test line `line`.
    fn admits_line(&self, line: usize) -> bool;
    /// The owner call, bare (`None`) or through the path `prefix`, can only
    /// reach the owner: no same-named `fn`, `const`, `static` or import in
    /// the test's scope takes it. `receiver` is the test's own binding of
    /// the call's result, when it has one.
    fn call_reaches_owner(&self, prefix: Option<&str>, receiver: Option<&Receiver>) -> bool;
    /// The primitive integer type `field` of the struct `type_name` (the
    /// one the owner's record literals name) is declared with, so `==`
    /// against an integer literal is the built-in one.
    fn field_integer(&self, type_name: &str, field: &str) -> Option<&'static str>;
    /// Whether the workspace may supply `iter`, `into_iter` or `any` to the
    /// array or its iterator through a trait or macro, so the membership
    /// test may iterate something other than the returned elements.
    fn may_redefine_iteration(&self) -> bool;
}

struct SharedAuthority<'a> {
    owner: &'a FunctionSummary,
    test: &'a TestSummary,
    index: &'a RustIndex,
    syntax: &'a OwnerPinSyntax,
}

impl PinAuthority for SharedAuthority<'_> {
    fn admits_line(&self, line: usize) -> bool {
        self.test.assertions.iter().any(|assertion| {
            assertion.line == line && self.syntax.admits(self.test, assertion, self.index)
        })
    }

    fn call_reaches_owner(&self, prefix: Option<&str>, receiver: Option<&Receiver>) -> bool {
        let Some(receiver) = receiver else {
            return targeted_free_call_reaches_owner(
                self.test,
                self.owner,
                prefix,
                self.index,
                self.syntax,
            );
        };
        // `let rows = rows(30, 3);` calls the owner before its own binding
        // takes the name, and the receiver is the body's only binding of
        // it. The shared check reads any `let` of the owner's name as a
        // shadow, so it reads the body with that one binding renamed.
        let mut test = self.test.clone();
        test.body = receiver.renamed_body.clone().into();
        test.let_bindings
            .retain(|binding| binding.name != receiver.name);
        targeted_free_call_reaches_owner(&test, self.owner, prefix, self.index, self.syntax)
    }

    fn field_integer(&self, type_name: &str, field: &str) -> Option<&'static str> {
        // Both forms need the closed workspace: a macro, an outside crate or
        // a module the index skipped may declare a type under the field's
        // primitive name, or bring the trait the membership calls (#8059
        // re-review H1, H7, H8, H15, H16).
        if self.may_redefine_iteration() {
            return None;
        }
        field_builtin_integer(self.owner, type_name, field, self.index, self.syntax)
    }

    fn may_redefine_iteration(&self) -> bool {
        !self
            .syntax
            .iteration_closed(|| iteration_is_closed(self.index))
    }
}

/// The test's one binding of the owner call's result.
struct Receiver {
    name: String,
    /// The test's source with the binding's name replaced by a fresh one.
    renamed_body: String,
}

const ITERATION_METHODS: &[&str] = &["iter", "into_iter", "any"];

/// Whether no trait or macro in reach can supply `iter`, `into_iter` or
/// `any` to an array or a slice iterator. Every indexed file must parse, and
/// the index must hold every source: no non-UTF-8 file, include limitation,
/// `include!` or `#[path]` module. Every `use`, macro path and attribute
/// path must be rooted in the standard library or the workspace (an outside
/// crate's trait or macro is invisible), with no `extern crate`. Every `fn`
/// a macro may name (`fn $m`) refuses, and every `fn iter`, `fn into_iter`
/// or `fn any` token sequence, comments skipped, must be an indexed free,
/// local or inherent function, none of which a method call on an array or a
/// slice iterator can reach.
fn iteration_is_closed(index: &RustIndex) -> bool {
    closed_field_workspace(index, ITERATION_METHODS)
}

/// Shared primitive-field closure premise. Scalar field projection does not
/// invoke iteration methods; the existing array observer keeps its method list.
pub(super) fn field_workspace_is_closed(index: &RustIndex) -> bool {
    closed_field_workspace(index, &[])
}

fn closed_field_workspace(index: &RustIndex, methods: &[&str]) -> bool {
    if !index.non_utf8_sources.is_empty()
        || !index.include_limitations.is_empty()
        || !index
            .member_crates
            .is_dependency_closed(index.files().iter().map(|(path, _)| path))
    {
        return false;
    }
    let roots: Vec<String> = CLOSED_ROOTS
        .iter()
        .map(|root| (*root).to_string())
        .chain(
            index
                .package_names
                .iter()
                .map(|name| name.replace('-', "_")),
        )
        .collect();
    let allowed = |root: &str| roots.iter().any(|known| known == root);
    index.files().iter().all(|(path, facts)| {
        if facts.used_lexical_fallback {
            return false;
        }
        let Some(parse) = parse_clean_source_file(&facts.source) else {
            return false;
        };
        let tree = parse.tree();
        let mut written = vec![0usize; methods.len()];
        let mut tokens = tree
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia());
        while let Some(token) = tokens.next() {
            if token.kind() != SyntaxKind::FN_KW {
                continue;
            }
            match tokens.next() {
                Some(next) if next.kind() == SyntaxKind::DOLLAR => return false,
                Some(next) => {
                    let text = next.text().trim_start_matches("r#");
                    if let Some(slot) = methods.iter().position(|name| *name == text) {
                        written[slot] += 1;
                    }
                }
                None => {}
            }
        }
        let harmless = |name: &str| {
            index
                .functions()
                .iter()
                .filter(|function| {
                    function.file == *path
                        && function.name == name
                        && matches!(
                            function.item.container,
                            FunctionContainer::Free
                                | FunctionContainer::Local
                                | FunctionContainer::Inherent { .. }
                        )
                })
                .count()
        };
        if methods
            .iter()
            .zip(written)
            .any(|(name, count)| count > harmless(name))
        {
            return false;
        }
        tree.syntax().descendants().all(|node| {
            // A macro the workspace defines may write a method from tokens
            // no scan here reads as one (`$kw iter` with `fn` passed in).
            if ast::ExternCrate::can_cast(node.kind())
                || ast::MacroRules::can_cast(node.kind())
                || ast::MacroDef::can_cast(node.kind())
            {
                return false;
            }
            if let Some(item) = ast::Use::cast(node.clone()) {
                return item
                    .use_tree()
                    .is_some_and(|tree| use_roots_are(&tree, &allowed));
            }
            if let Some(call) = ast::MacroCall::cast(node.clone()) {
                return call.path().is_some_and(|path| {
                    path.segment()
                        .is_some_and(|last| last.syntax().text() != "include")
                        && path_root(&path)
                            .is_some_and(|(root, qualified)| !qualified || allowed(&root))
                });
            }
            // A macro's tokens are not items until they expand: a `use`,
            // `extern crate` or `include!` there may bring in what the
            // scan above cannot see.
            if let Some(tokens) = ast::TokenTree::cast(node.clone()) {
                return tokens
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .all(|token| !matches!(token.text(), "use" | "extern" | "include"));
            }
            if let Some(attribute) = ast::Attr::cast(node) {
                return attribute_is_closed(&attribute, &allowed);
            }
            true
        })
    })
}

/// Roots a closed workspace may name besides its own packages. Lint tool
/// roots (`clippy::`, `rustdoc::`) appear only in lint attributes.
const CLOSED_ROOTS: &[&str] = &[
    "std", "core", "alloc", "crate", "self", "super", "Self", "clippy", "rustfmt", "rustdoc",
];

/// The first segment of `path` and whether more segments follow it.
fn path_root(path: &ast::Path) -> Option<(String, bool)> {
    let mut first = path.clone();
    while let Some(qualifier) = first.qualifier() {
        first = qualifier;
    }
    if first.coloncolon_token().is_some() {
        return None;
    }
    let segment = first.segment()?;
    let text = segment.syntax().text().to_string();
    Some((text, first.syntax() != path.syntax()))
}

/// Every root a `use` tree names is allowed; a leading `::` never is.
fn use_roots_are(tree: &ast::UseTree, allowed: &dyn Fn(&str) -> bool) -> bool {
    match tree.path() {
        Some(path) => path_root(&path).is_some_and(|(root, _)| allowed(&root)),
        None => tree
            .use_tree_list()
            .is_some_and(|list| list.use_trees().all(|child| use_roots_are(&child, allowed))),
    }
}

/// An attribute is closed when it is not `#[path]` and every `name::` it
/// writes, its own path and its arguments (`derive(x::Y)`) included, starts
/// at an allowed root. Read as text: a string argument naming a path refuses
/// too.
fn attribute_is_closed(attribute: &ast::Attr, allowed: &dyn Fn(&str) -> bool) -> bool {
    let text = attribute.syntax().text().to_string();
    // `path` anywhere, `#[cfg_attr(.., path = "..")]` included, may name a
    // module file the index does not read.
    if text
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .any(|word| word == "path")
    {
        return false;
    }
    let inner = text
        .trim_start_matches('#')
        .trim_start()
        .trim_start_matches('!')
        .trim_start()
        .trim_start_matches('[')
        .trim_start();
    inner.match_indices("::").all(|(offset, _)| {
        let before = inner[..offset].trim_end();
        // Step past the whole separator: `→` or `“` is several bytes.
        let root_start = before
            .char_indices()
            .rev()
            .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
            .map_or(0, |(found, ch)| found + ch.len_utf8());
        let root = &before[root_start..];
        let qualified = before[..root_start].trim_end().ends_with("::");
        qualified || (!root.is_empty() && allowed(root))
    })
}

fn observes_with(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    authority: &dyn PinAuthority,
) -> bool {
    // A bare `rows(..)` call names a free function; a method owner is
    // never called that way, so the call would be some other `rows`.
    if owner.item.container != FunctionContainer::Free {
        return false;
    }
    let Some(array) = ReturnedArray::read(probe, owner) else {
        return false;
    };
    // Exactly `#[test]`: the lexical fallback and harness-registered tests
    // carry no attributes, so an empty list cannot vouch for the test.
    if !matches!(test.attrs.as_slice(), [only] if only.trim() == "#[test]") {
        return false;
    }
    // An integer field compares by value; a workspace `PartialEq` may not.
    let Some(integer) = authority.field_integer(&array.type_name, &array.field) else {
        return false;
    };
    let source = test.body.as_str();
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    if function
        .attrs()
        .any(|attr| squeeze(&attr.syntax().text().to_string()) != "#[test]")
    {
        return false;
    }
    let Some(body) = function.body() else {
        return false;
    };
    // A test that may leave early, through syntax or inside any macro's
    // tokens, or declares an item that may shadow the owner, is not read.
    if body.syntax().descendants_with_tokens().any(|element| {
        matches!(
            element.kind(),
            SyntaxKind::RETURN_KW
                | SyntaxKind::BREAK_KW
                | SyntaxKind::CONTINUE_KW
                | SyntaxKind::QUESTION
        ) || element.as_node().is_some_and(|node| {
            ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind())
        })
    }) {
        return false;
    }
    let Some(statements) = body.stmt_list() else {
        return false;
    };
    // The pinning assertion runs first: before it the test may only bind
    // the owner's result, so nothing can rebind the receiver, settle the
    // test or leave before the assertion is evaluated.
    let mut rest = statements.statements().peekable();
    let mut receiver = None;
    if let Some(ast::Stmt::LetStmt(binding)) = rest.peek()
        && let Some((pattern, call)) = owner_binding(binding)
    {
        let Some(name) = pattern.name() else {
            return false;
        };
        let name = name.text().to_string();
        if binds_more_than_once(&body, &name) {
            return false;
        }
        let range = pattern.syntax().text_range();
        let (Some(before), Some(after)) = (
            source.get(..usize::from(range.start())),
            source.get(usize::from(range.end())..),
        ) else {
            return false;
        };
        let bound = Receiver {
            renamed_body: format!("{before}ripr_element_receiver{after}"),
            name,
        };
        if !is_owner_call(&call, &owner.name, authority, Some(&bound)) {
            return false;
        }
        receiver = Some(bound);
        rest.next();
    }
    // `let r = &rows[k];` borrows one element of the result. It evaluates
    // the same literal index the inline read does, so it can settle nothing
    // the read would not, and the assertion then reads that element's field
    // through the alias.
    let mut alias = None;
    if let Some(ast::Stmt::LetStmt(binding)) = rest.peek()
        && let Some(name) =
            element_alias(binding, &array, receiver.as_ref(), &owner.name, authority)
    {
        if binds_more_than_once(&body, &name) {
            return false;
        }
        alias = Some(name);
        rest.next();
    }
    let first = match rest.next() {
        Some(ast::Stmt::ExprStmt(statement)) => statement.expr(),
        Some(_) => return false,
        None => statements.tail_expr(),
    };
    let Some(ast::Expr::MacroExpr(call)) = first.map(unwrap_parens) else {
        return false;
    };
    let Some(call) = call.macro_call() else {
        return false;
    };
    let Some(line) = source
        .get(..usize::from(call.syntax().text_range().start()))
        .map(|before| test.start_line + before.matches('\n').count())
    else {
        return false;
    };
    array.pinned_by(
        &call,
        receiver.as_ref(),
        alias.as_deref(),
        &owner.name,
        integer,
        authority,
    ) && authority.admits_line(line)
}

/// The owner's whole body is one array of struct literals, and the probe
/// constructs `field` in exactly one of them.
#[derive(Debug)]
struct ReturnedArray {
    /// The struct every element builds, named by one plain identifier.
    type_name: String,
    field: String,
    index: usize,
    /// The integer literal each other element builds `field` from, or
    /// `None` when that element builds it from anything else.
    others: Vec<Option<i128>>,
}

impl ReturnedArray {
    fn read(probe: &Probe, owner: &FunctionSummary) -> Option<Self> {
        let source = owner.body.as_str();
        let parse = parse_clean_source_file(source)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        let body = function.body()?;
        let statements = body.stmt_list()?;
        if statements.statements().next().is_some() {
            return None;
        }
        let ast::Expr::ArrayExpr(array) = unwrap_parens(statements.tail_expr()?) else {
            return None;
        };
        // A `#[cfg]` element or field may be compiled out and shift every
        // later index, so an attribute anywhere in the array refuses.
        if array.semicolon_token().is_some()
            || array
                .syntax()
                .descendants()
                .any(|node| node.kind() == SyntaxKind::ATTR)
        {
            return None;
        }
        let wanted = squeeze(probe.expression.trim().trim_end_matches(','));
        let mut path = None;
        let mut field = None;
        let mut index = None;
        let mut fields = Vec::new();
        for (position, element) in array.exprs().enumerate() {
            let ast::Expr::RecordExpr(record) = element else {
                return None;
            };
            let list = record.record_expr_field_list()?;
            if list.spread().is_some() || list.dotdot_token().is_some() {
                return None;
            }
            let record_path = squeeze(&record.path()?.syntax().text().to_string());
            if path.get_or_insert_with(|| record_path.clone()) != &record_path {
                return None;
            }
            let mut probed = None;
            for initializer in list.fields() {
                let text = squeeze(&initializer.syntax().text().to_string());
                let line = owner.start_line
                    + source
                        .get(..usize::from(initializer.syntax().text_range().start()))?
                        .matches('\n')
                        .count();
                if text == wanted && line == probe.location.line {
                    probed = Some(initializer.field_name()?.text().to_string());
                }
            }
            if let Some(name) = probed {
                if index.replace(position).is_some() {
                    return None;
                }
                field = Some(name);
            }
            fields.push(list);
        }
        let field = field?;
        let index = index?;
        let type_name = path?;
        if !type_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        let mut others = Vec::new();
        for (position, list) in fields.iter().enumerate() {
            let mut named = list.fields().filter(|initializer| {
                initializer
                    .field_name()
                    .is_some_and(|name| name.text() == field.as_str())
            });
            let initializer = named.next()?;
            if named.next().is_some() {
                return None;
            }
            if position != index {
                others.push(initializer.expr().as_ref().and_then(integer_literal));
            }
        }
        Some(Self {
            type_name,
            field,
            index,
            others,
        })
    }

    /// Whether the assertion `call` pins this element's field on the owner's
    /// result: `assert_eq!(recv[index].field, literal)` in either order, or
    /// `assert!(recv.iter().any(|r| r.field == literal))` when no other
    /// element can hold that literal.
    fn pinned_by(
        &self,
        call: &ast::MacroCall,
        receiver: Option<&Receiver>,
        alias: Option<&str>,
        owner: &str,
        integer: &str,
        authority: &dyn PinAuthority,
    ) -> bool {
        // A bare macro name only: a qualified path may name any crate's
        // macro, and the shared validator decides what a bare name binds.
        let Some(path) = call.path() else {
            return false;
        };
        if path.qualifier().is_some() || path.coloncolon_token().is_some() {
            return false;
        }
        let Some(name) = path.segment() else {
            return false;
        };
        let Some(arguments) = macro_arguments(call) else {
            return false;
        };
        let is_receiver = |expression: &ast::Expr| match unwrap_parens(expression.clone()) {
            ast::Expr::PathExpr(path) => receiver.is_some_and(|receiver| {
                path.path()
                    .is_some_and(|path| path.syntax().text() == receiver.name.as_str())
            }),
            ast::Expr::CallExpr(call) => is_owner_call(&call, owner, authority, receiver),
            _ => false,
        };
        match (
            name.syntax().text().to_string().as_str(),
            arguments.as_slice(),
        ) {
            ("assert_eq", [left, right]) => {
                [(left, right), (right, left)]
                    .into_iter()
                    .any(|(read, expected)| {
                        is_literal(expected)
                            && (self.indexed_read(read, &is_receiver)
                                || self.aliased_read(read, alias))
                    })
            }
            ("assert", [condition]) => {
                self.membership(condition, integer, &is_receiver)
                    && !authority.may_redefine_iteration()
            }
            _ => false,
        }
    }

    /// `recv[index].field` with the probed element's literal index.
    fn indexed_read(&self, read: &ast::Expr, is_receiver: &dyn Fn(&ast::Expr) -> bool) -> bool {
        let ast::Expr::FieldExpr(read) = unwrap_parens(read.clone()) else {
            return false;
        };
        if read
            .name_ref()
            .is_none_or(|name| name.text() != self.field.as_str())
        {
            return false;
        }
        let Some(ast::Expr::IndexExpr(element)) = read.expr().map(unwrap_parens) else {
            return false;
        };
        element.base().is_some_and(|base| is_receiver(&base))
            && element
                .index()
                .as_ref()
                .and_then(integer_literal)
                .is_some_and(|index| usize::try_from(index).is_ok_and(|index| index == self.index))
    }

    /// `alias.field`, where `alias` is the test's `let alias = &recv[index];`.
    fn aliased_read(&self, read: &ast::Expr, alias: Option<&str>) -> bool {
        let ast::Expr::FieldExpr(read) = unwrap_parens(read.clone()) else {
            return false;
        };
        read.name_ref()
            .is_some_and(|name| name.text() == self.field.as_str())
            && matches!(
                (read.expr().map(unwrap_parens), alias),
                (Some(ast::Expr::PathExpr(path)), Some(alias))
                    if path.path().is_some_and(|path| path.syntax().text() == alias)
            )
    }

    /// `recv.iter().any(|r| r.field == literal)`, or `into_iter`, or the
    /// literal first, where every other element builds the field from a
    /// different integer literal.
    fn membership(
        &self,
        condition: &ast::Expr,
        integer: &str,
        is_receiver: &dyn Fn(&ast::Expr) -> bool,
    ) -> bool {
        let ast::Expr::MethodCallExpr(any) = unwrap_parens(condition.clone()) else {
            return false;
        };
        if any.name_ref().is_none_or(|name| name.text() != "any")
            || any.generic_arg_list().is_some()
        {
            return false;
        }
        let Some(ast::Expr::MethodCallExpr(iter)) = any.receiver().map(unwrap_parens) else {
            return false;
        };
        if iter
            .name_ref()
            .is_none_or(|name| !matches!(name.text(), "iter" | "into_iter"))
            || iter
                .arg_list()
                .is_none_or(|list| list.args().next().is_some())
            || iter
                .receiver()
                .is_none_or(|receiver| !is_receiver(&receiver))
        {
            return false;
        }
        let Some(list) = any.arg_list() else {
            return false;
        };
        let arguments = list.args().collect::<Vec<_>>();
        let [ast::Expr::ClosureExpr(closure)] = arguments.as_slice() else {
            return false;
        };
        let Some(parameter) = sole_parameter(closure) else {
            return false;
        };
        let Some(ast::Expr::BinExpr(comparison)) = closure.body().map(unwrap_parens) else {
            return false;
        };
        if comparison.op_kind() != Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { negated: false })) {
            return false;
        }
        let (Some(left), Some(right)) = (comparison.lhs(), comparison.rhs()) else {
            return false;
        };
        let reads_field = |expression: &ast::Expr| {
            let ast::Expr::FieldExpr(read) = unwrap_parens(expression.clone()) else {
                return false;
            };
            read.name_ref()
                .is_some_and(|name| name.text() == self.field.as_str())
                && matches!(
                    read.expr().map(unwrap_parens),
                    Some(ast::Expr::PathExpr(path))
                        if path.path().is_some_and(|path| path.syntax().text() == parameter.as_str())
                )
        };
        [(&left, &right), (&right, &left)]
            .into_iter()
            .any(|(read, expected)| {
                // Every literal must fit the field's type: an out-of-range
                // one under `overflowing_literals` wraps onto another value.
                reads_field(read)
                    && integer_literal(expected).is_some_and(|expected| {
                        fits(expected, integer)
                            && self.others.iter().all(|other| {
                                other.is_some_and(|other| fits(other, integer) && other != expected)
                            })
                    })
            })
    }
}

/// `let NAME = CALL;` with a plain, immutable binding: the pattern and the
/// call, which [`is_owner_call`] then reads.
fn owner_binding(binding: &ast::LetStmt) -> Option<(ast::IdentPat, ast::CallExpr)> {
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    if binding.let_else().is_some() || binding.ty().is_some() {
        return None;
    }
    let ast::Expr::CallExpr(call) = unwrap_parens(binding.initializer()?) else {
        return None;
    };
    Some((pattern, call))
}

/// `let NAME = &RECV[index];` with a plain immutable binding, the probed
/// element's literal index and the owner's result (the test's receiver or a
/// direct owner call) as the indexed base: the alias's name. A binding named
/// like the owner could take the owner's name from a later call.
fn element_alias(
    binding: &ast::LetStmt,
    array: &ReturnedArray,
    receiver: Option<&Receiver>,
    owner: &str,
    authority: &dyn PinAuthority,
) -> Option<String> {
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    if binding.let_else().is_some() || binding.ty().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    if name == owner || receiver.is_some_and(|receiver| receiver.name == name) {
        return None;
    }
    let ast::Expr::RefExpr(borrow) = unwrap_parens(binding.initializer()?) else {
        return None;
    };
    if borrow.mut_token().is_some() || borrow.raw_token().is_some() {
        return None;
    }
    let ast::Expr::IndexExpr(element) = unwrap_parens(borrow.expr()?) else {
        return None;
    };
    let base_is_result = match unwrap_parens(element.base()?) {
        ast::Expr::PathExpr(path) => receiver.is_some_and(|receiver| {
            path.path()
                .is_some_and(|path| path.syntax().text() == receiver.name.as_str())
        }),
        ast::Expr::CallExpr(call) => is_owner_call(&call, owner, authority, receiver),
        _ => false,
    };
    let at_probed_index = element
        .index()
        .as_ref()
        .and_then(integer_literal)
        .is_some_and(|index| usize::try_from(index).is_ok_and(|index| index == array.index));
    (base_is_result && at_probed_index).then_some(name)
}

/// A direct call of the owner by its own name, or through `super::`/`crate::`,
/// with literal or path arguments only, so evaluating them cannot leave the
/// test or settle it before the assertion. The shared binding authority
/// decides that the name reaches the owner.
fn is_owner_call(
    call: &ast::CallExpr,
    owner: &str,
    authority: &dyn PinAuthority,
    receiver: Option<&Receiver>,
) -> bool {
    let plain_arguments = call.arg_list().is_some_and(|list| {
        list.args().all(|argument| {
            integer_literal(&argument).is_some()
                || matches!(
                    unwrap_parens(argument),
                    ast::Expr::Literal(_) | ast::Expr::PathExpr(_)
                )
        })
    });
    if !plain_arguments {
        return false;
    }
    let Some(ast::Expr::PathExpr(path)) = call.expr() else {
        return false;
    };
    let Some(path) = path.path() else {
        return false;
    };
    let text = squeeze(&path.syntax().text().to_string());
    if text == owner {
        return authority.call_reaches_owner(None, receiver);
    }
    ["super", "crate"].into_iter().any(|prefix| {
        text == format!("{prefix}::{owner}") && authority.call_reaches_owner(Some(prefix), receiver)
    })
}

/// The receiver is bound once in the whole test body, so no shadowing
/// binding, closure or pattern parameter can stand in for it.
fn binds_more_than_once(body: &ast::BlockExpr, name: &str) -> bool {
    body.syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| pattern.name().is_some_and(|ident| ident.text() == name))
        .count()
        > 1
}

fn sole_parameter(closure: &ast::ClosureExpr) -> Option<String> {
    let list = closure.param_list()?;
    let parameters = list.params().collect::<Vec<_>>();
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    if parameter.ty().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    pattern.name().map(|name| name.text().to_string())
}

/// The comma-separated arguments of an assertion macro, parsed as
/// expressions.
fn macro_arguments(call: &ast::MacroCall) -> Option<Vec<ast::Expr>> {
    let tree = call.token_tree()?;
    let text = tree.syntax().text().to_string();
    let inner = text.get(1..text.len().checked_sub(1)?)?;
    let parse = parse_clean_source_file(&format!("fn arguments() {{ arguments({inner}); }}"))?;
    let call = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)?;
    Some(call.arg_list()?.args().collect())
}

fn is_literal(expression: &ast::Expr) -> bool {
    matches!(unwrap_parens(expression.clone()), ast::Expr::Literal(_))
        || integer_literal(expression).is_some()
}

fn integer_literal(expression: &ast::Expr) -> Option<i128> {
    let (negative, literal) = match unwrap_parens(expression.clone()) {
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
            let ast::Expr::Literal(literal) = prefix.expr()? else {
                return None;
            };
            (true, literal)
        }
        ast::Expr::Literal(literal) => (false, literal),
        _ => return None,
    };
    let ast::LiteralKind::IntNumber(number) = literal.kind() else {
        return None;
    };
    let text = number.text().replace('_', "");
    let digits = INTEGER_SUFFIXES
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<i128>().ok()?;
    Some(if negative { -value } else { value })
}

/// Whether `value` is in range for the primitive integer `ty`. `usize` and
/// `isize` take their 32-bit ranges, which every target holds.
fn fits(value: i128, ty: &str) -> bool {
    let (low, high): (i128, i128) = match ty {
        "u8" => (0, u8::MAX.into()),
        "u16" => (0, u16::MAX.into()),
        "u32" | "usize" => (0, u32::MAX.into()),
        "u64" => (0, u64::MAX.into()),
        "u128" => (0, i128::MAX),
        "i8" => (i8::MIN.into(), i8::MAX.into()),
        "i16" => (i16::MIN.into(), i16::MAX.into()),
        "i32" | "isize" => (i32::MIN.into(), i32::MAX.into()),
        "i64" => (i64::MIN.into(), i64::MAX.into()),
        "i128" => (i128::MIN, i128::MAX),
        _ => return false,
    };
    (low..=high).contains(&value)
}

const INTEGER_SUFFIXES: &[&str] = &[
    "i128", "isize", "i16", "i32", "i64", "i8", "u128", "usize", "u16", "u32", "u64", "u8",
];

fn unwrap_parens(expression: ast::Expr) -> ast::Expr {
    let mut expression = expression;
    while let ast::Expr::ParenExpr(inner) = &expression {
        let Some(next) = inner.expr() else {
            break;
        };
        expression = next;
    }
    expression
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation};
    use std::path::PathBuf;

    const PROBE: &str = "total: a + b + 0";

    /// A file whose owner `rows` returns `array` on line 6 and whose one
    /// test runs `test`, under `attrs`.
    fn source(array: &str, attrs: &str, test: &str) -> String {
        format!(
            "pub struct Row {{\n    pub total: u32,\n}}\n\n\
             pub fn rows(a: u32, b: u32) -> [Row; 2] {{\n    {array}\n}}\n\n\
             #[cfg(test)]\nmod tests {{\n    use super::*;\n\n    {attrs}\n    fn pins() {{\n{test}\n    }}\n}}\n"
        )
    }

    fn probe() -> Probe {
        Probe {
            id: ProbeId("probe:src/lib.rs:6:rows".to_string()),
            location: SourceLocation::new("src/lib.rs", 6, 24),
            owner: None,
            family: ProbeFamily::FieldConstruction,
            delta: DeltaKind::Value,
            before: Some("total: a + b".to_string()),
            after: Some(PROBE.to_string()),
            expression: PROBE.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    /// Answers the shared-authority questions as configured and records the
    /// owner-call prefixes and field it was asked about, so the unit tests
    /// read the syntax gates alone.
    struct Stub<'a> {
        admits: &'a dyn Fn(usize) -> bool,
        reaches: bool,
        integer: bool,
        redefines: bool,
        prefixes: std::cell::RefCell<Vec<Option<String>>>,
        fields: std::cell::RefCell<Vec<(String, String)>>,
    }

    impl<'a> Stub<'a> {
        fn admitting(admits: &'a dyn Fn(usize) -> bool) -> Self {
            Self {
                admits,
                reaches: true,
                integer: true,
                redefines: false,
                prefixes: std::cell::RefCell::default(),
                fields: std::cell::RefCell::default(),
            }
        }
    }

    impl PinAuthority for Stub<'_> {
        fn admits_line(&self, line: usize) -> bool {
            (self.admits)(line)
        }
        fn call_reaches_owner(&self, prefix: Option<&str>, _: Option<&Receiver>) -> bool {
            self.prefixes.borrow_mut().push(prefix.map(str::to_string));
            self.reaches
        }
        fn field_integer(&self, type_name: &str, field: &str) -> Option<&'static str> {
            self.fields
                .borrow_mut()
                .push((type_name.to_string(), field.to_string()));
            self.integer.then_some("i32")
        }
        fn may_redefine_iteration(&self) -> bool {
            self.redefines
        }
    }

    const ARRAY: &str = "[Row { total: 0 }, Row { total: a + b + 0 }]";
    const ANY: &str =
        "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == 33));";

    /// `Ok(None)` when the owner's array cannot be read, else whether the
    /// test pins the probed element.
    fn pinned(array: &str, attrs: &str, test: &str) -> Result<Option<bool>, String> {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(array, attrs, test),
        );
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let Some(read) = ReturnedArray::read(&probe(), owner) else {
            return Ok(None);
        };
        if read.field != "total" || read.index != 1 {
            return Err(format!("unexpected read {read:?}"));
        }
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        Ok(Some(observes_with(
            &probe(),
            owner,
            test,
            &Stub::admitting(&|_| true),
        )))
    }

    #[test]
    fn a_test_pinning_the_probed_element_observes_its_field() -> Result<(), String> {
        for test in [
            ANY,
            "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| 33 == r.total));",
            "        let rows = rows(30, 3);\n        assert!(rows.into_iter().any(|r| r.total == 33u32));",
            "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);",
            "        let rows = rows(30, 3);\n        assert_eq!(33, rows[1].total);",
            "        assert_eq!(rows(30, 3)[1].total, 33);",
            "        assert!(super::rows(30, 3).iter().any(|r| r.total == 33));",
            "        let rows = crate::rows(30, 3);\n        assert_eq!(rows[1].total, 33)",
            "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert_eq!(r.total, 33);",
            "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert_eq!(33, r.total);",
            "        let r = &rows(30, 3)[1];\n        assert_eq!(r.total, 33);",
        ] {
            if pinned(ARRAY, "#[test]", test)? != Some(true) {
                return Err(format!("expected a pin: {test}"));
            }
        }
        Ok(())
    }

    #[test]
    fn a_membership_another_element_may_satisfy_observes_nothing() -> Result<(), String> {
        for array in [
            "[Row { total: 33 }, Row { total: a + b + 0 }]",
            "[Row { total: a }, Row { total: a + b + 0 }]",
            "[Row { total: 33_u32 }, Row { total: a + b + 0 }]",
        ] {
            if pinned(array, "#[test]", ANY)? != Some(false) {
                return Err(format!("expected no pin: {array}"));
            }
        }
        // A negative sibling literal is compared by value: `-33` equal to
        // the expected `-33` refuses, `-33` beside an expected 33 does not.
        let negative = "[Row { total: -33 }, Row { total: a + b + 0 }]";
        let membership = |literal: &str| {
            format!(
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == {literal}));"
            )
        };
        if pinned(negative, "#[test]", &membership("-33"))? != Some(false) {
            return Err("a sibling equal to the negative literal still pins".to_string());
        }
        if pinned(negative, "#[test]", &membership("33"))? != Some(true) {
            return Err("a sibling of the opposite sign refused the pin".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_test_that_does_not_pin_the_element_alone_observes_nothing() -> Result<(), String> {
        let refused = [
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().all(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total != 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total >= 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(!rows.iter().any(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == 33), \"{}\", 1);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r: &Row| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[0].total, 0);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_ne!(rows[1].total, 34);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let expected = 33;\n        assert_eq!(rows[1].total, expected);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let rows = [Row { total: 33 }];\n        assert!(rows.iter().any(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let mut rows = rows(30, 3);\n        rows[1].total = 33;\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        if rows.len() == 2 { return; }\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        fn rows(_: u32, _: u32) -> [Row; 2] { [Row { total: 33 }, Row { total: 33 }] }\n        assert_eq!(rows(30, 3)[1].total, 33);",
            ),
            (
                "#[test]",
                "        let other = other(30, 3);\n        assert_eq!(other[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33, \"sum\");",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        fake!(rows);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        std::process::exit(0);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);\n        settle!(return);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        lenient::assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows({ std::process::exit(0) }, 3);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        assert_eq!(rows(30, { std::process::exit(0) })[1].total, 33);",
            ),
            // The alias must borrow the probed element, immutably, once.
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[0];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &mut rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let mut r = &rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1];\n        let r = Row { total: 33 };\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert_eq!(r.total, 33);\n        let r = 1;",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let rows = &rows[1];\n        assert_eq!(rows.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert_eq!(q.total, 33);",
            ),
            (
                "#[test]",
                "        let other = [Row { total: 33 }, Row { total: 33 }];\n        let r = &other[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1 + 0];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let got = rows(30, 3);\n        let rows = &got[1];\n        assert_eq!(rows.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r: &Row = &rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let ref r = &rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &&rows[1];\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1];\n        settle!();\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let r = &rows[1];\n        let rows = rows(30, 3);\n        assert_eq!(r.total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert!(r.total == 33);",
            ),
            ("#[test]\n    #[should_panic]", ANY),
            ("#[test]\n    #[ignore]", ANY),
        ];
        for (attrs, test) in refused {
            if pinned(ARRAY, attrs, test)? != Some(false) {
                return Err(format!("expected no pin: {attrs} {test}"));
            }
        }
        Ok(())
    }

    /// The lexical fallback and harness-registered tests carry no
    /// attributes, and the shared validator decides for the assertion line.
    #[test]
    fn an_unattributed_test_or_a_refused_assertion_context_observes_nothing() -> Result<(), String>
    {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(ARRAY, "#[test]", ANY),
        );
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        let asked = std::cell::Cell::new(None);
        let admitted = |line: usize| {
            asked.set(Some(line));
            true
        };
        if !observes_with(&probe(), owner, test, &Stub::admitting(&admitted)) {
            return Err("premise: the attributed test pins the element".to_string());
        }
        // `assert!` sits on file line 16, after `let rows = rows(30, 3);`.
        if asked.get() != Some(16) {
            return Err(format!(
                "validator asked for {:?}, not line 16",
                asked.get()
            ));
        }
        if observes_with(&probe(), owner, test, &Stub::admitting(&|_| false)) {
            return Err("a refused assertion context still pins".to_string());
        }
        let mut bare = test.clone();
        bare.attrs.clear();
        if observes_with(&probe(), owner, &bare, &Stub::admitting(&|_| true)) {
            return Err("a test without attributes still pins".to_string());
        }
        Ok(())
    }

    /// A method owner: the test's bare `rows(..)` calls the free function
    /// of the same name, which is not the owner (#8059 review).
    #[test]
    fn a_bare_call_never_reaches_a_method_owner() -> Result<(), String> {
        let source = "pub struct Row {\n    pub total: u32,\n}\npub struct Table;\n\
             impl Table {\n    pub fn rows(a: u32, b: u32) -> [Row; 2] {\n        \
             [Row { total: 0 }, Row { total: a + b + 0 }]\n    }\n}\n\
             pub fn rows(_: u32, _: u32) -> [Row; 2] { [Row { total: 33 }, Row { total: 33 }] }\n\n\
             #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn pins() {\n\
             ANY\n    }\n}\n"
            .replace("ANY", ANY);
        let facts =
            crate::analysis::rust_index::summarize_file(PathBuf::from("src/lib.rs"), source);
        let mut probe = probe();
        probe.location.line = 7;
        let owner = facts
            .functions
            .iter()
            .find(|function| {
                function.name == "rows"
                    && matches!(function.item.container, FunctionContainer::Inherent { .. })
            })
            .ok_or("method owner indexed")?;
        if ReturnedArray::read(&probe, owner).is_none() {
            return Err("premise: the method's array reads".to_string());
        }
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        if observes_with(&probe, owner, test, &Stub::admitting(&|_| true)) {
            return Err("a bare call credited a method owner".to_string());
        }
        Ok(())
    }

    #[test]
    fn an_owner_that_is_not_one_array_of_one_struct_is_not_read() -> Result<(), String> {
        for array in [
            "let rows = [Row { total: 0 }, Row { total: a + b + 0 }]; rows",
            "[Row { total: a + b + 0 }; 2]",
            "[Row { total: 0 }, Other { total: a + b + 0 }]",
            "[Row { total: 0, ..base() }, Row { total: a + b + 0 }]",
            "[Row { ..base() }, Row { total: a + b + 0 }]",
            "[make(), Row { total: a + b + 0 }]",
            "[Row { total: a + b + 0 }, Row { total: a + b + 0 }]",
        ] {
            if pinned(array, "#[test]", ANY)?.is_some() {
                return Err(format!("expected no read: {array}"));
            }
        }
        Ok(())
    }

    /// The owner call, the field's equality and the iteration methods are
    /// the shared authorities' to decide; a refusal from any of them keeps
    /// the gap (#8059 review).
    #[test]
    fn each_shared_authority_can_refuse_the_pin() -> Result<(), String> {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(ARRAY, "#[test]", ANY),
        );
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        let granted = Stub::admitting(&|_| true);
        if !observes_with(&probe(), owner, test, &granted) {
            return Err("premise: the granted membership pins".to_string());
        }
        if *granted.prefixes.borrow() != [None]
            || *granted.fields.borrow() != [("Row".to_string(), "total".to_string())]
        {
            return Err(format!(
                "asked {:?} and {:?}",
                granted.prefixes.borrow(),
                granted.fields.borrow()
            ));
        }
        for (label, stub) in [
            (
                "the owner call",
                Stub {
                    reaches: false,
                    ..Stub::admitting(&|_| true)
                },
            ),
            (
                "the field's equality",
                Stub {
                    integer: false,
                    ..Stub::admitting(&|_| true)
                },
            ),
            (
                "the iteration methods",
                Stub {
                    redefines: true,
                    ..Stub::admitting(&|_| true)
                },
            ),
        ] {
            if observes_with(&probe(), owner, test, &stub) {
                return Err(format!("{label} refused, still pinned"));
            }
        }
        // A redefined `iter` leaves the index form alone: it never iterates.
        let indexed = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(
                ARRAY,
                "#[test]",
                "        assert_eq!(super::rows(30, 3)[1].total, 33);",
            ),
        );
        let owner = indexed
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let test = indexed
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        let redefined = Stub {
            redefines: true,
            ..Stub::admitting(&|_| true)
        };
        if !observes_with(&probe(), owner, test, &redefined) {
            return Err("a redefined iter refused the index form".to_string());
        }
        if *redefined.prefixes.borrow() != [Some("super".to_string())] {
            return Err(format!("asked {:?}", redefined.prefixes.borrow()));
        }
        Ok(())
    }

    /// A multi-byte character before a path root is read whole, not
    /// sliced mid-character (#8059 review).
    #[test]
    fn a_multibyte_attribute_character_is_read_whole() {
        let allowed = |root: &str| root == "std";
        let closed = |source: &str| {
            parse_clean_source_file(source).and_then(|parse| {
                parse
                    .tree()
                    .syntax()
                    .descendants()
                    .find_map(ast::Attr::cast)
                    .map(|attribute| attribute_is_closed(&attribute, &allowed))
            })
        };
        assert_eq!(closed("#[doc = \"→std::io\"]\nfn f() {}\n"), Some(true));
        assert_eq!(closed("#[doc = \"“hack::io\"]\nfn f() {}\n"), Some(false));
    }

    #[test]
    fn an_attributed_element_or_field_is_not_read() {
        for array in [
            "[#[cfg(feature = \"x\")] Row { total: 0 }, Row { total: a + b + 0 }]",
            "[Row { #[cfg(all())] total: 0 }, Row { total: a + b + 0 }]",
        ] {
            let facts = crate::analysis::rust_index::summarize_file(
                PathBuf::from("src/lib.rs"),
                source(array, "#[test]", ANY),
            );
            let read = facts
                .functions
                .iter()
                .find(|function| function.name == "rows")
                .and_then(|owner| ReturnedArray::read(&probe(), owner));
            assert!(read.is_none(), "{array}");
        }
    }

    #[test]
    fn a_literal_out_of_the_field_range_never_counts() {
        assert!(fits(255, "u8") && !fits(289, "u8") && !fits(-1, "u32"));
        assert!(fits(-128, "i8") && !fits(128, "i8") && !fits(1 << 40, "usize"));
        assert!(!fits(1, "f64"));
    }
}
