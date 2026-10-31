//! Nesting budget for Rust parses.
//!
//! `ra_ap_parser` is recursive descent, and both ripr's syntax walkers and
//! rowan's tree drop recurse once per tree level. Rust files are parsed on
//! rayon workers (2 MiB stacks) and parse trees are dropped on the
//! `ParseNodeDropper` thread `ra_ap_syntax` spawns with a default stack, so
//! a deeply nested file anywhere in the workspace aborted the whole process
//! with a stack overflow, even when the file was not in the diff. Measured
//! abort points on Linux x86_64: about 2,000 nested blocks or `if`s and
//! 4,000 nested parens, closures or prefix operators in release builds;
//! about 600 nested blocks in debug builds; 16,000-branch `else if` chains
//! and 64,000-term operator chains in release builds.
//!
//! Every `SourceFile::parse` in the analyzer goes through
//! [`parse_clean_source_file`] or checks [`rust_nesting_refusal`] first. A
//! refused source is handled exactly like a source with parse errors: each
//! caller already fails closed on that path (lexical fallback for file
//! facts, typed limitations or no evidence elsewhere).

use ra_ap_syntax::{Edition, Parse, SourceFile};

/// Maximum estimated bracket depth plus prefix-operator run. Real Rust
/// rarely nests past a few dozen levels; the budget sits well below the
/// measured debug-build abort depth (about 600 nested blocks on a 2 MiB
/// worker stack).
pub(crate) const RUST_NESTING_BUDGET: usize = 256;

/// Maximum `else if` links in one chain. Each link nests the next `if`
/// inside the previous one's `else` branch.
pub(crate) const RUST_ELSE_IF_CHAIN_BUDGET: usize = 2_048;

/// Maximum binary-operator characters in one run between `;`, `,`, `{` and
/// `}` at the same bracket depth. A left-associative chain nests one tree
/// level per operator.
pub(crate) const RUST_OPERATOR_RUN_BUDGET: usize = 4_096;

/// Parse `text` as a Rust source file when it is within the nesting budget
/// and parses without errors; `None` otherwise.
pub(crate) fn parse_clean_source_file(text: &str) -> Option<Parse<SourceFile>> {
    if rust_nesting_refusal(text).is_some() {
        return None;
    }
    let (parse, error_count) = parse_source_file_checked(text);
    (error_count == 0).then_some(parse)
}

/// Parse with the current edition, retrying as Rust 2021 when that fails.
/// ripr does not read each crate's edition, and 2024 reserves `gen`, so a
/// 2021 crate's `fn arbitrary(gen: &mut Gen)` is a syntax error under the
/// current edition alone and its whole file fell to lexical fallback. The
/// 2021 parse is used only when it is error-free. A file that uses the
/// legacy `try!(..)` macro and still fails both is retried once as Rust 2015
/// (#10054), again used only when error-free.
#[cfg(test)]
pub(crate) fn parse_source_file(text: &str) -> Parse<SourceFile> {
    parse_source_file_checked(text).0
}

/// [`parse_source_file`] plus the error count of the parse it returns.
/// `Parse::errors` re-runs the whole-tree validation pass and allocates on
/// every call (about 14% of a cold tokio `check`), so the count is computed
/// once per distinct parse and memoized with it: callers that only need to
/// know whether the file is clean (or how many errors it has) no longer
/// validate the same tree again on every memo hit.
pub(crate) fn parse_source_file_checked(text: &str) -> (Parse<SourceFile>, usize) {
    if text.len() > PARSE_CACHE_MAX_BYTES {
        return parse_source_file_uncached(text);
    }
    PARSE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(position) = cache.iter().position(|(source, _, _)| source == text) {
            // Move-to-front keeps the hot file resident.
            let entry = cache.remove(position);
            let result = (entry.1.clone(), entry.2);
            cache.insert(0, entry);
            return result;
        }
        let (parse, error_count) = parse_source_file_uncached(text);
        cache.insert(0, (text.to_string(), parse.clone(), error_count));
        cache.truncate(PARSE_CACHE_ENTRIES);
        (parse, error_count)
    })
}

/// Per-thread memo of recent parses, keyed by the full source text (equality,
/// never a hash), so a hit is exactly the parse a fresh call would return.
/// Classification asks the same file the same question once per
/// (probe, related test) pair; without the memo each ask re-parsed the whole
/// file (regex `64ad0b6`: 148 s, nearly all in this reparse). Parses are
/// immutable green trees, so sharing a clone is observationally identical.
const PARSE_CACHE_ENTRIES: usize = 4;
const PARSE_CACHE_MAX_BYTES: usize = 512 * 1024;

type ParseCacheEntries = Vec<(String, Parse<SourceFile>, usize)>;

