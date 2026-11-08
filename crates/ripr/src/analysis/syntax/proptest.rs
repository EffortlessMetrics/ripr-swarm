//! Closed syntax of the audited proptest singleton profile (#6643).
//!
//! This is not provider authority. The facts producer may register its result
//! only after a fresh resolved registry and Cargo target receipt. Unmatched token trees
//! remain opaque. No prop_assert macro enters TRUSTED_MACRO_NAMES: its early
//! Err return is safe only in this proved runner callback.

use super::local_test_macros::{inner_elements, is_item_position};
use super::{
    NameScope, NameScopes, UseBinding, name_scope_for_items, name_scopes_for_fn,
    parse_clean_source_file, supported_targeted_item_context,
};
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};
use std::collections::BTreeSet;

const MAX_MACRO_BYTES: usize = 64 * 1024;
const MAX_PROPERTIES: usize = 32;

#[derive(Clone, Debug)]
pub(crate) struct ProptestExpansion {
    pub(crate) macro_line: usize,
    pub(crate) macro_name: String,
    /// A mixed macro keeps its limitation for the refused siblings.
    pub(crate) macro_complete: bool,
    /// The original function keyword line. Local expansion line 2 maps here.
    pub(crate) function_line: usize,
    pub(crate) source: String,
    pub(crate) name_scopes: NameScopes,
    /// The value of the unchanged, typed singleton binding. This is separate
    /// from the generated source: replacing `&String` with `&str` would change
    /// generic calls and type inference, not merely supply a known input.
    pub(crate) singleton_value: Option<(String, String)>,
}

/// Closed file-root singleton profiles. Syntax is only a candidate; provider,
/// target and configuration authority is supplied by the facts producer.
pub(crate) fn proptest_expansions(source: &str) -> Vec<ProptestExpansion> {
    let Some(parsed) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let root = parsed.tree();
    let file_scope = name_scope_for_items(&root.items().collect::<Vec<_>>());
    let Some(prelude) = closed_binding_context(root.syntax(), &file_scope) else {
        return Vec::new();
    };
    let lines = super::ra::LineIndex::new(source);
    let single_root_macro = root
        .items()
        .filter(|item| matches!(item, ast::Item::MacroCall(_)))
        .count()
        == 1;
    let empty_file_bindings_closed =
        single_root_macro && closed_empty_file_macros(root.syntax(), &file_scope, prelude);
    let mut expansions = Vec::new();
    for call in root.syntax().descendants().filter_map(ast::MacroCall::cast) {
        if !is_item_position(call.syntax())
            || call
                .syntax()
                .ancestors()
                .any(|node| ast::Module::can_cast(node.kind()))
            || !supported_targeted_item_context(call.syntax())
            || call.attrs().next().is_some()
        {
            continue;
        }
        let Some(path) = call.path() else { continue };
        let name = compact(&path.syntax().text().to_string());
        if !proptest_macro(&name, prelude) {
            continue;
        }
        let Some(tree) = call.token_tree() else {
            continue;
        };
        if usize::from(tree.syntax().text_range().len()) > MAX_MACRO_BYTES {
            continue;
        }
        let Some(mut properties) = expand_functions(
            tree.syntax(),
            &lines,
            &file_scope,
            prelude,
            empty_file_bindings_closed,
        ) else {
            continue;
        };
        let macro_line = lines.line(call.syntax().text_range().start());
        // The existing limitation DTO names a line, not a byte occurrence.
        // Two same-line invocations cannot borrow each other's admission.
        if root
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
            .filter(|other| {
                is_item_position(other.syntax())
                    && lines.line(other.syntax().text_range().start()) == macro_line
            })
            .count()
            != 1
        {
            continue;
        }
        for property in &mut properties {
            property.macro_line = macro_line;
            property.macro_name = name.clone();
        }
        expansions.extend(properties);
    }
    expansions
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn proptest_macro(name: &str, prelude: bool) -> bool {
    matches!(name, "proptest::proptest" | "::proptest::proptest") || prelude && name == "proptest"
}

fn protected_name(name: &str) -> bool {
    matches!(
        name,
        "proptest" | "test" | "String" | "Just" | "prop_assert" | "prop_assert_eq"
    )
}

fn inert_attribute(text: &str) -> bool {
    let text = compact(text);
    if matches!(text.as_str(), "#[test]" | "#[cfg(test)]") {
        return true;
    }
    let Some(inner) = text.strip_prefix("#[").or_else(|| text.strip_prefix("#![")) else {
        return false;
    };
    inner.starts_with("doc=")
        || ["allow(", "warn(", "deny(", "forbid(", "expect("]
            .iter()
            .any(|prefix| inner.starts_with(prefix))
}

fn closed_binding_context(root: &SyntaxNode, scope: &NameScope) -> Option<bool> {
    let prelude = !scope.globs.is_empty();
    if scope.globs.len() > 1
        || scope.globs.iter().any(|glob| {
            glob.as_deref()
                .is_none_or(|path| path != ["proptest", "prelude"])
        })
        || scope.imports.iter().any(|import| {
            import.bound.starts_with("r#")
                || protected_name(&import.bound)
                || !matches!(&import.binding, UseBinding::Path(path)
                    if path.iter().all(|segment| !segment.starts_with("r#")))
        })
    {
        return None;
    }
    root.descendants()
        .all(|node| {
            // Only file-root explicit imports and the audited prelude glob are
            // read. A local import, alias, macro definition or unknown attribute
            // cannot silently replace the provider or the standard String.
            if let Some(import) = ast::Use::cast(node.clone()) {
                return import
                    .syntax()
                    .parent()
                    .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()));
            }
            if ast::ExternCrate::can_cast(node.kind())
                || ast::MacroRules::can_cast(node.kind())
                || ast::MacroDef::can_cast(node.kind())
            {
                return false;
            }
            if let Some(module) = ast::Module::cast(node.clone())
                && module.item_list().is_none()
            {
                return false;
            }
            // The existing scope producer retains raw spelling, whereas Rust
            // binds `r#name` and `name` identically. Do not lend generated
            // owner-call authority through an unrecognized declaration.
            if let Some(name) = ast::Name::cast(node.clone())
                && (name.text().starts_with("r#") || protected_name(name.text()))
            {
                return false;
            }
            if let Some(attr) = ast::Attr::cast(node.clone()) {
                return inert_attribute(&attr.syntax().text().to_string());
            }
            if let Some(call) = ast::MacroCall::cast(node)
                && is_item_position(call.syntax())
            {
                return call.path().is_some_and(|path| {
                    proptest_macro(&compact(&path.syntax().text().to_string()), prelude)
                });
            }
            true
        })
        .then_some(prelude)
}

