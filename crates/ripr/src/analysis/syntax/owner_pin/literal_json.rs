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

/// The `use` items in `file` that can make `json!` visible, collected once.
/// Escape-gating asks visibility per macro call; walking every descendant of
/// a 20k-line test file for each call made dogfood `ripr check` hang through
/// the 3600s PR-evidence ceiling. Whether an item is a canonical import does
/// not depend on the call, so it is decided here rather than once per
/// (macro call, `use` item): that product was 5% of a `check` run's
/// allocations on this repository (#13703).
pub(super) fn literal_json_uses(file: &ast::SourceFile) -> Vec<ast::Use> {
    #[cfg(test)]
    USE_SCAN_COUNT.with(|count| count.set(count.get() + 1));
    file.syntax()
        .descendants()
        .filter_map(ast::Use::cast)
        .filter(is_canonical_json_import)
        .collect()
}

/// An attribute-free `use` whose tree holds an unrenamed, ungrouped
/// `<drop-in crate>::json` leaf.
fn is_canonical_json_import(import: &ast::Use) -> bool {
    #[cfg(test)]
    CANONICAL_IMPORT_CHECKS.with(|count| count.set(count.get() + 1));
    import.attrs().next().is_none()
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
}

/// Only an unchanged canonical import in this call's lexical ancestry. Binding
/// competitors and registry identity remain the classifier's shared authority.
/// `uses` is any list of the file's `use` items; [`literal_json_uses`] hands
/// back the canonical ones, so the scope walk runs only for those.
pub(super) fn literal_json_import_visible_among(uses: &[ast::Use], call: &SyntaxNode) -> bool {
    uses.iter().any(|import| {
        is_canonical_json_import(import)
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
    })
}

/// Only an unchanged canonical import in this call's lexical ancestry. Binding
/// competitors and registry identity remain the classifier's shared authority.
#[cfg(test)]
pub(super) fn literal_json_import_visible(call: &SyntaxNode) -> bool {
    let Some(file) = call.ancestors().find_map(ast::SourceFile::cast) else {
        return false;
    };
    literal_json_import_visible_among(&literal_json_uses(&file), call)
}

#[cfg(test)]
thread_local! {
    static USE_SCAN_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static CANONICAL_IMPORT_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn take_canonical_import_checks() -> usize {
    CANONICAL_IMPORT_CHECKS.with(|count| count.replace(0))
}

#[cfg(test)]
pub(super) fn take_use_scan_count() -> usize {
    USE_SCAN_COUNT.with(|count| count.replace(0))
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

#[cfg(test)]
mod tests {
    use super::super::owner_pin_assertions;
    use super::{
        literal_json_import_visible, literal_json_import_visible_among,
        take_canonical_import_checks, take_use_scan_count,
    };
    use crate::analysis::syntax::parse_clean_source_file;
    use ra_ap_syntax::AstNode;

    #[test]
    fn json_use_scan_runs_once_per_file_not_per_macro() {
        let mut src = String::from("use serde_json::json;\n#[test]\nfn checks() {\n");
        for i in 0..40 {
            src.push_str(&format!(
                "    let _p{i} = json!(null);\n    assert_eq!(1, 1);\n"
            ));
        }
        src.push_str("}\n");
        let _ = take_use_scan_count();
        let assertions = owner_pin_assertions(&src, &["assert_eq", "json"]);
        assert!(assertions.parsed, "synthetic json-macro file must parse");
        assert_eq!(
            take_use_scan_count(),
            1,
            "import visibility must scan the file's use items once, not once per macro"
        );
    }

    /// #13703: whether a `use` is a canonical json import is decided once
    /// per item when the file's uses are collected, not once per (macro
    /// call, item), and the answers match asking every item for every call.
    #[test]
    fn canonical_json_imports_are_decided_once_per_use_not_per_macro() -> Result<(), String> {
        let mut src = String::new();
        for i in 0..30 {
            src.push_str(&format!("use std::collections::BTreeMap as M{i};\n"));
        }
        src.push_str("#[cfg(test)]\nuse serde_json::json;\n");
        src.push_str("mod inner {\n    use serde_json::json;\n    fn g() { json!(null); }\n}\n");
        src.push_str(
            "mod renamed {\n    use serde_json::json as j;\n    fn h() { json!(1); }\n}\n",
        );
        src.push_str("#[test]\nfn checks() {\n");
        for _ in 0..40 {
            src.push_str("    assert_eq!(1, 1);\n");
        }
        src.push_str("}\n");
        let _ = take_canonical_import_checks();
        let assertions = owner_pin_assertions(&src, &["assert_eq", "json"]);
        assert!(assertions.parsed, "synthetic file must parse");
        let checks = take_canonical_import_checks();
        // 33 `use` items are each asked once at collection; only the one
        // canonical import is asked again per macro call (42 calls), where
        // asking every item for every call would be 33 * 42.
        assert!(
            (33..=33 + 2 * 42).contains(&checks),
            "{checks} canonical-import checks for 33 uses and 42 macro calls"
        );

        let parse = parse_clean_source_file(&src).ok_or_else(|| "parse".to_string())?;
        let file = parse.tree();
        let every_use: Vec<_> = file
            .syntax()
            .descendants()
            .filter_map(ra_ap_syntax::ast::Use::cast)
            .collect();
        let canonical = super::literal_json_uses(&file);
        assert_eq!(every_use.len(), 33);
        assert_eq!(canonical.len(), 1, "attributed and renamed imports are out");
        let calls: Vec<_> = file
            .syntax()
            .descendants()
            .filter_map(ra_ap_syntax::ast::MacroCall::cast)
            .collect();
        let visible: Vec<bool> = calls
            .iter()
            .map(|call| literal_json_import_visible_among(&canonical, call.syntax()))
            .collect();
        let unfiltered: Vec<bool> = calls
            .iter()
            .map(|call| literal_json_import_visible_among(&every_use, call.syntax()))
            .collect();
        assert_eq!(visible, unfiltered);
        assert_eq!(visible.iter().filter(|seen| **seen).count(), 1);
        assert!(visible[0], "only the call beside the plain import sees it");
        Ok(())
    }

    #[test]
    fn cached_uses_keep_per_call_scope() -> Result<(), String> {
        let source = "mod other { use serde_json::json; }\nfn f() { json!(null); }\n";
        let parse = parse_clean_source_file(source).ok_or_else(|| "parse".to_string())?;
        let file = parse.tree();
        let call = file
            .syntax()
            .descendants()
            .find_map(ra_ap_syntax::ast::MacroCall::cast)
            .ok_or_else(|| "json call".to_string())?;
        assert_eq!(
            literal_json_import_visible_among(&super::literal_json_uses(&file), call.syntax()),
            literal_json_import_visible(call.syntax())
        );
        assert!(
            !literal_json_import_visible(call.syntax()),
            "a use in another module must not make json visible"
        );
        Ok(())
    }
}