thread_local! {
    static PARSE_CACHE: std::cell::RefCell<ParseCacheEntries> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// Test-only count of `Parse::errors` validation passes run by the
    /// uncached parse, so a test can assert the memo does not revalidate.
    static VALIDATION_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The parse for `text` and the error count of that parse.
fn parse_source_file_uncached(text: &str) -> (Parse<SourceFile>, usize) {
    let validated = |parse: &Parse<SourceFile>| {
        #[cfg(test)]
        VALIDATION_PASSES.with(|passes| passes.set(passes.get() + 1));
        parse.errors().len()
    };
    let parse = SourceFile::parse(text, Edition::CURRENT);
    let current_errors = validated(&parse);
    if current_errors == 0 {
        return (parse, 0);
    }
    let legacy = SourceFile::parse(text, Edition::Edition2021);
    if validated(&legacy) == 0 {
        return (legacy, 0);
    }
    // A Rust 2015 crate may still use the `try!(..)` macro, which 2018+
    // reserves as a keyword; both newer editions reject the whole file and
    // it would fall to the lexical fallback (#10054).
    // The retry is gated on that macro so other 2015-only spellings (`async`
    // or `dyn` as identifiers) and genuinely broken buffers keep failing
    // closed and do not pay a third full parse.
    if !(text.contains("try!") || text.contains("try !")) {
        return (parse, current_errors);
    }
    let oldest = SourceFile::parse(text, Edition::Edition2015);
    if validated(&oldest) == 0 {
        (oldest, 0)
    } else {
        (parse, current_errors)
    }
}

/// Typed `rust_nesting_budget` static-limit reason when `text` is over any
/// nesting budget, else `None`.
pub(crate) fn rust_nesting_refusal(text: &str) -> Option<String> {
    let estimate = estimate_rust_nesting(text);
    if estimate.depth > RUST_NESTING_BUDGET {
        return Some(format!(
            "static limit rust_nesting_budget: estimated nesting depth {} exceeds the {RUST_NESTING_BUDGET}-level parse budget; parse refused to avoid a stack overflow",
            estimate.depth
        ));
    }
    if estimate.else_if_chain > RUST_ELSE_IF_CHAIN_BUDGET {
        return Some(format!(
            "static limit rust_nesting_budget: `else if` chain of {} links exceeds the {RUST_ELSE_IF_CHAIN_BUDGET}-link parse budget; parse refused to avoid a stack overflow",
            estimate.else_if_chain
        ));
    }
    if estimate.operator_run > RUST_OPERATOR_RUN_BUDGET {
        return Some(format!(
            "static limit rust_nesting_budget: operator chain of {} operators exceeds the {RUST_OPERATOR_RUN_BUDGET}-operator parse budget; parse refused to avoid a stack overflow",
            estimate.operator_run
        ));
    }
    None
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NestingEstimate {
    pub(crate) depth: usize,
    pub(crate) else_if_chain: usize,
    pub(crate) operator_run: usize,
}

/// One left-to-right byte scan; a lexical approximation, not a parse.
/// Comments, string, raw string and char literals are skipped so their
/// contents cannot inflate the estimate. Over-estimates only cost a lexical
/// fallback; the scan never recurses, so it is safe on any input.
pub(crate) fn estimate_rust_nesting(text: &str) -> NestingEstimate {
    let bytes = text.as_bytes();
    let mut estimate = NestingEstimate::default();
    let mut depth = 0usize;
    let mut unary_run = 0usize;
    // Right-associative nesting (`a = b = c`, `for x in for y in ..`): one
    // parser level per link, reset at statement and bracket boundaries.
    let mut right_run = 0usize;
    // Per-depth counters, indexed by bracket depth.
    let mut else_if: Vec<usize> = vec![0];
    let mut operators: Vec<usize> = vec![0];
    // Sum of `operators` over the open-bracket stack: the length of the
    // operator chain the current token extends. A run before `(` continues
    // into it, and a `(..)` or `[..]` run continues after it closes, so
    // `((x + 1 ..) + 1 ..)` nested deep counts as one chain.
    let mut open_operators = 0usize;
    // True when the previous significant token ends an operand, so a
    // following `-`, `*`, `&` or `!` is binary, not prefix.
    let mut after_operand = false;
    let mut previous_word_was_else = false;
    let mut index = frontmatter_end(text);

    while let Some(&byte) = bytes.get(index) {
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let next = bytes.get(index + 1).copied();
        if byte == b'/' && next == Some(b'/') {
            index = skip_line(bytes, index);
            continue;
        }
        if byte == b'/' && next == Some(b'*') {
            index = skip_block_comment(bytes, index);
            continue;
        }
        if byte == b'"' {
            index = skip_string(bytes, index + 1);
            after_operand = true;
            unary_run = 0;
            previous_word_was_else = false;
            continue;
        }
        if byte == b'\'' {
            let start = index;
            index = skip_char_or_lifetime(text, index);
            // A char literal is an operand. A lifetime or label (`break 'a`,
            // `&'a T`) is neither an operand nor a run breaker, so it must
            // not reset a prefix-keyword run (`break 'a break 'a ..`).
            let char_literal =
                index.saturating_sub(start) >= 3 && bytes.get(index - 1) == Some(&b'\'');
            if char_literal {
                after_operand = true;
                unary_run = 0;
            } else {
                // `skip_char_or_lifetime` stops after the quote; consume the
                // name so it is not scanned as an operand word.
                while bytes
                    .get(index)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b >= 0x80)
                {
                    index += 1;
                }
            }
            previous_word_was_else = false;
            continue;
        }
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80 {
            let start = index;
            while bytes
                .get(index)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b >= 0x80)
            {
                index += 1;
            }
            let word = text.get(start..index).unwrap_or("");
            if matches!(word, "r" | "br" | "cr")
                && let Some(end) = skip_raw_string(bytes, index)
            {
                index = end;
                after_operand = true;
                unary_run = 0;
                previous_word_was_else = false;
                continue;
            }
            if word == "if"
                && let Some(chain) = else_if.get_mut(depth)
            {
                // A bare `if` starts a new chain at this depth.
                *chain = if previous_word_was_else {
                    chain.saturating_add(1)
                } else {
                    0
                };
                estimate.else_if_chain = estimate.else_if_chain.max(*chain);
            }
            previous_word_was_else = word == "else";
            if word == "as" {
                bump_operator_run(&mut operators, depth, &mut open_operators, &mut estimate);
            }
            if word == "in" {
                right_run = right_run.saturating_add(1);
                estimate.depth = estimate
                    .depth
                    .max(depth.saturating_add(unary_run).saturating_add(right_run));
            }
            if is_recursive_prefix_keyword(word) {
                // `return return ..`, `if if ..` and the like recurse once per
                // keyword in the parser exactly like a prefix operator run, but
                // never reach the bracket arms below (#11407).
                unary_run = unary_run.saturating_add(1);
                estimate.depth = estimate.depth.max(depth.saturating_add(unary_run));
            }
            after_operand = !is_expression_keyword(word);
            if after_operand {
                unary_run = 0;
            }
            continue;
        }
        previous_word_was_else = false;
        match byte {
            b'(' | b'[' | b'{' => {
                if after_operand && byte != b'{' && follows_closer(bytes, index) {
                    // Postfix call or index on a closed group: `f()()..`,
                    // `x[0][0]..` nest left. A flat run of siblings such as
                    // the token tree of `m!(foo() foo() ..)` does not.
                    bump_operator_run(&mut operators, depth, &mut open_operators, &mut estimate);
                }
                depth = depth.saturating_add(1);
                right_run = 0;
                else_if.push(0);
                operators.push(0);
                after_operand = false;
                estimate.depth = estimate.depth.max(depth.saturating_add(unary_run));
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                right_run = 0;
                else_if.truncate(depth + 1);
                let closed: usize = operators.drain(depth + 1..).sum();
                if let Some(run) = operators.get_mut(depth) {
                    if byte == b'}' {
                        // A block ends the enclosing chain.
                        open_operators = open_operators.saturating_sub(closed + *run);
                        *run = 0;
                    } else {
                        *run = run.saturating_add(closed);
                    }
                }
                after_operand = true;
                unary_run = 0;
            }
            b';' | b',' => {
                if let Some(chain) = else_if.get_mut(depth) {
                    *chain = 0;
                }
                if let Some(run) = operators.get_mut(depth) {
                    open_operators = open_operators.saturating_sub(*run);
                    *run = 0;
                }
                after_operand = false;
                unary_run = 0;
                right_run = 0;
            }
            b'!' | b'-' | b'*' | b'&' if !after_operand => {
                unary_run = unary_run.saturating_add(1);
                estimate.depth = estimate.depth.max(depth.saturating_add(unary_run));
            }
            // Postfix `.` and `?` count too: a long method or field chain is
            // a left-nested tree just like a binary-operator run.
            b':' if bytes.get(index + 1) == Some(&b':')
                || index.checked_sub(1).and_then(|i| bytes.get(i)) == Some(&b':') =>
            {
                // Each `::` path segment nests the path expression once.
                bump_operator_run(&mut operators, depth, &mut open_operators, &mut estimate);
                after_operand = false;
            }
            b'.' | b'?' | b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'|' | b'&' | b'<' | b'>'
            | b'=' | b'@' => {
                if let Some(run) = operators.get_mut(depth) {
                    *run = run.saturating_add(1);
                    open_operators = open_operators.saturating_add(1);
                    estimate.operator_run = estimate.operator_run.max(open_operators);
                }
                if byte == b'=' && is_plain_assignment(bytes, index) {
                    right_run = right_run.saturating_add(1);
                    estimate.depth = estimate
                        .depth
                        .max(depth.saturating_add(unary_run).saturating_add(right_run));
                }
                after_operand = byte == b'?';
            }
            _ => {
                after_operand = false;
            }
        }
        index += 1;
    }
    estimate
}

