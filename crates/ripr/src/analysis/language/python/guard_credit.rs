//! Credit-side prefix safety. Native tokens keep comments, escaped strings and
//! multiline strings from hiding executable escapes or inventing new ones.

use super::PythonOwner;
use super::source_utils::SourceText;
use rustpython_parser::ast::{self, ExprContext, Fold, Ranged};
use rustpython_parser::text_size::TextRange;
use rustpython_parser::{Mode, Tok, lexer::lex};
use std::path::Path;

/// Last significant byte on each physical prefix line. Interior string lines
/// and comment-only lines have no entry. Unknown execution/binding syntax
/// refuses only the new guarded-write credit, leaving direct writes unchanged.
pub(super) fn safe_prefix_line_ends(
    owner: &PythonOwner,
    source: &str,
    changed_index: usize,
) -> Option<Vec<Option<usize>>> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(offset, _)| offset + 1))
        .collect();
    let body_start = *starts.get(owner.start_line)?;
    let changed_start = *starts.get(changed_index)?;
    let text = SourceText::new(source);
    let mut ends = vec![None; changed_index];
    let mut tokens = Vec::new();
    for result in lex(source, Mode::Module) {
        let (token, range) = result.ok()?;
        let start = usize::from(range.start());
        if start >= changed_start {
            if text.line_for_offset(start) > changed_index + 1 {
                break;
            }
            // The effect parser admits the first assignment on a line. A
            // second statement can immediately mask it before any later row.
            if matches!(token, Tok::Semi) {
                return None;
            }
            continue;
        }
        if start < body_start {
            continue;
        }
        if matches!(
            token,
            Tok::Newline | Tok::Indent | Tok::Dedent | Tok::EndOfFile
        ) {
            tokens.push(token);
            continue;
        }
        match &token {
            Tok::Return
            | Tok::Raise
            | Tok::Assert
            | Tok::Break
            | Tok::Continue
            | Tok::Yield
            | Tok::While
            | Tok::Try
            | Tok::With
            | Tok::Except
            | Tok::Finally
            | Tok::Match => return None,
            // Formatted strings contain executable expressions; their token
            // does not expose the embedded calls or binding targets.
            Tok::String { kind, .. } if kind.is_any_fstring() => return None,
            // Python normalizes identifiers with NFKC. Refuse unreadable
            // spellings instead of confusing a distinct spelling with a name.
            Tok::Name { name }
                if !source.get(start..usize::from(range.end()))?.is_ascii()
                    || matches!(name.as_str(), "exit" | "quit" | "_exit") =>
            {
                return None;
            }
            _ => {}
        }
        let line = text.line_for_offset(start).checked_sub(1)?;
        let line_end = starts.get(line + 1).copied().unwrap_or(source.len());
        *ends.get_mut(line)? = Some(usize::from(range.end()).min(line_end));
        tokens.push(token);
    }
    for (index, token) in tokens.iter().enumerate() {
        let Tok::Name { name } = token else { continue };
        if !matches!(name.as_str(), "self" | "cls")
            && !owner
                .parameters
                .iter()
                .any(|parameter| parameter.name == *name)
        {
            continue;
        }
        if index > 0 && matches!(tokens[index - 1], Tok::Dot) {
            continue;
        }
        let tail = &tokens[index + 1..];
        let statement = tokens[..index]
            .iter()
            .rev()
            .take_while(|token| !matches!(token, Tok::Newline | Tok::Semi))
            .collect::<Vec<_>>();
        let declaration = statement
            .iter()
            .any(|token| matches!(token, Tok::Import | Tok::From | Tok::Del));
        if matches!(tail.first(), Some(Tok::Dot)) && !declaration {
            continue;
        }
        let definition = matches!(statement.first(), Some(Tok::Def | Tok::Class | Tok::As));
        // Formal parameters of a nested definition do not bind this owner.
        // Named expressions in defaults/annotations still execute here.
        let nested_header = statement
            .iter()
            .any(|token| matches!(token, Tok::Def | Tok::Class));
        if nested_header && !definition && !matches!(tail.first(), Some(Tok::ColonEqual)) {
            continue;
        }
        // Only a top-level `in` ends a loop target. Membership inside a
        // subscript target must not hide a later receiver/parameter target.
        let mut depth = 0usize;
        let mut loop_target = false;
        for token in statement.iter().rev() {
            match token {
                Tok::For => loop_target = true,
                Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
                Tok::Rpar | Tok::Rsqb | Tok::Rbrace => depth = depth.saturating_sub(1),
                Tok::In if depth == 0 => loop_target = false,
                _ => {}
            }
        }
        if declaration || definition || loop_target || target_is_written(tail) {
            return None;
        }
    }
    Some(ends)
}

fn binding_operator(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Equal
            | Tok::ColonEqual
            | Tok::Colon
            | Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::PercentEqual
            | Tok::DoubleStarEqual
            | Tok::DoubleSlashEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::VbarEqual
            | Tok::AmperEqual
            | Tok::CircumflexEqual
            | Tok::AtEqual
    )
}