/// An unsupported sibling still expands. A macro exported from its callback,
/// strategy expression or another file-local helper can replace a bare
/// prelude assertion at crate root. Only these audited macros, which cannot
/// introduce crate-root bindings, may surround the closed empty property.
fn non_emitting_empty_macro(path: &str, scope: &NameScope, prelude: bool) -> bool {
    let canonical = path.strip_prefix("::").unwrap_or(path);
    if matches!(
        canonical,
        "proptest::prop_assert" | "proptest::prop_assert_eq" | "proptest::prop_oneof"
    ) {
        return true;
    }
    let provider = prelude && matches!(path, "prop_assert" | "prop_assert_eq" | "prop_oneof");
    let standard = matches!(path, "vec" | "assert_eq");
    (provider || standard) && scope.imports_of(path).next().is_none()
}

fn empty_tokens_cannot_emit_bindings(node: &SyntaxNode, scope: &NameScope, prelude: bool) -> bool {
    let tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect::<Vec<_>>();
    tokens.iter().enumerate().all(|(at, token)| {
        // Opaque attributes can invoke a procedural expander. Local namespace
        // declarations can redirect an allowed macro path, and a nested macro
        // definition can export its binding into the shared crate root.
        if matches!(
            token.kind(),
            SyntaxKind::POUND | SyntaxKind::USE_KW | SyntaxKind::MOD_KW | SyntaxKind::EXTERN_KW
        ) || matches!(
            token.text().trim_start_matches("r#"),
            "macro" | "macro_rules" | "macro_export" | "macro_use"
        ) {
            return false;
        }
        if token.kind() != SyntaxKind::BANG
            || at == 0
            || tokens[at - 1].kind() != SyntaxKind::IDENT
            || !tokens.get(at + 1).is_some_and(|next| {
                matches!(
                    next.kind(),
                    SyntaxKind::L_PAREN | SyntaxKind::L_BRACK | SyntaxKind::L_CURLY
                )
            })
        {
            return true;
        }
        // Token trees retain `::` as either COLON2 or two COLON tokens.
        // Keep the entire path: a foreign `other::prop_assert_eq!` is not
        // the provider merely because its final token has the same name.
        let mut start = at - 1;
        while start > 0
            && matches!(
                tokens[start - 1].kind(),
                SyntaxKind::IDENT | SyntaxKind::COLON | SyntaxKind::COLON2
            )
        {
            start -= 1;
        }
        let path = tokens[start..at]
            .iter()
            .map(|token| token.text())
            .collect::<String>();
        non_emitting_empty_macro(&path, scope, prelude)
    })
}

fn closed_empty_file_macros(root: &SyntaxNode, scope: &NameScope, prelude: bool) -> bool {
    // Audited proptest 1.11.0 still consults these call-site macros: the
    // prop_oneof catch-all uses vec!, formatted prop_assert_eq uses concat!,
    // and the proptest_helper @_BODY runner uses file!. Their bindings must
    // be closed even when the original source never spells those calls.
    if ["vec", "concat", "file"]
        .iter()
        .any(|name| scope.imports_of(name).next().is_some())
    {
        return false;
    }
    root.descendants()
        .filter_map(ast::MacroCall::cast)
        .all(|call| {
            let Some(path) = call.path() else {
                return false;
            };
            let path = compact(&path.syntax().text().to_string());
            // The outer property token tree has its own bounded row/attribute
            // parser; its strategies and bodies are checked separately below.
            if proptest_macro(&path, prelude)
                && call
                    .syntax()
                    .parent()
                    .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()))
            {
                return true;
            }
            non_emitting_empty_macro(&path, scope, prelude)
                && call.token_tree().is_some_and(|tree| {
                    empty_tokens_cannot_emit_bindings(tree.syntax(), scope, prelude)
                })
        })
}

