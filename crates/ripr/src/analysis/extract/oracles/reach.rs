//! Execution reach for oracle extraction (RIPR-SPEC-0231 rule 8, #7638).
//!
//! A test statement that never runs cannot notice a wrong value, so it is
//! not an oracle. This reading is lexical and deliberately narrow: it marks
//! a line unreached only when every code byte on it sits inside one of
//! three never-executed regions, and it keeps every other line reached.
//!
//! - the block of an `if` or `while` whose whole condition is the literal
//!   `false` or `!true` (an `else` block still runs);
//! - a `let NAME = |..| ..;` closure statement when `NAME` is never named
//!   again in the text that follows (a closure that is never referenced is
//!   never called);
//! - the statements after an unconditional `return`, `panic!`,
//!   `unreachable!`, `todo!`, `unimplemented!` or `std::process::exit(..)`
//!   statement, up to the end of the same block or the first item
//!   declaration (a nested `fn` stays callable from earlier code).
//!
//! Unbalanced delimiters, and regions inside a macro invocation's input,
//! read as reached. Both adapters filter their oracle facts through
//! [`retain_reached_oracles`] so they agree by construction.
//!
//! One kind of oracle stays for a later authority: a strong `assert!` or
//! `assert_eq!` inside a literal-false block, an uncalled closure or after a
//! `return`. RIPR-SPEC-0197 execution admission already refuses that
//! assertion's owner pin and names why ("not credited"), so removing it here
//! would only hide that explanation and could let a weaker sibling oracle
//! read as the test's evidence. After `panic!`, `unreachable!`, `todo!`,
//! `unimplemented!` or `process::exit(..)` that admission sees no exit, so
//! every oracle there is dropped.

use std::collections::BTreeMap;

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::OracleFact;
use crate::domain::OracleStrength;

use super::patterns::contains_macro_invocation;

/// A never-executed line, and whether every code byte on it lies in a
/// region RIPR-SPEC-0197 execution admission already refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unreached {
    /// Inside a literal-false block, an uncalled closure or after a
    /// `return`: an admission-governed assertion stays for that refusal.
    AdmissionRefused,
    /// Some code byte is unreached only after a diverging macro or
    /// `process::exit(..)`, which that admission does not read.
    Unrefused,
}

/// Drop the oracles whose fact line `unreached_lines` marks unreached.
/// `text` is the scanned function text whose first line is `start_line`.
pub(crate) fn retain_reached_oracles(oracles: &mut Vec<OracleFact>, text: &str, start_line: usize) {
    let unreached = unreached_lines(text);
    if unreached.is_empty() {
        return;
    }
    oracles.retain(|oracle| {
        let Some(offset) = oracle.line.checked_sub(start_line) else {
            return true;
        };
        match unreached.get(&offset) {
            None => true,
            Some(Unreached::AdmissionRefused) => is_admission_governed(oracle),
            Some(Unreached::Unrefused) => false,
        }
    });
}

/// A strong `assert!` / `assert_eq!` oracle: the spellings whose owner pin
/// RIPR-SPEC-0197 execution admission decides.
fn is_admission_governed(oracle: &OracleFact) -> bool {
    if oracle.strength != OracleStrength::Strong {
        return false;
    }
    let masked = mask_comments_and_strings(&oracle.text);
    contains_macro_invocation(&masked, "assert_eq!")
        || contains_macro_invocation(&masked, "assert!")
}