/// True when the last significant byte before `index` closes a group, so a
/// following `(` or `[` is a postfix call or index on that group's value.
fn follows_closer(bytes: &[u8], index: usize) -> bool {
    bytes
        .get(..index)
        .and_then(|head| head.iter().rev().find(|b| !b.is_ascii_whitespace()))
        .is_some_and(|b| matches!(b, b')' | b']'))
}

/// Words after which a `-`, `*`, `&` or `!` is a prefix operator.
fn is_expression_keyword(word: &str) -> bool {
    is_recursive_prefix_keyword(word) || matches!(word, "in" | "else" | "let" | "mut" | "move")
}

/// Count one more link of a left-nested chain (`::`, `as`, postfix call or
/// index) in the operator run at `depth`, exactly like a binary operator.
fn bump_operator_run(
    operators: &mut [usize],
    depth: usize,
    open_operators: &mut usize,
    estimate: &mut NestingEstimate,
) {
    if let Some(run) = operators.get_mut(depth) {
        *run = run.saturating_add(1);
        *open_operators = open_operators.saturating_add(1);
        estimate.operator_run = estimate.operator_run.max(*open_operators);
    }
}

/// A bare `=` (assignment or `let` initializer), not part of `==`, `<=`, `=>`,
/// `+=` and the like. Assignment is right-associative, so a chain nests once
/// per link.
fn is_plain_assignment(bytes: &[u8], index: usize) -> bool {
    let before = index.checked_sub(1).and_then(|i| bytes.get(i)).copied();
    let after = bytes.get(index + 1).copied();
    let joins = |b: Option<u8>| {
        b.is_some_and(|b| {
            matches!(
                b,
                b'=' | b'<' | b'>' | b'!' | b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'&' | b'|'
            )
        })
    };
    !joins(before) && !matches!(after, Some(b'=' | b'>'))
}

/// Keywords whose operand is parsed by recursing into the expression parser,
/// so a run of them nests one parser level per keyword like a prefix operator.
fn is_recursive_prefix_keyword(word: &str) -> bool {
    matches!(
        word,
        "return" | "break" | "yield" | "become" | "if" | "match" | "while" | "box"
    )
}

/// Borrow the existing Rust literal/comment boundaries without allocating a
/// masked source. A lifetime is code; malformed literals consume the suffix.
pub(crate) fn non_code_token_end(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let tail = text.get(index..)?;
    let end = if tail.starts_with("//") {
        skip_line(bytes, index)
    } else if tail.starts_with("/*") {
        skip_block_comment(bytes, index)
    } else if tail.starts_with('"') {
        skip_string(bytes, index + 1)
    } else if tail.starts_with('\'') {
        let end = skip_char_or_lifetime(text, index);
        if end == index + 1 {
            return None;
        }
        end
    } else {
        let prefix = ["br", "cr", "r"]
            .into_iter()
            .find(|prefix| tail.starts_with(*prefix))?;
        if index > 0
            && bytes
                .get(index - 1)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80)
        {
            return None;
        }
        skip_raw_string(bytes, index + prefix.len())?
    };
    Some(end.min(text.len()))
}

