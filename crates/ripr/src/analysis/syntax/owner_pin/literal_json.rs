//! SPEC-0197's closed literal-data construction grammar (#7776).
//! Dependency and workspace binding decisions stay with their existing owners.
use super::{is_drop_in_assertion, operand_tokens};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, AstToken, HasAttrs, HasName},
};

/// The closed JSON-data grammar excludes interpolated expressions and nested
/// expansions. JSON parsing also rejects Rust suffixes, raw/byte strings and
/// oversized/non-finite numbers. This proves no caller control flow, not a pin.
pub(super) fn literal_json_tree(tree: &ast::TokenTree) -> bool {
    literal_json_tokens(&operand_tokens(tree))
}

pub(super) fn literal_json_tokens(tokens: &[ra_ap_syntax::SyntaxToken]) -> bool {
    let Some(open) = tokens.first() else {
        return false;
    };
    if !matches!(open.text(), "(" | "[" | "{") {
        return false;
    }
    let mut depth = 0usize;
    let mut content = String::new();
    for (at, token) in tokens.iter().enumerate() {
        // Token-tree strings are not AST Literal nodes, so the file parser's
        // ordinary literal validation does not validate their Rust escapes.
        if ast::String::cast(token.clone()).is_some_and(|value| value.value().is_err()) {
            return false;
        }
        if let Some(integer) = ast::IntNumber::cast(token.clone()) {
            let negative = at
                .checked_sub(1)
                .and_then(|previous| tokens.get(previous))
                .is_some_and(|previous| previous.text() == "-");
            let maximum = i32::MAX as u128 + u128::from(negative);
            // json_internal's generic Serialize call gives an unsuffixed
            // integer its ordinary i32 default, not the JSON decoder's i64.
            if !integer.value().is_ok_and(|value| value <= maximum) {
                return false;
            }
        }
        match token.text() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
        if depth == 0 {
            return serde_json::from_str::<serde_json::Value>(&content).is_ok();
        }
        if content.is_empty() && depth == 1 && token == open {
            continue;
        }
        // Keep token boundaries: `1 . 0` is not the Rust literal `1.0`.
        content.push_str(token.text());
        // JSON permits a unary minus immediately before its numeric literal.
        // Preserve every other token boundary to reject split Rust numbers.
        if token.text() != "-" {
            content.push(' ');
        }
    }
    false
}

/// Only an unchanged canonical import in this call's lexical ancestry. Binding
/// competitors and registry identity remain the classifier's shared authority.
pub(super) fn literal_json_import_visible(call: &SyntaxNode) -> bool {
    let Some(file) = call.ancestors().find_map(ast::SourceFile::cast) else {
        return false;
    };
    file.syntax()
        .descendants()
        .filter_map(ast::Use::cast)
        .any(|import| {
            import.attrs().next().is_none()
                && import
                    .syntax()
                    .ancestors()
                    .skip(1)
                    .find(|node| {
                        ast::BlockExpr::can_cast(node.kind())
                            || ast::Module::can_cast(node.kind())
                            || ast::SourceFile::can_cast(node.kind())
                    })
                    .is_some_and(|scope| call.ancestors().any(|node| node == scope))
                && import.use_tree().is_some_and(|tree| {
                    tree.syntax()
                        .descendants()
                        .filter_map(ast::UseTree::cast)
                        .any(|item| {
                            item.rename().is_none()
                                && item.use_tree_list().is_none()
                                && is_drop_in_assertion(&item, "json")
                        })
                })
        })
}

pub(super) fn json_namespace_rebound(node: &SyntaxNode) -> bool {
    let named = |name: ast::Name| name.text().trim_start_matches("r#") == "serde_json";
    ast::Rename::cast(node.clone())
        .and_then(|rename| rename.name())
        .is_some_and(named)
        || ast::Module::cast(node.clone())
            .and_then(|module| module.name())
            .is_some_and(named)
        || ast::UseTree::cast(node.clone()).is_some_and(|tree| {
            tree.use_tree_list().is_none()
                && tree.rename().is_none()
                && tree
                    .path()
                    .and_then(|path| path.segments().last())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text().trim_start_matches("r#") == "serde_json")
        })
}