/// Zero-based line offsets of `text` whose every code byte never executes.
pub(crate) fn unreached_lines(text: &str) -> BTreeMap<usize, Unreached> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let Some(groups) = Groups::new(bytes) else {
        return BTreeMap::new();
    };
    let mut dead: Vec<Region> = Vec::new();
    let mut previous: Option<u8> = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if !is_ident_start(byte) || (index > 0 && is_ident_byte(bytes[index - 1])) {
            previous = Some(byte);
            index += 1;
            continue;
        }
        let end = ident_end(bytes, index);
        let at_statement_start = previous.is_none_or(|byte| matches!(byte, b'{' | b';' | b'}'));
        match &masked[index..end] {
            "if" | "while" => {
                if let Some(block_end) = never_entered_block_end(bytes, end, &groups) {
                    dead.push(Region::refused(index, block_end));
                }
            }
            "let" if at_statement_start => {
                if let Some(statement_end) = uncalled_closure_statement_end(&masked, end, &groups) {
                    dead.push(Region::refused(index, statement_end));
                }
            }
            _ if at_statement_start => {
                if let Some(region) = after_diverging_statement(&masked, index, &groups) {
                    dead.push(region);
                }
            }
            _ => {}
        }
        previous = Some(bytes[end - 1]);
        index = end;
    }
    dead.retain(|region| {
        region.start < region.end && !groups.inside_macro_input(region.start, region.end)
    });
    lines_inside(bytes, &dead)
}

/// A never-executed byte range `start..end`.
#[derive(Clone, Copy, Debug)]
struct Region {
    start: usize,
    end: usize,
    /// RIPR-SPEC-0197 execution admission already refuses an assertion here.
    admission_refused: bool,
}

impl Region {
    fn refused(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            admission_refused: true,
        }
    }
}

/// Matched delimiter pairs of masked text. `None` when unbalanced.
struct Groups {
    /// `close[open]` is the matching closer of the opener at `open`.
    close: std::collections::HashMap<usize, usize>,
    /// Groups that are a macro invocation's input (`name!(..)`, `name! {..}`,
    /// `macro_rules! name {..}`).
    macro_inputs: Vec<(usize, usize)>,
}

impl Groups {
    fn new(bytes: &[u8]) -> Option<Self> {
        let mut close = std::collections::HashMap::new();
        let mut macro_inputs = Vec::new();
        let mut stack: Vec<(u8, usize)> = Vec::new();
        for (index, &byte) in bytes.iter().enumerate() {
            match byte {
                b'(' | b'[' | b'{' => stack.push((byte, index)),
                b')' | b']' | b'}' => {
                    let (open_byte, open) = stack.pop()?;
                    let expected = match open_byte {
                        b'(' => b')',
                        b'[' => b']',
                        _ => b'}',
                    };
                    if byte != expected {
                        return None;
                    }
                    close.insert(open, index);
                    if opens_macro_input(bytes, open) {
                        macro_inputs.push((open, index));
                    }
                }
                _ => {}
            }
        }
        stack.is_empty().then_some(Self {
            close,
            macro_inputs,
        })
    }

    fn close_of(&self, open: usize) -> Option<usize> {
        self.close.get(&open).copied()
    }

    fn inside_macro_input(&self, start: usize, end: usize) -> bool {
        self.macro_inputs
            .iter()
            .any(|&(open, close)| open < start && end <= close)
    }
}

/// `name!(`, `name![`, `name! {` or `macro_rules! name {`.
fn opens_macro_input(bytes: &[u8], open: usize) -> bool {
    let Some(before) = last_code_byte_before(bytes, open) else {
        return false;
    };
    if bytes[before] == b'!' {
        return before > 0 && is_ident_byte(bytes[before - 1]);
    }
    if !is_ident_byte(bytes[before]) {
        return false;
    }
    let mut start = before;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    last_code_byte_before(bytes, start).is_some_and(|bang| bytes[bang] == b'!')
}

fn last_code_byte_before(bytes: &[u8], at: usize) -> Option<usize> {
    (0..at)
        .rev()
        .find(|&index| !bytes[index].is_ascii_whitespace())
}

/// After `if` / `while` at `after_keyword`: the end (exclusive) of the
/// block when the whole condition is `false` or `!true`.
fn never_entered_block_end(bytes: &[u8], after_keyword: usize, groups: &Groups) -> Option<usize> {
    let mut at = skip_whitespace(bytes, after_keyword);
    if bytes.get(at) == Some(&b'!') {
        at = word_end(bytes, skip_whitespace(bytes, at + 1), "true")?;
    } else {
        at = word_end(bytes, at, "false")?;
    }
    let open = skip_whitespace(bytes, at);
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    groups.close_of(open).map(|close| close + 1)
}