/// 1-based lines of `text` that hold only block-comment content (`/* .. */`,
/// `/*! .. */`, `/** .. */`, including interior lines). A line that also
/// carries code before the opening or after the closing delimiter is not
/// listed. Line comments are not listed; callers already skip them by prefix.
pub(crate) fn block_comment_only_lines(text: &str) -> std::collections::BTreeSet<usize> {
    let mut lines = std::collections::BTreeSet::new();
    if !text.contains("/*") {
        return lines;
    }
    let mut line_starts = vec![0usize];
    line_starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
    let line_of = |offset: usize| line_starts.partition_point(|start| *start <= offset);
    let line_start = |line: usize| line_starts[line - 1];
    let mut index = 0;
    while index < text.len() {
        match non_code_token_end(text, index) {
            Some(end) if end > index => {
                if text[index..].starts_with("/*") {
                    let first = line_of(index);
                    let last = line_of(end.saturating_sub(1).max(index));
                    let before_blank = text[line_start(first)..index].trim().is_empty();
                    let line_end = text[end..]
                        .find('\n')
                        .map_or(text.len(), |offset| end + offset);
                    let after_blank = text[end..line_end].trim().is_empty();
                    for line in first..=last {
                        let opens_with_code = line == first && !before_blank;
                        let closes_with_code = line == last && !after_blank;
                        if !opens_with_code && !closes_with_code {
                            lines.insert(line);
                        }
                    }
                }
                index = end;
            }
            _ => {
                index += text[index..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    lines
}

/// Byte offset just past a terminated leading frontmatter block (`---`
/// fences around TOML, optionally after a shebang line), or 0 when the file
/// has none. The parser lexes the whole block as one token, so its contents
/// and fence length cannot nest anything; counting a 230-dash fence as an
/// operator chain refused cargo's own `fence-too-many-dashes.rs` fixture and
/// made the whole workspace analysis partial (#9794). An unterminated fence
/// is not skipped and keeps its old estimate.
fn frontmatter_end(text: &str) -> usize {
    // The parser's whitespace set (Pattern_White_Space), narrower than
    // `char::is_whitespace`: a skip must never exceed what it lexes.
    fn pattern_ws(c: char) -> bool {
        matches!(
            c,
            ' ' | '\t'
                | '\n'
                | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{85}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
        )
    }
    let mut offset = 0;
    // A `#!` line is a shebang only when the parser would not read it as
    // the start of an inner attribute (`#! [..]`, `#!\n[..]`).
    if let Some(after) = text.strip_prefix("#!")
        && !after.trim_start_matches(pattern_ws).starts_with('[')
    {
        offset = text.find('\n').map_or(text.len(), |n| n + 1);
    }
    let rest = &text[offset..];
    let body_start = rest.len() - rest.trim_start_matches(pattern_ws).len();
    // The opening fence starts its line (column 0), the only shape the
    // parser lexes as frontmatter.
    let line_start = rest[..body_start].rfind('\n').map_or(0, |n| n + 1);
    if line_start != body_start {
        return 0;
    }
    let mut lines = rest[line_start..].split_inclusive('\n');
    let Some(opening) = lines.next() else {
        return 0;
    };
    let fence = opening.bytes().take_while(|b| *b == b'-').count();
    if fence < 3 {
        return 0;
    }
    let mut end = offset + line_start + opening.len();
    for line in lines {
        end += line.len();
        let dashes = line.bytes().take_while(|b| *b == b'-').count();
        if dashes < fence {
            continue;
        }
        // The first line that starts with the fence closes the block, and
        // only if it is the fence and nothing else; the parser rejects the
        // block otherwise (extra dashes, trailing text) or when a second
        // fence follows.
        let tail = line[dashes..].trim_end_matches([' ', '\t', '\r', '\n']);
        let second = text[end..]
            .trim_start_matches(pattern_ws)
            .starts_with("---");
        return if dashes == fence && tail.is_empty() && !second {
            end
        } else {
            0
        };
    }
    0
}

fn skip_line(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(|b| *b != b'\n') {
        index += 1;
    }
    index
}

/// Rust block comments nest.
fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    let mut open = 0usize;
    while let Some(&byte) = bytes.get(index) {
        let next = bytes.get(index + 1).copied();
        if byte == b'/' && next == Some(b'*') {
            open += 1;
            index += 2;
        } else if byte == b'*' && next == Some(b'/') {
            open = open.saturating_sub(1);
            index += 2;
            if open == 0 {
                return index;
            }
        } else {
            index += 1;
        }
    }
    index
}

/// `index` is just past the opening quote; returns the index past the
/// closing quote.
fn skip_string(bytes: &[u8], mut index: usize) -> usize {
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    index
}

/// `index` is just past an `r`, `br` or `cr` prefix. Returns the index past
/// the raw string when one starts here.
fn skip_raw_string(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut hashes = 0usize;
    while bytes.get(index) == Some(&b'#') {
        hashes += 1;
        index += 1;
    }
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        if byte == b'"' {
            let closing = bytes.get(index..index + hashes);
            if closing.is_some_and(|run| run.iter().all(|b| *b == b'#')) {
                return Some(index + hashes);
            }
        }
    }
    Some(index)
}

/// A char literal (`'x'`, `'\n'`, `'é'`) or a lifetime/label (`'a`).
///
/// Mirrors `rustc_lexer`: a `'` that cannot start a lifetime opens a char
/// literal that runs to the closing `'`, and an unterminated one ends at the
/// end of the line (or a `/`). The suffix of a malformed literal such as
/// `'git version ...', got: {x}"` therefore stays inside the token; treating
/// the lone `'` as a lifetime let the scan open a string at a quote the
/// parser never lexes as one, inverting string parity for the rest of the
/// file and hiding thousands of nested parens from the budget.
fn skip_char_or_lifetime(text: &str, index: usize) -> usize {
    let bytes = text.as_bytes();
    let mut rest = text.get(index + 1..).unwrap_or("").chars();
    let first = rest.next();
    let can_be_lifetime = first.is_some_and(|c| {
        c != '\'' && (c == '_' || c.is_ascii_digit() || unicode_ident::is_xid_start(c))
    });
    if !can_be_lifetime {
        return skip_single_quoted(text, index + 1);
    }
    // A lifetime, or a multi-character char literal such as `'ab'`.
    let ident_len = first.map_or(0, char::len_utf8)
        + rest
            .take_while(|c| unicode_ident::is_xid_continue(*c))
            .map(char::len_utf8)
            .sum::<usize>();
    if bytes.get(index + 1 + ident_len) == Some(&b'\'') {
        return index + 2 + ident_len;
    }
    index + 1
}

/// `index` is just past the opening `'` of a char literal. Returns the index
/// past its closing `'`, or where the lexer gives up on an unterminated one.
fn skip_single_quoted(text: &str, mut index: usize) -> usize {
    let bytes = text.as_bytes();
    // `rustc_lexer` consumes any single symbol followed by `'` as a terminated
    // char (`'/'`, `'''`) before the `/` and newline rules below apply.
    if bytes.get(index) != Some(&b'\\') {
        let width = text
            .get(index..)
            .and_then(|tail| tail.chars().next())
            .map_or(0, char::len_utf8);
        if width > 0 && bytes.get(index + width) == Some(&b'\'') {
            return index + width + 1;
        }
    }
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\'' => return index + 1,
            b'/' => break,
            b'\n' if bytes.get(index + 1) != Some(&b'\'') => break,
            b'\\' => {
                let escaped = text
                    .get(index + 1..)
                    .and_then(|tail| tail.chars().next())
                    .map_or(0, char::len_utf8);
                index += 1 + escaped;
            }
            _ => index += 1,
        }
    }
    index
}

