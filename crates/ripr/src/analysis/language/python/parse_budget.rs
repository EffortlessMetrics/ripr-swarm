//! Pre-parse nesting budgets for the Python preview adapter (#4109).
//!
//! `rustpython_parser` is recursive in bracket nesting. A discovered file with
//! deep bracket nesting overflows the process stack before any finding can be
//! emitted, including when that file is not in the diff. This scanner refuses
//! the file first.
//!
//! Brackets inside comments are ignored. Brackets inside string literals are
//! counted: that over-declines a literal full of brackets (fail-closed) and
//! still sees f-string expressions, which the parser recurses into. A `#`
//! inside a string must not be treated as a comment, or the rest of the line
//! — including real nesting — would be skipped.
//!
//! The same pass also estimates syntax-tree depth that brackets do not show.
//! The LR parser builds `x + 1 + 1 + ...`, `not not ... x`, attribute/call
//! chains and `elif` chains without deep recursion, but the result is one AST
//! level per operator or `elif`. Both the source-fact walker and the AST's
//! own `Drop` recurse per level, so a long enough chain aborts the process
//! after the parse succeeds. The tree estimate refuses such a file before it
//! is parsed, so neither the walker nor the drop ever sees the deep tree.

pub(in crate::analysis::language::python) const MAX_PYTHON_PARSE_NESTING_DEPTH: usize = 128;

/// Maximum estimated syntax-tree depth accepted for a parse.
///
/// Measured on Linux debug builds with a 2 MiB thread stack: source-fact
/// extraction overflowed at roughly 1,900 levels of operator, unary,
/// attribute, call, ternary, lambda or `elif` nesting (about 1 KiB of stack
/// per level); the AST drop alone overflowed at roughly 13,000 levels. The
/// budget keeps a wide margin under a 1 MiB main-thread stack (Windows) and
/// larger debug frames, while real code rarely chains more than a few dozen
/// operators in one comma-separated operand or a few dozen `elif` arms.
pub(in crate::analysis::language::python) const MAX_PYTHON_PARSE_TREE_DEPTH: usize = 256;