/// A later explicit write to this channel can replace the changed value.
/// Receiver aliases are deliberately not inferred: a same-named field on an
/// unknown receiver withholds new guard credit. Dynamic helper effects remain
/// outside this bounded reader, as for the existing direct-write path.
pub(super) fn field_retained_after(source: &str, start: usize, end: usize, attr: &str) -> bool {
    for result in lex(source, Mode::Module) {
        let Ok((token, range)) = result else {
            return false;
        };
        let offset = usize::from(range.start());
        if offset >= end {
            break;
        }
        if offset < start {
            continue;
        }
        match &token {
            Tok::Name { name }
                if !name.is_ascii() || matches!(name.as_str(), "setattr" | "delattr") =>
            {
                return false;
            }
            Tok::String { kind, .. } if kind.is_any_fstring() => return false,
            _ => {}
        }
    }
    let Ok(module) = super::source_facts::parse_module_result(Path::new("<guard-channel>"), source)
    else {
        return false;
    };
    FieldStores { start, end, attr }.fold_mod(module).is_ok()
}

/// Native target contexts distinguish writes from reads in a dictionary, slice,
/// truthiness condition or keyword call. They also include loop/comprehension
/// targets, which can replace the field without an assignment operator.
struct FieldStores<'a> {
    start: usize,
    end: usize,
    attr: &'a str,
}

impl Fold<TextRange> for FieldStores<'_> {
    type TargetU = TextRange;
    type Error = ();
    type UserContext = ();

    fn will_map_user(&mut self, _: &TextRange) {}

    fn map_user(&mut self, range: TextRange, _: ()) -> Result<TextRange, ()> {
        Ok(range)
    }

    fn fold_stmt(&mut self, node: ast::Stmt) -> Result<ast::Stmt, ()> {
        let value = match &node {
            ast::Stmt::Assign(statement) => Some(statement.value.as_ref()),
            ast::Stmt::AugAssign(statement) => Some(statement.value.as_ref()),
            ast::Stmt::AnnAssign(statement) => statement.value.as_deref(),
            _ => None,
        };
        let previous_start = self.start;
        if value.is_some_and(|value| {
            usize::from(value.range().start()) <= self.start
                && self.start < usize::from(value.range().end())
        }) {
            // Assignment stores after its RHS call. Include its target even
            // when spelled before that call, without including an earlier
            // semicolon-separated setup statement on the same physical line.
            self.start = usize::from(node.range().start());
        }
        let result = ast::fold::fold_stmt(self, node);
        self.start = previous_start;
        result
    }

    fn fold_expr_attribute(&mut self, node: ast::ExprAttribute) -> Result<ast::ExprAttribute, ()> {
        let offset = usize::from(node.range.start());
        if offset >= self.start
            && offset < self.end
            && node.attr.as_str() == self.attr
            && matches!(node.ctx, ExprContext::Store | ExprContext::Del)
        {
            return Err(());
        }
        ast::fold::fold_expr_attribute(self, node)
    }

    fn fold_stmt_ann_assign(
        &mut self,
        mut node: ast::StmtAnnAssign,
    ) -> Result<ast::StmtAnnAssign, ()> {
        if node.value.is_none() {
            // An annotation alone does not store to the attribute. Preserve
            // traversal of its expressions without treating the target as a write.
            if let ast::Expr::Attribute(target) = node.target.as_mut() {
                target.ctx = ExprContext::Load;
            }
        }
        ast::fold::fold_stmt_ann_assign(self, node)
    }
}

fn target_is_written(tail: &[Tok]) -> bool {
    let next = tail
        .iter()
        .find(|token| !matches!(token, Tok::Rpar | Tok::Rsqb));
    next.is_some_and(binding_operator)
        || matches!(next, Some(Tok::Comma))
            && tail
                .iter()
                .take_while(|token| !matches!(token, Tok::Newline | Tok::Semi))
                .any(|token| matches!(token, Tok::Equal))
}

/// The shared exclusion reader compares literal text. New credit is narrower:
/// floats need native rounding and escaped strings need native decoding. Refuse
/// those unresolved values, including values supplied only by a call row.
pub(super) fn condition_literals_supported(
    condition: &str,
    row: &super::boundary::CallRow,
    constants: &[super::module_constants::PythonModuleConstant],
) -> bool {
    literal_tokens_supported(condition, Some((row, constants)))
}

fn literal_tokens_supported(
    source: &str,
    bindings: Option<(
        &super::boundary::CallRow,
        &[super::module_constants::PythonModuleConstant],
    )>,
) -> bool {
    for result in lex(source, Mode::Expression) {
        let Ok((token, range)) = result else {
            return false;
        };
        match token {
            Tok::Float { .. } | Tok::Complex { .. } | Tok::Or | Tok::Not => return false,
            Tok::String { kind, .. }
                if kind.is_any_fstring()
                    || source
                        .get(usize::from(range.start())..usize::from(range.end()))
                        .is_none_or(|text| text.contains('\\')) =>
            {
                return false;
            }
            Tok::Name { name } => {
                if let Some((row, constants)) = bindings
                    && let Some(value) = super::boundary::resolve_operand(&name, row, constants)
                    && !literal_tokens_supported(&value, None)
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}
