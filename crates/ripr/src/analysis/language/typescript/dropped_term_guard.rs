//! RIPR-SPEC-0248 rule 1: a change that only drops (or adds) one additive
//! term needs a tested input where that term is not zero.
//!
//! `return { tier, totalCents: total + shippingFee(subtotalCents) }` becoming
//! `return { tier, totalCents: total }` changes the result only when the
//! dropped term is non-zero. An exact assertion on `quote(6000, 100)` passes
//! on both versions when `shippingFee(6000)` is `0`, however exact the
//! matcher is. So an `exposed` ladder result whose change is one additive
//! term keeps its credit only when the term folds to a non-zero number for
//! the arguments of an observed owner call; otherwise it reads
//! `weakly_exposed`.

use super::*;

/// The additive term one side of the change carries and the other lacks:
/// `a + t` against `a`, `a - t` against `a`, or `t + a` against `a`, in either
/// direction. The term is returned as written. Anything else (a replaced
/// operand, an operator that binds tighter than `+`, a compound assignment,
/// an unbalanced or statement-level fragment) is `None`, and so is a term
/// that names nothing: a literal term is the same for every input, so a
/// non-zero one always changes the value and a zero one is an equivalent
/// edit this rule does not judge.
pub(crate) fn ts_dropped_additive_term(old_line: &str, new_line: &str) -> Option<String> {
    let (old, new) = (old_line.trim(), new_line.trim());
    let (long, short) = if old.chars().count() > new.chars().count() {
        (old, new)
    } else {
        (new, old)
    };
    let long_chars: Vec<char> = long.chars().collect();
    let short_chars: Vec<char> = short.chars().collect();
    let prefix = long_chars
        .iter()
        .zip(&short_chars)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = long_chars[prefix..]
        .iter()
        .rev()
        .zip(short_chars[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if prefix + suffix != short_chars.len() {
        return None; // the short side is not the long side minus one span
    }
    let before: String = long_chars[..prefix].iter().collect();
    let removed: String = long_chars[prefix..long_chars.len() - suffix]
        .iter()
        .collect();
    let after: String = long_chars[long_chars.len() - suffix..].iter().collect();
    trailing_term(&before, &removed, &after)
        .or_else(|| leading_term(&before, &removed, &after))
        .filter(|term| names_something(term))
}

/// `true` when `term` has an identifier outside its literals; the `x` of
/// `0x10` or the `e` of `1e3` continues a number and names nothing.
fn names_something(term: &str) -> bool {
    let masked = ts_mask_literals_and_comments(term);
    let mut previous = ' ';
    masked.chars().any(|c| {
        let starts = (c.is_alphabetic() || c == '_' || c == '$')
            && !(previous.is_alphanumeric() || previous == '_' || previous == '$');
        previous = c;
        starts
    })
}

/// `true` when `term` has a `+` or `-` outside brackets past its first
/// character: `- b + c` drops `-b + c`, not `-(b + c)`.
fn has_top_level_additive(term: &str) -> bool {
    let masked = ts_mask_literals_and_comments(term.trim());
    let mut depth = 0usize;
    masked.char_indices().any(|(index, c)| {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index > 0 && depth == 0 && matches!(c, '+' | '-')
    })
}

/// `left + term` / `left - term` with ` + term` removed.
fn trailing_term(before: &str, removed: &str, after: &str) -> Option<String> {
    let rest = removed.trim_start();
    let operator = rest.chars().next().filter(|c| matches!(c, '+' | '-'))?;
    let term = &rest[operator.len_utf8()..];
    if term.starts_with(['+', '-', '='])
        || (operator == '-' && has_top_level_additive(term))
        || !ends_operand(before)
        || !closes_term(after)
    {
        return None;
    }
    is_additive_operand(term).then(|| term.trim().to_string())
}

/// `term + right` with `term + ` removed.
fn leading_term(before: &str, removed: &str, after: &str) -> Option<String> {
    let term = removed.trim_end().strip_suffix('+')?;
    if term.ends_with(['+', '-']) || !opens_term(before) || !starts_operand(after) {
        return None;
    }
    is_additive_operand(term).then(|| term.trim().to_string())
}

/// `true` when `term` is one balanced operand of `+`: identifiers, numbers,
/// calls, members, literals and multiplicative or additive operators only,
/// so dropping it leaves the rest of the sum intact.
fn is_additive_operand(term: &str) -> bool {
    let masked = ts_mask_literals_and_comments(term);
    if masked.trim().is_empty() {
        return false;
    }
    let mut depth = 0usize;
    for c in masked.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ if depth > 0 => {}
            c if c.is_alphanumeric() || c.is_whitespace() => {}
            '_' | '$' | '.' | '"' | '\'' | '`' | '+' | '-' | '*' | '/' | '%' => {}
            _ => return false,
        }
    }
    depth == 0 && !masked.contains("++") && !masked.contains("--")
}

/// Keywords that can end the text before an operand without being one.
const OPERAND_KEYWORDS: [&str; 13] = [
    "return", "typeof", "void", "await", "yield", "case", "in", "of", "new", "delete", "throw",
    "else", "do",
];