fn expand_functions(
    tree: &SyntaxNode,
    lines: &super::ra::LineIndex,
    file_scope: &NameScope,
    prelude: bool,
    mut empty_bindings_closed: bool,
) -> Option<Vec<ProptestExpansion>> {
    let elements = inner_elements(tree);
    if elements.is_empty() {
        return None;
    }
    // Bound every sibling before admitting any one of them. Unsupported
    // strategy/body syntax stays opaque, but its generated function name is
    // still a binding in the shared file root.
    let mut rows = Vec::new();
    let mut at = 0;
    let mut names = BTreeSet::new();
    while at < elements.len() {
        let mut has_test = false;
        let mut attributes_supported = true;
        while elements.get(at)?.kind() == SyntaxKind::POUND {
            let attr = elements.get(at + 1)?;
            let text = compact(&attr.to_string());
            if text == "[test]" {
                attributes_supported &= !has_test;
                has_test = true;
            } else {
                let inert = inert_attribute(&format!("#{text}"));
                // An opaque attribute on a sibling can replace its function
                // with new root bindings. Only built-in non-expanding gates
                // may remain as an unsupported sibling beside an admitted one.
                if !inert
                    && !matches!(text.as_str(), "[ignore]" | "[should_panic]")
                    && !text.starts_with("[cfg(")
                    && !text.starts_with("[ignore=")
                    && !text.starts_with("[should_panic(")
                {
                    return None;
                }
                attributes_supported &= inert && text != "[cfg(test)]";
            }
            at += 2;
        }
        let row = elements.get(at..at + 4)?;
        if row[0].kind() != SyntaxKind::FN_KW || row[1].kind() != SyntaxKind::IDENT {
            return None;
        }
        let name = row[1].to_string();
        if name.starts_with("r#") || protected_name(&name) || !names.insert(name) {
            return None;
        }
        let parameters = row[2].as_node()?;
        let body = row[3].as_node()?;
        if parameters.first_token()?.kind() != SyntaxKind::L_PAREN
            || body.first_token()?.kind() != SyntaxKind::L_CURLY
        {
            return None;
        }
        empty_bindings_closed &= empty_tokens_cannot_emit_bindings(parameters, file_scope, prelude)
            && empty_tokens_cannot_emit_bindings(body, file_scope, prelude);
        rows.push((row, has_test && attributes_supported));
        if rows.len() > MAX_PROPERTIES {
            return None;
        }
        at += 4;
    }
    let mut result = Vec::new();
    let count = rows.len();
    let mut generated_scope = file_scope.clone();
    generated_scope.defines.extend(names);
    for (row, attributes_supported) in rows {
        let name = row[1].to_string();
        let parameters = row[2].as_node()?;
        let body = row[3].as_node()?;
        let function_line = lines.line(row[0].text_range().start());
        if !attributes_supported
            || parameters.text().to_string().contains('\n')
            || lines.line(parameters.text_range().start()) != function_line
            || lines.line(body.text_range().start()) != function_line
        {
            continue;
        }
        let Some((parameter, value)) = singleton_parameter(parameters, prelude) else {
            continue;
        };
        let (body, singleton_value) = match value {
            Singleton::Primitive(literal) if !prelude && file_scope.imports.is_empty() => {
                let Some(body) = lower_body(&body.text().to_string(), &parameter, &literal) else {
                    continue;
                };
                (body, None)
            }
            Singleton::EmptyString if empty_bindings_closed => {
                let Some(body) = lower_empty_string_body(
                    &body.text().to_string(),
                    &parameter,
                    &generated_scope,
                    prelude,
                ) else {
                    continue;
                };
                (body, Some((parameter, "\"\"".to_string())))
            }
            Singleton::Primitive(_) | Singleton::EmptyString => continue,
        };
        let source = format!("#[test]\nfn {name}() {body}");
        let mut name_scopes = name_scopes_for_fn(&source, 2, &name)?;
        let [body_scope, root_scope] = name_scopes.scopes.as_mut_slice() else {
            return None;
        };
        if !body_scope.fn_body
            || root_scope.fn_body
            || !body_scope.modules.is_empty()
            || !root_scope.modules.is_empty()
        {
            return None;
        }
        // Only the callback body is synthesized. Its namespace still comes
        // from the original file root, including module/type/const shadows
        // of an owner-looking crate path. A short synthetic file alone would
        // erase those bindings and could lend inputs to the wrong owner.
        *root_scope = generated_scope.clone();
        // Only the audited prelude is present, and the empty-string profile
        // requires explicit, unrenamed owner imports. Those imports take
        // precedence over the glob; this receipt-local scope is not a global
        // foreign-glob exemption.
        root_scope.globs.clear();
        result.push(ProptestExpansion {
            macro_line: 0,
            macro_name: String::new(),
            macro_complete: false,
            function_line,
            source,
            name_scopes,
            singleton_value,
        });
    }
    let complete = result.len() == count;
    for property in &mut result {
        property.macro_complete = complete;
    }
    Some(result)
}

fn expression(text: &str) -> Option<ast::Expr> {
    let parsed = parse_clean_source_file(&format!(
        "fn __property_operand() {{ let __operand = {text}; }}"
    ))?;
    parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()
}

