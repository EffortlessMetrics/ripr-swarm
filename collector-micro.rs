use ra_ap_syntax::{AstNode,TextSize,SourceFile,Edition,ast::{self,HasName}};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleItemScopes {
    /// Every fn that is a direct item of the file or of an inline
    /// `mod name { .. }`, mapped to the line span of its innermost inline
    /// module (`None` at the file's top level). A fn nested in another fn's
    /// body, or an associated fn, is absent: it is not visible module-wide.
    pub(crate) item_fns: BTreeMap<(usize, String), Option<(usize, usize)>>,
    /// Fns whose body holds a `use` item, which may shadow a module item.
    pub(crate) fns_with_local_use: std::collections::BTreeSet<(usize, String)>,
    /// Fns whose code may not run when called: an `async fn` (the call
    /// only builds a future) or a body holding a closure or `async` block.
    pub(crate) fns_with_deferred_code: std::collections::BTreeSet<(usize, String)>,
    /// Names each fn calls as a parsed single-segment free function
    /// (`check(..)`), outside macro arguments, strings, comments, closures,
    /// `async` blocks and nested `fn` items.
    pub(crate) direct_calls: BTreeMap<(usize, String), std::collections::BTreeSet<String>>,
    /// Fns carrying a `cfg` or `cfg_attr` attribute anywhere in their
    /// syntax (outer, inner `#![..]`, or on a statement), whitespace
    /// ignored: such a fn may not be the one that compiles.
    pub(crate) fns_with_cfg: std::collections::BTreeSet<(usize, String)>,
    /// Names each fn binds inside itself: every identifier pattern
    /// (parameters, `let`, `for`, `if let`, closure and match bindings)
    /// and every named node in its body (`const`, `static`, nested `fn`,
    /// tuple `struct`, enum variant, macro, and so on).
    pub(crate) bound_names: BTreeMap<(usize, String), std::collections::BTreeSet<String>>,
}

