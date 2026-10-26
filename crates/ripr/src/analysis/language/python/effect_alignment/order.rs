//! Byte-order evidence for an effect read inside its native assertion span.
use super::super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, ExprContext, Fold, Ranged};
use rustpython_parser::text_size::TextRange;
use std::{convert::Infallible, path::Path};

pub(super) fn read_after(
    source: &str,
    assertion: TextRange,
    operand: &str,
    after: usize,
) -> Option<usize> {
    let module = parse_module_result(Path::new("<effect-order>"), source).ok()?;
    let mut reader = ReadPosition {
        source,
        assertion,
        operand,
        after,
        found: None,
    };
    let _ = reader.fold_mod(module);
    reader.found
}

struct ReadPosition<'a> {
    source: &'a str,
    assertion: TextRange,
    operand: &'a str,
    after: usize,
    found: Option<usize>,
}

impl Fold<TextRange> for ReadPosition<'_> {
    type TargetU = TextRange;
    type Error = Infallible;
    type UserContext = ();

    fn will_map_user(&mut self, _: &TextRange) {}
    fn map_user(&mut self, range: TextRange, _: ()) -> Result<TextRange, Infallible> {
        Ok(range)
    }

    fn fold_expr(&mut self, node: ast::Expr) -> Result<ast::Expr, Infallible> {
        let range = node.range();
        let start = usize::from(range.start());
        let is_read = match &node {
            ast::Expr::Name(name) => name.ctx == ExprContext::Load,
            ast::Expr::Attribute(attr) => attr.ctx == ExprContext::Load,
            ast::Expr::Subscript(subscript) => subscript.ctx == ExprContext::Load,
            _ => true,
        };
        if is_read
            && start >= self.after
            && range.start() >= self.assertion.start()
            && range.end() <= self.assertion.end()
            && self.source.get(start..usize::from(range.end())) == Some(self.operand)
        {
            self.found = Some(self.found.map_or(start, |found| found.min(start)));
        }
        ast::fold::fold_expr(self, node)
    }
}
