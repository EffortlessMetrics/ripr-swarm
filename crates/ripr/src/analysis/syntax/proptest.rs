//! Closed syntax of the audited proptest singleton profile (#6643).
//!
//! This is not provider authority. The facts producer may register its result
//! only after a fresh resolved registry and Cargo target receipt. Unmatched token trees
//! remain opaque. No prop_assert macro enters TRUSTED_MACRO_NAMES: its early
//! Err return is safe only in this proved runner callback.

use super::local_test_macros::{inner_elements, is_item_position};
use super::{
    NameScope, NameScopes, name_scope_for_items, name_scopes_for_fn, parse_clean_source_file,
    supported_targeted_item_context,
};
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasModuleItem};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};

const MAX_MACRO_BYTES: usize = 64 * 1024;
const MAX_PROPERTIES: usize = 32;

#[derive(Clone, Debug)]
pub(crate) struct ProptestExpansion {
    pub(crate) macro_line: usize,
    pub(crate) macro_name: String,
    /// The original function keyword line. Local expansion line 2 maps here.
    pub(crate) function_line: usize,
    pub(crate) source: String,
    pub(crate) name_scopes: NameScopes,
}

/// File-root canonical paths, one typed-integer/bool Just domain, exact #[test],
/// and only direct failure-capable assertions. No config, imports or models.
pub(crate) fn proptest_expansions(source: &str) -> Vec<ProptestExpansion> {
    let Some(parsed) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let root = parsed.tree();
    if !closed_binding_context(root.syntax()) {
        return Vec::new();
    }
    let file_scope = name_scope_for_items(&root.items().collect::<Vec<_>>());
    let lines = super::ra::LineIndex::new(source);
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
        if !matches!(name.as_str(), "proptest::proptest" | "::proptest::proptest") {
            continue;
        }
        let Some(tree) = call.token_tree() else {
            continue;
        };
        if usize::from(tree.syntax().text_range().len()) > MAX_MACRO_BYTES {
            continue;
        }
        let Some(mut properties) = expand_functions(tree.syntax(), &lines, &file_scope) else {
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

fn closed_binding_context(root: &SyntaxNode) -> bool {
    root.descendants().all(|node| {
        // Canonical paths avoid import inference. These exclusions also stop
        // a renamed #[test] attribute, injected crate root or opaque item.
        if ast::Use::can_cast(node.kind())
            || ast::ExternCrate::can_cast(node.kind())
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
            && (name.text().starts_with("r#") || matches!(name.text(), "proptest" | "test"))
        {
            return false;
        }
        if let Some(attr) = ast::Attr::cast(node.clone()) {
            return matches!(
                compact(&attr.syntax().text().to_string()).as_str(),
                "#[test]" | "#[cfg(test)]"
            );
        }
        if let Some(call) = ast::MacroCall::cast(node)
            && is_item_position(call.syntax())
        {
            return call.path().is_some_and(|path| {
                matches!(
                    compact(&path.syntax().text().to_string()).as_str(),
                    "proptest::proptest" | "::proptest::proptest"
                )
            });
        }
        true
    })
}

fn expand_functions(
    tree: &SyntaxNode,
    lines: &super::ra::LineIndex,
    file_scope: &NameScope,
) -> Option<Vec<ProptestExpansion>> {
    let elements = inner_elements(tree);
    if elements.is_empty()
        || !elements.len().is_multiple_of(6)
        || elements.len() / 6 > MAX_PROPERTIES
    {
        return None;
    }
    let mut result = Vec::new();
    for row in elements.chunks_exact(6) {
        if row[0].kind() != SyntaxKind::POUND
            || compact(&row[1].to_string()) != "[test]"
            || row[2].kind() != SyntaxKind::FN_KW
            || row[3].kind() != SyntaxKind::IDENT
        {
            return None;
        }
        let name = row[3].to_string();
        // These declarations are opaque tokens in the original source, so
        // the file-level ast::Name check cannot inspect them.
        if name.starts_with("r#") {
            return None;
        }
        let parameters = row[4].as_node()?;
        let body = row[5].as_node()?;
        let function_line = lines.line(row[2].text_range().start());
        if parameters.first_token()?.kind() != SyntaxKind::L_PAREN
            || body.first_token()?.kind() != SyntaxKind::L_CURLY
            || parameters.text().to_string().contains('\n')
            || lines.line(parameters.text_range().start()) != function_line
            || lines.line(body.text_range().start()) != function_line
        {
            return None;
        }
        let (parameter, literal) = singleton_parameter(parameters)?;
        let body = lower_body(&body.text().to_string(), &parameter, &literal)?;
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
        *root_scope = file_scope.clone();
        root_scope.defines.insert(name.clone());
        result.push(ProptestExpansion {
            macro_line: 0,
            macro_name: String::new(),
            function_line,
            source,
            name_scopes,
        });
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

fn singleton_parameter(parameters: &SyntaxNode) -> Option<(String, String)> {
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
    if parameter.starts_with("r#") {
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
    if !matches!(
        compact(&call.expr()?.syntax().text().to_string()).as_str(),
        "proptest::strategy::Just" | "::proptest::strategy::Just"
    ) {
        return None;
    }
    let arguments = call.arg_list()?.args().collect::<Vec<_>>();
    let [literal] = arguments.as_slice() else {
        return None;
    };
    typed_singleton_literal(literal).then(|| (parameter, literal.syntax().text().to_string()))
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
    fn a_refused_property_prevents_partial_macro_registration() {
        let changed = SOURCE.replace(
            "\n}",
            "\n #[test]\n fn unsupported(n in 0..2) { proptest::prop_assert!(owner(n)); }\n}",
        );
        assert!(proptest_expansions(&changed).is_empty());
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