#[cfg(test)]
mod tests {
    /// #8389: the memo returns exactly the parse a fresh call would, for
    /// repeated and interleaved sources, so classification results cannot
    /// depend on call order.
    #[test]
    fn parse_source_file_memo_matches_a_fresh_parse_for_interleaved_sources() {
        let sources = [
            "fn a() { let x = 1; }\n",
            "struct W; impl W { fn f(&self) {} }\n",
            "fn broken( {\n",
        ];
        for round in 0..3 {
            for text in sources.iter().chain(sources.iter().rev()) {
                let cached = parse_source_file(text);
                let (fresh, fresh_errors) = parse_source_file_uncached(text);
                assert_eq!(
                    format!("{:#?}", cached.syntax_node()),
                    format!("{:#?}", fresh.syntax_node()),
                    "round {round}: {text}"
                );
                assert_eq!(cached.errors().len(), fresh.errors().len(), "{text}");
                let (_, checked) = parse_source_file_checked(text);
                assert_eq!(checked, fresh_errors, "{text}");
                assert_eq!(checked, fresh.errors().len(), "{text}");
            }
        }
        // More distinct sources than cache slots still return correct parses.
        for n in 0..(PARSE_CACHE_ENTRIES * 2) {
            let text = format!("fn f{n}() {{}}\n");
            let parse = parse_source_file(&text);
            assert!(parse.errors().is_empty(), "{text}");
            assert!(format!("{:?}", parse.syntax_node()).contains("SOURCE_FILE"));
            assert!(parse.syntax_node().text().to_string() == text, "{text}");
        }
    }

    /// Counter oracle: the memo hit must not rerun `Parse::errors`. One
    /// clean source asked for five times validates once; a source that
    /// fails every edition validates once per edition tried, once overall.
    #[test]
    fn repeated_clean_checks_of_one_source_validate_once() {
        let clean = "fn counter_probe_clean() { let x = 1; }\n";
        let before = VALIDATION_PASSES.with(std::cell::Cell::get);
        for _ in 0..5 {
            assert!(parse_clean_source_file(clean).is_some());
            assert_eq!(parse_source_file_checked(clean).1, 0);
        }
        let after = VALIDATION_PASSES.with(std::cell::Cell::get);
        assert_eq!(after - before, 1, "clean source validated more than once");

        let broken = "fn counter_probe_broken( {\n";
        let before = VALIDATION_PASSES.with(std::cell::Cell::get);
        let expected = SourceFile::parse(broken, Edition::CURRENT).errors().len();
        assert!(expected > 0);
        for _ in 0..5 {
            assert!(parse_clean_source_file(broken).is_none());
            assert_eq!(parse_source_file_checked(broken).1, expected);
        }
        let after = VALIDATION_PASSES.with(std::cell::Cell::get);
        // current + 2021 editions on the first ask (no `try!`), then memo hits.
        assert_eq!(after - before, 2, "broken source revalidated on memo hits");
    }

    use super::*;

    /// #11407: a long run of recursive prefix keywords nests the parser once
    /// per keyword; it must hit the same budget as a prefix-operator run.
    #[test]
    fn keyword_prefix_runs_are_refused_like_prefix_operator_runs() {
        for keyword in ["return", "break", "yield", "become", "if", "match", "while"] {
            let over = format!(
                "fn f() {{ let _v = {}x; }}\n",
                format!("{keyword} ").repeat(3_000)
            );
            assert!(
                rust_nesting_refusal(&over)
                    .is_some_and(|reason| reason.contains("rust_nesting_budget")),
                "{keyword} x3000 must be refused"
            );
        }
    }

    /// #11410: right-associative `=` and `in` links nest the parser once each.
    #[test]
    fn right_nested_assignment_and_for_in_runs_are_refused() {
        let lets = format!("fn f() {{ if {}x {{}} }}\n", "let _ = ".repeat(3_000));
        let fors = format!("fn f() {{ {}y {{}} }}\n", "for x in ".repeat(3_000));
        let assigns = format!("fn f() {{ {}1; }}\n", "a = ".repeat(3_000));
        for source in [lets, fors, assigns] {
            assert!(
                rust_nesting_refusal(&source)
                    .is_some_and(|reason| reason.contains("rust_nesting_budget")),
                "{}",
                &source[..40]
            );
        }
        let ordinary = "fn f(v: Vec<u8>) { let mut a = 1; let b = a == 2; a += 1; for x in v { let _y = x <= 3; } let c = |x| x => y; }\n";
        assert_eq!(rust_nesting_refusal(ordinary), None);
    }

