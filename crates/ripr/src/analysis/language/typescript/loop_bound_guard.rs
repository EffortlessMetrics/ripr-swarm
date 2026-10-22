//! RIPR-SPEC-0243 rule 15: a changed inclusive/exclusive loop bound in a
//! generator that yields on every iteration.
//!
//! `for (let i = 0; i < n; i++) { yield i; }` becoming `i <= n` runs one more
//! iteration whenever the counter can reach `n`, and that iteration yields one
//! more element. An exact assertion on the whole drained sequence
//! (`[...range(3)]`) sees the extra element; no boundary input is needed
//! beyond a call whose counter reaches the bound.

use super::*;

/// The counting direction of a `for` header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopDirection {
    Up,
    Down,
}

/// What the loop bound is compared with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LoopBound {
    /// A positional owner parameter.
    Parameter(usize),
    /// An integer literal the owner always uses.
    Literal(i64),
}

/// The facts of a changed loop comparison that adds or drops the final
/// iteration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoopBoundChange {
    direction: LoopDirection,
    init: i64,
    bound: LoopBound,
}

impl LoopBoundChange {
    /// Whether a call passing `argument` for the bounded parameter lets the
    /// counter reach the bound, so the extra iteration runs.
    pub(crate) fn reaches_bound(&self, arguments: &[String]) -> bool {
        let bound = match &self.bound {
            LoopBound::Literal(value) => *value,
            LoopBound::Parameter(index) => {
                let Some(value) = arguments
                    .get(*index)
                    .and_then(|argument| integer_literal(argument))
                else {
                    return false;
                };
                value
            }
        };
        match self.direction {
            LoopDirection::Up => self.init <= bound,
            LoopDirection::Down => self.init >= bound,
        }
    }
}

/// A header's parts: counter, initial value, comparison, bound text, update.
struct LoopHeader<'a> {
    counter: &'a str,
    init: &'a str,
    operator: &'a str,
    bound: &'a str,
    update: &'a str,
}

fn parse_header(line_text: &str) -> Option<LoopHeader<'_>> {
    let text = line_text.trim();
    let text = text.strip_prefix("for")?.trim_start().strip_prefix('(')?;
    let close = text.rfind(')')?;
    // Only the opening `{` may follow the header.
    if !matches!(text.get(close + 1..)?.trim(), "{" | "") {
        return None;
    }
    let mut parts = text.get(..close)?.split(';');
    let (declaration, condition, update) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let declaration = declaration.trim();
    let declaration = declaration
        .strip_prefix("let ")
        .or_else(|| declaration.strip_prefix("var "))?;
    let (counter, init) = declaration.split_once('=')?;
    let condition = condition.trim();
    let counter = counter.trim();
    let after_counter = condition.strip_prefix(counter)?;
    if after_counter.starts_with(is_identifier_part) {
        return None;
    }
    let after_counter = after_counter.trim_start();
    let operator = ["<=", ">=", "<", ">"]
        .into_iter()
        .find(|operator| after_counter.starts_with(operator))?;
    Some(LoopHeader {
        counter,
        init: init.trim(),
        operator,
        bound: after_counter.get(operator.len()..)?.trim(),
        update: update.trim(),
    })
}

