//! Change shapes whose infection the TypeScript preview adapter cannot model
//! (#10704).
//!
//! The adapter has no value-level infection model: whether a test input makes
//! the changed value differ is only decided for the shapes the classifier
//! guards read (boundary inputs, untaken `if` arms, ...). A literal return
//! value, a default parameter, a `??` / `||` fallback operand, a comment-only
//! edit or an arithmetic no-op therefore cannot be called `weakly_exposed` on
//! oracle strength alone: that reading asks for an assertion on the changed
//! expression text. This module names those shapes from the old and new line
//! so the classifier can fail closed to `infection_unknown`.
//!
//! Every function here is conservative: `None` keeps the ladder verdict, and a
//! shape is named only when the old and new lines differ in exactly that
//! position and nowhere else.

/// An edit whose effect on the returned value the adapter does not compute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnmodeledInfection {
    /// Stable shape name, spelled in the `typescript_infection_shape:` line.
    pub(crate) shape: &'static str,
    /// Why infection is not established for this shape.
    pub(crate) reason: &'static str,
    /// The edit is behavior-neutral by construction (a comment, `+ 0`), so
    /// even an `exposed` ladder verdict is not an infection claim.
    pub(crate) behavior_neutral: bool,
}

/// The unmodeled shape the edit from `old` to `new` has, if any.
///
/// `predicate_seam` is true when the probe is a predicate: a literal operand
/// there is a boundary the boundary-input facts model, so no fallback shape is
/// named. A line with a JSX tag, a regex literal or an unterminated quote
/// (a multi-line template body) is declined: its text cannot be split into
/// code and comment reliably.
pub(crate) fn ts_unmodeled_infection(
    old: Option<&str>,
    new: &str,
    predicate_seam: bool,
) -> Option<UnmodeledInfection> {
    let old = old?;
    if old.trim() == new.trim() || is_undecidable_line(old) || is_undecidable_line(new) {
        return None;
    }
    let (old_code, new_code) = (code_of(old), code_of(new));
    if old_code == new_code && !new_code.is_empty() {
        return Some(UnmodeledInfection {
            shape: "comment_only",
            reason: "the edit changes only a comment, so no input changes the executed code; no infection exists to establish",
            behavior_neutral: true,
        });
    }
    if is_arithmetic_noop(&old_code, &new_code) {
        return Some(UnmodeledInfection {
            shape: "arithmetic_noop",
            reason: "the edit adds or drops an identity operand (`+ 0`, `- 0`, `* 1`, `/ 1`), which the adapter does not fold; whether any input changes the value is not established (a string `+ 0` or a BigInt operand still changes behavior)",
            behavior_neutral: false,
        });
    }
    if !predicate_seam && fallback_literal_changed(&old_code, &new_code) {
        return Some(UnmodeledInfection {
            shape: "fallback_literal",
            reason: "the edit changes the literal operand of a `??` / `||` fallback, and the adapter does not model whether any test input takes the fallback",
            behavior_neutral: false,
        });
    }
    if default_parameter_changed(&old_code, &new_code) {
        return Some(UnmodeledInfection {
            shape: "default_parameter",
            reason: "the edit changes a default parameter value, and the adapter does not model whether any test call omits the argument",
            behavior_neutral: false,
        });
    }
    if literal_return_changed(&old_code, &new_code) {
        return Some(UnmodeledInfection {
            shape: "literal_return",
            reason: "the edit changes a literal return value, and the adapter does not model whether any test input reaches this return",
            behavior_neutral: false,
        });
    }
    None
}

/// The line with a trailing `//` comment and a trailing `;` removed. Quotes
/// are tracked so a `//` inside a string literal is kept.
fn code_of(line: &str) -> String {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut cut = line.len();
    let mut previous: Option<char> = None;
    for (index, ch) in line.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
        } else if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
        } else if line[index..].starts_with("//") && previous.is_none_or(char::is_whitespace) {
            // A comment opens at line start or after whitespace; `a//b`,
            // `http://x` and `/\//g` are code.
            cut = index;
            break;
        }
        previous = Some(ch);
    }
    let code = line[..cut].trim();
    code.strip_suffix(';')
        .unwrap_or(code)
        .trim_end()
        .to_string()
}

/// Lines whose code/comment split cannot be read reliably: an unterminated
/// quote (the middle of a multi-line template literal), a JSX or generic
/// `<tag`, or a regex literal.
fn is_undecidable_line(line: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut previous_code: Option<char> = None;
    let chars: Vec<char> = line.chars().collect();
    for (index, &ch) in chars.iter().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '<' if chars
                .get(index + 1)
                .is_some_and(|next| next.is_alphabetic() || *next == '/' || *next == '>') =>
            {
                return true;
            }
            // A `/` after an operator or opener starts a regex literal.
            '/' if chars.get(index + 1) != Some(&'/')
                && previous_code.is_none_or(|p| "(,=:[!&|?{};+-*%~^".contains(p)) =>
            {
                return true;
            }
            _ => {}
        }
        if !ch.is_whitespace() {
            previous_code = Some(ch);
        }
    }
    quote.is_some()
}