fn primitive_literal(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::Literal(literal) => literal.syntax().first_token().is_some_and(|token| {
            matches!(token.kind(), SyntaxKind::INT_NUMBER | SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW)
        }),
        ast::Expr::PrefixExpr(prefix) => prefix.syntax().first_token().is_some_and(|token| token.text() == "-")
            && prefix.expr().is_some_and(|inner| {
                matches!(inner, ast::Expr::Literal(ref literal)
                    if literal.syntax().first_token().is_some_and(|token| token.kind() == SyntaxKind::INT_NUMBER))
            }),
        _ => false,
    }
}

fn typed_singleton_literal(expression: &ast::Expr) -> bool {
    if matches!(expression, ast::Expr::Literal(literal)
    if literal.syntax().first_token().is_some_and(|token| {
        matches!(token.kind(), SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW)
    })) {
        return true;
    }
    if !primitive_literal(expression) {
        return false;
    }
    // One original parameter has one inferred type. Repeating an unsuffixed
    // token in separate call/comparison contexts could infer different types.
    let suffixes: &[&str] = if matches!(expression, ast::Expr::PrefixExpr(_)) {
        &["i8", "i16", "i32", "i64", "i128", "isize"]
    } else {
        &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        ]
    };
    let text = expression.syntax().text().to_string();
    suffixes.iter().any(|suffix| text.ends_with(*suffix))
}

enum Singleton {
    Primitive(String),
    EmptyString,
}

fn singleton_parameter(parameters: &SyntaxNode, prelude: bool) -> Option<(String, Singleton)> {
    let mut elements = inner_elements(parameters);
    if elements
        .last()
        .is_some_and(|element| element.kind() == SyntaxKind::COMMA)
    {
        elements.pop();
    }
    if elements.first()?.kind() != SyntaxKind::IDENT || elements.get(1)?.kind() != SyntaxKind::IN_KW
    {
        return None;
    }
    let parameter = elements[0].to_string();
    if parameter.starts_with("r#") || protected_name(&parameter) {
        return None;
    }
    let strategy = elements
        .iter()
        .skip(2)
        .map(ToString::to_string)
        .collect::<String>();
    let ast::Expr::CallExpr(call) = expression(&strategy)? else {
        return None;
    };
    let strategy = compact(&call.expr()?.syntax().text().to_string());
    if !(matches!(
        strategy.as_str(),
        "proptest::strategy::Just" | "::proptest::strategy::Just"
    ) || prelude && strategy == "Just")
    {
        return None;
    }
    let arguments = call.arg_list()?.args().collect::<Vec<_>>();
    let [literal] = arguments.as_slice() else {
        return None;
    };
    if typed_singleton_literal(literal) {
        Some((
            parameter,
            Singleton::Primitive(literal.syntax().text().to_string()),
        ))
    } else if compact(&literal.syntax().text().to_string()) == "String::new()" {
        Some((parameter, Singleton::EmptyString))
    } else {
        None
    }
}

fn plain_binding(pattern: ast::Pat) -> Option<String> {
    let ast::Pat::IdentPat(pattern) = pattern else {
        return None;
    };
    if pattern.ref_token().is_some() || pattern.mut_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    (!name.starts_with("r#") && !protected_name(&name)).then_some(name)
}

fn property_equality(
    statement: &ast::Stmt,
    prelude: bool,
) -> Option<(ast::MacroCall, [ast::Expr; 2])> {
    let call = statement
        .syntax()
        .descendants()
        .find_map(ast::MacroCall::cast)?;
    let text = call.syntax().text().to_string();
    if call.attrs().next().is_some()
        || text.contains('\n')
        || statement
            .syntax()
            .text()
            .to_string()
            .trim()
            .trim_end_matches(';')
            .trim()
            != text.trim()
    {
        return None;
    }
    let path = compact(&call.path()?.syntax().text().to_string());
    if !(matches!(
        path.as_str(),
        "proptest::prop_assert_eq" | "::proptest::prop_assert_eq"
    ) || prelude && path == "prop_assert_eq")
    {
        return None;
    }
    let tree = call.token_tree()?.syntax().text().to_string();
    let inside = tree.get(1..tree.len().checked_sub(1)?)?;
    let ast::Expr::CallExpr(arguments) = expression(&format!("__arguments({inside})"))? else {
        return None;
    };
    let arguments = arguments.arg_list()?.args().collect::<Vec<_>>();
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    Some((call, [left.clone(), right.clone()]))
}