/// After `let` at `after_let`: the end (exclusive) of a
/// `let [mut] NAME = [move] |..| ..;` statement whose `NAME` is never named
/// again in the following text.
fn uncalled_closure_statement_end(
    masked: &str,
    after_let: usize,
    groups: &Groups,
) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut at = skip_whitespace(bytes, after_let);
    if let Some(after_mut) = word_end(bytes, at, "mut") {
        at = skip_whitespace(bytes, after_mut);
    }
    if !bytes.get(at).copied().is_some_and(is_ident_start) {
        return None;
    }
    let name_end = ident_end(bytes, at);
    let name = &masked[at..name_end];
    if matches!(name, "mut" | "ref") {
        return None;
    }
    at = skip_whitespace(bytes, name_end);
    if bytes.get(at) != Some(&b'=') || bytes.get(at + 1) == Some(&b'=') {
        return None;
    }
    at = skip_whitespace(bytes, at + 1);
    if let Some(after_move) = word_end(bytes, at, "move") {
        at = skip_whitespace(bytes, after_move);
    }
    if bytes.get(at) != Some(&b'|') {
        return None;
    }
    let semicolon = statement_semicolon(bytes, at, groups)?;
    let rest = &masked[semicolon + 1..];
    (!names_word(rest, name)).then_some(semicolon + 1)
}

/// At an identifier `start` that begins a statement: the never-executed
/// region after the statement when it diverges unconditionally.
fn after_diverging_statement(masked: &str, start: usize, groups: &Groups) -> Option<Region> {
    let bytes = masked.as_bytes();
    let head_end = diverging_head_end(masked, start)?;
    let semicolon = statement_semicolon(bytes, head_end, groups)?;
    let region_start = semicolon + 1;
    let region_end = region_end_in_block(masked, region_start, groups)?;
    Some(Region {
        start: region_start,
        end: region_end,
        admission_refused: masked[start..].starts_with("return"),
    })
}

/// The end of a diverging statement's head: `return`, a diverging macro
/// name with its `!` (optionally `std::` / `core::` qualified) followed by
/// its delimiter, or `std::process::exit` / `process::exit` followed by `(`.
fn diverging_head_end(masked: &str, start: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut segments = Vec::new();
    let mut at = start;
    loop {
        let end = ident_end(bytes, at);
        segments.push(&masked[at..end]);
        if bytes.get(end) == Some(&b':')
            && bytes.get(end + 1) == Some(&b':')
            && bytes.get(end + 2).copied().is_some_and(is_ident_start)
        {
            at = end + 2;
            continue;
        }
        at = end;
        break;
    }
    if segments == ["return"] {
        let next = bytes.get(at).copied();
        return next
            .is_none_or(|byte| byte.is_ascii_whitespace() || matches!(byte, b';' | b'('))
            .then_some(at);
    }
    if bytes.get(at) == Some(&b'!') {
        let (last, prefix) = segments.split_last()?;
        let qualified = matches!(prefix, [] | ["std"] | ["core"]);
        let diverges = matches!(*last, "panic" | "unreachable" | "todo" | "unimplemented");
        let delimiter = skip_whitespace(bytes, at + 1);
        return (qualified && diverges && matches!(bytes.get(delimiter), Some(b'(' | b'[' | b'{')))
            .then_some(delimiter);
    }
    let exits = matches!(
        segments.as_slice(),
        ["std", "process", "exit"] | ["process", "exit"]
    );
    let open = skip_whitespace(bytes, at);
    (exits && bytes.get(open) == Some(&b'(')).then_some(open)
}

