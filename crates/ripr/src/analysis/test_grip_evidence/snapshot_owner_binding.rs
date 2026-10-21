//! Owner-result binding for snapshot oracles on return-value seams.
//!
//! A snapshot observes only the value it names (#8120/#8121). The seam-kind
//! match that admits `OracleKind::Snapshot` for a `ReturnValue` seam cannot
//! tell a snapshot of the owner's result from a snapshot of some other value
//! in the same test, so the snapshot discriminates the seam only when it
//! names the owner call or a plain local bound to a direct owner call.
//! Everything else stays unconfirmed: under-credit, never over-credit.

use super::call_text_contains_named_call;
use super::related_tests::strip_comments_and_strings;
use crate::analysis::facts::{OracleFact, TestSummary};

/// Whether `oracle` (a snapshot) observes the result of `owner_name`.
pub(super) fn snapshot_observes_owner_result(
    owner_name: &str,
    oracle: &OracleFact,
    test: &TestSummary,
) -> bool {
    if owner_name.is_empty() {
        return false;
    }
    let snapshot = strip_oracle_text(&oracle.text);
    if call_text_contains_named_call(&snapshot, owner_name) {
        return true;
    }
    free_identifiers(&snapshot)
        .iter()
        .any(|name| name_is_direct_owner_result(name, owner_name, oracle, test))
}

fn strip_oracle_text(text: &str) -> String {
    text.lines()
        .map(strip_comments_and_strings)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Identifiers read as plain names: not a field/method (`.x`), a path
/// segment (`a::x`), a macro or a callee.
fn free_identifiers(code: &str) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_identifier_byte(bytes[index]) {
            index += 1;
        }
        let before = code[..start].trim_end().chars().next_back();
        let after = code[index..].trim_start().chars().next();
        let qualified = matches!(before, Some('.') | Some(':'));
        let callee_or_macro = matches!(after, Some('(') | Some('!') | Some(':'));
        if !qualified && !callee_or_macro {
            names.push(code[start..index].to_string());
        }
    }
    names
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// How strictly a `let` initializer must name the owner call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingReach {
    /// The initializer is the owner call itself, with an optional `?` or
    /// `.unwrap()`; a `mut` binding withdraws the credit.
    DirectOwnerCall,
    /// The initializer contains an owner call anywhere and the binding may be
    /// `mut`; an assignment, shadow or pattern binder still withdraws it.
    OwnerCallInInitializer,
}

/// The latest statement before `oracle_line` that introduces or reassigns
/// `name` must be a plain `let name = <expr naming the owner call>;`.
/// A `mut` binding, a destructuring pattern, a compound assignment, or a
/// rebinding to anything else withdraws the credit.
fn name_is_direct_owner_result(
    name: &str,
    owner_name: &str,
    oracle: &OracleFact,
    test: &TestSummary,
) -> bool {
    let oracle_line = oracle.line;
    // The assertion's own line is read up to the assertion macro, so a
    // shadow or closure parameter earlier on that line counts.
    let macro_name = oracle.text.trim().split('!').next().unwrap_or("").trim();
    // Literals and comments are blanked over the whole body first: a `}` or
    // `;` on its own line inside a multi-line string is not code.
    let body = blank_literals_and_comments(&test.body);
    let before: Vec<String> = body
        .lines()
        .enumerate()
        .take_while(|(offset, _)| test.start_line + offset <= oracle_line)
        .map(|(offset, line)| {
            if test.start_line + offset < oracle_line {
                return strip_comments_and_strings(line);
            }
            match line.find(macro_name).filter(|_| !macro_name.is_empty()) {
                Some(at) => strip_comments_and_strings(&line[..at]),
                None => String::new(),
            }
        })
        .collect();
    walk_bindings(
        &before.join("\n"),
        name,
        owner_name,
        BindingReach::DirectOwnerCall,
        true,
    )
}

/// Whether the live binding of `name` at the end of `code` is bound to the
/// owner's result. `code` is Rust with comments and strings already masked and
/// ends exactly where the asserting operation begins, so a binding that
/// follows the assertion, sits in a closed block, or is replaced by a later
/// `let` or assignment (even on the same line) never reaches it. A statement
/// still open at the cut is judged too.
pub(crate) fn local_is_owner_result_before(
    code: &str,
    name: &str,
    owner_name: &str,
    reach: BindingReach,
) -> bool {
    walk_bindings(code, name, owner_name, reach, true)
}

/// The statement in progress. `skeleton` elides what sits inside braces
/// (`{}`), so a binder inside a closure body is not read as a binder of the
/// enclosing statement; `full` keeps everything, for the initializer and the
/// pattern of a `let`.
#[derive(Default)]
struct Pending {
    skeleton: String,
    full: String,
}

impl Pending {
    fn push(&mut self, ch: char) {
        self.skeleton.push(ch);
        self.full.push(ch);
    }

    fn is_blank(&self) -> bool {
        self.skeleton.trim().is_empty()
    }

    fn starts_with_let(&self) -> bool {
        without_attributes(self.skeleton.trim_start()).starts_with("let ")
    }
}

/// One open `{`: the enclosing statement it interrupts.
struct OpenBrace {
    /// The enclosing statement when this brace is part of it (a closure or
    /// `async` body, a struct literal or block in a `let` initializer) with
    /// its brace depth and parenthesis nesting; `None` for a plain block,
    /// which ends its header statement.
    resumes: Option<(Pending, usize, usize)>,
    /// A struct-like or pattern brace: its text stays visible to binder
    /// detection (`Foo { x }`), unlike a closure body.
    keep_text: bool,
    /// The body of a `match`: a `{` before an arm's `=>` is a pattern.
    in_match: bool,
    /// The enclosing brace level's text so far.
    outer_text: String,
}