/// The unchanged real empty-string property: two straight-line owner calls,
/// an exact tuple reassembly and an exact scalar result assertion. Preserve
/// the String, borrow, tuple and result bindings; only the audited assertion
/// spelling changes. No inlining or assumed tuple-to-literal substitution.
fn lower_empty_string_body(
    body: &str,
    parameter: &str,
    scope: &NameScope,
    prelude: bool,
) -> Option<String> {
    let prefix = "fn __property() ";
    let parsed = parse_clean_source_file(&format!("{prefix}{body}"))?;
    let function = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?;
    if block.tail_expr().is_some() {
        return None;
    }
    let statements = block.statements().collect::<Vec<_>>();
    let [
        ast::Stmt::LetStmt(tuple_let),
        tuple_assert,
        ast::Stmt::LetStmt(result_let),
        result_assert,
    ] = statements.as_slice()
    else {
        return None;
    };
    if tuple_let.ty().is_some()
        || tuple_let.let_else().is_some()
        || result_let.ty().is_some()
        || result_let.let_else().is_some()
        || tuple_let.attrs().next().is_some()
        || result_let.attrs().next().is_some()
    {
        return None;
    }
    let ast::Pat::TuplePat(tuple) = tuple_let.pat()? else {
        return None;
    };
    let names = tuple
        .fields()
        .map(plain_binding)
        .collect::<Option<Vec<_>>>()?;
    let [first, second] = names.as_slice() else {
        return None;
    };
    let result = plain_binding(result_let.pat()?)?;
    let bindings = [parameter, first.as_str(), second.as_str(), result.as_str()];
    if bindings.iter().copied().collect::<BTreeSet<_>>().len() != bindings.len() {
        return None;
    }
    let calls = [tuple_let.initializer()?, result_let.initializer()?];
    for (position, value) in calls.iter().enumerate() {
        let ast::Expr::CallExpr(call) = value else {
            return None;
        };
        let ast::Expr::PathExpr(callee) = call.expr()? else {
            return None;
        };
        let name = callee.syntax().text().to_string();
        // The original uses explicit imports. Keeping this bound to those
        // leaves lets ordinary call identity settle the owner without treating
        // every name exported by a foreign prelude as absent.
        if name.contains("::")
            || bindings.contains(&name.as_str())
            || scope.defines.contains(&name)
            || scope.child_modules.contains(&name)
            || scope.imports_of(&name).count() != 1
            || !scope
                .imports_of(&name)
                .all(|binding| matches!(binding, UseBinding::Path(_)))
        {
            return None;
        }
        let arguments = call.arg_list()?.args().collect::<Vec<_>>();
        let expected = if position == 0 {
            vec![format!("&{parameter}"), "0".to_string()]
        } else {
            vec![format!("&{parameter}"), first.clone(), second.clone()]
        };
        if arguments
            .iter()
            .map(|arg| compact(&arg.syntax().text().to_string()))
            .collect::<Vec<_>>()
            != expected
        {
            return None;
        }
    }
    let (tuple_call, [actual_tuple, expected_tuple]) = property_equality(tuple_assert, prelude)?;
    let ast::Expr::TupleExpr(actual_tuple) = actual_tuple else {
        return None;
    };
    let actual_names = actual_tuple
        .fields()
        .map(|field| compact(&field.syntax().text().to_string()))
        .collect::<Vec<_>>();
    if actual_names != names || compact(&expected_tuple.syntax().text().to_string()) != "(0,0)" {
        return None;
    }
    let (result_call, [actual_result, expected_result]) =
        property_equality(result_assert, prelude)?;
    if compact(&actual_result.syntax().text().to_string()) != result
        || compact(&expected_result.syntax().text().to_string()) != "0"
    {
        return None;
    }
    let mut lowered = body.to_string();
    for call in [result_call, tuple_call] {
        let range = call.path()?.syntax().text_range();
        lowered.replace_range(
            usize::from(range.start()).checked_sub(prefix.len())?
                ..usize::from(range.end()).checked_sub(prefix.len())?,
            "assert_eq",
        );
    }
    lowered.insert_str(1, &format!(" let {parameter} = String::new();"));
    Some(lowered)
}

fn lower_body(body: &str, parameter: &str, literal: &str) -> Option<String> {
    let prefix = "fn __property() ";
    let parsed = parse_clean_source_file(&format!("{prefix}{body}"))?;
    let function = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?;
    if block.tail_expr().is_some() {
        return None;
    }
    let statements = block.statements().collect::<Vec<_>>();
    if statements.is_empty() || statements.len() > 16 {
        return None;
    }
    let mut rewrites = Vec::new();
    for statement in statements {
        let call = statement
            .syntax()
            .descendants()
            .find_map(ast::MacroCall::cast)?;
        let text = call.syntax().text().to_string();
        if text.contains('\n') || call.attrs().next().is_some() {
            return None;
        }
        let statement_text = statement.syntax().text().to_string();
        if statement_text.trim().trim_end_matches(';').trim() != text.trim() {
            return None;
        }
        let lowered = lower_assertion(&call, parameter, literal)?;
        let range = call.syntax().text_range();
        rewrites.push((
            usize::from(range.start()).checked_sub(prefix.len())?,
            usize::from(range.end()).checked_sub(prefix.len())?,
            lowered,
        ));
    }
    let mut result = body.to_string();
    for (start, end, lowered) in rewrites.into_iter().rev() {
        result.replace_range(start..end, &lowered);
    }
    Some(result)
}

fn literal_operand(value: &ast::Expr, parameter: &str, literal: &str) -> Option<String> {
    if primitive_literal(value) {
        Some(value.syntax().text().to_string())
    } else if matches!(value, ast::Expr::PathExpr(_)) && value.syntax().text() == parameter {
        Some(literal.to_string())
    } else {
        None
    }
}

