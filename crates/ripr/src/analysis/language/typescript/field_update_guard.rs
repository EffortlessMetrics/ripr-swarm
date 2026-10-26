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
    // An earlier `return` or `throw` can skip the statement.
    if ["return", "throw"]
        .iter()
        .any(|exit| identifier_offsets(&body, exit).iter().any(|x| *x < at))
    {
        return None;
    }
    // `this` used as anything but `this.name` (`this['f'] = 0`, `Object.assign(this, ..)`)
    // can write the field where the text search below cannot see it.
    if identifier_offsets(&body, "this")
        .iter()
        .any(|at| !body[at + 4..].starts_with('.'))
    {
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

/// Whether the owner's class declares `get field` or `set field`. A read of
/// such a field runs the accessor, which need not return what the method wrote,
/// so an exact assertion on it does not observe the update.
pub(crate) fn ts_field_has_accessor(
    change: &FieldUpdateChange,
    owner: &TypeScriptOwner,
    owners: &[TypeScriptOwner],
) -> bool {
    owners.iter().any(|other| {
        other.file == owner.file
            && other.class_name == owner.class_name
            && other.name == change.field
            && matches!(
                other.method_kind,
                TypeScriptMethodKind::Getter | TypeScriptMethodKind::Setter
            )
    })
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
    field_update_precedes_read(change, owner_name, body, read_offset, false)
}

/// As [`ts_field_update_precedes_read`], but the call may sit in counting loops
/// nested inside the read's scope. A loop may run zero times, so the caller
/// must prove a positive iteration count itself (the literal-table witness).
pub(crate) fn ts_field_update_precedes_read_through_loops(
    change: &FieldUpdateChange,
    owner_name: &str,
    body: &str,
    read_offset: usize,
) -> Option<String> {
    field_update_precedes_read(change, owner_name, body, read_offset, true)
}

fn field_update_precedes_read(
    change: &FieldUpdateChange,
    owner_name: &str,
    body: &str,
    read_offset: usize,
    through_loops: bool,
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
    // Nothing else may write the field through the receiver, and the owner
    // call must not be a no-op by argument (`bump(0)`, `bump("")`).
    for at in identifier_offsets(body, receiver) {
        let after = body[at + receiver.len()..].trim_start();
        if at != *declaration + body[*declaration..].find(receiver).unwrap_or(0)
            && (after.starts_with("++")
                || after.starts_with("--")
                || COMPOUND_OPERATORS.iter().any(|op| after.starts_with(op))
                || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>")))
        {
            return None;
        }
        // Every other use is `recv.name`: a computed access (`recv["f"] = 9`), an
        // alias (`const a = recv`), or the instance passed on (`Object.assign(recv, ..)`)
        // can write the field where this scan cannot see it.
        let Some(member) = body[at + receiver.len()..].strip_prefix('.') else {
            if at == *declaration + body[*declaration..].find(receiver).unwrap_or(0) {
                continue;
            }
            return None;
        };
        let name_len = member
            .find(|c: char| !is_identifier_part(c))
            .unwrap_or(member.len());
        let (name, rest) = member.split_at(name_len);
        let rest = rest.trim_start();
        let assigned = rest.starts_with("++")
            || rest.starts_with("--")
            || COMPOUND_OPERATORS.iter().any(|op| rest.starts_with(op))
            || (rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>"));
        if (rest.starts_with('(') && name != owner_name) || (name == change.field && assigned) {
            return None;
        }
        if name == owner_name && rest.starts_with('(') {
            let args = rest[1..].split(')').next().unwrap_or("").trim();
            if matches!(
                args,
                "0" | "\"\"" | "''" | "``" | "false" | "null" | "undefined"
            ) {
                return None;
            }
        }
    }
    let call = format!("{receiver}.{owner_name}(");
    let read_scopes = open_scopes(body, read_offset)?;
    let ran = body.match_indices(&call).any(|(at, _)| {
        at > *declaration
            && at < read_offset
            // The call begins its own statement: a braceless `if (c) call();`,
            // `c && call();` or `else call();` opens no scope but can skip it.
            && body[..at]
                .trim_end()
                .ends_with([';', '{', '}'])
            // The call is in the read's own scope, so it runs whenever the
            // read does. A loop around only the call may run zero times.
            && open_scopes(body, at).is_some_and(|scopes| {
                let nested = scopes.len() >= read_scopes.len()
                    && scopes[..read_scopes.len()] == read_scopes[..]
                    && scopes[read_scopes.len()..].iter().all(|s| *s == Scope::Loop);
                (scopes == read_scopes || (through_loops && nested))
                    && scopes.iter().skip(1).all(|s| *s == Scope::Loop)
            })
    });
    ran.then(|| receiver.to_string())
}

/// The bare identifier that follows an observed expression as the expected
/// argument: `, want)` of `assert.equal(a, want)` or `).toBe(want)`.
pub(crate) fn ts_expected_identifier(after_observed: &str) -> Option<&str> {
    let rest = after_observed.trim_start();
    let argument = if let Some(rest) = rest.strip_prefix(',') {
        rest
    } else {
        rest.strip_prefix(')')?.split_once('(')?.1
    };
    let argument = argument.split([')', ',']).next()?.trim();
    (!argument.is_empty()
        && argument.chars().all(is_identifier_part)
        && !argument.starts_with(|c: char| c.is_ascii_digit()))
    .then_some(argument)
}

/// Loop headers (`for (...)` text) whose block encloses `offset`, outermost first.
fn enclosing_loop_headers(body: &str, offset: usize) -> Vec<String> {
    let mut headers = Vec::new();
    for at in identifier_offsets(body, "for") {
        if at >= offset {
            break;
        }
        let Some(open) = body[at + 3..].trim_start().strip_prefix('(') else {
            continue;
        };
        let open_at = body.len() - open.len();
        let mut depth = 1usize;
        let Some(close) = open.char_indices().find_map(|(i, c)| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        }) else {
            continue;
        };
        let after = open_at + close + 1;
        let Some(block) = body[after..].trim_start().strip_prefix('{') else {
            continue;
        };
        let block_at = body.len() - block.len();
        let mut depth = 1usize;
        let end = body[block_at..].char_indices().find_map(|(i, c)| {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(block_at + i)
        });
        if end.is_some_and(|end| end > offset) {
            headers.push(open[..close].to_string());
        }
    }
    headers
}

/// Rows of the literal numeric table `const name ... = [[..], [..]]`.
fn numeric_table(body: &str, name: &str) -> Option<Vec<Vec<i64>>> {
    let declared: Vec<usize> = ["const", "let"]
        .iter()
        .flat_map(|keyword| {
            body.match_indices(&format!("{keyword} {name}"))
                .map(|(at, _)| at)
                .collect::<Vec<_>>()
        })
        .collect();
    let [declaration] = declared.as_slice() else {
        return None;
    };
    let after = &body[*declaration..];
    let value = after[after.find('=')? + 1..]
        .trim_start()
        .strip_prefix('[')?;
    let (mut depth, mut rows, mut row, mut token) =
        (1usize, Vec::new(), None::<Vec<i64>>, String::new());
    let push = |token: &mut String, row: &mut Option<Vec<i64>>| -> Option<()> {
        let text = token.trim();
        if !text.is_empty() {
            row.as_mut()?.push(text.parse().ok()?);
        }
        token.clear();
        Some(())
    };
    for c in value.chars() {
        match c {
            '[' => {
                depth += 1;
                if depth != 2 || row.is_some() {
                    return None;
                }
                row = Some(Vec::new());
            }
            ']' => {
                depth -= 1;
                if depth == 1 {
                    push(&mut token, &mut row)?;
                    rows.push(row.take()?);
                } else if depth == 0 {
                    return (!rows.is_empty() && rows.iter().all(|r| r.len() == rows[0].len()))
                        .then_some(rows);
                }
            }
            ',' if depth == 2 => push(&mut token, &mut row)?,
            ',' if depth == 1 => {}
            c if depth == 2 && (c.is_ascii_digit() || c == '-' || c.is_whitespace()) => {
                token.push(c);
            }
            c if depth == 1 && c.is_whitespace() => {}
            _ => return None,
        }
    }
    None
}

/// Whether the dynamic expected side `expected` of a read is a column of a
/// constant numeric table iterated by `for (const [a, b] of table)`, with the
/// owner call inside that loop (and counting loops bounded by a column or a
/// literal): some row must call the owner at least once and expect a value
/// other than zero, so deleting the update changes that row's assertion.
pub(crate) fn ts_table_expected_witnessed(
    owner_name: &str,
    receiver: &str,
    body: &str,
    expected: &str,
    read_offset: usize,
) -> bool {
    let Some(call) = body
        .match_indices(&format!("{receiver}.{owner_name}("))
        .map(|(at, _)| at)
        .filter(|at| *at < read_offset)
        .last()
    else {
        return false;
    };
    let headers = enclosing_loop_headers(body, call);
    // Only loops (and the test callback) may enclose the call.
    if open_scopes(body, call).is_none_or(|scopes| scopes.len() != headers.len() + 1) {
        return false;
    }
    let mut table = None;
    let mut bound = None::<&str>;
    for header in &headers {
        let header = header.trim();
        if let Some(rest) = header
            .strip_prefix("const [")
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(names, after)| Some((names, after.trim().strip_prefix("of ")?.trim())))
        {
            if table.is_some() {
                return false;
            }
            table = Some((rest.0.split(',').map(str::trim).collect::<Vec<_>>(), rest.1));
        } else if let Some(limit) = header
            .strip_prefix("let ")
            .and_then(|rest| rest.split_once('='))
            .and_then(|(index, rest)| {
                let (init, rest) = rest.split_once(';')?;
                let (cond, step) = rest.split_once(';')?;
                let index = index.trim();
                let limit = cond.trim().strip_prefix(index)?.trim().strip_prefix('<')?;
                (init.trim() == "0" && step.trim() == format!("{index}++")).then_some(limit.trim())
            })
        {
            if bound.is_some() {
                return false;
            }
            bound = Some(limit);
        } else {
            return false;
        }
    }
    let Some((names, table_name)) = table else {
        return false;
    };
    let Some(want) = names.iter().position(|name| *name == expected) else {
        return false;
    };
    let Some(rows) = numeric_table(&body[..call], table_name) else {
        return false;
    };
    if names.len() != rows[0].len() {
        return false;
    }
    // Neither the table nor its columns may be rebound inside the test.
    if names.iter().any(|name| {
        !name.is_empty()
            && identifier_offsets(body, name).iter().any(|at| {
                body[at + name.len()..].trim_start().starts_with('=')
                    && !body[at + name.len()..].trim_start().starts_with("==")
            })
    }) {
        return false;
    }
    let column = bound.and_then(|limit| names.iter().position(|name| *name == limit));
    rows.iter().any(|row| {
        let calls = match (bound, column) {
            (None, _) => 1,
            (Some(_), Some(column)) => row[column],
            (Some(limit), None) => limit.parse().unwrap_or(0),
        };
        calls >= 1 && row[want] != 0
    })
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

    fn through_loops(body: &str) -> Option<String> {
        let read = body.rfind("stats.hits").unwrap_or(0);
        ts_field_update_precedes_read_through_loops(&change(), "recordHit", body, read)
    }

    fn precedes(body: &str) -> Option<String> {
        let read = body.rfind("stats.hits").unwrap_or(0);
        ts_field_update_precedes_read(&change(), "recordHit", body, read)
    }

    fn method(source: &str) -> TypeScriptOwner {
        TypeScriptOwner {
            name: "recordHit".to_string(),
            file: std::path::PathBuf::from("src/stats.ts"),
            start_line: 1,
            end_line: 1,
            owner_kind: OwnerKind::Method,
            class_name: Some("Stats".to_string()),
            decorated: false,
            exported_as_default: false,
            commonjs_default_export: false,
            commonjs_whole_function: false,
            imports: Vec::new(),
            method_kind: TypeScriptMethodKind::Ordinary,
            class_default_export: false,
            default_object_member: false,
            module_entries: Vec::new(),
            params: Vec::new(),
            arity: Some(0),
            source_text: Some(source.to_string()),
        }
    }

    fn changed(source: &str, new: &str, old: &str) -> Option<FieldUpdateChange> {
        ts_field_update_change(&method(source), new, Some(old))
    }

    #[test]
    fn accessor_for_the_field_in_the_same_class_is_detected() {
        let change = change();
        let owner = method("");
        let accessor = |name: &str, kind: TypeScriptMethodKind, class: &str| TypeScriptOwner {
            name: name.to_string(),
            class_name: Some(class.to_string()),
            method_kind: kind,
            ..method("")
        };
        let has = |owners: &[TypeScriptOwner]| ts_field_has_accessor(&change, &owner, owners);
        assert!(has(&[accessor(
            "hits",
            TypeScriptMethodKind::Getter,
            "Stats"
        )]));
        assert!(has(&[accessor(
            "hits",
            TypeScriptMethodKind::Setter,
            "Stats"
        )]));
        assert!(!has(&[accessor(
            "hits",
            TypeScriptMethodKind::Ordinary,
            "Stats"
        )]));
        assert!(!has(&[accessor(
            "misses",
            TypeScriptMethodKind::Getter,
            "Stats"
        )]));
        assert!(!has(&[accessor(
            "hits",
            TypeScriptMethodKind::Getter,
            "Other"
        )]));
        assert!(!has(&[]));
    }

    #[test]
    fn change_requires_one_straight_line_write_of_the_same_field() {
        let new = "this.hits += 1;";
        let old = "this.hits = this.hits + 1;";
        let plain = "recordHit() {\n  this.hits += 1;\n}";
        assert_eq!(
            changed(plain, new, old).map(|c| c.field).as_deref(),
            Some("hits")
        );
        let branch = "recordHit() {\n  if (this.on) {\n    this.hits += 1;\n  }\n}";
        assert_eq!(changed(branch, new, old), None);
        let twice = "recordHit() {\n  this.hits += 1;\n  this.hits += 1;\n}";
        assert_eq!(changed(twice, new, old), None);
        let early_return = "recordHit() {\n  if (this.on) return;\n  this.hits += 1;\n}";
        assert_eq!(changed(early_return, new, old), None);
        let early_throw =
            "recordHit() {\n  if (this.on) throw new Error('x');\n  this.hits += 1;\n}";
        assert_eq!(changed(early_throw, new, old), None);
        let callback = "recordHit() {\n  this.hits += 1;\n  items.forEach((i) => i);\n}";
        assert_eq!(changed(callback, new, old), None);
        let computed = "recordHit() {\n  this.hits += 1;\n  this['hits'] = 0;\n}";
        assert_eq!(changed(computed, new, old), None);
        let escaped = "recordHit() {\n  this.hits += 1;\n  Object.assign(this, { hits: 0 });\n}";
        assert_eq!(changed(escaped, new, old), None);
        assert_eq!(changed(plain, new, "this.misses = this.misses + 1;"), None);
        assert_eq!(changed(plain, "this.hits = other;", old), None);
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
        let looped = "{ const stats = new Stats(); for (let i = 0; i < 3; i++) { stats.recordHit(); expect(stats.hits).toBe(i + 1); } }";
        assert_eq!(precedes(looped).as_deref(), Some("stats"));
        let nested = "{ const stats = new Stats(); for (let i = 0; i < 3; i++) { stats.recordHit(); } expect(stats.hits).toBe(3); }";
        assert_eq!(precedes(nested), None);
        assert_eq!(through_loops(nested).as_deref(), Some("stats"));
        let skipped = "{ const stats = new Stats(); for (let i = 0; i < 0; i++) { stats.recordHit(); } expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(skipped), None);
        let braceless = "{ const stats = new Stats(); if (ready) stats.recordHit(); expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(braceless), None);
        let computed = "{ const stats = new Stats(); stats.recordHit(); stats[\"hits\"] = 9; expect(stats.hits).toBe(9); }";
        assert_eq!(precedes(computed), None);
        let aliased = "{ const stats = new Stats(); stats.recordHit(); const alias = stats; alias.hits = 9; expect(stats.hits).toBe(9); }";
        assert_eq!(precedes(aliased), None);
        let escaped = "{ const stats = new Stats(); stats.recordHit(); Object.assign(stats, { hits: 9 }); expect(stats.hits).toBe(9); }";
        assert_eq!(precedes(escaped), None);
        let direct = "{ const stats = new Stats(); stats.recordHit(); stats.hits = 9; expect(stats.hits).toBe(9); }";
        assert_eq!(precedes(direct), None);
        let reassigned = "{ let stats = new Stats(); stats.recordHit(); stats = new Stats(); expect(stats.hits).toBe(1); }";
        assert_eq!(precedes(reassigned), None);
    }

    const TABLE: &str = "{ const rows: Array<[number, number]> = [[0, 0], [1, 1], [3, 3]];\n  for (const [calls, want] of rows) {\n    const stats = new Stats();\n    for (let i = 0; i < calls; i++) {\n      stats.recordHit();\n    }\n    assert.equal(stats.hits, want);\n  }\n}";

    fn table(body: &str, expected: &str) -> bool {
        let read = body.rfind("stats.hits").unwrap_or(0);
        ts_table_expected_witnessed("recordHit", "stats", body, expected, read)
    }

    #[test]
    fn table_expected_side_needs_a_row_that_calls_and_expects_nonzero() {
        assert!(table(TABLE, "want"));
        assert!(!table(TABLE, "other"));
        let zero_rows = TABLE.replace("[[0, 0], [1, 1], [3, 3]]", "[[0, 0], [2, 0]]");
        assert!(!table(&zero_rows, "want"));
        let no_calls = TABLE.replace("[[0, 0], [1, 1], [3, 3]]", "[[0, 1], [0, 2]]");
        assert!(!table(&no_calls, "want"));
        let mixed = TABLE.replace("[[0, 0], [1, 1], [3, 3]]", "[[0, 5], [2, 0]]");
        assert!(!table(&mixed, "want"));
        let ragged = TABLE.replace("[3, 3]", "[3]");
        assert!(!table(&ragged, "want"));
        let rebound = TABLE.replace("stats.hits, want", "stats.hits, want = 4");
        assert!(!table(&rebound, "want"));
        let branch = TABLE.replace("for (let i = 0; i < calls; i++)", "if (calls)");
        assert!(!table(&branch, "want"));
        let computed = TABLE.replace("i < calls", "i < size(calls)");
        assert!(!table(&computed, "want"));
    }

    #[test]
    fn expected_identifier_reads_both_matcher_spellings() {
        assert_eq!(ts_expected_identifier(", want);"), Some("want"));
        assert_eq!(ts_expected_identifier(").toBe(want);"), Some("want"));
        assert_eq!(ts_expected_identifier(").toBe(2);"), None);
        assert_eq!(ts_expected_identifier(", rows[i]);"), None);
        assert_eq!(ts_expected_identifier(").toBe(f(want));"), None);
    }

    #[test]
    fn other_writers_and_neutral_arguments_are_not_witnessed() {
        let reset = "{ const stats = new Stats(); stats.reset(); stats.recordHit(); expect(stats.hits).toBe(7); }";
        assert_eq!(precedes(reset), None);
        let assigned = "{ const stats = new Stats(); stats.hits = 7; stats.recordHit(); expect(stats.hits).toBe(7); }";
        assert_eq!(precedes(assigned), None);
        let neutral =
            "{ const stats = new Stats(); stats.recordHit(0); expect(stats.hits).toBe(0); }";
        assert_eq!(precedes(neutral), None);
        let empty =
            "{ const stats = new Stats(); stats.recordHit(\"\"); expect(stats.hits).toBe(\"\"); }";
        assert_eq!(precedes(empty), None);
        let real = "{ const stats = new Stats(); stats.recordHit(2); expect(stats.hits).toBe(2); }";
        assert_eq!(precedes(real).as_deref(), Some("stats"));
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
