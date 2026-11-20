use std::collections::BTreeSet;

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasLoopBody, HasName},
};

use crate::analysis::syntax::parse_clean_source_file_memoized;

/// Binding extents use byte offsets in the parsed source. Later declarations
/// shadow earlier ones only inside their own extent.
#[derive(Default)]
pub(crate) struct PlaceholderScope {
    bindings: Vec<Binding>,
    functions: Vec<std::ops::Range<usize>>,
    prefix: usize,
    line_starts: Vec<usize>,
    fallback: BTreeSet<String>,
}

struct Binding {
    name: String,
    extent: std::ops::Range<usize>,
    function: Option<usize>,
    placeholder: bool,
}

fn mentions_placeholder(body: &str) -> bool {
    ["todo", "unimplemented", "panic", "unreachable"]
        .iter()
        .any(|name| body.contains(name))
}

impl PlaceholderScope {
    pub(crate) fn from_body(body: &str) -> Self {
        if !mentions_placeholder(body) {
            return Self::default();
        }
        const PREFIX: &str = "fn __ripr_body() {\n";
        let wrapped = format!("{PREFIX}{body}\n}}");
        let Some(parsed) = parse_clean_source_file_memoized(&wrapped) else {
            return Self {
                fallback: super::scan::placeholder_bound_variables(body),
                ..Self::default()
            };
        };
        Self::from_node(parsed.tree().syntax(), body, PREFIX.len())
    }

    pub(crate) fn from_node(root: &SyntaxNode, body: &str, prefix: usize) -> Self {
        if !mentions_placeholder(body) {
            return Self::default();
        }
        let mut scope = Self {
            prefix,
            line_starts: std::iter::once(0)
                .chain(body.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
            ..Self::default()
        };
        scope.functions = root
            .descendants()
            .filter_map(ast::Fn::cast)
            .map(|f| bytes(f.syntax()))
            .collect();
        for ident in root.descendants().filter_map(ast::IdentPat::cast) {
            let Some(name) = ident.name() else { continue };
            if let Some((range, placeholder)) = binding_extent(&ident) {
                let function = ident
                    .syntax()
                    .ancestors()
                    .find_map(ast::Fn::cast)
                    .map(|f| bytes(f.syntax()).start);
                scope.bindings.push(Binding {
                    name: name.text().to_string(),
                    extent: range,
                    function,
                    placeholder,
                });
            }
        }
        let candidates: BTreeSet<_> = scope
            .bindings
            .iter()
            .filter(|b| b.placeholder)
            .map(|b| b.name.clone())
            .collect();
        scope.bindings.retain(|b| candidates.contains(&b.name));
        scope
    }

    pub(crate) fn on_line(&self, body: &str, row: usize, assertion: &str) -> BTreeSet<String> {
        let Some(&start) = self.line_starts.get(row) else {
            return self.fallback.clone();
        };
        let line = body[start..].lines().next().unwrap_or_default();
        let token = assertion
            .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .next()
            .unwrap_or_default();
        self.at(start + line.find(token).unwrap_or(0))
    }

    pub(crate) fn at(&self, offset: usize) -> BTreeSet<String> {
        let offset = self.prefix + offset;
        let mut names = self.fallback.clone();
        let function = self
            .functions
            .iter()
            .rev()
            .find(|r| r.contains(&offset))
            .map(|r| r.start);
        for binding in &self.bindings {
            if binding.function == function && binding.extent.contains(&offset) {
                if binding.placeholder {
                    names.insert(binding.name.clone());
                } else {
                    names.remove(&binding.name);
                }
            }
        }
        names
    }
}

fn bytes(node: &SyntaxNode) -> std::ops::Range<usize> {
    node.text_range().into()
}

fn binding_extent(ident: &ast::IdentPat) -> Option<(std::ops::Range<usize>, bool)> {
    for node in ident.syntax().ancestors().skip(1) {
        if let Some(stmt) = ast::LetStmt::cast(node.clone()) {
            let direct = stmt.pat()?.syntax() == ident.syntax();
            let placeholder = direct
                && matches!(stmt.initializer(), Some(ast::Expr::MacroExpr(mac))
                if mac.macro_call().and_then(|call| call.path()).is_some_and(|path|
                    matches!(path.syntax().text().to_string().as_str(), "todo" | "unimplemented" | "panic" | "unreachable")));
            return Some((bytes(&node).end..bytes(&node.parent()?).end, placeholder));
        }
        if ast::Param::can_cast(node.kind()) {
            let owner = node.parent()?.parent()?;
            return Some((bytes(&owner), false));
        }
        if ast::MatchArm::can_cast(node.kind()) {
            return Some((bytes(&node), false));
        }
        if let Some(expr) = ast::ForExpr::cast(node.clone()) {
            return Some((bytes(expr.loop_body()?.syntax()), false));
        }
        if ast::LetExpr::can_cast(node.kind()) {
            let end = node.ancestors().find_map(|owner| {
                if let Some(expr) = ast::IfExpr::cast(owner.clone()) {
                    expr.then_branch()
                } else {
                    ast::WhileExpr::cast(owner)?.loop_body()
                }
            })?;
            return Some((bytes(&node).end..bytes(end.syntax()).end, false));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::PlaceholderScope;

    #[test]
    fn associated_type_equals_in_a_spaced_generic_preserves_the_initializer() {
        for ty in ["&dyn Iterator<Item = u8>", "&dyn Iterator <Item = u8>"] {
            for (init, lowers) in [("todo!()", true), ("real_value()", false)] {
                let body =
                    format!("let expected: {ty} = {init}; assert!(expected.size_hint().0 > 0);");
                let scope = PlaceholderScope::from_body(&body);
                assert_eq!(
                    scope.at(body.find("assert!").unwrap()).contains("expected"),
                    lowers,
                    "{body}"
                );
            }
        }
    }
}
