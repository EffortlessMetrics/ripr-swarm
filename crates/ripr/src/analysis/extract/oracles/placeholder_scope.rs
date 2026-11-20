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

pub(super) fn mentions_placeholder(body: &str) -> bool {
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
        Self::collect(root, body, prefix, &BTreeSet::new())
    }

    fn collect(root: &SyntaxNode, body: &str, prefix: usize, outer: &BTreeSet<String>) -> Self {
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
        let mut candidates: BTreeSet<_> = scope
            .bindings
            .iter()
            .filter(|b| b.placeholder)
            .map(|b| b.name.clone())
            .collect();
        candidates.extend(outer.iter().cloned());
        scope.bindings.retain(|b| candidates.contains(&b.name));
        scope
    }

    pub(crate) fn on_line(&self, body: &str, row: usize, assertion: &str) -> BTreeSet<String> {
        if self.bindings.is_empty() {
            return self.fallback.clone();
        }
        // A scanned statement can start with a closure declaration or match
        // pattern. Locate the same macro whose operands the classifier reads.
        let offset = super::arguments::assertion_oracle_text_at(assertion)
            .map(|(offset, _)| offset)
            .or_else(|| super::arguments::custom_assertion_call_offset(assertion))
            .unwrap_or(0);
        let prefix = &assertion[..offset];
        let preceding_rows = prefix.bytes().filter(|&b| b == b'\n').count();
        let Some(&start) = self.line_starts.get(row + preceding_rows) else {
            return self.fallback.clone();
        };
        let line = body[start..].lines().next().unwrap_or_default();
        let column = prefix.rsplit('\n').next().unwrap_or_default().len();
        let first = assertion.lines().nth(preceding_rows).unwrap_or_default();
        let shift = line
            .find(first.trim_end_matches(';'))
            .unwrap_or_else(|| line.len() - line.trim_start().len());
        self.at(start + shift + column)
    }

    pub(crate) fn at(&self, offset: usize) -> BTreeSet<String> {
        self.visible_at(self.prefix + offset, &self.fallback)
    }

    fn visible_at(&self, offset: usize, outer: &BTreeSet<String>) -> BTreeSet<String> {
        let mut names = outer.clone();
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

/// Macro arguments are token trees in the file AST. Parse the semantic operands
/// as tuple elements so declarations, patterns and reads have real Rust scopes.
pub(super) fn scoped_operand_read(operands: &str, outer: &BTreeSet<String>) -> Option<bool> {
    let source = format!("fn __ripr_operands() {{ let _ = ({operands}\n); }}");
    let parsed = parse_clean_source_file_memoized(&source)?;
    let tree = parsed.tree();
    let scope = PlaceholderScope::collect(tree.syntax(), &source, 0, outer);
    let wrapper = scope.functions.first()?;
    for node in tree.syntax().descendants() {
        let offset = bytes(&node).start;
        if scope
            .functions
            .iter()
            .skip(1)
            .any(|nested| nested.contains(&offset))
            || !wrapper.contains(&offset)
        {
            continue;
        }
        if let Some(path) = ast::PathExpr::cast(node.clone())
            && let Some(name) = path.path().and_then(|p| p.as_single_name_ref())
            && scope.visible_at(offset, outer).contains(name.text())
        {
            return Some(true);
        }
        if let Some(call) = ast::MacroCall::cast(node)
            && super::classify::text_reads_placeholder(
                &call.syntax().text().to_string(),
                &scope.visible_at(offset, outer),
            )
        {
            return Some(true);
        }
    }
    Some(false)
}

fn bytes(node: &SyntaxNode) -> std::ops::Range<usize> {
    node.text_range().into()
}

fn binding_extent(ident: &ast::IdentPat) -> Option<(std::ops::Range<usize>, bool)> {
    for node in ident.syntax().ancestors().skip(1) {
        if let Some(stmt) = ast::LetStmt::cast(node.clone()) {
            let direct = stmt.pat()?.syntax() == ident.syntax();
            let placeholder = direct
                && stmt.initializer().is_some_and(|expr| {
                    let Some(first) = expr.syntax().first_token() else {
                        return false;
                    };
                    // Preserve the existing diverging-prefix rule, including
                    // `todo!() == value`: evaluating its left operand cannot return.
                    first
                        .parent_ancestors()
                        .find_map(ast::MacroCall::cast)
                        .is_some_and(|call| {
                            call.syntax().text_range().start() == expr.syntax().text_range().start()
                                && call.path().is_some_and(|path| {
                                    matches!(
                                        path.syntax().text().to_string().as_str(),
                                        "todo" | "unimplemented" | "panic" | "unreachable"
                                    )
                                })
                        })
                });
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
                    format!("let expected: {ty} = {init};\nassert!(expected.size_hint().0 > 0);");
                let scope = PlaceholderScope::from_body(&body);
                assert_eq!(
                    scope.on_line(&body, 1, "assert!").contains("expected"),
                    lowers,
                    "{body}"
                );
            }
        }
    }
}