pub(crate) fn baseline_scopes(
    source: &ast::SourceFile,
    line_index: &LineIndex,
) -> ModuleItemScopes {
    let mut scopes = ModuleItemScopes::default();
    for function in source.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(name), Some(fn_token)) = (function.name(), function.fn_token()) else {
            continue;
        };
        let key = (
            line_index.line(fn_token.text_range().start()),
            name.text().to_string(),
        );
        if function
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attribute| {
                let compact: String = attribute
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                compact.starts_with("#[cfg") || compact.starts_with("#![cfg")
            })
        {
            scopes.fns_with_cfg.insert(key.clone());
        }
        let bound = function
            .syntax()
            .descendants()
            .filter(|node| node != function.syntax())
            .filter_map(|node| {
                ast::AnyHasName::cast(node)
                    .and_then(|named| named.name())
                    .map(|name| name.text().to_string())
            })
            .collect();
        scopes.bound_names.insert(key.clone(), bound);
        if let Some(body) = function.body() {
            if body
                .syntax()
                .descendants()
                .any(|node| ast::Use::can_cast(node.kind()))
            {
                scopes.fns_with_local_use.insert(key.clone());
            }
            if function.async_token().is_some()
                || body.syntax().descendants().any(|node| {
                    ast::ClosureExpr::can_cast(node.kind())
                        || ast::BlockExpr::cast(node)
                            .is_some_and(|block| block.async_token().is_some())
                })
            {
                scopes.fns_with_deferred_code.insert(key.clone());
            }
            let called = body
                .syntax()
                .descendants()
                .filter_map(ast::CallExpr::cast)
                // A call inside a closure, `async` block or nested `fn` may
                // never run.
                .filter(|call| !enclosed_by_nested_fn_like(call.syntax(), &body))
                .filter_map(|call| match call.expr()? {
                    ast::Expr::PathExpr(path) => {
                        let path = path.path()?;
                        if path.qualifier().is_some() {
                            return None;
                        }
                        Some(path.segment()?.name_ref()?.text().to_string())
                    }
                    _ => None,
                })
                .collect();
            scopes.direct_calls.insert(key.clone(), called);
        }
        let Some(parent) = function.syntax().parent() else {
            continue;
        };
        let module = match parent.kind() {
            ra_ap_syntax::SyntaxKind::SOURCE_FILE => None,
            ra_ap_syntax::SyntaxKind::ITEM_LIST => {
                match parent.parent().and_then(ast::Module::cast) {
                    Some(module) => {
                        let range = module.syntax().text_range();
                        Some((
                            line_index.line(range.start()),
                            line_index.line_for_range_end(range.end()),
                        ))
                    }
                    None => continue,
                }
            }
            _ => continue,
        };
        scopes.item_fns.insert(key, module);
    }
    scopes
}
pub(crate) fn candidate_scopes(
    source: &ast::SourceFile,
    line_index: &LineIndex,
) -> ModuleItemScopes {
    let mut scopes = ModuleItemScopes::default();
    for function in source.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(name), Some(fn_token)) = (function.name(), function.fn_token()) else {
            continue;
        };
        let key = (
            line_index.line(fn_token.text_range().start()),
            name.text().to_string(),
        );
        let body = function.body();
        let body_range = body.as_ref().map(|body| body.syntax().text_range());
        let mut has_cfg = false;
        let mut bound = Vec::new();
        let mut local_use = false;
        let mut deferred = body.is_some() && function.async_token().is_some();
        let mut called = Vec::new();
        // Keep descendant (not just immediate-child) semantics: nested
        // attributes and bindings still belong to the enclosing summary.
        // Body-only facts exclude signature nodes. Direct calls retain the
        // existing nested-fn/closure/async boundary check below.
        for node in function.syntax().descendants() {
            if !has_cfg && let Some(attribute) = ast::Attr::cast(node.clone()) {
                let compact: String = attribute
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                has_cfg = compact.starts_with("#[cfg") || compact.starts_with("#![cfg");
            }
            if &node != function.syntax()
                && let Some(name) =
                    ast::AnyHasName::cast(node.clone()).and_then(|named| named.name())
            {
                bound.push(name.text().to_string());
            }
            if body_range.is_none_or(|range| !range.contains_range(node.text_range())) {
                continue;
            }
            local_use |= ast::Use::can_cast(node.kind());
            if !deferred {
                deferred = ast::ClosureExpr::can_cast(node.kind())
                    || ast::BlockExpr::cast(node.clone())
                        .is_some_and(|block| block.async_token().is_some());
            }
            if let (Some(body), Some(call)) = (&body, ast::CallExpr::cast(node))
                && !enclosed_by_nested_fn_like(call.syntax(), body)
                && let Some(ast::Expr::PathExpr(path)) = call.expr()
                && let Some(path) = path.path()
                && path.qualifier().is_none()
                && let Some(name) = path.segment().and_then(|segment| segment.name_ref())
            {
                called.push(name.text().to_string());
            }
        }
        if has_cfg {
            scopes.fns_with_cfg.insert(key.clone());
        }
        scopes
            .bound_names
            .insert(key.clone(), bound.into_iter().collect());
        if body.is_some() {
            if local_use {
                scopes.fns_with_local_use.insert(key.clone());
            }
            if deferred {
                scopes.fns_with_deferred_code.insert(key.clone());
            }
            scopes
                .direct_calls
                .insert(key.clone(), called.into_iter().collect());
        }
        let Some(parent) = function.syntax().parent() else {
            continue;
        };
        let module = match parent.kind() {
            ra_ap_syntax::SyntaxKind::SOURCE_FILE => None,
            ra_ap_syntax::SyntaxKind::ITEM_LIST => {
                match parent.parent().and_then(ast::Module::cast) {
                    Some(module) => {
                        let range = module.syntax().text_range();
                        Some((
                            line_index.line(range.start()),
                            line_index.line_for_range_end(range.end()),
                        ))
                    }
                    None => continue,
                }
            }
            _ => continue,
        };
        scopes.item_fns.insert(key, module);
    }
    scopes
}
pub(crate) struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub(crate) fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { starts }
    }

    pub(crate) fn line(&self, offset: TextSize) -> usize {
        self.line_from_offset(text_size_to_usize(offset))
    }

    pub(crate) fn line_for_range_end(&self, offset: TextSize) -> usize {
        self.line_from_offset(text_size_to_usize(offset).saturating_sub(1))
    }

    fn line_from_offset(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index.max(1),
        }
    }
}

fn text_size_to_usize(offset: TextSize) -> usize {
    let value: u32 = offset.into();
    value as usize
}

fn enclosed_by_nested_fn_like(node: &ra_ap_syntax::SyntaxNode, body: &ast::BlockExpr) -> bool {
    node.ancestors()
        .take_while(|ancestor| ancestor != body.syntax())
        .any(|ancestor| {
            ast::ClosureExpr::can_cast(ancestor.kind())
                || ast::Fn::can_cast(ancestor.kind())
                || ast::BlockExpr::cast(ancestor).is_some_and(|block| block.async_token().is_some())
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
 let text = std::fs::read_to_string(std::env::args().nth(1).ok_or("input")?)?;
 let parse = SourceFile::parse(&text, Edition::CURRENT); assert!(parse.errors().is_empty());
 let tree=parse.tree();let lines=LineIndex::new(&text);let expected=baseline_scopes(&tree,&lines);
 assert_eq!(candidate_scopes(&tree,&lines),expected);
 for pair in 0..4 {
  let order=if pair%2==0 {["baseline","candidate"]}else{["candidate","baseline"]};
  for variant in order {
   let mut elapsed=0.0;
   for _ in 0..3 {
    let start=std::time::Instant::now();
    let result=if variant=="baseline" {baseline_scopes(std::hint::black_box(&tree),&lines)}else{candidate_scopes(std::hint::black_box(&tree),&lines)};
    elapsed+=start.elapsed().as_secs_f64();
    assert_eq!(result,expected);std::hint::black_box(result);
   }
   println!("{{\"pair\":{pair},\"variant\":\"{variant}\",\"iterations\":3,\"elapsed_s\":{elapsed},\"function_rows\":{}}}",expected.bound_names.len());
  }
 }
 Ok(())
}