fn owner_call(value: &ast::Expr, parameter: &str, literal: &str) -> Option<String> {
    let ast::Expr::CallExpr(call) = value else {
        return None;
    };
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    let path = path.syntax().text().to_string();
    if path.contains(['<', '>']) {
        return None;
    }
    if compact(&path)
        .trim_start_matches("::")
        .split("::")
        .next()
        .is_some_and(|name| name.trim_start_matches("r#") == parameter.trim_start_matches("r#"))
    {
        // The generated callback parameter binds a value, including the
        // callee spelling. Removing the parameter must not erase that shadow.
        return None;
    }
    let arguments = call
        .arg_list()?
        .args()
        .map(|arg| literal_operand(&arg, parameter, literal))
        .collect::<Option<Vec<_>>>()?;
    Some(format!("{path}({})", arguments.join(", ")))
}

fn lower_assertion(call: &ast::MacroCall, parameter: &str, literal: &str) -> Option<String> {
    let path = compact(&call.path()?.syntax().text().to_string());
    let leaf = path
        .strip_prefix("::")
        .unwrap_or(&path)
        .strip_prefix("proptest::")?;
    let ordinary = match leaf {
        "prop_assert" => "assert",
        "prop_assert_eq" => "assert_eq",
        // proptest tests `left != right`, while std assert_ne negates `eq`.
        // PartialEq::ne can disagree with eq, so no normalization is proved.
        _ => return None,
    };
    let tree = call.token_tree()?.syntax().text().to_string();
    let inside = tree.get(1..tree.len().checked_sub(1)?)?;
    let ast::Expr::CallExpr(arguments) = expression(&format!("__arguments({inside})"))? else {
        return None;
    };
    let arguments = arguments.arg_list()?.args().collect::<Vec<_>>();
    // Diagnostics are executable on failure and could return Ok early. They
    // cannot be silently discarded by an ordinary-assertion normalization.
    if ordinary == "assert" {
        let [condition] = arguments.as_slice() else {
            return None;
        };
        if let Some(call) = owner_call(condition, parameter, literal) {
            return Some(format!("assert!({call})"));
        }
        if let ast::Expr::PrefixExpr(prefix) = condition
            && prefix.syntax().first_token()?.text() == "!"
        {
            return Some(format!(
                "assert!(!{})",
                owner_call(&prefix.expr()?, parameter, literal)?
            ));
        }
        return None;
    }
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    let operands = match (
        owner_call(left, parameter, literal),
        owner_call(right, parameter, literal),
    ) {
        (Some(owner), None) => (owner, literal_operand(right, parameter, literal)?),
        (None, Some(owner)) => (literal_operand(left, parameter, literal)?, owner),
        _ => return None,
    };
    Some(format!("{ordinary}!({}, {})", operands.0, operands.1))
}

#[cfg(test)]
mod tests {
    use super::proptest_expansions;

    const SOURCE: &str = "proptest::proptest! {\n #[test]\n fn boundary(n in proptest::strategy::Just(100i32)) {\n  proptest::prop_assert_eq!(owner(n, 100), 90);\n }\n}\n";
    const ORIGINAL_EMPTY: &str = include_str!(
        "../../../tests/fixtures/proptest_empty_string/input/tests/prop_utf16_roundtrip.rs"
    );

    #[test]
    fn original_empty_string_property_keeps_typed_borrow_and_result_flow() -> Result<(), String> {
        let expanded = proptest_expansions(ORIGINAL_EMPTY);
        let [empty] = expanded.as_slice() else {
            return Err(format!(
                "only the original empty-string sibling is admitted: {expanded:?}"
            ));
        };
        assert_eq!(empty.macro_name, "proptest");
        assert!(!empty.macro_complete);
        assert!(
            empty
                .source
                .contains("fn empty_string() { let s = String::new();")
        );
        assert!(
            empty
                .source
                .contains("let (line, col) = offset_to_utf16_line_col(&s, 0);")
        );
        assert!(empty.source.contains("assert_eq!((line, col), (0, 0));"));
        assert!(
            empty
                .source
                .contains("let back = utf16_line_col_to_offset(&s, line, col);")
        );
        assert!(empty.source.contains("assert_eq!(back, 0);"));
        assert_eq!(empty.singleton_value, Some(("s".into(), "\"\"".into())));
        for (offset, line) in empty.source.lines().enumerate() {
            if line.contains("assert_eq!") {
                let original = ORIGINAL_EMPTY.lines().nth(empty.function_line + offset - 2);
                assert_eq!(
                    original.map(|text| text.replace("prop_assert_eq!", "assert_eq!")),
                    Some(line.to_string())
                );
            }
        }
        Ok(())
    }