/// `true` when `text` ends with one of the operand keywords as a whole word.
fn ends_with_keyword(text: &str) -> bool {
    OPERAND_KEYWORDS.iter().any(|keyword| {
        text.strip_suffix(keyword).is_some_and(|head| {
            !head
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        })
    })
}

/// The text before a trailing term ends with a complete operand.
fn ends_operand(before: &str) -> bool {
    let trimmed = before.trim_end();
    !ends_with_keyword(trimmed)
        && trimmed.chars().next_back().is_some_and(|c| {
            c.is_alphanumeric() || matches!(c, '_' | '$' | ')' | ']' | '"' | '\'' | '`')
        })
}

/// The text before a leading term opens an operand position whose operator
/// binds looser than `+`: `(`, `[`, `,`, `=`, `:`, `?`, `{`, a comparison or
/// logical operator, another binary `+`, or `return`.
fn opens_term(before: &str) -> bool {
    let trimmed = before.trim_end();
    if trimmed.ends_with("++") || trimmed.ends_with("--") {
        return false;
    }
    if trimmed.strip_suffix("return").is_some_and(|head| {
        !head
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
    }) {
        return true;
    }
    trimmed.chars().next_back().is_some_and(|c| {
        matches!(
            c,
            '(' | '[' | ',' | '=' | ':' | '?' | '{' | '<' | '>' | '&' | '|' | '+'
        )
    })
}

/// The text after a leading term starts an operand.
fn starts_operand(after: &str) -> bool {
    after.trim_start().chars().next().is_some_and(|c| {
        c.is_alphanumeric() || matches!(c, '_' | '$' | '(' | '[' | '"' | '\'' | '`')
    })
}

/// The text after a trailing term continues with an operator that binds no
/// tighter than `+`, or ends the expression.
fn closes_term(after: &str) -> bool {
    let rest = after.trim_start();
    if rest.starts_with("**") {
        return false;
    }
    rest.chars().next().is_none_or(|c| {
        matches!(
            c,
            ';' | ',' | ')' | ']' | '}' | ':' | '?' | '+' | '-' | '<' | '>' | '=' | '!' | '&' | '|'
        )
    })
}

/// `true` when parameter `name` holds the call's argument everywhere in the
/// owner body: the body has no nested function (whose parameter could shadow
/// it), never assigns, increments or iterates into `name`, never
/// destructures into it, and declares nothing that mentions it.
pub(crate) fn ts_parameter_is_stable(owner_source: &str, name: &str) -> bool {
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
    // `for (name of xs)` / `for (name in o)` assign without `=`.
    for at in identifier_offsets(&body, "for") {
        let header = &body[at + "for".len()..];
        let end = header.find(')').unwrap_or(header.len());
        if !identifier_offsets(&header[..end], name).is_empty() {
            return false;
        }
    }
    identifier_offsets(&body, name).into_iter().all(|at| {
        let after = body[at + name.len()..].trim_start();
        let before = body[..at].trim_end();
        let increments = after.starts_with("++")
            || after.starts_with("--")
            || before.ends_with("++")
            || before.ends_with("--");
        // `fee! = 0` assigns through a non-null assertion.
        let target = after
            .strip_prefix('!')
            .filter(|rest| !rest.starts_with('='))
            .map_or(after, str::trim_start);
        !increments && !assigns_after(target) && !inside_destructuring_target(&body, at)
    })
}

/// RIPR-SPEC-0246 rule 3: `true` when the masked text `masked` changes a
/// member of `name` in place: it writes one (`name.k = v`, `name[k] += v`,
/// `name.k++`), deletes one (`delete name.k`), or hands `name` to
/// `Object.assign` / `Object.defineProperty`. With `any_call`, passing `name`
/// whole to any call counts too, since the callee could write a member.
pub(crate) fn ts_members_written(masked: &str, name: &str, any_call: bool) -> bool {
    identifier_offsets(masked, name).into_iter().any(|at| {
        let before = masked[..at].trim_end();
        if before
            .strip_suffix("delete")
            .is_some_and(|head| !head.ends_with(is_identifier_part))
            || before.ends_with("++")
            || before.ends_with("--")
        {
            return true;
        }
        let after = &masked[at + name.len()..];
        if let Some(rest) = after_member_path(after) {
            let rest = rest.trim_start();
            return rest.starts_with("++") || rest.starts_with("--") || assigns_after(rest);
        }
        call_argument_callee(masked, at).is_some_and(|callee| {
            any_call || callee == "Object.assign" || callee == "Object.defineProperty"
        })
    })
}

