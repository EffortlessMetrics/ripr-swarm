//! Whether a `mut` parameter still holds the caller's value at a comparison
//! (#10306), so a boundary on it can be matched to the inputs tests pass.

use super::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode, TextRange,
    ast::{self, HasName},
};

/// `true` when the function `fn_text` declares `mut name` as a parameter and
/// nothing can have changed it when the condition on 0-based line
/// `line_offset` of `fn_text` is evaluated: the parameter is never written,
/// or every write comes after that condition, or sits in the `then` block of
/// an earlier arm of the `if` / `else if` chain the condition belongs to (an
/// arm that ran would not have reached the condition). `false` for anything
/// else: a write before the condition, in a loop or closure, a `&mut`
/// borrow, a method that may mutate it, a macro that touches it, a second
/// binding of the name, or a line that is not one `if` condition.
pub(crate) fn mut_parameter_unwritten_at(fn_text: &str, name: &str, line_offset: usize) -> bool {
    stable(fn_text, name, line_offset).is_some()
}

fn stable(fn_text: &str, name: &str, line_offset: usize) -> Option<()> {
    if name.is_empty() || name.starts_with("r#") || !fn_text.contains(name) {
        return None;
    }
    let parse = parse_clean_source_file(fn_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let declared_mut = function.param_list()?.params().any(|param| {
        matches!(param.pat(), Some(ast::Pat::IdentPat(ident))
            if ident.mut_token().is_some()
                && ident.ref_token().is_none()
                && ident.at_token().is_none()
                && ident.name().is_some_and(|bound| bound.text() == name))
    });
    if !declared_mut {
        return None;
    }
    // One binding of the name: no shadowing `let`, closure parameter or
    // pattern can make a later read mean something else.
    let bindings = function
        .syntax()
        .descendants()
        .filter(|node| node != function.syntax())
        .filter_map(|node| ast::AnyHasName::cast(node).and_then(|named| named.name()))
        .filter(|bound| bound.text() == name)
        .count();
    if bindings != 1 {
        return None;
    }
    // Without loops or closures every statement runs at most once, in text
    // order along one path, so "after the condition" cannot run before it.
    let body = function.body()?;
    if body.syntax().descendants().any(|node| {
        ast::ForExpr::can_cast(node.kind())
            || ast::WhileExpr::can_cast(node.kind())
            || ast::LoopExpr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
    }) || body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| {
            token.kind() == SyntaxKind::LIFETIME_IDENT
                || matches!(token.text(), "async" | "await" | "unsafe" | "yield")
        })
    {
        return None;
    }
    let writes = writes_of(&function, name)?;
    if writes.is_empty() {
        return Some(());
    }
    let (start, end) = line_range(fn_text, line_offset)?;
    let line = TextRange::new(
        u32::try_from(start).ok()?.into(),
        u32::try_from(end).ok()?.into(),
    );
    let mut conditions = body
        .syntax()
        .descendants()
        .filter_map(ast::IfExpr::cast)
        .filter(|branch| {
            branch
                .condition()
                .is_some_and(|condition| condition.syntax().text_range().intersect(line).is_some())
        });
    let branch = conditions.next()?;
    if conditions.next().is_some() {
        return None;
    }
    let condition = branch.condition()?.syntax().text_range();
    // The `then` blocks of the arms before this one in its chain.
    let earlier_arms: Vec<TextRange> =
        std::iter::successors(branch.syntax().parent(), |node| node.parent())
            .take_while(|_| true)
            .scan(branch.syntax().clone(), |child, parent| {
                let earlier = ast::IfExpr::cast(parent.clone()).filter(|enclosing| {
                    matches!(
                        enclosing.else_branch(),
                        Some(ast::ElseBranch::IfExpr(next)) if next.syntax() == &*child
                    )
                });
                *child = parent;
                Some(earlier)
            })
            .map_while(|earlier| earlier)
            .filter_map(|earlier| {
                earlier
                    .then_branch()
                    .map(|block| block.syntax().text_range())
            })
            .collect();
    writes
        .iter()
        .all(|write| {
            write.start() >= condition.end()
                || earlier_arms.iter().any(|arm| arm.contains_range(*write))
        })
        .then_some(())
}

/// Every range that may change `name`, or `None` when one cannot be read.
fn writes_of(function: &ast::Fn, name: &str) -> Option<Vec<TextRange>> {
    let mentions = |node: &SyntaxNode| {
        node.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.text() == name)
    };
    let mut writes = Vec::new();
    for node in function.syntax().descendants() {
        if let Some(assignment) = ast::BinExpr::cast(node.clone())
            && matches!(assignment.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
            && assignment
                .lhs()
                .is_some_and(|target| mentions(target.syntax()))
        {
            writes.push(assignment.syntax().text_range());
        } else if let Some(reference) = ast::RefExpr::cast(node.clone())
            && reference.mut_token().is_some()
            && reference
                .expr()
                .is_some_and(|operand| mentions(operand.syntax()))
        {
            writes.push(reference.syntax().text_range());
        } else if let Some(call) = ast::MethodCallExpr::cast(node.clone())
            && call
                .receiver()
                .is_some_and(|receiver| mentions(receiver.syntax()))
            && !read_only_method(call.name_ref().as_ref().map(|method| method.text()))
        {
            writes.push(call.syntax().text_range());
        } else if let Some(tree) = ast::TokenTree::cast(node) {
            // The parser does not see inside macro arguments.
            let tokens: Vec<String> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .map(|token| token.text().to_string())
                .filter(|token| !token.trim().is_empty())
                .collect();
            let touches = tokens.windows(2).any(|pair| {
                (pair[0] == name
                    && matches!(
                        pair[1].as_str(),
                        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "|=" | "^="
                    ))
                    || (pair[0] == "mut" && pair[1] == name)
                    || (pair[0] == name && pair[1] == ".")
            }) || tokens.iter().any(|token| token == name)
                && tokens
                    .iter()
                    .any(|token| matches!(token.as_str(), "let" | "fn"));
            if touches {
                return None;
            }
        }
    }
    Some(writes)
}