/// Walk `code` statement by statement, keeping the bindings of `name` in
/// scope, innermost last, each with the brace depth it was introduced at.
/// Every `{` opens a scope, wherever it sits (a block, a closure or `async`
/// body inside a call, a block in a `let` initializer): its statements are
/// judged at their own `;`, and its closing brace drops the bindings made
/// inside it so a shadow in a finished block never masks the outer value.
/// Only parentheses and brackets (an argument list, an array repeat `[x; n]`)
/// keep a `;` from ending a statement.
fn walk_bindings(
    code: &str,
    name: &str,
    owner_name: &str,
    reach: BindingReach,
    judge_open_statement: bool,
) -> bool {
    let mut scope: Vec<(usize, bool)> = Vec::new();
    let mut depth = 0usize;
    let mut statement_depth = 0usize;
    // Open parentheses and brackets of the statement in progress.
    let mut nest = 0usize;
    let mut statement = Pending::default();
    let mut text = String::new();
    let mut open: Vec<OpenBrace> = Vec::new();
    let judge =
        |statement: &mut Pending, statement_depth: usize, scope: &mut Vec<(usize, bool)>| {
            match statement_binding_verdict(
                &statement.skeleton,
                &statement.full,
                name,
                owner_name,
                reach,
            ) {
                Some(BindingVerdict::Scoped(owner_bound)) => {
                    scope.retain(|(bound_at, _)| *bound_at < statement_depth);
                    scope.push((statement_depth, owner_bound));
                }
                // A reassignment or a pattern binder we do not scope-track may
                // leave any enclosing binding changed after its block ends.
                Some(BindingVerdict::Withdrawn) => {
                    for (_, owner_bound) in scope.iter_mut() {
                        *owner_bound = false;
                    }
                    scope.push((statement_depth, false));
                }
                None => {}
            }
            *statement = Pending::default();
        };
    for ch in code.chars() {
        if statement.is_blank() {
            statement_depth = depth;
        }
        match ch {
            '{' => {
                let in_arm_pattern = open.last().is_some_and(|brace| brace.in_match)
                    && !statement
                        .skeleton
                        .rsplit(',')
                        .next()
                        .unwrap_or("")
                        .contains("=>");
                let pattern_brace = in_arm_pattern || in_pattern_position(&statement.skeleton);
                let part_of_statement = nest > 0
                    || statement.starts_with_let()
                    || pattern_brace
                    || struct_like_statement(&statement.skeleton);
                let keep_text = pattern_brace || ends_with_type_like_name(&statement.skeleton);
                let in_match = contains_word(&statement.skeleton, "match");
                let resumes = if part_of_statement {
                    // A binder in the header (a closure parameter, `if let
                    // Some(x) = ..` in an argument or initializer) is in scope
                    // inside the body only, and the body is where the cut may
                    // fall: the statement is judged again, whole, at its `;`.
                    // This holds even when the enclosing `let` binds the same
                    // name (`let x = |x| {`), which its own verdict calls a
                    // plain shadow.
                    if header_binds(&statement.skeleton, name)
                        || matches!(
                            statement_binding_verdict(
                                &statement.skeleton,
                                &statement.full,
                                name,
                                owner_name,
                                reach,
                            ),
                            Some(BindingVerdict::Withdrawn)
                        )
                    {
                        scope.push((depth + 1, false));
                    }
                    // Keep what precedes the brace; the statement resumes at
                    // the matching `}`.
                    Some((std::mem::take(&mut statement), statement_depth, nest))
                } else {
                    statement.push(ch);
                    judge(&mut statement, statement_depth, &mut scope);
                    None
                };
                open.push(OpenBrace {
                    resumes,
                    keep_text,
                    in_match,
                    outer_text: std::mem::take(&mut text),
                });
                nest = 0;
                depth += 1;
                continue;
            }
            '}' => {
                // The unterminated tail of the block is a statement too.
                judge(&mut statement, statement_depth, &mut scope);
                depth = depth.saturating_sub(1);
                scope.retain(|(bound_at, _)| *bound_at <= depth);
                let inner = std::mem::take(&mut text);
                match open.pop() {
                    Some(brace) => {
                        text = format!("{}{{{}}}", brace.outer_text, inner);
                        if let Some((mut before, before_depth, before_nest)) = brace.resumes {
                            if brace.keep_text {
                                before.skeleton.push_str(&format!("{{{inner}}}"));
                            } else {
                                before.skeleton.push_str("{}");
                            }
                            before.full.push_str(&format!("{{{inner}}}"));
                            statement = before;
                            statement_depth = before_depth;
                            nest = before_nest;
                        } else {
                            nest = 0;
                        }
                    }
                    None => nest = 0,
                }
                continue;
            }
            '(' | '[' => {
                nest += 1;
                statement.push(ch);
            }
            ')' | ']' => {
                nest = nest.saturating_sub(1);
                statement.push(ch);
            }
            ';' => {
                statement.push(ch);
                if nest == 0 {
                    judge(&mut statement, statement_depth, &mut scope);
                }
            }
            _ => statement.push(ch),
        }
        text.push(ch);
    }
    if judge_open_statement {
        // A `let` still open has not bound its name yet: the initializer
        // (where the cut falls) reads the previous binding. The initializer
        // itself can still shadow or rewrite it (`let x = |x|`,
        // `let x = (mem::replace(&mut x, 1),`), which is read from it alone.
        let pending_let = statement.starts_with_let()
            && !matches!(
                statement_binding_verdict(
                    &statement.skeleton,
                    &statement.full,
                    name,
                    owner_name,
                    reach,
                ),
                Some(BindingVerdict::Withdrawn)
            );
        if !pending_let {
            judge(&mut statement, statement_depth, &mut scope);
        } else if initializer_binds(&statement.skeleton, name) {
            for (_, owner_bound) in scope.iter_mut() {
                *owner_bound = false;
            }
            scope.push((statement_depth, false));
        }
    }
    scope.last().is_some_and(|(_, owner_bound)| *owner_bound)
}