    #[test]
    fn original_empty_string_profile_refuses_changed_bindings_and_flow() {
        for (from, to) in [
            ("use proptest::prelude::*;", "use other::prelude::*;"),
            (
                "use proptest::prelude::*;",
                "use proptest::prelude::*; use other::*;",
            ),
            (
                "use proptest::prelude::*;",
                "use proptest::prelude::*; use other::String;",
            ),
            (
                "use proptest::prelude::*;",
                "use proptest::prelude::*; use other::Just;",
            ),
            (
                "use proptest::prelude::*;",
                "use proptest::prelude::*; use other::prop_assert_eq;",
            ),
            (
                "use proptest::prelude::*;",
                "use proptest::prelude::*; struct String;",
            ),
            ("Just(String::new())", "Just(String::from(\"\"))"),
            (
                "let (line, col) = offset_to_utf16_line_col(&s, 0);",
                "let (line, col) = (0, 0);",
            ),
            (
                "prop_assert_eq!((line, col), (0, 0));",
                "prop_assert_eq!((col, line), (0, 0));",
            ),
            (
                "prop_assert_eq!((line, col), (0, 0));",
                "prop_assert_eq!((line, col), (0, 0), return Ok(()));",
            ),
            (
                "let back = utf16_line_col_to_offset(&s, line, col);",
                "let back = 0;",
            ),
            ("prop_assert_eq!(back, 0);", "prop_assert_eq!(0, 0);"),
            (
                "prop_assert_eq!(back, 0);",
                "let back = 0; prop_assert_eq!(back, 0);",
            ),
            ("fn empty_string(s in", "fn empty_string(mut s in"),
            (
                "fn empty_string(s in",
                "#[ignore]\n    fn empty_string(s in",
            ),
            (
                "fn roundtrip_offset_to_utf16_and_back(",
                "#[unknown_attribute]\n    fn roundtrip_offset_to_utf16_and_back(",
            ),
            (
                "fn roundtrip_offset_to_utf16_and_back(",
                "fn utf16_line_col_to_offset(",
            ),
        ] {
            assert!(
                proptest_expansions(&ORIGINAL_EMPTY.replace(from, to)).is_empty(),
                "accepted {from} -> {to}"
            );
        }
    }

    #[test]
    fn opaque_property_tokens_cannot_change_the_admitted_siblings_macro_bindings() {
        assert_eq!(proptest_expansions(ORIGINAL_EMPTY).len(), 1);
        let sibling =
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) {";
        for replacement in [
            format!("{sibling} #[macro_export] macro_rules! prop_assert_eq {{ ($($tokens:tt)*) => {{ () }}; }}"),
            "fn char_boundaries_roundtrip_with_separator_normalization(s in { #[macro_export] macro_rules! prop_assert_eq { ($($tokens:tt)*) => { () }; } source_text() }) {".into(),
            format!("{sibling} unknown::emit!();"),
            format!("{sibling} other::prop_assert_eq!(1, 1);"),
            format!("{sibling} use foreign::emit as prop_assert_eq; prop_assert_eq!();"),
            format!("{sibling} use foreign as proptest; proptest::prop_assert_eq!();"),
            format!("{sibling} #[unknown] fn emit() {{}}"),
        ] {
            let changed = ORIGINAL_EMPTY.replace(sibling, &replacement);
            assert_ne!(changed, ORIGINAL_EMPTY);
            assert!(proptest_expansions(&changed).is_empty(), "{replacement}");
        }
        for inserted in [
            "let _ = vec![unknown::emit!()];",
            "unknown::emit!();",
            "proptest::proptest! { #[test] fn emit(n in proptest::strategy::Just(0u8)) { #[macro_export] macro_rules! prop_assert_eq { ($($t:tt)*) => { () }; } } }",
            "mod hidden { proptest::proptest! { #[test] fn emit(n in proptest::strategy::Just(0u8)) { #[macro_export] macro_rules! prop_assert_eq { ($($t:tt)*) => { () }; } } } }",
        ] {
            let changed = ORIGINAL_EMPTY.replace(
                "fn source_text() -> impl Strategy<Value = String> {",
                &format!("fn source_text() -> impl Strategy<Value = String> {{ {inserted}"),
            );
            assert!(proptest_expansions(&changed).is_empty(), "{inserted}");
        }
        for import in [
            "use other::vec;",
            "use other::concat;",
            "use other::file;",
            "use other::prop_oneof;",
            "use other::r#String;",
            "use other::r#concat;",
        ] {
            assert!(proptest_expansions(&format!("{import}\n{ORIGINAL_EMPTY}")).is_empty());
        }
    }

