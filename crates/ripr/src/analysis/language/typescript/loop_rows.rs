//! The literal rows a test registered inside a `for (const [a, b] of rows)`
//! loop runs with.
//!
//! The test callback sees `a` and `b` only as loop variables. When the loop
//! iterates a file-local `const rows = [[1, 100], [2.5, 250]]` that nothing
//! else touches, each destructured name has one literal per row, so a reader
//! that folds owner-call arguments can fold each row. Anything else — a
//! computed table, a nested or defaulted pattern, a second reference to the
//! table — reads as no rows, and the caller keeps treating the arguments as
//! unreadable.

use super::*;

const MAX_LOOP_ROWS: usize = 32;

/// The destructured names of a loop header and one cell per name for every
/// row of the table it iterates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoopRows {
    names: Vec<Option<String>>,
    rows: Vec<Vec<String>>,
    /// The loop header through its opening brace and the table declaration:
    /// the only file text outside the test that the rows account for.
    inert_text: Vec<String>,
}

impl LoopRows {
    pub(crate) fn inert_text(&self) -> &[String] {
        &self.inert_text
    }

    /// One argument list per row when some argument is a loop variable;
    /// the arguments unchanged when none is.
    pub(crate) fn expand(&self, arguments: &[String]) -> Vec<Vec<String>> {
        let column = |argument: &str| {
            let argument = argument.trim();
            self.names
                .iter()
                .position(|name| name.as_deref() == Some(argument))
        };
        if !arguments.iter().any(|argument| column(argument).is_some()) {
            return vec![arguments.to_vec()];
        }
        self.rows
            .iter()
            .map(|row| {
                arguments
                    .iter()
                    .map(|argument| match column(argument) {
                        Some(index) => row[index].clone(),
                        None => argument.clone(),
                    })
                    .collect()
            })
            .collect()
    }
}

fn is_identifier(text: &str) -> bool {
    text.starts_with(|c: char| c.is_alphabetic() || matches!(c, '_' | '$'))
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
}

/// The offset of the bracket closing the one at `open`, in masked text.
fn matching_close(masked: &str, open: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Top-level comma-separated pieces of `source[start..end]` as offsets.
fn split_top_level(masked: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut pieces = Vec::new();
    let mut depth = 0usize;
    let mut piece_start = start;
    for (offset, byte) in masked.as_bytes()[start..end].iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                pieces.push((piece_start, start + offset));
                piece_start = start + offset + 1;
            }
            _ => {}
        }
    }
    pieces.push((piece_start, end));
    pieces
}

fn whole_word_count(masked: &str, word: &str) -> usize {
    masked
        .match_indices(word)
        .filter(|(at, _)| {
            let before = masked[..*at].chars().next_back();
            let after = masked[at + word.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
                && !after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
        })
        .count()
}

/// The `for (const [..] of <table>) {` loop whose block contains `body_at`,
/// innermost first, resolved to its literal rows.
pub(crate) fn ts_enclosing_loop_rows(source: &str, body_text: &str) -> Option<LoopRows> {
    let masked = ts_mask_literals_and_comments(source);
    if masked.len() != source.len() {
        return None;
    }
    let body_at = source.find(body_text)?;
    let mut innermost: Option<(usize, usize, usize, usize, usize)> = None;
    for (at, _) in masked.match_indices("for") {
        if masked[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            continue;
        }
        let after = masked[at + 3..].trim_start();
        let paren_at = masked.len() - after.len();
        if !after.starts_with('(') {
            continue;
        }
        let Some(paren_end) = matching_close(&masked, paren_at) else {
            continue;
        };
        let block_at = masked[paren_end + 1..]
            .find(|c: char| !c.is_whitespace())
            .map(|rel| paren_end + 1 + rel)?;
        if masked.as_bytes()[block_at] != b'{' {
            continue;
        }
        let Some(block_end) = matching_close(&masked, block_at) else {
            continue;
        };
        if block_at < body_at
            && body_at < block_end
            && innermost.is_none_or(|(_, _, _, b, _)| block_at > b)
        {
            innermost = Some((at, paren_at, paren_end, block_at, block_end));
        }
    }
    let (for_at, paren_at, paren_end, block_at, _) = innermost?;
    let header = source[paren_at + 1..paren_end].trim();
    let (left, table) = header.split_once(" of ")?;
    let table = table.trim();
    if !is_identifier(table) {
        return None;
    }
    let pattern = left.trim().strip_prefix("const ")?.trim();
    let inner = pattern.strip_prefix('[')?.strip_suffix(']')?;
    let names: Vec<Option<String>> = inner
        .split(',')
        .map(|name| {
            let name = name.trim();
            if name.is_empty() {
                Some(None)
            } else {
                is_identifier(name).then(|| Some(name.to_string()))
            }
        })
        .collect::<Option<_>>()?;
    // The table is declared once, and referenced only there and by the loop.
    if whole_word_count(&masked, table) != 2 {
        return None;
    }
    let declaration = format!("const {table}");
    let declared_at = masked.find(&declaration)?;
    if masked[..declared_at]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
    {
        return None;
    }
    let after_name = declared_at + declaration.len();
    let equals = masked[after_name..].find('=')?;
    let between = &masked[after_name..after_name + equals];
    if between.contains(['(', '{', ';', '\n']) && !between.trim_start().starts_with(':') {
        return None;
    }
    if !(between.trim().is_empty() || between.trim_start().starts_with(':')) {
        return None;
    }
    let value_at = masked[after_name + equals + 1..]
        .find(|c: char| !c.is_whitespace())
        .map(|rel| after_name + equals + 1 + rel)?;
    if masked.as_bytes()[value_at] != b'[' {
        return None;
    }
    let value_end = matching_close(&masked, value_at)?;
    let mut rows = Vec::new();
    for (start, end) in split_top_level(&masked, value_at + 1, value_end) {
        let element = masked[start..end].trim();
        if element.is_empty() {
            // A trailing comma leaves one empty piece.
            continue;
        }
        let lead = masked[start..end].len() - masked[start..end].trim_start().len();
        let row_at = start + lead;
        if !element.starts_with('[') {
            return None;
        }
        let row_end = matching_close(&masked, row_at)?;
        if row_at + element.len() != row_end + 1 {
            return None;
        }
        let cells: Vec<String> = split_top_level(&masked, row_at + 1, row_end)
            .into_iter()
            .filter(|(s, e)| !masked[*s..*e].trim().is_empty())
            .map(|(s, e)| source[s..e].trim().to_string())
            .collect();
        if cells.len() < names.len() || cells.iter().any(|cell| cell.contains(['\n', '\r'])) {
            return None;
        }
        rows.push(cells);
    }
    if rows.is_empty() || rows.len() > MAX_LOOP_ROWS {
        return None;
    }
    let declaration_end = match masked[value_end + 1..].trim_start().starts_with(';') {
        true => value_end + 1 + masked[value_end + 1..].find(';')? + 1,
        false => value_end + 1,
    };
    let inert_text = vec![
        source[for_at..=block_at].to_string(),
        source[declared_at..declaration_end].to_string(),
    ];
    Some(LoopRows {
        names,
        rows,
        inert_text,
    })
}