/// `text` with string, raw-string, char and comment content removed. Every
/// newline is kept so line offsets still match the original source.
fn blank_literals_and_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        let next = chars.get(at + 1).copied();
        if ch == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            at += 2;
            let mut depth = 1usize;
            while at < chars.len() && depth > 0 {
                if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    at += 2;
                    continue;
                }
                if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
                    depth += 1;
                    at += 2;
                    continue;
                }
                if chars[at] == '\n' {
                    out.push('\n');
                }
                at += 1;
            }
            continue;
        }
        let word_start =
            at == 0 || !(chars[at - 1].is_ascii_alphanumeric() || chars[at - 1] == '_');
        // `r"`, `br"` and `cr"` raw strings: a `b`/`c` prefix must not leave
        // the raw content to the escape-aware plain-string branch.
        let raw_at = if (ch == 'b' || ch == 'c') && next == Some('r') {
            at + 1
        } else {
            at
        };
        if chars[raw_at] == 'r' && word_start {
            let mut quote = raw_at + 1;
            while chars.get(quote) == Some(&'#') {
                quote += 1;
            }
            if chars.get(quote) == Some(&'"') {
                let hashes = quote - raw_at - 1;
                at = quote + 1;
                while at < chars.len() {
                    if chars[at] == '"' && (1..=hashes).all(|n| chars.get(at + n) == Some(&'#')) {
                        at += 1 + hashes;
                        break;
                    }
                    if chars[at] == '\n' {
                        out.push('\n');
                    }
                    at += 1;
                }
                continue;
            }
        }
        if ch == '"' {
            at += 1;
            while at < chars.len() {
                match chars[at] {
                    '\\' => {
                        if chars.get(at + 1) == Some(&'\n') {
                            out.push('\n');
                        }
                        at += 2;
                        continue;
                    }
                    '"' => {
                        at += 1;
                        break;
                    }
                    '\n' => out.push('\n'),
                    _ => {}
                }
                at += 1;
            }
            continue;
        }
        if ch == '\'' {
            if next == Some('\\') {
                // Skip the opening quote, the backslash and the escaped
                // character first, so `'\''` does not stop on its own escape.
                at += 3;
                while at < chars.len() && chars[at] != '\'' {
                    at += 1;
                }
                at += 1;
                continue;
            }
            if chars.get(at + 2) == Some(&'\'') {
                at += 3;
                continue;
            }
        }
        out.push(ch);
        at += 1;
    }
    out
}

enum BindingVerdict {
    /// A `let` of `name`: scoped to its block.
    Scoped(bool),
    /// A reassignment or other binder of `name`: withdraws all credit.
    Withdrawn,
}

/// `Scoped(true)` for a plain owner binding of `name`, `Scoped(false)` for a
/// `let` that shadows it with anything else, `Withdrawn` for a reassignment
/// or a pattern binder (`if let`, `while let`, `for`, match arm, closure
/// parameter), `None` when the statement does not bind `name`.
fn statement_binding_verdict(
    skeleton: &str,
    full: &str,
    name: &str,
    owner_name: &str,
    reach: BindingReach,
) -> Option<BindingVerdict> {
    let trimmed = without_attributes(skeleton.trim());
    if let Some(rest) = without_attributes(full.trim()).strip_prefix("let ") {
        let (pattern, initializer) = match rest.split_once('=') {
            Some((pattern, initializer)) => (pattern.trim(), Some(initializer)),
            None => (rest.trim_end_matches(';').trim(), None),
        };
        if !contains_word(pattern, name) {
            // Binders are read outside braces: a closure parameter, not a
            // pattern in the closure's body.
            let outside = trimmed.strip_prefix("let ").unwrap_or(rest);
            return (binds_by_pattern(outside, name) || borrows_mutably(outside, name))
                .then_some(BindingVerdict::Withdrawn);
        }
        let bound = pattern
            .split_once(':')
            .map_or(pattern, |(pattern, _)| pattern.trim());
        let plain = match reach {
            BindingReach::DirectOwnerCall => bound == name,
            BindingReach::OwnerCallInInitializer => {
                bound == name
                    || bound
                        .strip_prefix("mut ")
                        .is_some_and(|bound| bound.trim() == name)
            }
        };
        return Some(BindingVerdict::Scoped(
            plain
                && initializer.is_some_and(|initializer| match reach {
                    BindingReach::DirectOwnerCall => {
                        initializer_is_owner_call(initializer, owner_name)
                    }
                    BindingReach::OwnerCallInInitializer => text_calls(initializer, owner_name),
                }),
        ));
    }
    if binds_by_pattern(trimmed, name) || borrows_mutably(trimmed, name) {
        return Some(BindingVerdict::Withdrawn);
    }
    assignment_target(trimmed)
        .filter(|target| contains_word(target, name))
        .map(|_| BindingVerdict::Withdrawn)
}