/// Methods that read a value without mutating it, by the name alone.
fn read_only_method(method: Option<&str>) -> bool {
    let Some(method) = method else {
        return false;
    };
    [
        "checked_",
        "wrapping_",
        "saturating_",
        "overflowing_",
        "is_",
        "to_",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
        || matches!(
            method,
            "abs"
                | "pow"
                | "min"
                | "max"
                | "clamp"
                | "clone"
                | "len"
                | "cmp"
                | "partial_cmp"
                | "count_ones"
                | "leading_zeros"
                | "trailing_zeros"
                | "signum"
                | "rem_euclid"
                | "div_euclid"
                | "abs_diff"
                | "unsigned_abs"
                | "isqrt"
                | "ilog2"
                | "ilog10"
        )
}

fn line_range(text: &str, line_offset: usize) -> Option<(usize, usize)> {
    let mut start = 0usize;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index == line_offset {
            return Some((start, start + line.trim_end_matches('\n').len()));
        }
        start += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::mut_parameter_unwritten_at;

    /// serde's `format_u8` shape: the write sits in an earlier arm.
    const CHAIN: &str = "fn f(mut n: u8) -> usize {\n    if n >= 100 {\n        n /= 100;\n        3\n    } else if n >= 10 {\n        2\n    } else {\n        1\n    }\n}\n";

    fn at(source: &str, needle: &str) -> bool {
        let line = source
            .lines()
            .position(|line| line.contains(needle))
            .unwrap_or(usize::MAX);
        mut_parameter_unwritten_at(source, "n", line)
    }

    #[test]
    fn a_write_in_an_earlier_arm_does_not_reach_a_later_condition() {
        assert!(at(CHAIN, "else if n >= 10"));
        // The first condition runs before any write.
        assert!(at(CHAIN, "if n >= 100"));
    }

    #[test]
    fn a_parameter_never_written_is_stable_anywhere() {
        let plain =
            "fn f(mut n: u8) -> u8 {\n    let m = n.min(3);\n    if n > 9 { 1 } else { m }\n}\n";
        assert!(at(plain, "if n > 9"));
        // A read-only method with a prefix (`checked_`) is not a write.
        let checked = "fn f(mut n: u8) -> bool {\n    let _ = n.checked_add(1);\n    if n > 9 { true } else { false }\n}\n";
        assert!(at(checked, "if n > 9"));
    }

    #[test]
    fn a_write_that_can_run_before_the_condition_refuses() {
        for source in [
            "fn f(mut n: u8) -> u8 {\n    n -= 1;\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    n = n / 2;\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8, x: bool) -> u8 {\n    if x { n = 0; }\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    if { n += 1; true } && n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    bump(&mut n);\n    if n > 9 { 1 } else { 0 }\n}\n",
        ] {
            assert!(!at(source, "n > 9"), "{source}");
        }
    }

    #[test]
    fn a_write_in_a_later_arm_or_an_unrelated_chain_refuses() {
        // The write is in the arm after the condition's own chain position
        // but before it textually: a different chain.
        let other_chain = "fn f(mut n: u8, x: u8) -> u8 {\n    if x > 1 {\n        n = 0;\n    } else if x > 0 {\n        n = 1;\n    }\n    if n > 9 { 1 } else { 0 }\n}\n";
        assert!(!at(other_chain, "if n > 9"));
        // An `else` block write before a later condition in the same chain
        // does not exist; a write in the arm *of* the condition is after it.
        let own_arm = "fn f(mut n: u8) -> u8 {\n    if n > 9 {\n        n = 0;\n        1\n    } else {\n        0\n    }\n}\n";
        assert!(at(own_arm, "if n > 9"));
    }

    #[test]
    fn loops_closures_methods_and_macros_refuse() {
        for source in [
            "fn f(mut n: u8) -> u8 {\n    for _ in 0..2 { n += 1; }\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    let mut g = || n += 1;\n    g();\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    n.add_assign(1);\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    set!(n = 4);\n    if n > 9 { 1 } else { 0 }\n}\n",
            "fn f(mut n: u8) -> u8 {\n    let n = n / 2;\n    if n > 9 { 1 } else { 0 }\n}\n",
        ] {
            assert!(!at(source, "n > 9"), "{source}");
        }
    }

    #[test]
    fn only_a_declared_mut_parameter_qualifies() {
        let not_mut = "fn f(n: u8) -> u8 {\n    if n > 9 { 1 } else { 0 }\n}\n";
        assert!(!at(not_mut, "n > 9"));
        // The condition line must be exactly one `if` condition.
        let none = "fn f(mut n: u8) -> bool {\n    n -= 1;\n    n > 9\n}\n";
        assert!(!at(none, "n > 9"));
    }
}
