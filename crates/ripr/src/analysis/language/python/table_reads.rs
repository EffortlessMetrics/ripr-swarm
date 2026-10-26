//! Whether a source only reads a module-level table (#9619).
//!
//! A module constant bound to a dict literal is mutable in place:
//! `RATES["gold"] = 1`, `RATES.update(...)` or `alias = RATES; alias[k] = v`
//! change what a reader function returns without rebinding the name, so the
//! scalar rules (bound once, no attribute write) do not hold the table still.
//! A source is safe for the table `NAME` only when every mention of `NAME` is
//! one of:
//!
//! - an import or export of the name (`from .table import NAME`), unless it is
//!   renamed (`import ... as other` hides later uses behind another name);
//! - the receiver of a read, `NAME.get(...)`;
//! - the receiver of a subscript on a line that assigns, deletes, loops or
//!   binds nothing (`return NAME[key]`);
//! - the defining `NAME = {...}` statement, in the module that owns it.
//!
//! Anything else (a bare use, `NAME.update`, a store through a subscript, an
//! alias, a mention inside an f-string) is a possible write. The scan reads
//! tokens, so a lexing error marks every mentioned name unsafe.

use rustpython_parser::{Mode, Tok, lexer::lex};
use std::collections::BTreeSet;

/// The names of `watched` that `source` may mutate or alias. `defining_line`
/// is the 1-indexed line of the one `NAME = {...}` statement the owning module
/// is allowed to contain.
pub(super) fn unsafe_table_uses(
    source: &str,
    watched: &dyn Fn(&str) -> bool,
    defining_line: Option<usize>,
) -> BTreeSet<String> {
    let mut tokens = Vec::new();
    for token in lex(source, Mode::Module) {
        match token {
            // Indentation tokens lead a dedented line; they are not part of it.
            Ok((Tok::Indent | Tok::Dedent, _)) => {}
            Ok((tok, range)) => {
                tokens.push((tok, usize::from(range.start()), usize::from(range.end())));
            }
            Err(_) => return all_mentioned(source, watched),
        }
    }
    let mut unsafe_names = BTreeSet::new();
    let mut start = 0;
    while start < tokens.len() {
        let end = tokens[start..]
            .iter()
            .position(|(tok, _, _)| matches!(tok, Tok::Newline | Tok::Semi))
            .map_or(tokens.len(), |offset| start + offset);
        scan_line(
            source,
            &tokens[start..end],
            watched,
            defining_line,
            &mut unsafe_names,
        );
        start = end + 1;
    }
    unsafe_names
}

fn all_mentioned(source: &str, watched: &dyn Fn(&str) -> bool) -> BTreeSet<String> {
    source
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty() && watched(word))
        .map(str::to_string)
        .collect()
}

fn scan_line(
    source: &str,
    line: &[(Tok, usize, usize)],
    watched: &dyn Fn(&str) -> bool,
    defining_line: Option<usize>,
    unsafe_names: &mut BTreeSet<String>,
) {
    let imports = matches!(line.first(), Some((Tok::From | Tok::Import, _, _)));
    let assigns = line_binds(line);
    // Counted only for the rare watched name that opens a line, so a large
    // file is not rescanned from its start for every logical line.
    let line_number = |offset: usize| {
        source[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1
    };
    for (index, (tok, start, end)) in line.iter().enumerate() {
        if let Tok::String { kind, .. } = tok
            && kind.is_any_fstring()
        {
            // A formatted expression hides inside the string token, which
            // can span lines.
            for word in all_mentioned(&source[*start..*end], watched) {
                unsafe_names.insert(word);
            }
            continue;
        }
        let Tok::Name { name } = tok else { continue };
        if !watched(name) {
            continue;
        }
        let next = line.get(index + 1).map(|(tok, _, _)| tok);
        let safe = if imports {
            !matches!(next, Some(Tok::As))
        } else if index == 0
            && defining_line.is_some_and(|defining| defining == line_number(line[0].1))
            && matches!(next, Some(Tok::Equal | Tok::Colon))
        {
            true
        } else if matches!(next, Some(Tok::Dot)) {
            matches!(
                (line.get(index + 2), line.get(index + 3)),
                (Some((Tok::Name { name: method }, _, _)), Some((Tok::Lpar, _, _))) if method.as_str() == "get"
            )
        } else if matches!(next, Some(Tok::Lsqb)) {
            !assigns
        } else {
            false
        };
        if !safe {
            unsafe_names.insert(name.to_string());
        }
    }
}

/// Whether the logical line can bind or delete a target anywhere: an
/// assignment at the top level, an augmented assignment, a walrus, `del`, a
/// loop or comprehension, or a `with` target. Keyword arguments sit inside
/// brackets and do not count.
fn line_binds(line: &[(Tok, usize, usize)]) -> bool {
    let mut depth = 0usize;
    for (tok, _, _) in line {
        match tok {
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => depth = depth.saturating_sub(1),
            Tok::Equal if depth == 0 => return true,
            Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::PercentEqual
            | Tok::AmperEqual
            | Tok::VbarEqual
            | Tok::CircumflexEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::DoubleStarEqual
            | Tok::DoubleSlashEqual
            | Tok::AtEqual
            | Tok::ColonEqual
            | Tok::Del
            | Tok::For
            | Tok::With => return true,
            _ => {}
        }
    }
    false
}

/// Names bound at module scope to a dict literal: `NAME = {` or
/// `NAME: annotation = {` at the start of a top-level logical line.
pub(super) fn dict_literal_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut depth = 0usize;
    let mut line_start = true;
    let mut candidate: Option<String> = None;
    // Set by `=`, kept through opening parentheses: `NAME = ({...})` is the
    // same dict as `NAME = {...}`.
    let mut armed = false;
    for token in lex(source, Mode::Module) {
        let Ok((tok, _)) = token else { break };
        match &tok {
            // Indentation tokens lead a line and are not its first token.
            Tok::Indent => {
                depth += 1;
                continue;
            }
            Tok::Dedent => {
                depth = depth.saturating_sub(1);
                continue;
            }
            Tok::Newline => {
                line_start = true;
                candidate = None;
                armed = false;
                continue;
            }
            Tok::Name { name } if line_start && depth == 0 => candidate = Some(name.to_string()),
            Tok::Equal => armed = true,
            Tok::Lpar if armed => {}
            Tok::Lbrace if armed => {
                if let Some(name) = candidate.take() {
                    names.insert(name);
                }
                armed = false;
            }
            _ => armed = false,
        }
        line_start = false;
    }
    names
}

#[cfg(test)]
#[path = "table_reads_tests.rs"]
mod tests;
