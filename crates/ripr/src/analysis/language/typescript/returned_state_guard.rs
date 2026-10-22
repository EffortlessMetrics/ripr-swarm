//! RIPR-SPEC-0243 rule 15: a changed compound assignment to state the owner returns.
//!
//! `this.balance += amount;` followed by `return this.balance;` (or
//! `t += i * 2` inside a loop, then `return t`) changes the value the owner
//! returns. The line has no `return`, so it reads as an unrecognised shape;
//! this guard recognises the one structure where the assigned target is the
//! owner's single returned expression, so any exact assertion on the owner's
//! result observes the change.

use super::*;

/// Compound assignment operators, longest first so `>>>=` never reads as `>>=`.
const COMPOUND_OPERATORS: [&str; 15] = [
    ">>>=", "**=", "<<=", ">>=", "&&=", "||=", "??=", "+=", "-=", "*=", "/=", "%=", "&=", "|=",
    "^=",
];

/// Whether the changed line is a compound assignment to a target that the
/// owner returns, so the change reads as a return-value change.
///
/// The target is a plain identifier or `this.name`. The owner body must hold
/// exactly one `return`, at its top level and after the assignment, whose
/// whole expression is the target. Every enclosing block of the assignment is
/// a `for`/`while`/`do` loop (no branch can skip it), the body has no nested
/// function, class or method that could run it elsewhere, and the only other
/// writes to the target are its declaration before the assignment.
pub(crate) fn ts_returned_state_change(
    owner: &TypeScriptOwner,
    line_text: &str,
    old_line_text: Option<&str>,
) -> bool {
    let Some(source) = owner.source_text.as_deref() else {
        return false;
    };
    let Some(statement) = changed_statement(line_text, old_line_text) else {
        return false;
    };
    let masked_statement = ts_mask_literals_and_comments(statement);
    let masked_statement = masked_statement.trim();
    let Some(target) = compound_assignment_target(masked_statement) else {
        return false;
    };
    // The replaced statement, when there is one, wrote the same target: a
    // pure operator or operand edit, not a statement swapped for another.
    if let Some(old) = old_line_text {
        let old_statement = changed_statement(old, Some(line_text));
        let same_target = old_statement.is_some_and(|old_statement| {
            let masked = ts_mask_literals_and_comments(old_statement);
            compound_assignment_target(masked.trim()) == Some(target)
        });
        if !same_target {
            return false;
        }
    }
    let Some(body) = masked_owner_body(source) else {
        return false;
    };
    // An arrow owner's body starts after `=>`; step into its block. A function
    // body starts inside its block already.
    let masked_source = ts_mask_literals_and_comments(source);
    let arrow = masked_source
        .get(..masked_source.len().saturating_sub(body.len()))
        .is_some_and(|head| head.trim_end().ends_with("=>"));
    let body = match body.trim_start().strip_prefix('{') {
        Some(block) if arrow => block.to_string(),
        _ => body,
    };
    if body.contains("=>")
        || !identifier_offsets(&body, "function").is_empty()
        || !identifier_offsets(&body, "class").is_empty()
        || has_method_like_block(&body)
    {
        return false;
    }
    let mut statement_at = body.match_indices(masked_statement).map(|(at, _)| at);
    let (Some(statement_at), None) = (statement_at.next(), statement_at.next()) else {
        return false;
    };
    let statement_end = statement_at + masked_statement.len();
    let returns = identifier_offsets(&body, "return");
    let [return_at] = returns.as_slice() else {
        return false;
    };
    let return_at = *return_at;
    if return_at < statement_end || open_blocks(&body[..return_at]).is_none_or(|b| !b.is_empty()) {
        return false;
    }
    let returned = body[return_at + "return".len()..]
        .split([';', '}'])
        .next()
        .unwrap_or("")
        .trim();
    if returned != target {
        return false;
    }
    // Only loops may enclose the assignment.
    let Some(blocks) = open_blocks(&body[..statement_at]) else {
        return false;
    };
    if blocks.iter().any(|kind| *kind != BlockKind::Loop) {
        return false;
    }
    // A method called after the assignment can rewrite `this.name`
    // (`this.reset()`) without spelling it as a write target here.
    if target.starts_with("this.") && body.get(statement_end..return_at).is_none_or(has_call) {
        return false;
    }
    other_writes_are_declarations(&body, target, statement_at, return_at)
}

/// Whether `text` holds a call expression: a `(` that follows an identifier,
/// `)` or `]` and is not the head of a control keyword.
fn has_call(text: &str) -> bool {
    text.match_indices('(').any(|(at, _)| {
        let before = text[..at].trim_end();
        let word = trailing_identifier(before);
        if matches!(word, "for" | "while" | "if" | "switch" | "catch" | "return") {
            return false;
        }
        before.ends_with([')', ']']) || !word.is_empty()
    })
}