fn is_arithmetic_noop(old: &str, new: &str) -> bool {
    const IDENTITY: [&str; 4] = [" + 0", " - 0", " * 1", " / 1"];
    IDENTITY.iter().any(|suffix| {
        (new.strip_suffix(suffix) == Some(old) || old.strip_suffix(suffix) == Some(new))
            && !old.is_empty()
            && !new.is_empty()
    })
}

fn is_literal(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }
    if matches!(token, "true" | "false" | "null" | "undefined") {
        return true;
    }
    let unsigned = token.strip_prefix('-').unwrap_or(token);
    if unsigned.starts_with(|c: char| c.is_ascii_digit()) {
        return unsigned.replace('_', "").parse::<f64>().is_ok();
    }
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let last = chars.next_back().unwrap_or(first);
    if token.len() < 2
        || first != last
        || !matches!(first, '"' | '\'' | '`')
        || (first == '`' && token.contains("${"))
    {
        return false;
    }
    // The body may hold the quote only when escaped.
    let mut escaped = false;
    for ch in token[1..token.len() - 1].chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == first {
            return false;
        }
    }
    !escaped
}

/// Split `text` at its first top-level literal token: `(literal, rest)`. The
/// literal runs to the first `,`, `)`, `]`, `}` or `;` outside quotes.
fn leading_literal(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut end = text.len();
    for (index, ch) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                // The opening quote closes: the literal ends here.
                end = index + ch.len_utf8();
                break;
            }
        } else if index == 0 && matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
        } else if matches!(ch, ',' | ')' | ']' | '}' | ';') || ch.is_whitespace() {
            end = index;
            break;
        }
    }
    let (literal, rest) = text.split_at(end);
    is_literal(literal).then_some((literal, rest))
}

/// Byte offset of the first (or last) occurrence of `pattern` outside string
/// literals.
fn find_outside_quotes(line: &str, pattern: &str, last: bool) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut found = None;
    for (index, ch) in line.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
        } else if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
        } else if line[index..].starts_with(pattern) {
            if !last {
                return Some(index);
            }
            found = Some(index);
        }
    }
    found
}

fn fallback_literal_changed(old: &str, new: &str) -> bool {
    ["??", "||"].iter().any(|operator| {
        let (Some(o), Some(n)) = (
            find_outside_quotes(old, operator, true),
            find_outside_quotes(new, operator, true),
        ) else {
            return false;
        };
        if old[..o] != new[..n] {
            return false;
        }
        match (
            leading_literal(&old[o + operator.len()..]),
            leading_literal(&new[n + operator.len()..]),
        ) {
            (Some((old_literal, old_rest)), Some((new_literal, new_rest))) => {
                old_literal != new_literal && old_rest == new_rest
            }
            _ => false,
        }
    })
}

fn default_parameter_changed(old: &str, new: &str) -> bool {
    let (Some(old_open), Some(new_open)) = (
        find_outside_quotes(old, "(", false),
        find_outside_quotes(new, "(", false),
    ) else {
        return false;
    };
    if old[..old_open] != new[..new_open] {
        return false;
    }
    if !is_declaration_head(&old[..old_open], &old[old_open..]) {
        return false;
    }
    let (Some(old_close), Some(new_close)) = (
        find_outside_quotes(old, ")", false),
        find_outside_quotes(new, ")", false),
    ) else {
        return false;
    };
    if old_close < old_open || new_close < new_open || old[old_close..] != new[new_close..] {
        return false;
    }
    let old_params: Vec<&str> = old[old_open + 1..old_close].split(',').collect();
    let new_params: Vec<&str> = new[new_open + 1..new_close].split(',').collect();
    if old_params.len() != new_params.len() {
        return false;
    }
    let mut changed = false;
    for (o, n) in old_params.iter().zip(&new_params) {
        if o.trim() == n.trim() {
            continue;
        }
        let (Some((o_head, o_value)), Some((n_head, n_value))) =
            (o.split_once(" = "), n.split_once(" = "))
        else {
            return false;
        };
        if o_head.trim() != n_head.trim() || !is_literal(o_value) || !is_literal(n_value) {
            return false;
        }
        changed = true;
    }
    changed
}

fn literal_return_changed(old: &str, new: &str) -> bool {
    match (
        old.trim().strip_prefix("return "),
        new.trim().strip_prefix("return "),
    ) {
        (Some(o), Some(n)) => is_literal(o) && is_literal(n) && o.trim() != n.trim(),
        _ => false,
    }
}

/// A function or method declaration (`function f(`, `(a = 1) =>`,
/// `method(a = 1) {`), not a call such as `f(a = 1)`.
fn is_declaration_head(head: &str, from_open: &str) -> bool {
    let head = head.trim_end();
    if head.contains("function") {
        return true;
    }
    let tail = from_open
        .find(')')
        .map_or("", |close| from_open[close + 1..].trim());
    if tail.starts_with("=>") || tail.starts_with(": ") && tail.contains("=>") {
        return true;
    }
    let control = ["if", "while", "for", "switch", "catch", "with"];
    tail.ends_with('{') && !control.iter().any(|keyword| head.ends_with(keyword))
}