/// The first `;` at the statement's own depth from `from`, skipping nested
/// groups. `None` when the enclosing group closes first.
fn statement_semicolon(bytes: &[u8], from: usize, groups: &Groups) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b';' => return Some(at),
            b'(' | b'[' | b'{' => at = groups.close_of(at)? + 1,
            b')' | b']' | b'}' => return None,
            _ => at += 1,
        }
    }
    None
}

/// From `from`, the end (exclusive) of the never-executed tail: the closing
/// `}` of the enclosing block, or an earlier item declaration or attribute
/// at statement start. `None` when the enclosing group is not a block.
fn region_end_in_block(masked: &str, from: usize, groups: &Groups) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut previous = b';';
    let mut at = from;
    while at < bytes.len() {
        let byte = bytes[at];
        let at_statement_start = matches!(previous, b'{' | b';' | b'}');
        match byte {
            _ if byte.is_ascii_whitespace() => at += 1,
            b'}' => return Some(at),
            b')' | b']' => return None,
            b'(' | b'[' | b'{' => {
                let close = groups.close_of(at)?;
                previous = bytes[close];
                at = close + 1;
            }
            b'#' if at_statement_start => return Some(at),
            _ if is_ident_start(byte) && !(at > 0 && is_ident_byte(bytes[at - 1])) => {
                let end = ident_end(bytes, at);
                if at_statement_start && is_item_keyword(&masked[at..end]) {
                    return Some(at);
                }
                previous = bytes[end - 1];
                at = end;
            }
            _ => {
                previous = byte;
                at += 1;
            }
        }
    }
    None
}

fn is_item_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "struct"
            | "enum"
            | "union"
            | "impl"
            | "trait"
            | "mod"
            | "use"
            | "const"
            | "static"
            | "type"
            | "extern"
            | "pub"
            | "unsafe"
            | "async"
            | "macro_rules"
    )
}

/// Line offsets whose every non-whitespace byte lies inside `regions`.
fn lines_inside(bytes: &[u8], regions: &[Region]) -> BTreeMap<usize, Unreached> {
    let mut lines = BTreeMap::new();
    if regions.is_empty() {
        return lines;
    }
    // (inside any region, inside an admission-refused region)
    let reach = |index: usize| {
        regions
            .iter()
            .filter(|region| region.start <= index && index < region.end)
            .fold((false, false), |(_, refused), region| {
                (true, refused || region.admission_refused)
            })
    };
    let mut line = 0usize;
    let mut has_code = false;
    let mut all_inside = true;
    let mut all_refused = true;
    let mut close_line = |line: usize, has_code: bool, all_inside: bool, all_refused: bool| {
        if has_code && all_inside {
            let kind = if all_refused {
                Unreached::AdmissionRefused
            } else {
                Unreached::Unrefused
            };
            lines.insert(line, kind);
        }
    };
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            close_line(line, has_code, all_inside, all_refused);
            line += 1;
            has_code = false;
            all_inside = true;
            all_refused = true;
            continue;
        }
        if byte.is_ascii_whitespace() || !all_inside {
            has_code |= !byte.is_ascii_whitespace();
            continue;
        }
        has_code = true;
        let (inside, refused) = reach(index);
        all_inside = inside;
        all_refused &= refused;
    }
    close_line(line, has_code, all_inside, all_refused);
    lines
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// End of `word` at `at` when it stands there as a whole identifier.
fn word_end(bytes: &[u8], at: usize, word: &str) -> Option<usize> {
    let end = at + word.len();
    (bytes.get(at..end) == Some(word.as_bytes())
        && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte)))
    .then_some(end)
}

fn names_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(relative) = text[from..].find(word) {
        let at = from + relative;
        let after = at + word.len();
        if (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(after).is_none_or(|byte| !is_ident_byte(*byte))
        {
            return true;
        }
        from = at + 1;
    }
    false
}

fn ident_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while bytes.get(end).copied().is_some_and(is_ident_byte) {
        end += 1;
    }
    end
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
