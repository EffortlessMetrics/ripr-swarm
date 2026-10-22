//! RIPR-SPEC-0243 decision 18: a changed update of a field a test then reads.
//!
//! `this.hits = this.hits + 1;` rewritten as `this.hits += 1;` has no
//! `return`, so the line reads as an unrecognised shape. The statement does
//! change observable state, though: when it sits on the owner method's own
//! straight-line path and a test reads that field from an instance it called
//! the method on, the assertion observes the update. This module only reads
//! the shape of the change and of the test; `classifier` decides credit.

use super::mock_payload_guard::{identifier_offsets, is_identifier_part, masked_owner_body};
use super::*;

/// Compound assignment operators, longest first so `>>>=` never reads as `>>=`.
const COMPOUND_OPERATORS: [&str; 15] = [
    ">>>=", "**=", "<<=", ">>=", "&&=", "||=", "??=", "+=", "-=", "*=", "/=", "%=", "&=", "|=",
    "^=",
];

/// An update of `this.<field>` on the owner method's straight-line path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldUpdateChange {
    pub(crate) field: String,
    pub(crate) class_name: String,
}

/// The field the changed statement updates, when both the new and the old
/// statement update the same `this.<field>` and the new one sits once at the
/// top level of the owner method's body.
pub(crate) fn ts_field_update_change(
    owner: &TypeScriptOwner,
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<FieldUpdateChange> {
    if !matches!(owner.owner_kind, OwnerKind::Method) {
        return None;
    }
    let class_name = owner.class_name.clone()?;
    let source = owner.source_text.as_deref()?;
    let masked_new = ts_mask_literals_and_comments(line_text);
    let masked_old = ts_mask_literals_and_comments(old_line_text?);
    let field = updated_field(masked_new.trim())?;
    if updated_field(masked_old.trim())? != field {
        return None;
    }
    let body = masked_owner_body(source)?;
    let statement = masked_new.trim().trim_end_matches(';').trim_end();
    let mut at = body.match_indices(statement).map(|(at, _)| at);
    let (Some(at), None) = (at.next(), at.next()) else {
        return None;
    };
    // The body begins inside the method's block: the statement must be at
    // depth zero, with no branch, loop or nested function to skip it.
    let depth_zero = body[..at].chars().try_fold(0i32, |depth, c| match c {
        '{' | '(' | '[' => Some(depth + 1),
        '}' | ')' | ']' => (depth > 0).then_some(depth - 1),
        _ => Some(depth),
    }) == Some(0);
    if !depth_zero || body.contains("=>") || !identifier_offsets(&body, "function").is_empty() {
        return None;
    }
    // A single write: another update of the field could hide this one.
    let target = format!("this.{field}");
    let writes = body
        .match_indices(&target)
        .filter(|(at, _)| {
            let after = body[at + target.len()..].trim_start();
            after.starts_with("++")
                || after.starts_with("--")
                || COMPOUND_OPERATORS.iter().any(|op| after.starts_with(op))
                || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
        })
        .count();
    (writes == 1).then_some(FieldUpdateChange { field, class_name })
}

/// `field` of `this.<field> <op>= e` or `this.<field> = this.<field> <op> e`.
fn updated_field(statement: &str) -> Option<String> {
    let statement = statement.trim_end_matches(';').trim();
    let rest = statement.strip_prefix("this.")?;
    let len = rest
        .find(|c: char| !is_identifier_part(c))
        .unwrap_or(rest.len());
    let field = rest.get(..len).filter(|name| !name.is_empty())?;
    let after = rest.get(len..)?.trim_start();
    if let Some(operator) = COMPOUND_OPERATORS.iter().find(|op| after.starts_with(**op)) {
        let expression = after.get(operator.len()..)?.trim();
        return (!expression.is_empty() && !expression.starts_with('=')).then(|| field.to_string());
    }
    let value = after
        .strip_prefix('=')
        .filter(|v| !v.starts_with(['=', '>']))?
        .trim();
    let reread = value.strip_prefix("this.")?.strip_prefix(field)?;
    // `this.n = this.n + 1`: the field is read, then combined; `this.nx`
    // or `this.n.length` is another read.
    let reread = reread.trim_start();
    let operator = reread.chars().next()?;
    (matches!(operator, '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^')
        && !reread[operator.len_utf8()..].starts_with('='))
    .then(|| field.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Loop,
    Other,
}

/// The scopes still open at `offset` of `body`, outermost first.
fn open_scopes(body: &str, offset: usize) -> Option<Vec<Scope>> {
    let mut stack = Vec::new();
    for (at, c) in body.get(..offset)?.char_indices() {
        match c {
            '{' => stack.push(scope_of(body.get(..at)?)),
            '}' => {
                stack.pop()?;
            }
            _ => {}
        }
    }
    Some(stack)
}

fn scope_of(before: &str) -> Scope {
    let before = before.trim_end();
    let Some(head) = before.strip_suffix(')') else {
        return Scope::Other;
    };
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
    let keyword = open.map_or("", |open| {
        let text = head[..open].trim_end();
        let start = text
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_identifier_part(*c))
            .last()
            .map_or(text.len(), |(at, _)| at);
        &text[start..]
    });
    if matches!(keyword, "for" | "while") {
        Scope::Loop
    } else {
        Scope::Other
    }
}

/// Whether, in the masked test `body`, an instance of the change's class is
/// updated by the owner and then read: a receiver declared once as
/// `new Class(..)`, a call `receiver.owner(..)` that always runs (only
/// counting loops separate it from the test callback), and a read
/// `receiver.field` at `read_offset`, after the call, in a scope that
/// encloses the call.
pub(crate) fn ts_field_update_precedes_read(
    change: &FieldUpdateChange,
    owner_name: &str,
    body: &str,
    read_offset: usize,
) -> Option<String> {
    let read = body.get(read_offset..)?;
    let receiver_len = read.find(|c: char| !is_identifier_part(c))?;
    let receiver = &read[..receiver_len];
    if receiver.is_empty()
        || !read[receiver_len..].starts_with(&format!(".{}", change.field))
        || read[receiver_len + 1 + change.field.len()..].starts_with(is_identifier_part)
    {
        return None;
    }
    // Declared once, as a fresh instance of the class.
    let declarations: Vec<usize> = ["const", "let"]
        .iter()
        .flat_map(|keyword| {
            body.match_indices(&format!("{keyword} {receiver}"))
                .map(|(at, _)| at)
                .collect::<Vec<_>>()
        })
        .collect();
    let [declaration] = declarations.as_slice() else {
        return None;
    };
    let initializer = body[*declaration..].split(';').next()?;
    let constructs = initializer.split_once('=').is_some_and(|(_, value)| {
        value
            .trim_start()
            .starts_with(&format!("new {}(", change.class_name))
    });
    if !constructs
        || !identifier_offsets(body, receiver)
            .iter()
            .all(|at| *at >= *declaration)
    {
        return None;
    }
    let call = format!("{receiver}.{owner_name}(");
    let read_scopes = open_scopes(body, read_offset)?;
    let ran = body.match_indices(&call).any(|(at, _)| {
        at > *declaration
            && at < read_offset
            && open_scopes(body, at).is_some_and(|scopes| {
                // The test callback is the outermost scope; every scope the
                // call is in beyond the read's own must be a loop.
                scopes.len() >= read_scopes.len()
                    && scopes[..read_scopes.len()] == read_scopes[..]
                    && scopes[read_scopes.len()..]
                        .iter()
                        .all(|s| *s == Scope::Loop)
                    && scopes.iter().skip(1).all(|s| *s == Scope::Loop)
            })
    });
    ran.then(|| receiver.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change() -> FieldUpdateChange {
        FieldUpdateChange {
            field: "hits".to_string(),
            class_name: "Stats".to_string(),
        }
    }

    fn precedes(body: &str) -> Option<String> {
        let read = body.rfind("stats.hits").unwrap_or(0);
        ts_field_update_precedes_read(&change(), "recordHit", body, read)
    }

    #[test]
    fn updated_field_reads_compound_and_respelled_forms() {
        assert_eq!(updated_field("this.hits += 1;").as_deref(), Some("hits"));
        assert_eq!(
            updated_field("this.hits = this.hits + 1;").as_deref(),
            Some("hits")
        );
        assert_eq!(updated_field("this.hits = this.hitsX + 1;"), None);
        assert_eq!(updated_field("this.hits = other.hits + 1;"), None);
        assert_eq!(updated_field("this.hits == 1;"), None);
        assert_eq!(updated_field("hits += 1;"), None);
    }

    #[test]
    fn read_after_an_unconditional_call_on_a_fresh_instance_is_witnessed() {
        let body = "{ const stats = new Stats(); stats.recordHit(); expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(body).as_deref(), Some("stats"));
        let looped = "{ const stats = new Stats(); for (let i = 0; i < 3; i++) { stats.recordHit(); } expect(stats.hits).toBe(3); }";
        assert_eq!(precedes(looped).as_deref(), Some("stats"));
    }

    #[test]
    fn read_without_a_guaranteed_prior_call_is_not_witnessed() {
        let before =
            "{ const stats = new Stats(); expect(stats.hits).toBe(0); stats.recordHit(); }";
        assert_eq!(precedes(before), None);
        let branch = "{ const stats = new Stats(); if (ready) { stats.recordHit(); } expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(branch), None);
        let other_class =
            "{ const stats = makeStats(); stats.recordHit(); expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(other_class), None);
        let rebound = "{ let stats = new Stats(); stats.recordHit(); let stats = new Stats(); expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(rebound), None);
        let other_field =
            "{ const stats = new Stats(); stats.recordHit(); expect(stats.hitsTotal).toBe(1); }";
        assert_eq!(precedes(other_field), None);
    }
}