/// `if let` / `while let` / `for` headers, match arms and closure
/// parameters that name `name`. Over-matching only withdraws credit.
fn binds_by_pattern(statement: &str, name: &str) -> bool {
    if !contains_word(statement, name) {
        return false;
    }
    let header = statement.trim_start_matches('}').trim_start();
    let header = header.strip_prefix("else ").unwrap_or(header);
    if header.starts_with("if let ")
        || header.starts_with("while let ")
        || header.starts_with("for ")
        || ["if let ", "while let ", "for "].iter().any(|keyword| {
            // The binder can sit inside an argument list or an initializer.
            statement.match_indices(keyword).any(|(at, _)| {
                !statement[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            })
        })
    {
        return true;
    }
    // A `let` anywhere but at the start is a let chain (`c && let Some(x) =
    // ..`) or a binder in an initializer.
    let body = statement.strip_prefix("let ").unwrap_or(statement);
    if body.match_indices("let ").any(|(at, _)| {
        !body[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }) {
        return true;
    }
    if statement.contains("=>") {
        let mut arms: Vec<&str> = statement.split("=>").collect();
        arms.pop();
        if arms
            .iter()
            .any(|pattern| contains_word(current_arm(pattern), name))
        {
            return true;
        }
    }
    closure_parameters_name(statement, name)
}

/// The pattern of the arm that ends at the `=>` after `piece`: what follows
/// the last top-level `,` (or the `{` that opened the match body), the whole
/// pattern including tuple and struct parts and any guard.
fn current_arm(piece: &str) -> &str {
    let mut stack: Vec<char> = Vec::new();
    let mut start = 0;
    for (at, ch) in piece.char_indices() {
        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' | ']' | '}' => {
                stack.pop();
            }
            ',' if stack.is_empty() => start = at + 1,
            _ => {}
        }
    }
    // An unmatched `{` is the match body itself.
    if let Some(bottom) = stack_bottom_brace(piece) {
        start = start.max(bottom + 1);
    }
    &piece[start..]
}