fn integer_literal(text: &str) -> Option<i64> {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The `LoopBoundChange` when the new and old `for` headers differ only in
/// `<`/`<=` (counting up by one) or `>`/`>=` (counting down by one), and the
/// owner is a generator whose loop body yields on every iteration.
pub(crate) fn ts_loop_bound_change(
    owner: &TypeScriptOwner,
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<LoopBoundChange> {
    let new = parse_header(line_text)?;
    let old = parse_header(old_line_text?)?;
    if new.counter != old.counter
        || new.init != old.init
        || new.bound != old.bound
        || new.update != old.update
        || new.operator == old.operator
    {
        return None;
    }
    let counter = new.counter;
    let update: String = new.update.chars().filter(|c| !c.is_whitespace()).collect();
    let direction = match (new.operator, old.operator) {
        ("<" | "<=", "<" | "<=") => LoopDirection::Up,
        (">" | ">=", ">" | ">=") => LoopDirection::Down,
        _ => return None,
    };
    let steps_by_one = match direction {
        LoopDirection::Up => [
            format!("{counter}++"),
            format!("++{counter}"),
            format!("{counter}+=1"),
        ],
        LoopDirection::Down => [
            format!("{counter}--"),
            format!("--{counter}"),
            format!("{counter}-=1"),
        ],
    };
    if !steps_by_one.contains(&update) {
        return None;
    }
    let init = integer_literal(new.init)?;
    let source = owner.source_text.as_deref()?;
    let masked = ts_mask_literals_and_comments(source);
    // A generator: `function* name(` or a `*name(` member.
    let head = masked.get(..masked.find('(')?)?;
    if !head.contains('*') {
        return None;
    }
    let bound = match integer_literal(new.bound) {
        Some(value) => LoopBound::Literal(value),
        None => {
            if !bound_parameter_is_read_only(source, new.bound) {
                return None;
            }
            LoopBound::Parameter(owner.params.iter().position(|param| param == new.bound)?)
        }
    };
    loop_yields_every_iteration(&masked, line_text, counter).then_some(LoopBoundChange {
        direction,
        init,
        bound,
    })
}

/// Whether the one loop whose header is `header_line` has a body that yields
/// at its own top level, never leaves early, never calls a function value,
/// and never writes the counter.
fn loop_yields_every_iteration(masked: &str, header_line: &str, counter: &str) -> bool {
    let header = ts_mask_literals_and_comments(header_line);
    let header = header.trim();
    let mut found = masked.match_indices(header).map(|(at, _)| at);
    let (Some(header_at), None) = (found.next(), found.next()) else {
        return false;
    };
    let after_header = header_at + header.len();
    let Some(rest) = masked.get(after_header..) else {
        return false;
    };
    // The block opens right after the header (the header may end in `{`).
    let block_start = if header.ends_with('{') {
        after_header
    } else {
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('{') {
            return false;
        }
        after_header + (rest.len() - trimmed.len()) + 1
    };
    let mut depth = 1usize;
    let mut block_end = None;
    for (at, c) in masked.get(block_start..).unwrap_or("").char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    block_end = Some(block_start + at);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(block_end) = block_end else {
        return false;
    };
    let Some(block) = masked.get(block_start..block_end) else {
        return false;
    };
    if block.contains("=>")
        || ["break", "continue", "return", "throw", "function", "class"]
            .iter()
            .any(|word| !identifier_offsets(block, word).is_empty())
        || has_method_like_block(block)
    {
        return false;
    }
    // A `yield` at the block's own top level runs on every iteration.
    let mut level = 0usize;
    let mut yields_at_top = false;
    for (at, c) in block.char_indices() {
        match c {
            '{' => level += 1,
            '}' => level = level.saturating_sub(1),
            _ if level == 0 && block[at..].starts_with("yield") => {
                let boundary_before = block[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !is_identifier_part(before));
                let boundary_after = block[at + "yield".len()..]
                    .chars()
                    .next()
                    .is_none_or(|after| !is_identifier_part(after));
                // `yield*` delegates to a sequence of unknown length; the
                // extra iteration still adds elements, but keep to plain yield.
                if boundary_before
                    && boundary_after
                    && !block[at + "yield".len()..].starts_with('*')
                {
                    yields_at_top = true;
                }
            }
            _ => {}
        }
    }
    yields_at_top && counter_is_read_only(block, counter)
}

/// Whether `counter` is never assigned, incremented or decremented in `block`.
fn counter_is_read_only(block: &str, counter: &str) -> bool {
    identifier_offsets(block, counter).into_iter().all(|at| {
        let after = block.get(at + counter.len()..).unwrap_or("").trim_start();
        let before = block.get(..at).unwrap_or("").trim_end();
        !(after.starts_with("++")
            || after.starts_with("--")
            || before.ends_with("++")
            || before.ends_with("--")
            || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
            || ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="]
                .iter()
                .any(|operator| after.starts_with(operator)))
    })
}

/// Whether the owner never writes, redeclares or shadows the parameter `name`
/// after its signature. Unlike [`ts_parameter_is_stable`], a mention in a
/// `for` header (`i < name`) is a read.
fn bound_parameter_is_read_only(owner_source: &str, name: &str) -> bool {
    let Some(body) = masked_owner_body(owner_source) else {
        return false;
    };
    if body.contains("=>") || !identifier_offsets(&body, "function").is_empty() {
        return false;
    }
    for keyword in ["let", "const", "var", "catch", "class"] {
        for at in identifier_offsets(&body, keyword) {
            let declarator = &body[at + keyword.len()..];
            let end = declarator.find(['=', ';']).unwrap_or(declarator.len());
            if !identifier_offsets(&declarator[..end], name).is_empty() {
                return false;
            }
        }
    }
    // `for (name of xs)` / `for (name in o)` assign the name.
    let assigned_by_for = identifier_offsets(&body, name).into_iter().any(|at| {
        let after = body[at + name.len()..].trim_start();
        let before = body[..at].trim_end();
        (after.starts_with("of ") || after.starts_with("in ")) && before.ends_with('(')
    });
    !assigned_by_for && counter_is_read_only(&body, name)
}