    /// #11661: left-nested `::`, `as`, call and index chains and `box`
    /// patterns must hit the budget too (release builds abort near 100k).
    #[test]
    fn path_cast_call_index_and_box_chains_are_refused() {
        let cases = [
            format!("fn f() {{ let _v = a{}; }}\n", "::b".repeat(5_000)),
            format!("fn f() {{ let _v = x{}; }}\n", " as u8".repeat(5_000)),
            format!("fn f() {{ let _v = x{}; }}\n", "[0]".repeat(5_000)),
            format!("fn f() {{ let _v = f{}; }}\n", "()".repeat(5_000)),
            format!("fn f() {{ let {}x = 1; }}\n", "box ".repeat(3_000)),
        ];
        for source in cases {
            assert!(
                rust_nesting_refusal(&source)
                    .is_some_and(|reason| reason.contains("rust_nesting_budget")),
                "{}",
                &source[..40]
            );
        }
        let ordinary = "use std::collections::BTreeMap;\nfn f(v: Vec<Vec<u8>>, m: BTreeMap<u8, u8>) -> u8 { let a = v[0][1] as u8; let b = std::cmp::max(a, m.get(&1).copied().unwrap_or(0)); g(b)(a) }\n";
        assert_eq!(rust_nesting_refusal(ordinary), None);
    }

    /// #11572: left-nested `@` binding chains and `break 'a` label runs, and
    /// the flat sibling-call controls that must stay accepted.
    #[test]
    fn binding_and_label_chains_are_refused_and_flat_runs_are_not() {
        let refused = [
            (
                "binding",
                format!("fn f() {{ let {}z = 1; }}\n", "y @ ".repeat(5_000)),
            ),
            (
                "label",
                format!("fn f() {{ let _v = {}x; }}\n", "break 'a ".repeat(5_000)),
            ),
        ];
        for (name, source) in refused {
            assert!(
                rust_nesting_refusal(&source)
                    .is_some_and(|reason| reason.contains("rust_nesting_budget")),
                "{name}"
            );
        }
        let flat = [
            (
                "flat macro calls",
                format!("fn f() {{ m!({}); }}\n", "foo() ".repeat(5_000)),
            ),
            (
                "flat macro indexes",
                format!("fn f() {{ m!({}); }}\n", "a[0] ".repeat(5_000)),
            ),
            (
                "flat lifetimes",
                format!("fn f<{}>() {{}}\n", "'a, ".repeat(5_000)),
            ),
        ];
        for (name, source) in flat {
            assert_eq!(rust_nesting_refusal(&source), None, "{name}");
        }
    }

    #[test]
    fn ordinary_keyword_use_stays_within_the_nesting_budget() {
        let source = "fn f(a: bool, b: Option<u8>) -> u8 {\n    if a { return 1; }\n    while a { break; }\n    match b { Some(x) if x > 1 => return x, _ => {} }\n    else_ok(if a { 1 } else if !a { 2 } else { 3 })\n}\n";
        assert_eq!(rust_nesting_refusal(source), None);
        let chained = format!("fn f() {{ {}1; }}\n", "return ".repeat(10));
        assert_eq!(rust_nesting_refusal(&chained), None);
    }