pub(in crate::analysis::language::python) fn nesting_budget_reason(source: &str) -> Option<String> {
    match scan_nesting(
        source,
        MAX_PYTHON_PARSE_NESTING_DEPTH,
        MAX_PYTHON_PARSE_TREE_DEPTH,
    ) {
        Some(Exceeded::Brackets) => Some(format!(
            "parse_budget: nesting depth exceeded {MAX_PYTHON_PARSE_NESTING_DEPTH}"
        )),
        Some(Exceeded::Tree) => Some(format!(
            "parse_budget: syntax tree depth estimate exceeded {MAX_PYTHON_PARSE_TREE_DEPTH}"
        )),
        None => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exceeded {
    Brackets,
    Tree,
}

enum ScanMode {
    Code,
    Comment,
    String {
        quote: u8,
        triple: bool,
        raw: bool,
        escaped: bool,
    },
}

/// Lexical estimate of how deep the parsed AST will be.
///
/// This is an upper-leaning approximation, not a parse. Within one operand
/// segment, every binary operator and ternary `if` nests the whole left side
/// one level deeper (`ops`), while prefix operators (`not`, unary `-`/`+`/`~`,
/// `await`, `lambda`, star-args) and trailers (`.attr`, calls, subscripts)
/// deepen only the operand they belong to (`operand`). Tokens that separate
/// sibling operands (`,`, `;`, `=`, comparison operators, `and`, `or`, `in`,
/// `is`) and logical line ends start a new segment, so long flat literals and
/// sequential statements stay cheap. Each open code bracket saves the
/// enclosing segment, and on close the bracket's deepest content becomes a
/// trailer of the enclosing operand. Statement nesting is the indentation
/// depth plus the length of each open `elif` chain, because every `elif` is
/// parsed as an `If` nested in the previous arm's `orelse`.
#[derive(Clone, Copy, Default)]
struct Segment {
    /// Binary operators and ternaries applied so far in this operand run.
    ops: usize,
    /// Deepest finished operand in this run.
    max_operand: usize,
    /// Prefix and trailer depth of the operand being read.
    operand: usize,
    /// Deepest finished sibling run inside the current bracket.
    siblings: usize,
    /// The previous significant token ended an operand, so a following
    /// `-`/`+`/`*` is binary rather than prefix.
    after_operand: bool,
}

impl Segment {
    fn depth(&self) -> usize {
        self.ops + self.max_operand.max(self.operand)
    }

    fn deepest(&self) -> usize {
        self.siblings.max(self.depth())
    }

    fn binary(&mut self) {
        self.ops += 1;
        self.max_operand = self.max_operand.max(self.operand);
        self.operand = 0;
        self.after_operand = false;
    }

    fn prefix(&mut self) {
        self.operand += 1;
        self.after_operand = false;
    }

    fn separator(&mut self) {
        *self = Self {
            siblings: self.deepest(),
            ..Self::default()
        };
    }
}

struct TreeEstimate {
    /// Enclosing segment saved at each open code bracket.
    brackets: Vec<Segment>,
    /// Sum of each saved segment's depth plus one per open bracket.
    saved: usize,
    current: Segment,
    /// Open indentation levels as (width, `elif` arms seen at that level).
    indents: Vec<(usize, usize)>,
    /// `indents.len()` plus every open `elif` count.
    statements: usize,
    /// The next significant token starts a logical line.
    line_start: bool,
}

impl TreeEstimate {
    fn new() -> Self {
        Self {
            brackets: Vec::new(),
            saved: 0,
            current: Segment::default(),
            indents: Vec::new(),
            statements: 0,
            line_start: true,
        }
    }

    fn depth(&self) -> usize {
        self.statements + self.saved + self.current.depth()
    }

    fn open(&mut self) {
        self.saved += self.current.depth() + 1;
        self.brackets.push(self.current);
        self.current = Segment::default();
    }

    fn close(&mut self) {
        let inner = self.current.deepest();
        if let Some(outer) = self.brackets.pop() {
            self.saved -= outer.depth() + 1;
            self.current = outer;
        }
        self.current.operand = self.current.operand.max(inner) + 1;
        self.current.after_operand = true;
    }

    fn operand(&mut self) {
        self.current.after_operand = true;
    }

    fn logical_line_end(&mut self) {
        if self.brackets.is_empty() {
            self.current = Segment::default();
            self.line_start = true;
        }
    }

    fn indentation(&mut self, width: usize) {
        while let Some(&(level, elifs)) = self.indents.last() {
            if level <= width {
                break;
            }
            self.indents.pop();
            self.statements -= 1 + elifs;
        }
        if self.indents.last().is_none_or(|&(level, _)| level < width) {
            self.indents.push((width, 0));
            self.statements += 1;
        }
    }

    /// First significant token of a logical line: an `elif` extends the open
    /// chain at this indentation, `else` keeps it, anything else ends it.
    fn statement_start(&mut self, word: &[u8]) {
        self.line_start = false;
        let Some(level) = self.indents.last_mut() else {
            return;
        };
        match word {
            b"elif" => {
                level.1 += 1;
                self.statements += 1;
            }
            b"else" => {}
            _ => {
                self.statements -= level.1;
                level.1 = 0;
            }
        }
    }

    fn word(&mut self, word: &[u8]) {
        match word {
            b"not" | b"lambda" | b"await" => self.current.prefix(),
            b"if" => self.current.binary(),
            b"else" => {
                self.current.max_operand = self.current.max_operand.max(self.current.operand);
                self.current.operand = 0;
                self.current.after_operand = false;
            }
            b"and" | b"or" | b"in" | b"is" => self.current.separator(),
            _ => self.operand(),
        }
    }
}

fn scan_nesting(source: &str, bracket_budget: usize, tree_budget: usize) -> Option<Exceeded> {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut mode = ScanMode::Code;
    let mut depth = 0usize;
    let mut tree = TreeEstimate::new();
    while index < bytes.len() {
        let byte = bytes[index];
        match mode {
            ScanMode::Comment => {
                if byte == b'\n' {
                    mode = ScanMode::Code;
                    tree.logical_line_end();
                }
                index += 1;
            }
            ScanMode::String {
                quote,
                triple,
                raw,
                escaped,
            } => {
                if note_bracket(byte, &mut depth, bracket_budget) {
                    return Some(Exceeded::Brackets);
                }
                if !raw && escaped {
                    mode = ScanMode::String {
                        quote,
                        triple,
                        raw,
                        escaped: false,
                    };
                    index += 1;
                    continue;
                }
                if !raw && byte == b'\\' {
                    mode = ScanMode::String {
                        quote,
                        triple,
                        raw,
                        escaped: true,
                    };
                    index += 1;
                    continue;
                }
                if byte == quote && raw_quote_can_terminate(bytes, index, raw) {
                    if triple {
                        if index + 2 < bytes.len()
                            && bytes[index + 1] == quote
                            && bytes[index + 2] == quote
                        {
                            mode = ScanMode::Code;
                            index += 3;
                            continue;
                        }
                    } else {
                        mode = ScanMode::Code;
                        index += 1;
                        continue;
                    }
                }
                index += 1;
            }
            ScanMode::Code => {
                if tree.line_start {
                    let width = leading_whitespace(bytes, index);
                    let next = bytes.get(index + width).copied();
                    if matches!(next, None | Some(b'\n' | b'\r' | b'#' | b'\\')) {
                        // Blank, comment-only or continuation-only line:
                        // not a statement, so indentation is unchanged.
                        index += width;
                        if next == Some(b'#') {
                            mode = ScanMode::Comment;
                            index += 1;
                        } else if next.is_some() {
                            index += 1;
                        }
                        continue;
                    }
                    tree.indentation(width);
                    index += width;
                    let word_end = identifier_end(bytes, index);
                    tree.statement_start(&bytes[index..word_end]);
                    if tree.depth() > tree_budget {
                        return Some(Exceeded::Tree);
                    }
                    continue;
                }
                if byte == b'#' {
                    mode = ScanMode::Comment;
                    index += 1;
                    continue;
                }
                if byte == b'\'' || byte == b'"' {
                    let triple = index + 2 < bytes.len()
                        && bytes[index + 1] == byte
                        && bytes[index + 2] == byte;
                    mode = ScanMode::String {
                        quote: byte,
                        triple,
                        raw: prefix_is_raw(bytes, index),
                        escaped: false,
                    };
                    tree.operand();
                    index += if triple { 3 } else { 1 };
                    continue;
                }
                if note_bracket(byte, &mut depth, bracket_budget) {
                    return Some(Exceeded::Brackets);
                }
                let word_end = identifier_end(bytes, index);
                if word_end > index {
                    tree.word(&bytes[index..word_end]);
                    index = word_end;
                } else {
                    index += tree_token(&mut tree, bytes, index);
                }
                if tree.depth() > tree_budget {
                    return Some(Exceeded::Tree);
                }
            }
        }
    }
    None
}

/// Apply one non-identifier code byte to the tree estimate and return how
/// many bytes it consumed.
fn tree_token(tree: &mut TreeEstimate, bytes: &[u8], index: usize) -> usize {
    let byte = bytes[index];
    match byte {
        b'(' | b'[' | b'{' => tree.open(),
        b')' | b']' | b'}' => tree.close(),
        b',' | b';' | b'=' | b'!' => tree.current.separator(),
        b'<' | b'>' => {
            if bytes.get(index + 1) == Some(&byte) {
                // `<<` / `>>` are binary operators.
                tree.current.binary();
                return 2;
            }
            // Comparisons are one flat node over sibling operands.
            tree.current.separator();
        }
        b'*' | b'/' if bytes.get(index + 1) == Some(&byte) => {
            // `**` / `//` are one operator (or `**kwargs`).
            binary_or_prefix(tree);
            return 2;
        }
        b'+' | b'-' | b'*' | b'/' | b'%' | b'@' | b'&' | b'|' | b'^' => binary_or_prefix(tree),
        b'~' => tree.current.prefix(),
        b'.' if tree.current.after_operand => tree.current.operand += 1,
        b'0'..=b'9' => tree.operand(),
        b'\n' if !line_is_continued(bytes, index) => tree.logical_line_end(),
        _ => {}
    }
    1
}

/// A backslash before the newline (optionally before `\r\n`) joins lines.
fn line_is_continued(bytes: &[u8], newline: usize) -> bool {
    newline > 0 && bytes[newline - 1] == b'\\'
        || newline > 1 && bytes[newline - 1] == b'\r' && bytes[newline - 2] == b'\\'
}

fn binary_or_prefix(tree: &mut TreeEstimate) {
    if tree.current.after_operand {
        tree.current.binary();
    } else {
        tree.current.prefix();
    }
}

fn leading_whitespace(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\x0c'))
        .count()
}

/// End of the identifier (or keyword) starting at `start`; `start` itself
/// when no identifier starts there. Non-ASCII bytes are identifier bytes.
fn identifier_end(bytes: &[u8], start: usize) -> usize {
    match bytes.get(start) {
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' || !byte.is_ascii() => {}
        _ => return start,
    }
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || !bytes[end].is_ascii())
    {
        end += 1;
    }
    end
}

fn note_bracket(byte: u8, depth: &mut usize, budget: usize) -> bool {
    match byte {
        b'(' | b'[' | b'{' => {
            *depth += 1;
            *depth > budget
        }
        b')' | b']' | b'}' => {
            *depth = depth.saturating_sub(1);
            false
        }
        _ => false,
    }
}

/// Raw strings keep a quote that is preceded by an odd number of backslashes.
/// Non-raw escapes are consumed before this check, so they always terminate.
fn raw_quote_can_terminate(bytes: &[u8], index: usize, raw: bool) -> bool {
    if !raw {
        return true;
    }
    let mut slashes = 0usize;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes.is_multiple_of(2)
}

fn prefix_is_raw(bytes: &[u8], quote_index: usize) -> bool {
    let mut start = quote_index;
    while start > 0
        && matches!(
            bytes[start - 1],
            b'r' | b'R' | b'u' | b'U' | b'b' | b'B' | b'f' | b'F'
        )
    {
        start -= 1;
    }
    if start > 0 {
        let previous = bytes[start - 1];
        if previous.is_ascii_alphanumeric() || previous == b'_' || !previous.is_ascii() {
            return false;
        }
    }
    bytes[start..quote_index]
        .iter()
        .any(|byte| matches!(byte, b'r' | b'R'))
}

#[cfg(test)]
mod tests {
    use super::super::source_facts::extract_source_facts;
    use super::{
        MAX_PYTHON_PARSE_NESTING_DEPTH, MAX_PYTHON_PARSE_TREE_DEPTH, nesting_budget_reason,
    };
    use std::path::Path;

    /// 5,000 levels overflow a 2 MiB debug test thread in the source-fact
    /// walker (measured overflow near 1,900) when the file reaches the parser.
    const OVERFLOWING_CHAIN: usize = 5_000;

    fn tree_reason() -> String {
        format!("parse_budget: syntax tree depth estimate exceeded {MAX_PYTHON_PARSE_TREE_DEPTH}")
    }

    fn binary_chain(terms: usize) -> String {
        format!("def f(x):\n    return x{}\n", " + 1".repeat(terms))
    }

    fn unary_chain(terms: usize) -> String {
        format!("def f(x):\n    return {}x\n", "not ".repeat(terms))
    }

    fn elif_chain(arms: usize) -> String {
        let mut source = String::from("def f(x):\n    if x == 0:\n        return 0\n");
        for arm in 1..arms {
            source.push_str(&format!("    elif x == {arm}:\n        return {arm}\n"));
        }
        source
    }

    fn assert_degrades(label: &str, source: &str) -> Result<(), String> {
        let facts = extract_source_facts(Path::new("deep.py"), source);
        let evidence: Vec<&str> = facts
            .limitations
            .iter()
            .map(|limitation| limitation.evidence.as_str())
            .collect();
        let expected = format!("source_fact_parse_error: {}", tree_reason());
        if evidence != [expected.as_str()] {
            return Err(format!(
                "{label}: expected exactly the tree-depth refusal, got {evidence:?}"
            ));
        }
        if !facts.facts.is_empty() || !facts.owners.is_empty() {
            return Err(format!("{label}: a refused file must not produce facts"));
        }
        Ok(())
    }

    fn assert_parses(label: &str, source: &str) -> Result<(), String> {
        if let Some(reason) = nesting_budget_reason(source) {
            return Err(format!("{label}: must stay under the budget, got {reason}"));
        }
        let facts = extract_source_facts(Path::new("ok.py"), source);
        if !facts.limitations.is_empty() {
            return Err(format!(
                "{label}: unexpected limitations {:?}",
                facts
                    .limitations
                    .iter()
                    .map(|limitation| limitation.evidence.as_str())
                    .collect::<Vec<_>>()
            ));
        }
        if facts.facts.is_empty() {
            return Err(format!("{label}: an admitted file must produce facts"));
        }
        Ok(())
    }

    #[test]
    fn deep_operator_unary_and_elif_chains_degrade_instead_of_overflowing() -> Result<(), String> {
        assert_degrades("binary", &binary_chain(OVERFLOWING_CHAIN))?;
        assert_degrades("unary", &unary_chain(OVERFLOWING_CHAIN))?;
        assert_degrades("elif", &elif_chain(OVERFLOWING_CHAIN))?;
        assert_degrades(
            "attribute",
            &format!("x = y{}\n", ".a".repeat(OVERFLOWING_CHAIN)),
        )?;
        assert_degrades(
            "call",
            &format!("x = y{}\n", "()".repeat(OVERFLOWING_CHAIN)),
        )?;
        assert_degrades(
            "ternary",
            &format!("x = {}0\n", "1 if y else ".repeat(OVERFLOWING_CHAIN)),
        )?;
        assert_degrades(
            "lambda",
            &format!("x = {}0\n", "lambda: ".repeat(OVERFLOWING_CHAIN)),
        )?;
        assert_degrades(
            "continued lines",
            &format!("x = 1{}\n", " \\\n + 1".repeat(OVERFLOWING_CHAIN)),
        )?;
        Ok(())
    }

    #[test]
    fn tree_budget_boundary_is_exact_for_a_top_level_chain() -> Result<(), String> {
        // Module statement level (1) plus one level per `+`.
        let at_budget = format!("x = 1{}\n", " + 1".repeat(MAX_PYTHON_PARSE_TREE_DEPTH - 1));
        assert_parses("at budget", &at_budget)?;
        let over = format!("x = 1{}\n", " + 1".repeat(MAX_PYTHON_PARSE_TREE_DEPTH));
        if nesting_budget_reason(&over) != Some(tree_reason()) {
            return Err("one level past the budget must trip the tree estimate".to_string());
        }
        Ok(())
    }

    #[test]
    fn wide_but_shallow_code_stays_under_the_tree_budget() -> Result<(), String> {
        let long_list = format!("data = [{}]\n", "1 + 1, ".repeat(20_000));
        assert_parses("flat list", &long_list)?;
        let many_lines = "x = y + 1 - 2 * 3 / 4 % 5 @ z | w & v ^ u << 1 >> 2\n".repeat(2_000);
        assert_parses("sequential statements", &many_lines)?;
        let comparisons = format!("ok = a{}\n", " < b".repeat(2_000));
        assert_parses("chained comparison", &comparisons)?;
        let boolean = format!("ok = a{}\n", " and b or c".repeat(2_000));
        assert_parses("boolean chain", &boolean)?;
        let mut chains = String::from("def f(x):\n");
        for chain in 0..200 {
            chains.push_str(&format!("    if x == {chain}:\n        pass\n"));
            for arm in 0..10 {
                chains.push_str(&format!("    elif x == {arm}:\n        pass\n"));
            }
            chains.push_str("    else:\n        pass\n");
        }
        assert_parses("many short elif chains", &chains)?;
        assert_parses("elif chain under budget", &elif_chain(200))?;
        // idna's generated `uts46data.py` shape: a sum of calls is one BinOp
        // level per `+`, not two.
        let calls: Vec<String> = (0..200).map(|index| format!("_seg_{index}()")).collect();
        let sum_of_calls = format!("data = tuple({})\n", calls.join(" + "));
        assert_parses("sum of calls", &sum_of_calls)?;
        let string_operators = format!("s = \"{}\"\n", "-+.".repeat(5_000));
        assert_parses("operators inside a string", &string_operators)?;
        let comment_operators = format!("# {}\nx = 1\n", "-+.".repeat(5_000));
        assert_parses("operators inside a comment", &comment_operators)?;
        Ok(())
    }

    fn nested(depth: usize) -> String {
        let mut source = String::from("x = ");
        source.push_str(&"(".repeat(depth));
        source.push('1');
        source.push_str(&")".repeat(depth));
        source.push('\n');
        source
    }

    #[test]
    fn budget_allows_depth_128_and_refuses_129() -> Result<(), String> {
        if nesting_budget_reason(&nested(MAX_PYTHON_PARSE_NESTING_DEPTH)).is_some() {
            return Err(
                "depth 128 is under the abort threshold and must stay parseable".to_string(),
            );
        }
        let reason = nesting_budget_reason(&nested(MAX_PYTHON_PARSE_NESTING_DEPTH + 1))
            .ok_or_else(|| "depth 129 must trip the parse budget".to_string())?;
        if reason
            != format!("parse_budget: nesting depth exceeded {MAX_PYTHON_PARSE_NESTING_DEPTH}")
        {
            return Err(format!("unexpected budget reason: {reason}"));
        }
        Ok(())
    }

    #[test]
    fn flat_calls_and_comment_nesting_do_not_trip() -> Result<(), String> {
        let flat = format!("x = {}\n", "()".repeat(200));
        if nesting_budget_reason(&flat).is_some() {
            return Err("sequential calls are depth 1 and must not trip".to_string());
        }
        let comment = format!("# {}\nx = 1\n", "(".repeat(400));
        if nesting_budget_reason(&comment).is_some() {
            return Err("brackets inside a comment are not parser nesting".to_string());
        }
        Ok(())
    }

    #[test]
    fn string_hash_does_not_hide_following_code() -> Result<(), String> {
        let mut source = String::from("x = \"foo # \" + ");
        source.push_str(&nested(MAX_PYTHON_PARSE_NESTING_DEPTH + 1));
        if nesting_budget_reason(&source).is_none() {
            return Err("a # inside a string must not comment out the following call".to_string());
        }
        let mut raw = String::from("x = r\"foo \\\" # \" + ");
        raw.push_str(&nested(MAX_PYTHON_PARSE_NESTING_DEPTH + 1));
        if nesting_budget_reason(&raw).is_none() {
            return Err("a raw string must not swallow the following call".to_string());
        }
        Ok(())
    }

    #[test]
    fn string_literals_fail_closed_when_they_themselves_are_deep() -> Result<(), String> {
        let literal = format!(
            "x = \"{}\"\n",
            "(".repeat(MAX_PYTHON_PARSE_NESTING_DEPTH + 1)
        );
        if nesting_budget_reason(&literal).is_none() {
            return Err(
                "deep brackets inside a string are declined rather than handed to the parser"
                    .to_string(),
            );
        }
        Ok(())
    }
}