    #[test]
    fn transitive_macro_dependencies_need_no_literal_call_to_be_shadowed() {
        let property = ORIGINAL_EMPTY
            .lines()
            .skip(152)
            .take(7)
            .collect::<Vec<_>>()
            .join("\n");
        let choices = std::iter::repeat_n("1 => Just(0u8)", 11)
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "use perl_position_tracking::{{offset_to_utf16_line_col, utf16_line_col_to_offset}};\nuse proptest::prelude::*;\nfn choices() -> impl Strategy<Value = u8> {{ proptest::prop_oneof![{choices}] }}\nproptest! {{\n{property}\n}}"
        );
        assert_eq!(proptest_expansions(&source).len(), 1);
        for name in ["vec", "concat", "file"] {
            assert!(!source.contains(&format!("{name}!")));
            let shadowed = format!("use other::{name};\n{source}");
            assert!(proptest_expansions(&shadowed).is_empty(), "{name}");
        }
    }

    #[test]
    fn singleton_substitution_preserves_the_assertions_original_line() {
        let expanded = proptest_expansions(SOURCE);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].function_line, 3);
        assert_eq!(expanded[0].macro_line, 1);
        assert!(
            expanded[0]
                .source
                .contains("assert_eq!(owner(100i32, 100), 90)")
        );
        assert_eq!(
            expanded[0]
                .source
                .lines()
                .position(|line| line.contains("assert_eq!")),
            Some(2)
        );
    }

    #[test]
    fn generated_body_retains_original_file_root_bindings() {
        let source =
            format!("mod local_library {{}}\nstruct LibraryType;\nconst OWNER: u8 = 1;\n{SOURCE}");
        let expanded = proptest_expansions(&source);
        assert_eq!(expanded.len(), 1);
        let scopes = &expanded[0].name_scopes.scopes;
        assert_eq!(scopes.len(), 2);
        assert!(scopes[0].fn_body);
        assert!(scopes[1].child_modules.contains("local_library"));
        assert!(scopes[1].defines.contains("LibraryType"));
        assert!(scopes[1].defines.contains("OWNER"));
        assert!(scopes[1].defines.contains("boundary"));
    }

    #[test]
    fn raw_spelled_declarations_and_bindings_receive_no_expansion() {
        for declaration in [
            "mod r#local_library {}",
            "struct r#LibraryType;",
            "const r#OWNER: u8 = 1;",
        ] {
            let source = format!("{declaration}\n{SOURCE}");
            assert!(proptest_expansions(&source).is_empty(), "{declaration}");
        }
        let function = SOURCE.replace("fn boundary(", "fn r#boundary(");
        assert!(proptest_expansions(&function).is_empty());
        let parameter = SOURCE
            .replace("n in", "r#n in")
            .replace("owner(n,", "owner(r#n,");
        assert!(proptest_expansions(&parameter).is_empty());
    }

    #[test]
    fn a_refused_sibling_keeps_its_limitation_without_erasing_the_supported_property() {
        let changed = SOURCE.replace(
            "\n}",
            "\n #[test]\n fn unsupported(n in 0..2) { proptest::prop_assert!(owner(n)); }\n}",
        );
        let expanded = proptest_expansions(&changed);
        assert_eq!(expanded.len(), 1);
        assert!(!expanded[0].macro_complete);
        assert!(
            expanded[0].name_scopes.scopes[1]
                .defines
                .contains("unsupported")
        );
        for prefix in [
            "use proptest::prelude::*;\n",
            "mod proptest {}\n",
            "macro_rules! test { () => {} }\n",
        ] {
            assert!(proptest_expansions(&format!("{prefix}{SOURCE}")).is_empty());
        }
    }

    #[test]
    fn diagnostic_returns_and_shadowed_callees_cannot_be_erased() {
        let diagnostics =
            SOURCE.replace(", 90);", ", 90, \"{}\", { return Ok(()); \"ignored\" });");
        assert!(proptest_expansions(&diagnostics).is_empty());
        let shadow = SOURCE.replace("n in", "owner in");
        assert!(proptest_expansions(&shadow).is_empty());
        let inequality = SOURCE.replace("prop_assert_eq", "prop_assert_ne");
        assert!(proptest_expansions(&inequality).is_empty());
    }

    #[test]
    fn inline_modules_cannot_borrow_file_root_binding_authority() {
        for source in [
            format!("mod nested {{\n{SOURCE}\n}}"),
            "pub fn owner(n: u8) -> u8 { n + 1 }\n  mod inner { const owner: fn(u8) -> u8 = |_| 8; proptest::proptest! { #[test] fn p(n in proptest::strategy::Just(7u8)) { proptest::prop_assert_eq!(owner(n), 8u8); } } }".into(),
        ] {
            assert!(proptest_expansions(&source).is_empty());
        }
    }

    #[test]
    fn custom_inequality_is_not_normalized_to_negated_equality() {
        let source = "struct Contrary;\nimpl std::fmt::Debug for Contrary { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(\"Contrary\") } }\nimpl PartialEq<u8> for Contrary { fn eq(&self, _: &u8) -> bool { true } fn ne(&self, _: &u8) -> bool { true } }\nfn owner(_: u8) -> Contrary { Contrary }\nproptest::proptest! {\n #[test]\n fn p(n in proptest::strategy::Just(7u8)) {\n proptest::prop_assert_ne!(owner(n), 8u8);\n }\n}";
        assert!(proptest_expansions(source).is_empty());
    }

    #[test]
    fn singleton_substitution_preserves_one_explicit_primitive_type() {
        let source = r#"struct Width;
impl std::fmt::Debug for Width {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Width") }
}
impl PartialEq<i32> for Width { fn eq(&self, _: &i32) -> bool { false } }
impl PartialEq<i64> for Width { fn eq(&self, _: &i64) -> bool { true } }
fn owner(_: i64) -> Width { Width }
proptest::proptest! {
 #[test]
 fn p(n in proptest::strategy::Just(7)) {
  proptest::prop_assert_eq!(owner(n), n);
 }
}"#;
        assert!(proptest_expansions(source).is_empty());
        for literal in ["7i64", "-7i64"] {
            let typed = source.replace("Just(7)", &format!("Just({literal})"));
            let expanded = proptest_expansions(&typed);
            assert_eq!(expanded.len(), 1);
            assert!(
                expanded[0]
                    .source
                    .contains(&format!("assert_eq!(owner({literal}), {literal})"))
            );
        }
        let boolean = "proptest::proptest! {\n #[test]\n fn p(n in proptest::strategy::Just(true)) {\n proptest::prop_assert!(owner(n));\n }\n}";
        let expanded = proptest_expansions(boolean);
        assert_eq!(expanded.len(), 1);
        assert!(expanded[0].source.contains("assert!(owner(true))"));
    }
}