/// Byte offset of the outermost brace left open at the end of `piece`.
fn stack_bottom_brace(piece: &str) -> Option<usize> {
    let mut stack: Vec<(usize, char)> = Vec::new();
    for (at, ch) in piece.char_indices() {
        match ch {
            '(' | '[' | '{' => stack.push((at, ch)),
            ')' | ']' | '}' => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack
        .first()
        .filter(|(_, ch)| *ch == '{')
        .map(|(at, _)| *at)
}

/// The initializer of an open `let` (what follows its first `=`) binds or
/// mutably borrows `name`.
fn initializer_binds(skeleton: &str, name: &str) -> bool {
    let trimmed = without_attributes(skeleton.trim());
    let Some((_, initializer)) = trimmed.split_once('=') else {
        return false;
    };
    binds_by_pattern(initializer, name) || borrows_mutably(initializer, name)
}

/// A binder of `name` in the header of the brace being opened. The pattern of
/// a `let` is not part of its own initializer (`let x = |Foo { y }| {`).
fn header_binds(skeleton: &str, name: &str) -> bool {
    let trimmed = without_attributes(skeleton.trim());
    match trimmed.strip_prefix("let ") {
        Some(rest) => rest
            .split_once('=')
            .is_some_and(|(_, initializer)| binds_by_pattern(initializer, name)),
        None => binds_by_pattern(trimmed, name),
    }
}

/// A brace after a type-like name outside an item or block header: a struct
/// pattern or literal that continues the statement (`Foo { x, y } = ..;`).
fn struct_like_statement(skeleton: &str) -> bool {
    let trimmed = without_attributes(skeleton.trim());
    let first = trimmed
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .next()
        .unwrap_or("");
    !matches!(
        first,
        "if" | "while"
            | "match"
            | "for"
            | "loop"
            | "else"
            | "unsafe"
            | "impl"
            | "struct"
            | "enum"
            | "trait"
            | "mod"
            | "fn"
            | "pub"
            | "type"
            | "union"
            | "return"
            | "async"
            | "move"
            | "const"
            | "static"
            | "extern"
            | "use"
    ) && ends_with_type_like_name(skeleton)
}

/// A single `|` in `statement` (a name was already found in it): a closure
/// parameter list, a bitwise or, or an or-pattern. They are not told apart, so
/// the statement is read as possibly binding `name`; over-matching only
/// withdraws credit. `||` (an empty parameter list, or a boolean or) binds
/// nothing and does not hide a later single `|`.
fn closure_parameters_name(statement: &str, _name: &str) -> bool {
    let bytes = statement.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'|' {
            at += 1;
        } else if bytes.get(at + 1) == Some(&b'|') {
            at += 2;
        } else {
            return true;
        }
    }
    false
}

/// `statement` ends in a type-like name (`Foo`, `Some`, `Self`): a brace after
/// it is a struct pattern or literal, not a block header (`else`, `loop`,
/// `async`, `unsafe`, `move`, a condition).
fn ends_with_type_like_name(statement: &str) -> bool {
    let mut statement = statement.trim_end();
    // `Foo::<u8> {`: a turbofish between the path and the brace.
    if statement.ends_with('>') {
        let mut angles = 0usize;
        let mut open_at = None;
        for (at, ch) in statement.char_indices().rev() {
            match ch {
                '>' if statement[..at].ends_with('-') => {}
                '>' => angles += 1,
                '<' => {
                    angles = angles.saturating_sub(1);
                    if angles == 0 {
                        open_at = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        match open_at.and_then(|at| statement[..at].strip_suffix("::")) {
            Some(path) => statement = path,
            None => return false,
        }
    }
    let word: String = statement
        .trim_end()
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    word.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// The statement is inside the pattern of an `if let`, `while let` or `for`
/// header: the keyword is open and its `=` (or `in`) has not come yet.
fn in_pattern_position(statement: &str) -> bool {
    ["let ", "for "]
        .iter()
        .filter_map(|keyword| {
            statement
                .match_indices(*keyword)
                .filter(|(at, _)| {
                    !statement[..*at]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                })
                .last()
                .map(|(at, _)| (at, *keyword))
        })
        .max_by_key(|(at, _)| *at)
        .is_some_and(|(at, keyword)| {
            let rest = &statement[at + keyword.len()..];
            if keyword == "for " {
                !contains_word(rest, "in")
            } else {
                !rest.replace("=>", "").replace("==", "").contains('=')
            }
        })
}

/// The initializer's top-level expression is the owner call itself, with an
/// optional `?` / `.unwrap()`: `make_count(2)`, `svc.make_count(2)?`. A call
/// wrapped in another call, comparison or method chain is another value.
fn initializer_is_owner_call(initializer: &str, owner_name: &str) -> bool {
    let text = initializer.trim().trim_end_matches(';').trim();
    let mut search = 0;
    while let Some(found) = text[search..].find(owner_name) {
        let start = search + found;
        search = start + owner_name.len();
        let before_ok = text[..start]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.' | '&'));
        let boundary_ok = !text[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !before_ok || !boundary_ok || !text[search..].trim_start().starts_with('(') {
            continue;
        }
        let args = &text[search..].trim_start();
        let mut depth = 0usize;
        for (at, ch) in args.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        let rest = args[at + 1..].trim();
                        return matches!(rest, "" | "?" | ".unwrap()");
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// The left-hand side of the assignment in `statement` (`x = ..`, `x += ..`,
/// `x[0] = ..`, `(*x).y = ..`, `(x, y) = ..`, `Foo { x, .. } = ..`,
/// `Some(x) = ..`): everything before the first `=` that is not part of a
/// comparison (`==`, `!=`, `<=`, `>=`), a match arm (`=>`) or a range (`..=`).
/// Any left-hand side that names the local is read as possibly rewriting it.
fn assignment_target(statement: &str) -> Option<&str> {
    let bytes = statement.as_bytes();
    let at = (0..bytes.len()).find(|&at| {
        bytes[at] == b'='
            && !matches!(bytes.get(at + 1), Some(b'=') | Some(b'>'))
            && !matches!(
                at.checked_sub(1).map(|prev| bytes[prev]),
                Some(b'=') | Some(b'!') | Some(b'.')
            )
            && !is_comparison_angle(bytes, at)
    })?;
    let target = statement[..at]
        .trim_end_matches(['+', '-', '*', '/', '%', '&', '|', '^', '<', '>'])
        .trim();
    (!target.is_empty()).then_some(target)
}

/// `<=` / `>=` compare; `<<=` / `>>=` assign.
fn is_comparison_angle(bytes: &[u8], at: usize) -> bool {
    match at.checked_sub(1).map(|prev| bytes[prev]) {
        Some(angle @ (b'<' | b'>')) => at.checked_sub(2).map(|prev| bytes[prev]) != Some(angle),
        _ => false,
    }
}

/// `statement` without leading outer attributes (`#[inline] let x = ..`).
fn without_attributes(statement: &str) -> &str {
    let mut rest = statement;
    while let Some(attribute) = rest.strip_prefix("#[") {
        let mut depth = 1usize;
        let Some(end) = attribute.char_indices().find_map(|(at, ch)| {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(at)
        }) else {
            return rest;
        };
        rest = attribute[end + 1..].trim_start();
    }
    rest
}

/// A mutable borrow of `name` in `statement` (`&mut name`, `& mut (name)`,
/// `&raw mut name`, `&mut *name`): the value may be swapped or rewritten
/// through the borrow (`mem::swap(&mut name, ..)`, `fill(&mut name)`).
fn borrows_mutably(statement: &str, name: &str) -> bool {
    statement.match_indices('&').any(|(at, _)| {
        let mut rest = statement[at + 1..].trim_start();
        if let Some(raw) = rest.strip_prefix("raw") {
            rest = raw.trim_start();
        }
        let Some(rest) = rest.strip_prefix("mut") else {
            return false;
        };
        if rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
            return false;
        }
        let rest =
            rest.trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, '(' | '*' | '&'));
        rest.strip_prefix(name).is_some_and(|after| {
            !after.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
        })
    })
}

/// Whether `text` calls `name` (`name(` as a whole identifier).
pub(crate) fn text_calls(text: &str, name: &str) -> bool {
    !name.is_empty()
        && text.match_indices(name).any(|(at, _)| {
            !text[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
                && text[at + name.len()..].trim_start().starts_with('(')
        })
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + word.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::{
        assignment_target, blank_literals_and_comments, initializer_is_owner_call,
        statement_binding_verdict,
    };

    #[test]
    fn reads_and_comparisons_are_not_assignments() {
        for read in [
            "if count == 0 {",
            "debug_assert!(count == 3);",
            "while count >= 1 {",
            "if count <= 1 {",
            "assert!(count != 4);",
            "match count { 1 => {",
        ] {
            assert_eq!(assignment_target(read), None, "{read}");
            assert!(
                statement_binding_verdict(
                    read,
                    read,
                    "count",
                    "make_count",
                    super::BindingReach::DirectOwnerCall
                )
                .is_none(),
                "{read}"
            );
        }
    }

    #[test]
    fn plain_and_compound_assignments_are_assignments() {
        for (write, target) in [
            ("count = 5;", "count"),
            ("count += 1;", "count"),
            ("count <<= 1;", "count"),
            ("*count = 2;", "*count"),
            ("record.count -= 1;", "record.count"),
        ] {
            assert_eq!(assignment_target(write), Some(target), "{write}");
        }
        assert!(matches!(
            statement_binding_verdict(
                "count = 5;",
                "count = 5;",
                "count",
                "make_count",
                super::BindingReach::DirectOwnerCall
            ),
            Some(super::BindingVerdict::Withdrawn)
        ));
    }

    #[test]
    fn pattern_binders_withdraw_and_wrapped_calls_are_not_the_owner_result() {
        for binder in [
            "if let Some(count) = pick() {",
            "for count in items {",
            "Some(count) => {",
            "let f = |count| count + 1;",
        ] {
            let verdict = statement_binding_verdict(
                binder,
                binder,
                "count",
                "make_count",
                super::BindingReach::DirectOwnerCall,
            );
            assert!(
                matches!(
                    verdict,
                    Some(super::BindingVerdict::Withdrawn | super::BindingVerdict::Scoped(false))
                ),
                "{binder}"
            );
        }
        assert!(initializer_is_owner_call(" make_count(2);", "make_count"));
        assert!(initializer_is_owner_call(
            " svc.make_count(2)?;",
            "make_count"
        ));
        for other in [
            " make_count(2) == 3;",
            " unrelated(make_count(2));",
            " make_count(2).to_string();",
        ] {
            assert!(!initializer_is_owner_call(other, "make_count"), "{other}");
        }
    }

    #[test]
    fn escaped_quote_char_literal_is_consumed_whole() {
        for source in [
            "f('\\'','a'); keep();",
            "f('\\'', 'a'); keep();",
            "f('\\n','a'); keep();",
        ] {
            let blanked = blank_literals_and_comments(source);
            assert!(!blanked.contains('\''), "{source} -> {blanked}");
            assert!(blanked.contains("keep();"), "{source} -> {blanked}");
        }
    }

    #[test]
    fn literals_and_comments_are_blanked_across_lines_keeping_newlines() {
        let source =
            "a;\nlet n = \"x\n}\n\";\nlet r = r#\"{\n}\"#; // \"q\nlet c = '\"'; /* {\n} */ b;";
        let blanked = blank_literals_and_comments(source);
        assert_eq!(blanked.matches('\n').count(), source.matches('\n').count());
        assert!(
            !blanked.contains('{') && !blanked.contains('}'),
            "{blanked}"
        );
        assert!(
            blanked.contains("let n =") && blanked.contains("let c =") && blanked.contains("b;")
        );
    }

    #[test]
    fn prefixed_raw_strings_and_nested_comments_are_blanked() {
        for source in [
            "let s = br\"\\\"; let count = other(); keep(); let t = br\"x\";",
            "let s = cr\"\\\"; let count = other(); keep(); let t = cr\"x\";",
            "let s = br#\"\\\"#; let count = other(); keep(); let t = br#\"x\"#;",
        ] {
            let blanked = blank_literals_and_comments(source);
            assert!(
                blanked.contains("let count = other();"),
                "{source} -> {blanked}"
            );
            assert!(blanked.contains("keep();"), "{source} -> {blanked}");
        }
        let blanked = blank_literals_and_comments("a; /* x /* y */ let count = z; */ b;");
        assert!(!blanked.contains("let count"), "{blanked}");
        assert!(
            blanked.contains("a;") && blanked.contains("b;"),
            "{blanked}"
        );
    }

    fn test_with(body: &str) -> crate::analysis::facts::TestSummary {
        crate::analysis::facts::TestSummary {
            name: "t".into(),
            file: std::path::PathBuf::from("t.rs"),
            start_line: 1,
            end_line: 1 + body.lines().count(),
            body: body.into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    /// The strict snapshot grader on `setup` followed by a snapshot of `x` on
    /// its own last line.
    fn strict(setup: &str) -> bool {
        let body = format!("{setup}\ninsta::assert_snapshot!(x);");
        let test = test_with(&body);
        let line = test.start_line + body.lines().count() - 1;
        let oracle = super::OracleFact {
            line,
            text: "insta::assert_snapshot!(x)".to_string(),
            kind: crate::domain::OracleKind::Snapshot,
            strength: crate::domain::OracleStrength::Medium,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        super::name_is_direct_owner_result("x", "own", &oracle, &test)
    }

    /// The same, with the assertion sharing its line with `prefix`.
    fn strict_same_line(setup: &str, prefix: &str) -> bool {
        let body = format!("{setup}\n{prefix}insta::assert_snapshot!(x);");
        let test = test_with(&body);
        let line = test.start_line + body.lines().count() - 1;
        let oracle = super::OracleFact {
            line,
            text: "insta::assert_snapshot!(x)".to_string(),
            kind: crate::domain::OracleKind::Snapshot,
            strength: crate::domain::OracleStrength::Medium,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        super::name_is_direct_owner_result("x", "own", &oracle, &test)
    }

    #[test]
    fn a_shadow_or_parameter_earlier_on_the_assertion_line_is_judged() {
        assert!(!strict_same_line("let x = own(2);", "let x = 7; "));
        assert!(!strict_same_line("let x = own(2);", "run(|x: usize| { "));
        assert!(!strict_same_line(
            "let x = own(2);",
            "if let Some(x) = p { "
        ));
        assert!(strict_same_line("let x = own(2);", ""));
        assert!(strict_same_line("let x = own(2);", "let y = 7; "));
        assert!(strict_same_line("let x = own(2);", "run(|y: usize| { "));
    }

    #[test]
    fn a_shadow_in_a_callback_or_closure_body_is_judged_where_the_assertion_sits() {
        // The assertion is inside the block that rebinds the local.
        for shadowed in [
            "let x = own(2);\napply(|| {\n    let x = 2usize;",
            "let x = own(2);\nlet f = || {\n    let x = 2usize;",
            "let x = own(2);\nstd::thread::spawn(move || {\n    let x = 2usize;",
            "let x = own(2);\nlet fut = async {\n    let x = 2usize;",
            "let x = own(2);\nfoo({\n    let x = 2usize;",
            "let x = own(2);\nlet s = S { a: {\n    let x = 2usize;",
            "let x = own(2);\napply(|| {\n    if c {\n        let x = 2usize;",
            "let x = own(2);\napply(|| { let x = 2usize;",
            // An attribute does not hide the shadowing `let`.
            "let x = own(2);\n#[inline]\nlet x = 2usize;",
            // A closure or `if let` binder of the same name shadows inside the body.
            "let x = own(2);\nrun(|x| {",
            "let x = own(2);\nrun(a, |x| {\n    foo();",
            "let x = own(2);\nlet g = |x| {",
            "let x = own(2);\nitems.iter().for_each(|x| {",
            "let x = own(2);\nitems.iter().for_each(|(i, x)| {",
            "let x = own(2); run(|x| {",
            "let x = own(2);\nfoo(if let Some(x) = p {",
            "let x = own(2);\nlet r = if let Some(x) = p {",
            // `||` earlier in the header does not hide a later parameter list.
            "let x = own(2);\nrun(|| 1, |x| {",
            "let x = own(2);\nrun(a || b, |x| {",
            "let x = own(2);\nrun(move || (), |x| {",
            "let x = own(2);\nrun(|| {\n    foo();\n}, |x| {",
            "let x = own(2);\nrun(|| {\n    a || b;\n}, |x| {",
            "let x = own(2);\nlet r = foo(a || b, |x| {",
            "let x = own(2);\nrun(a, b || c,\n    |x|\n    {",
            // A bitwise or earlier in the header does not split the real `|x|`.
            "let x = own(2);\nrun(a | b, |x| {",
            "let x = own(2);\nrun(FLAG | M, move |x| {",
            "let x = own(2);\nlet r = foo(a | b, |x| -> u8 {",
            "let x = own(2);\nrun(|a| a | 1, |x| {",
            "let x = own(2);\nrun(a & b | c, |x| {",
            "let x = own(2);\nrun(a ^ b | c, |x| {",
            "let x = own(2);\nrun(f(a | b), |x| {",
            "let x = own(2);\nrun([a | b], |x| {",
            "let x = own(2);\nrun(a || b | c, |x| {",
            "let x = own(2);\nrun(a | b || c, |x| {",
            "let x = own(2);\nrun(a | b, c | d, e | f, |x| {",
            "let x = own(2);\nfoo(a, b | c,\n |x|\n {",
            // Struct patterns bind inside braces.
            "let x = own(2);\nif let Foo { x, .. } = foo() {",
            "let x = own(2);\nif let Foo { x } = foo() {",
            "let x = own(2);\nif let Some(Foo { x }) = foo() {",
            "let x = own(2);\nmatch p {\n    Foo { x } =>",
            "let x = own(2);\nmatch p {\n    Some(Foo { x }) => {",
            "let x = own(2);\nmatch p {\n    A => 1,\n    Foo { x } => {",
            "let x = own(2);\nfor Foo { x } in v {",
            "let x = own(2);\nrun(|Foo { x }| {",
            "let x = own(2);\nlet g = |Foo { x }| 1;",
            // The enclosing `let` binds the same name as the header binder.
            "let x = own(2);\nlet x = |x: usize| {",
            "let x = own(2);\nlet x = if let Some(x) = Some(2usize) {",
            // A statement still open at the assertion is judged too.
            "let x = own(2);\nrun(|x|",
            "let x = own(2);\nmatch p {\n    Some(x) =>",
            // A closure body or a borrow that rewrites the binding.
            "let mut x = own(2);\napply(|| { x = 2usize; });",
        ] {
            assert!(!strict(shadowed), "{shadowed}");
        }
        // Controls: the callback is closed before the snapshot, or binds
        // something else, so the owner binding is still the live `x`.
        for live in [
            "let x = own(2);\napply(|| {\n    let x = 2usize;\n});",
            "let x = own(2);\nlet f = || {\n    let x = 2usize;\n};",
            "let x = own(2);\napply(|| {\n    let y = 2usize;\n});",
            // A parameter of another name leaves the owner binding visible.
            "let x = own(2);\nrun(|y| {",
            "let x = own(2);\nrun(a, |y| {\n    foo();",
            "let x = own(2);\nfoo(if let Some(y) = p {",
            "let x = own(2);\nrun(|| 1, |y| {",
            "let x = own(2);\nlet z = |y: usize| {",
            // An unfinished `let x =` has not rebound `x` where its initializer runs.
            "let x = own(2);\nlet x =",
            "let x = own(2);\nlet x = foo(",
            "let x = own(2);\nif let Foo { y } = foo() {",
            "let x = own(2);\nmatch p {\n    Foo { y } => {",
            "let x = own(2);\nrun(a | b, |y| {",
            "let x = own(2);\nif FLAG {\n    foo();\n}\nlet y = 1;",
            "let x = own(2);\nrun(a || b, || {",
            "let x = own(2);\nrun(|| {\n    foo(x);\n});",
            "let x = own(2);\napply(|| {\n    if c {\n        let x = 2usize;\n    }\n});",
            "let x = own(2);",
        ] {
            assert!(strict(live), "{live}");
        }
    }

    #[test]
    fn a_mutable_borrow_or_attribute_prefixed_shadow_withdraws_the_binding() {
        assert!(matches!(
            statement_binding_verdict(
                "std::mem::swap(&mut count, &mut other);",
                "std::mem::swap(&mut count, &mut other);",
                "count",
                "make_count",
                super::BindingReach::OwnerCallInInitializer
            ),
            Some(super::BindingVerdict::Withdrawn)
        ));
        for text in [
            "#[inline] let count = 1;",
            "#[inline]\n#[cfg(test)] let count = 1;",
        ] {
            assert!(
                matches!(
                    statement_binding_verdict(
                        text,
                        text,
                        "count",
                        "make_count",
                        super::BindingReach::DirectOwnerCall
                    ),
                    Some(super::BindingVerdict::Scoped(false))
                ),
                "{text}"
            );
        }
    }

    /// Every binder shape of the local, in every placement, in both modes:
    /// the cut falls where the binder is live, so the owner credit must go.
    /// The same shapes over a fresh name keep it.
    #[test]
    fn a_generated_sweep_of_binder_shapes_and_placements_withholds_credit() {
        let shapes: &[&str] = &[
            "let {n} = |{n}|\n",
            "let {n} = (std::mem::replace(&mut {n}, 1),\n",
            "let {n} = |Foo::<u8> { {n} }| {\n",
            "let f = |Foo::<u8> { {n} }| {\n",
            "run(|Foo::<u8> { {n} }| {\n",
            "run(|module::Foo::<fn() -> u8> { {n} }| {\n",
            "run(|{n}| {\n",
            "run(a, |{n}| {\n",
            "run(a | b, |{n}| {\n",
            "run(|(i, {n})| {\n",
            "let g = |{n}| {\n",
            "let {n} = |{n}: usize| {\n",
            "let {n} = if let Some({n}) = p {\n",
            "foo(if let Some({n}) = p {\n",
            "if let Some({n}) = foo() {\n",
            "if let Foo { {n}, .. } = foo() {\n",
            "while let Some({n}) = it.next() {\n",
            "for {n} in v {\n",
            "for Foo { {n} } in v {\n",
            "match p {\n ({n}, y) => {\n",
            "match p {\n ({n}, y) =>\n",
            "match p {\n A => 1,\n ({n}, y) => {\n",
            "match (a, b) {\n ({n}, y) => {\n",
            "match p {\n ({n}, y)\n => {\n",
            "match p {\n ({n}, _) if c => {\n",
            "match p {\n (1, {n}) if f(1, 2) => {\n",
            "match p {\n Some(({n}, y)) => {\n",
            "match p {\n Foo({n}, y) => {\n",
            "match p {\n [{n}, y] => {\n",
            "match p {\n Foo { {n}, y } => {\n",
            "match p {\n Foo { {n}, .. } => {\n",
            "match p {\n Foo { {n}, y, } => {\n",
            "match p {\n Foo { ref {n}, .. } => {\n",
            "match p {\n Foo {\n a,\n {n},\n } => {\n",
            "match p {\n Foo {\n a,\n {n},\n } =>\n",
            "match p {\n Foo { {n} } if matches!({n}, 1) => {\n",
            "if c && let Some({n}) = foo() {\n",
            "if c && let Foo { {n} } = foo() {\n",
            "while c && let Some({n}) = it.next() {\n",
            "if c {\n} else if c && let Some({n}) = foo() {\n",
            "if c && let Some(a) = f() && let Some({n}) = g() {\n",
            "if let Some(a) = f() && let Foo { {n} } = g() {\n",
            "Foo { {n}, y } = foo();\n",
            "Foo { {n}: {n}, .. } = foo();\n",
            "Foo { y: {n}, .. } = foo();\n",
            "Foo({n}, y) = foo();\n",
            "Foo({n}, _) = foo();\n",
            "Some({n}) = foo();\n",
            "({n}, y) = foo();\n",
            "[{n}, y] = foo();\n",
            "{n} = 5;\n",
            "{n} += 5;\n",
            "{n}[0] = 5;\n",
            "{n}[0] += 5;\n",
            "{n}.f[0] = 5;\n",
            "(*{n}).y = 5;\n",
            "let t = std::mem::take(&mut {n});\n",
            "f(&mut ({n}));\n",
            "f(& mut {n});\n",
            "f(&raw mut {n});\n",
            "swap(&mut *{n}, &mut y);\n",
        ];
        let placements: &[&str] = &[
            "",
            "{\n",
            "if c {\n",
            "run(|| {\n",
            "let r = {\n",
            "unsafe {\n",
            "match p {\n _ => {\n",
        ];
        let modes: &[(&str, super::BindingReach)] = &[
            ("let x = own(2);\n", super::BindingReach::DirectOwnerCall),
            (
                "let mut x = own(2);\n",
                super::BindingReach::OwnerCallInInitializer,
            ),
        ];
        let mut cases = 0;
        for (setup, reach) in modes {
            for placement in placements {
                for shape in shapes {
                    let hit = format!("{setup}{placement}{}", shape.replace("{n}", "x"));
                    assert!(
                        !super::local_is_owner_result_before(&hit, "x", "own", *reach),
                        "binder must withdraw credit: {hit:?}"
                    );
                    let miss = format!("{setup}{placement}{}", shape.replace("{n}", "q"));
                    assert!(
                        super::local_is_owner_result_before(&miss, "x", "own", *reach),
                        "other name must keep credit: {miss:?}"
                    );
                    cases += 1;
                }
            }
        }
        assert!(cases > 100, "{cases}");
    }

    #[test]
    fn an_open_plain_let_keeps_the_previous_binding_but_not_a_binder_in_its_initializer() {
        for reach in [
            super::BindingReach::DirectOwnerCall,
            super::BindingReach::OwnerCallInInitializer,
        ] {
            assert!(super::local_is_owner_result_before(
                "let x = own(2);\nlet x =\n",
                "x",
                "own",
                reach
            ));
            assert!(!super::local_is_owner_result_before(
                "let x = own(2);\nlet x = |x|\n",
                "x",
                "own",
                reach
            ));
            for kept in [
                "let x = own(2);\nlet x = |Foo::<u8> { y }| {\n",
                "let x = own(2);\n#[inline]\nlet x =\n",
                "let x = own(2);\n#[inline] let x =\n",
                "let x = own(2);\n#[inline]\nlet x = {\n",
            ] {
                assert!(
                    super::local_is_owner_result_before(kept, "x", "own", reach),
                    "{kept:?}"
                );
            }
            assert!(!super::local_is_owner_result_before(
                "let x = own(2);\n#[inline]\nlet x = 5;\n",
                "x",
                "own",
                reach
            ));
        }
    }
}