/// The text of the statement the change touches: the span between the common
/// prefix and suffix of the old and new line, widened to the `;`, `{` and `}`
/// that bound it. Without an old line the whole line is the statement.
fn changed_statement<'a>(line_text: &'a str, old_line_text: Option<&str>) -> Option<&'a str> {
    let (start, end) = match old_line_text {
        Some(old) => {
            let prefix = line_text
                .char_indices()
                .zip(old.chars())
                .find(|((_, new), old)| new != old)
                .map_or_else(|| line_text.len().min(old.len()), |((at, _), _)| at);
            let max_suffix = line_text.len().min(old.len()).saturating_sub(prefix);
            let suffix = line_text
                .chars()
                .rev()
                .zip(old.chars().rev())
                .take(max_suffix)
                .take_while(|(new, old)| new == old)
                .map(|(new, _)| new.len_utf8())
                .sum::<usize>()
                .min(max_suffix);
            (prefix, line_text.len().saturating_sub(suffix).max(prefix))
        }
        None => (0, line_text.len()),
    };
    let is_boundary = |c: char| matches!(c, ';' | '{' | '}');
    let left = line_text
        .get(..start)?
        .rfind(is_boundary)
        .map_or(0, |at| at + 1);
    let right = line_text
        .get(end..)?
        .find(is_boundary)
        .map_or(line_text.len(), |at| end + at);
    let statement = line_text.get(left..right)?.trim();
    (!statement.is_empty()).then_some(statement)
}

/// `target` of `<target> <op>= <expression>` where `target` is `name` or
/// `this.name`, `op` a compound operator, and nothing but the expression
/// follows. The statement is masked, so a string cannot spell an operator.
fn compound_assignment_target(masked: &str) -> Option<&str> {
    let target_len = target_prefix_len(masked)?;
    let target = masked.get(..target_len)?;
    let rest = masked.get(target_len..)?.trim_start();
    let operator = COMPOUND_OPERATORS
        .iter()
        .find(|operator| rest.starts_with(**operator))?;
    let expression = rest.get(operator.len()..)?.trim();
    // `a += b == c` is fine; a second top-level `=` assignment would not be
    // one statement of the shape this guard reads.
    (!expression.is_empty() && !expression.starts_with('=')).then_some(target)
}

fn target_prefix_len(masked: &str) -> Option<usize> {
    let identifier_len = |text: &str| {
        let len = text
            .find(|c: char| !is_identifier_part(c))
            .unwrap_or(text.len());
        let valid = text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'));
        (valid && len > 0).then_some(len)
    };
    if let Some(rest) = masked.strip_prefix("this.") {
        return identifier_len(rest).map(|len| "this.".len() + len);
    }
    let len = identifier_len(masked)?;
    // `a.b += 1` writes a member of another object, not a plain target.
    let after = masked.get(len..)?.trim_start();
    (!after.starts_with(['.', '[', '(']) && masked.get(..len) != Some("this")).then_some(len)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Loop,
    Other,
}

/// The kinds of the blocks still open at the end of `prefix`, outermost
/// first, or `None` when a `}` closes a block that was never opened.
fn open_blocks(prefix: &str) -> Option<Vec<BlockKind>> {
    let mut stack = Vec::new();
    for (at, c) in prefix.char_indices() {
        match c {
            '{' => stack.push(block_kind(prefix.get(..at)?)),
            '}' => {
                stack.pop()?;
            }
            _ => {}
        }
    }
    Some(stack)
}

/// What opens the block that follows `before`: `for (...)`, `while (...)` or
/// `do` is a loop; anything else (a branch, `try`, a bare or object block)
/// may not run its statements.
fn block_kind(before: &str) -> BlockKind {
    let before = before.trim_end();
    if let Some(head) = before.strip_suffix(')') {
        let mut depth = 1usize;
        let mut open = None;
        for (at, c) in head.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        let keyword = open.map_or("", |open| trailing_identifier(head[..open].trim_end()));
        return if matches!(keyword, "for" | "while") {
            BlockKind::Loop
        } else {
            BlockKind::Other
        };
    }
    if trailing_identifier(before) == "do" {
        BlockKind::Loop
    } else {
        BlockKind::Other
    }
}

/// Whether every mention of `target` in `body`, other than the assignment at
/// `statement_at` and the `return` at `return_at`, is a read or a `let`/`var`
/// declaration before the assignment: no other write can replace the value
/// the assignment built.
fn other_writes_are_declarations(
    body: &str,
    target: &str,
    statement_at: usize,
    return_at: usize,
) -> bool {
    let mentions: Vec<usize> = if target.starts_with("this.") {
        body.match_indices(target).map(|(at, _)| at).collect()
    } else {
        identifier_offsets(body, target)
    };
    mentions.into_iter().all(|at| {
        if at == statement_at || at > return_at {
            // The assignment itself, or the returned read (nothing follows
            // the single top-level `return` that matters).
            return true;
        }
        let after = body.get(at + target.len()..).unwrap_or("").trim_start();
        let before = body.get(..at).unwrap_or("").trim_end();
        let writes = after.starts_with("++")
            || after.starts_with("--")
            || before.ends_with("++")
            || before.ends_with("--")
            || COMPOUND_OPERATORS
                .iter()
                .any(|operator| after.starts_with(operator))
            || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"));
        if !writes {
            return true;
        }
        let declaration = ["let", "var"]
            .iter()
            .any(|keyword| trailing_identifier(before) == *keyword);
        declaration && at < statement_at
    })
}