    #[test]
    fn block_comment_only_lines_lists_interior_lines_and_not_code_lines() {
        let source = "fn f() {}\n/* name: value\n   x < y\n*/\nlet a = 1; /* tail */\n/* head */ let b = 2;\nlet s = \"/* not a comment\";\nlet c = 3;\n";
        let lines = block_comment_only_lines(source);
        assert_eq!(lines.into_iter().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn block_comment_only_lines_handles_doc_blocks_and_nesting() {
        let source =
            "/*! crate\n For example:\n*/\n/** doc /* nested */ still\ncomment */\nfn g() {}\n";
        let lines = block_comment_only_lines(source);
        assert_eq!(lines.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_rust_2021_gen_identifier_parses_clean_and_real_errors_do_not() {
        // regex-automata: `fn arbitrary(gen: &mut quickcheck::Gen)`. `gen` is
        // reserved in 2024, so the current edition alone rejected the file.
        let legacy = "fn arbitrary(gen: &mut Gen) -> u8 { gen.next() }";
        assert!(
            !SourceFile::parse(legacy, Edition::CURRENT)
                .errors()
                .is_empty(),
            "control: the current edition must reject `gen` as a binding"
        );
        assert!(parse_clean_source_file(legacy).is_some());
        assert!(parse_clean_source_file("fn f() -> u8 { 1 }").is_some());
        assert!(parse_clean_source_file("fn f( {").is_none());
        // An error under both editions keeps the current edition's errors.
        assert!(!parse_source_file("fn gen( {").errors().is_empty());
    }

    #[test]
    fn a_rust_2015_try_macro_parses_clean_and_real_errors_do_not() {
        // regex-syntax 2016: `try!(..)` is a keyword in 2018+, so both newer
        // editions rejected the whole file (#10054).
        let legacy = "fn f(x: Result<u8, ()>) -> Result<u8, ()> { let v = try!(x); Ok(v) }";
        assert!(
            !SourceFile::parse(legacy, Edition::CURRENT)
                .errors()
                .is_empty(),
            "control: the current edition must reject `try!`"
        );
        assert!(
            !SourceFile::parse(legacy, Edition::Edition2021)
                .errors()
                .is_empty(),
            "control: 2021 must reject `try!`"
        );
        assert!(parse_clean_source_file(legacy).is_some());
        assert!(!parse_source_file("fn f( { try!(x)").errors().is_empty());
        // The retry is gated on `try!`: a 2015-only identifier alone stays an error.
        assert!(parse_clean_source_file("fn f(async: u8) -> u8 { async }").is_none());
        // The tree keeps `try!` as a macro call.
        let parse = parse_source_file(legacy);
        assert!(format!("{:#?}", parse.syntax_node()).contains("MACRO_CALL"));
    }

    fn nested(open: &str, close: &str, levels: usize) -> String {
        format!(
            "pub fn f(x: i32) -> i32 {{ {}x{} }}\n",
            open.repeat(levels),
            close.repeat(levels)
        )
    }

    #[test]
    fn ordinary_rust_is_within_budget() {
        let source = r####"
            // comment with ((((((( and "quotes"
            /* nested /* block */ ((((( */
            pub fn f<'a>(x: &'a [u8], y: &mut Vec<Option<u8>>) -> Result<(), String> {
                let s = "(((((((((((\"(((";
                let r = r#"((((("(((("#;
                let c = '(';
                let q = '\'';
                if x.len() > 3 && !y.is_empty() { return Err(format!("{}", -1)); }
                else if x.is_empty() { *y.first_mut().unwrap_or(&mut None) = None; }
                'outer: loop { break 'outer; }
                Ok(())
            }
        "####;
        let estimate = estimate_rust_nesting(source);
        assert!(estimate.depth <= 6, "{estimate:?}");
        assert!(estimate.else_if_chain <= 1, "{estimate:?}");
        assert!(rust_nesting_refusal(source).is_none());
        assert!(parse_clean_source_file(source).is_some());
    }

    #[test]
    fn deep_nesting_is_refused_before_parsing() {
        // Each shape aborted `ripr check` with a stack overflow in a debug
        // build at 600-1,000 levels before the budget existed.
        let cases = [
            nested("(", ")", 1_000),
            nested("{", "}", 1_000),
            nested("[", "]", 1_000),
            nested("if x > 1 { ", " } else { 0 }", 1_000),
            nested("(|y: i32| ", ")", 1_000),
            format!("pub fn f(x: i32) -> i32 {{ {}x }}\n", "- ".repeat(1_000)),
            format!("pub fn f(x: bool) -> bool {{ {}x }}\n", "!".repeat(1_000)),
            format!(
                "pub fn f(x: i32) -> i32 {{ if x == 0 {{ 0 }} {}else {{ 1 }} }}\n",
                (1..3_000)
                    .map(|i| format!("else if x == {i} {{ {i} }} "))
                    .collect::<String>()
            ),
            format!("pub fn f(x: i32) -> i32 {{ x{} }}\n", " + 1".repeat(5_000)),
            format!(
                "pub type T = {}u8{};\n",
                "Vec<".repeat(3_000),
                ">".repeat(3_000)
            ),
            format!(
                "pub fn f(x: i32) -> i32 {{ x{} }}\n",
                ".clone()".repeat(5_000)
            ),
            // Short runs per depth that chain through parens into one tree,
            // left-nested and right-nested.
            format!(
                "pub fn f(x: i32) -> i32 {{ {}x{} }}\n",
                "(".repeat(200),
                format!("){}", " + 1".repeat(40)).repeat(200)
            ),
            format!(
                "pub fn f(x: i32) -> i32 {{ {}x{} }}\n",
                format!("{}(", "1 + ".repeat(40)).repeat(200),
                ")".repeat(200)
            ),
            format!(
                "pub fn g(o: &S) -> Option<i32> {{ Some(o{}.v) }}\n",
                ".s.as_ref()?".repeat(20_000)
            ),
        ];
        for (case, source) in cases.iter().enumerate() {
            assert!(
                rust_nesting_refusal(source).is_some(),
                "case {case}: {:?}",
                estimate_rust_nesting(source)
            );
            assert!(parse_clean_source_file(source).is_none(), "case {case}");
        }
    }

    #[test]
    fn nesting_just_under_budget_still_parses() {
        let source = nested("(", ")", RUST_NESTING_BUDGET - 2);
        assert!(rust_nesting_refusal(&source).is_none());
        assert!(parse_clean_source_file(&source).is_some());
    }

    #[test]
    fn literal_contents_do_not_count() {
        let parens = "(".repeat(5_000);
        let source = format!(
            "pub const A: &str = \"{parens}\";\npub const B: &str = r##\"{parens}\"##;\n// {parens}\n/* {parens} */\n"
        );
        // Only the `&str` reference prefix counts.
        assert!(estimate_rust_nesting(&source).depth <= 1);
        assert!(parse_clean_source_file(&source).is_some());
    }

    /// #9794: cargo's `fence-too-many-dashes.rs` opens with a 230-dash
    /// frontmatter fence. The parser lexes the block as one token, so it must
    /// not count as an operator chain (it made the whole analysis partial).
    #[test]
    fn a_terminated_leading_frontmatter_block_is_not_nesting() {
        let fence = "-".repeat(5_000);
        let deep = "(".repeat(5_000);
        for source in [
            format!(
                "{fence}\n[dependencies]\n{fence}\n\n#![feature(frontmatter)]\nfn main() {{}}\n"
            ),
            format!("---\n[dependencies]\nx = \"{deep}\"\n---\nfn main() {{}}\n"),
            "#!/usr/bin/env cargo\n---cargo\n[dependencies]\n---\nfn main() {}\n".to_string(),
            format!("\n\n{fence}\n[dependencies]\n{fence}\nfn main() {{}}\n"),
        ] {
            assert_eq!(rust_nesting_refusal(&source), None, "{:?}", &source[..40]);
            assert!(
                parse_clean_source_file(&source).is_some(),
                "{}",
                &source[..40]
            );
        }
    }

    /// The skip is only for a terminated block at the top of the file: a
    /// dash run anywhere else, or a fence that never closes, keeps its
    /// estimate and its refusal.
    #[test]
    fn dash_runs_outside_a_terminated_leading_block_still_count() {
        let run = "-".repeat(5_000);
        for source in [
            format!("{run}\n[dependencies]\nfn main() {{}}\n"),
            format!("fn main() {{}}\n{run}\n[dependencies]\n{run}\n"),
            format!("fn f() -> i32 {{ {run}1 }}\n"),
            format!("// header\n{run}\n[dependencies]\n{run}\nfn main() {{}}\n"),
            format!("{run}\n[dependencies]\n---\nfn main() {{}}\n"),
            // The parser needs the closing fence to match the opening one.
            format!("---\n[dependencies]\n----\n{run}\nfn main() {{}}\n"),
            // A BOM before the fence keeps it from lexing as frontmatter.
            format!("\u{feff}{run}\n[dependencies]\n{run}\nfn main() {{}}\n"),
            // The parser rejects a block whose closing line has extra
            // dashes or trailing text, or that is followed by a second one.
            format!("---\nA\n----\n{run}\n---\n"),
            format!("---\n---x\n{run}\n---\n"),
            format!("---\n{run}\n---\n\n---\n"),
            format!("#! [x]\n---\n{run}\n---\n"),
            format!("\u{a0}\n---\n{run}\n---\n"),
            // Too short to be a fence, and an inner attribute is not a shebang.
            format!("--\n[dependencies]\n--\n{run}\n"),
            format!("#![feature(frontmatter)]\n---\n{run}\n---\n"),
            // Code after a skipped block is still estimated.
            format!("---\n[dependencies]\n---\nfn f() -> i32 {{ {run}1 }}\n"),
            // An indented fence is not frontmatter to the parser.
            format!("  {run}\n[dependencies]\n  {run}\nfn main() {{}}\n"),
        ] {
            assert!(rust_nesting_refusal(&source).is_some(), "{}", &source[..40]);
        }
    }

    /// `frontmatter_end` must never skip more than the parser lexes as its
    /// frontmatter token: compare against the real token boundary for
    /// accepted and rejected shapes (small payloads; the boundary does not
    /// depend on size).
    #[test]
    fn frontmatter_skip_matches_the_parsers_frontmatter_token() {
        let sources = [
            "---\n[dependencies]\n---\nfn main() {}\n",
            "-----\n[dependencies]\n-----\nfn main() {}\n",
            "\n\n---\nx = 1\n---\nfn main() {}\n",
            "#!/usr/bin/env cargo\n---cargo\n[dependencies]\n---\nfn main() {}\n",
            "---\r\nx = 1\r\n---  \r\nfn main() {}\r\n",
            "---\nx = 1\n---\n\nfn main() {}\n",
            "---\nA\n----\nfn main() {}\n---\n",
            "---\n---x\nfn main() {}\n---\n",
            "---\nx = 1\n---\n\n---\nfn main() {}\n",
            "#! [x]\n---\nx = 1\n---\nfn main() {}\n",
            "\u{a0}\n---\nx = 1\n---\nfn main() {}\n",
            "#![feature(frontmatter)]\n---\nx = 1\n---\nfn main() {}\n",
            "--\nx = 1\n--\nfn main() {}\n",
            "  ---\nx = 1\n  ---\nfn main() {}\n",
            "\u{feff}---\nx = 1\n---\nfn main() {}\n",
            "---\nx = 1\nfn main() {}\n",
            "fn main() {}\n",
        ];
        for source in sources {
            let parse = parse_source_file(source);
            let lexed = parse
                .syntax_node()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == ra_ap_syntax::SyntaxKind::FRONTMATTER)
                .map_or(0, |token| usize::from(token.text_range().end()));
            let skipped = frontmatter_end(source);
            // Skipping nothing is always safe; skipping must stop at the
            // parser's boundary.
            assert!(
                skipped == 0 || skipped == lexed,
                "{source:?}: skipped {skipped}, parser frontmatter ends at {lexed}"
            );
        }
    }

    #[test]
    fn statement_boundaries_reset_chains() {
        let statements: String = (0..5_000)
            .map(|i| format!("if x > {i} {{ return {i}; }}\n"))
            .collect();
        let source = format!("pub fn f(x: i32) -> i32 {{\n{statements}0 }}\n");
        let estimate = estimate_rust_nesting(&source);
        assert!(estimate.operator_run < 10, "{estimate:?}");
        assert!(rust_nesting_refusal(&source).is_none());

        // Ordinary field and method chains, floats, ranges and `?` reset per
        // statement too, so a long body of them stays within budget.
        let statements: String = (0..5_000)
            .map(|i| format!("let v{i} = self.a.b.get({i}..2).map(|x| x * 1.5)?;\n"))
            .collect();
        let source = format!("fn f(&self) -> Option<f64> {{\n{statements}None }}\n");
        let estimate = estimate_rust_nesting(&source);
        assert!(estimate.operator_run < 32, "{estimate:?}");
        assert!(rust_nesting_refusal(&source).is_none());
    }

    /// Fuzz finding: a lone `'` in a message (`'git version ...', got: {x}"`)
    /// is an unterminated char literal to the parser, so the quote after it is
    /// not a string opener. The scan used to treat it as a lifetime, invert
    /// string parity, and hide a 4,000-deep paren chain from the budget (stack
    /// overflow in `ra_ap_parser`).
    #[test]
    fn an_unterminated_char_literal_does_not_invert_string_parity() {
        let source = format!(
            "fn f(r: &str) -> String {{ format!(\"expected 'git version ...', got: {{r}}\") }}\nfn g(x: i32) -> i32 {{ {}x {{ 1 }} }}\n",
            "(".repeat(4_000)
        );
        assert!(rust_nesting_refusal(&source).is_some());
        assert!(parse_clean_source_file(&source).is_none());
        // Real chars and lifetimes still close where the lexer closes them.
        for ok in [
            "let c = '\\'';",
            "let c = 'é';",
            "fn f<'a>(x: &'a str) {}",
            "let c = '\\u{1F600}';",
            "let l = 'outer: loop {};",
        ] {
            let at = ok.find('\'').unwrap_or(0);
            assert!(skip_char_or_lifetime(ok, at) > at, "{ok}");
        }
        assert_eq!(skip_char_or_lifetime("'a' x", 0), 3);
        assert_eq!(skip_char_or_lifetime("'a x", 0), 1);
        assert_eq!(skip_char_or_lifetime("'\\'' x", 0), 4);
        assert_eq!(skip_char_or_lifetime("', got\n\"y", 0), 6);
        // An unterminated literal stops at a `/` (a possible comment).
        assert_eq!(skip_char_or_lifetime("', x/ y'", 0), 4);
        assert_eq!(skip_char_or_lifetime("'/' x", 0), 3);
        assert_eq!(skip_char_or_lifetime("''' x", 0), 3);
        // A lone quote outside any string is the case that used to flip parity.
        let source = format!(
            "fn g() {{ let c = 'git version ...', got: x\"; }}\nfn h(x: i32) -> i32 {{ {}x {{ 1 }} }}\n",
            "(".repeat(4_000)
        );
        assert!(rust_nesting_refusal(&source).is_some());
        // `'/'` closes as a char, so the brackets after it still balance.
        let source = "fn f(s: &str) { s.split('/'); s.split('/'); }\n".repeat(300);
        assert!(rust_nesting_refusal(&source).is_none());
        assert_eq!(
            block_comment_only_lines("x = '/'; /* c */\n/* d */\n").len(),
            1
        );
    }
}