/// RIPR-SPEC-0246 rule 3: `true` when every mention of the test local `name`
/// in the masked test body only reads it: its `const` declaration, a member
/// read (`name.k`, `name?.k`, `name[k]`, never written, incremented or
/// deleted), or a whole argument of `expect`, of a matcher (`.toEqual(name)`)
/// or of a call named in `callees` (the owner and its renames, bare or
/// qualified). Any other mention (a cast, an alias, an array or object
/// entry, a helper call, a callback parameter) could hand the call another
/// value, so it fails the read.
pub(crate) fn ts_local_only_read(masked: &str, name: &str, callees: &[&str]) -> bool {
    identifier_offsets(masked, name).into_iter().all(|at| {
        let before = masked[..at].trim_end();
        if before
            .strip_suffix("const")
            .is_some_and(|head| !head.ends_with(is_identifier_part))
        {
            return true;
        }
        if before
            .strip_suffix("delete")
            .is_some_and(|head| !head.ends_with(is_identifier_part))
            || before.ends_with("++")
            || before.ends_with("--")
        {
            return false;
        }
        let after = &masked[at + name.len()..];
        if let Some(rest) = after_member_path(after) {
            let rest = rest.trim_start();
            return !(rest.starts_with("++") || rest.starts_with("--") || assigns_after(rest));
        }
        let whole = (before.ends_with('(') || before.ends_with(','))
            && after.trim_start().starts_with([',', ')']);
        whole
            && call_argument_callee(masked, at).is_some_and(|callee| {
                callee == "expect"
                    || callee.starts_with('.')
                    || callees.iter().any(|owner| {
                        callee == *owner
                            || callee
                                .strip_suffix(owner)
                                .is_some_and(|head| head.ends_with('.'))
                    })
            })
    })
}

/// The text after a non-empty member path (`.k`, `?.k`, `[e]`, `!`, in any
/// order) at the start of `after`, or `None` when no member follows.
fn after_member_path(after: &str) -> Option<&str> {
    let mut rest = after;
    let mut stepped = false;
    loop {
        let trimmed = rest.trim_start();
        let next = if let Some(tail) = trimmed.strip_prefix("?.") {
            if tail.trim_start().starts_with('[') {
                tail.trim_start()
            } else {
                skip_identifier(tail.trim_start())?
            }
        } else if let Some(tail) = trimmed.strip_prefix('.') {
            skip_identifier(tail.trim_start())?
        } else if trimmed.starts_with('[') {
            let mut depth = 0usize;
            let end = trimmed.char_indices().find_map(|(index, c)| {
                match c {
                    '[' | '(' | '{' => depth += 1,
                    ']' | ')' | '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(index + 1);
                        }
                    }
                    _ => {}
                }
                None
            })?;
            &trimmed[end..]
        } else if trimmed.starts_with('!') && !trimmed.starts_with("!=") {
            &trimmed[1..]
        } else {
            return stepped.then_some(rest);
        };
        stepped = true;
        rest = next;
    }
}

fn skip_identifier(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .find(|(_, c)| !is_identifier_part(*c))
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then(|| &text[end..])
}

/// The callee (`f`, `a.b`) of the call whose argument list holds the name at
/// `at` directly (not inside an object, array or nested function), or `None`
/// when the name is no call argument. `if (x)`, `while (x)` and the like are
/// no calls.
fn call_argument_callee(masked: &str, at: usize) -> Option<&str> {
    let mut depth = 0usize;
    let mut open = None;
    for (index, c) in masked[..at].char_indices().rev() {
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' if depth == 0 => {
                open = Some(index);
                break;
            }
            '[' | '{' if depth == 0 => return None,
            '(' | '[' | '{' => depth -= 1,
            ';' if depth == 0 => return None,
            _ => {}
        }
    }
    let head = masked[..open?].trim_end();
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_identifier_part(*c) || *c == '.')
        .last()
        .map(|(index, _)| index)?;
    let callee = &head[start..];
    (!matches!(
        callee,
        "if" | "while" | "for" | "switch" | "catch" | "return" | "typeof" | "await"
    ) && !callee.is_empty())
    .then_some(callee)
}

/// `true` when `after` (the text after a name) opens with an assignment
/// operator: `=`, a compound `op=`, or a logical `&&=` / `||=` / `??=`,
/// however the right-hand side starts (`fee=-0` assigns).
fn assigns_after(after: &str) -> bool {
    // An assignment operator is at most four characters (`>>>=`).
    let Some(equals) = after
        .char_indices()
        .take(4)
        .find(|(_, c)| *c == '=')
        .map(|(i, _)| i)
    else {
        return false;
    };
    let head = &after[..equals];
    let compound = matches!(
        head,
        "" | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "**"
            | "<<"
            | ">>"
            | ">>>"
            | "&"
            | "|"
            | "^"
            | "&&"
            | "||"
            | "??"
    );
    compound && !after[equals + 1..].starts_with(['=', '>'])
}

/// `true` when the name at `at` sits inside any brackets or parentheses
/// that an assignment follows before the statement ends (`[fee] = pair`,
/// `[a, [fee]] = x`, `({ a: { fee } } = o)`, `(fee as number) = 0`).
fn inside_destructuring_target(body: &str, at: usize) -> bool {
    let mut depth = 0usize;
    for (index, c) in body[at..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    if assigns_after(body[at + index + 1..].trim_start()) {
                        return true;
                    }
                } else {
                    depth -= 1;
                }
            }
            ';' if depth == 0 => return false,
            _ => {}
        }
    }
    false
}
