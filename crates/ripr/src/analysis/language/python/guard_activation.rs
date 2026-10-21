//! Guarded-arm activation evidence for the Python preview adapter (#7392).
//!
//! A changed line inside a guarding `if` arm executes only when the guard
//! accepts the call's inputs. When every strong related call provably leaves
//! the guard untaken, the changed line never runs under any observed input,
//! so a strong exact-value oracle still passes unchanged: reach plus a
//! strong oracle does not discriminate the change. This mirrors the
//! predicate boundary rule (`boundary.rs`) on the reachability side instead
//! of the equality side: the sibling predicate probe names the boundary it
//! misses, while this gate stops a return-value probe inside the same arm
//! from reading `exposed` off an oracle that never sees the new line run.
//!
//! The rule engages only when static evidence can read everything it needs:
//! the enclosing `if` conditions, their comparison operands, and the literal
//! arguments strong related tests bind. Anything unreadable (`elif`/`else`
//! chains, `or`/`not` boolean syntax, a parameter the owner rebinds before
//! the guard, computed operands, non-literal rows, a strong call no row
//! binds) fails open and keeps the existing verdict, like the boundary
//! rule's `Unresolved` limitation. Equality follows Python semantics
//! (`True == 1`), since a wrong exclusion is a wrong actionable signal.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    CallRow, canonical_decimal, literals_equal, resolve_operand, simple_comparison_operands,
    strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::module_constants::PythonModuleConstant;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::{ValueContext, ValueFact};

/// Why a guarded changed line never executes under any observed input.
pub(super) struct GuardedArmGap {
    /// The guarding comparison a call input would need to satisfy
    /// (`total > 1000`), named as the repair hint.
    pub(super) discriminator: String,
    /// Literal owner-call arguments from the strong related tests, surfaced
    /// as observed values like the boundary rule does.
    pub(super) observed_values: Vec<ValueFact>,
    /// Why every observed input leaves the guard untaken.
    pub(super) reason: String,
}

/// Evidence that a changed line sits inside guarding arm(s) no strong
/// related call enters. Returns `None` when the line is unguarded, when any
/// observed input may enter the arm, or when static evidence cannot read the
/// enclosing structure — every one keeps the caller's existing verdict.
pub(super) fn python_guarded_arm_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<GuardedArmGap> {
    let conditions = guarding_conditions(owner, owner_source, changed_line).unwrap_or_default();
    let early_exits = early_exit_conditions(owner, owner_source, changed_line);
    if conditions.is_empty() && early_exits.is_empty() {
        return None;
    }
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    if rows.is_empty() {
        // No strong related test binds a readable literal input, so static
        // evidence cannot see reachability either way; the existing oracle
        // verdict stands (the boundary rule engages under the same rule).
        return None;
    }
    if gaps {
        // Some strong related test calls the owner in a way no row binds (a
        // `*args` unpack, an unreadable argument list). The gate's premise is
        // "EVERY strong call provably fails the guard", which the readable
        // rows alone cannot establish, so the exclusion is refused and the
        // existing verdict stands.
        return None;
    }

    // A row reaches the changed line only when EVERY guard holds; it is
    // provably excluded when ANY guard is provably false. A row with an
    // unevaluable guard is never excluded, so one unreadable input keeps
    // the verdict.
    let every_row_excluded = rows.iter().all(|row| {
        conditions.iter().any(|(condition_line, condition)| {
            condition_holds(
                condition,
                *condition_line,
                row,
                &constants,
                owner_source,
                owner.start_line,
            ) == Some(false)
        }) || early_exits.iter().any(|(condition_line, condition)| {
            // A row that satisfies an earlier `if X: return ...` leaves the
            // owner before the changed line, so the line never runs for it.
            condition_holds(
                condition,
                *condition_line,
                row,
                &constants,
                owner_source,
                owner.start_line,
            ) == Some(true)
        })
    });
    if !every_row_excluded {
        return None;
    }

    let mut observed_values: Vec<ValueFact> = rows
        .iter()
        .flat_map(|row| {
            row.bindings
                .iter()
                .filter(|(_, binding)| !binding.from_default)
                .map(|(parameter, binding)| ValueFact {
                    file: None,
                    line: row.line,
                    text: row.text.clone(),
                    value: format!("{parameter} = {}", binding.value),
                    context: ValueContext::FunctionArgument,
                })
        })
        .collect();
    observed_values.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.value.cmp(&right.value))
            .then(left.text.cmp(&right.text))
    });
    observed_values.dedup();

    let mut parts = Vec::new();
    if !early_exits.is_empty() {
        let names = early_exits
            .iter()
            .map(|(_, condition)| format!("`{condition}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut observed: Vec<String> = early_exits
            .iter()
            .flat_map(|(_, condition)| {
                condition_observed_values(condition, &rows)
                    .into_iter()
                    .map(|value| format!("{} = {value}", condition_lhs(condition)))
            })
            .collect();
        observed.sort();
        observed.dedup();
        parts.push(format!(
            "each strong related test input satisfies at least one preceding early-exit condition ({names}) and exits the owner before the changed line{}",
            if observed.is_empty() {
                String::new()
            } else {
                format!("; observed {}", observed.join(", "))
            }
        ));
    }
    for (_, condition) in &conditions {
        let observed = condition_observed_values(condition, &rows);
        parts.push(format!(
            "no strong related test input satisfies `{condition}`{}",
            if observed.is_empty() {
                String::new()
            } else {
                format!(
                    "; observed {} values: {}",
                    condition_lhs(condition),
                    observed.join(", ")
                )
            }
        ));
    }
    let reason = format!(
        "{}; the changed line never executes under an observed input",
        parts.join(" and ")
    );
    Some(GuardedArmGap {
        discriminator: early_exits
            .iter()
            .map(|(_, condition)| format!("not ({condition})"))
            .chain(conditions.iter().map(|(_, condition)| condition.clone()))
            .collect::<Vec<_>>()
            .join(" and "),
        observed_values,
        reason,
    })
}

/// The text of the left operand of a condition's first comparison, used to
/// name the observed values the strong related calls bind to it.
fn condition_lhs(condition: &str) -> String {
    comparison_operator(condition)
        .map(|(start, _)| condition[..start].trim_end().to_string())
        .unwrap_or_else(|| condition.to_string())
}

/// The literal values the rows bind to the condition's left operand.
fn condition_observed_values(condition: &str, rows: &[CallRow]) -> Vec<String> {
    let lhs = condition_lhs(condition);
    let mut values: Vec<String> = rows
        .iter()
        .filter_map(|row| row.bindings.get(&lhs).map(|binding| binding.value.clone()))
        .collect();
    values.sort();
    values.dedup();
    values
}

/// The `if`/`while` conditions enclosing `changed_line` inside its owner,
/// outermost first, each with the 0-based index of its header line (the
/// rebound-parameter check needs the line to bound the assignment scan).
/// Returns `None` when the enclosing structure cannot be read honestly: an
/// `else` arm can never be excluded by evidence, an unrecognized dedented
/// line means the source does not parse the way this reader expects, and
/// the owner bounds keep the walk inside one body.
pub(super) fn guarding_conditions(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_index = changed_line.checked_sub(1)?;
    if owner.start_line == 0 || changed_index >= lines.len() {
        return None;
    }
    let changed = lines.get(changed_index)?;
    if changed.trim().is_empty() {
        return None;
    }
    let mut threshold = leading_whitespace(changed);
    let mut conditions = Vec::new();
    for index in (owner.start_line..changed_index).rev() {
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_whitespace(raw);
        if indent >= threshold {
            // A body line (or this guard's own continuation): the nesting
            // question lives further up.
            continue;
        }
        if let Some(condition) = trimmed
            .strip_prefix("if ")
            .or_else(|| trimmed.strip_prefix("elif "))
        {
            // An `elif X:` arm is taken by exactly the rows `X` accepts (a
            // row an earlier arm caught is a fail-open over-approximation,
            // never a false exclusion), so it reads like an `if`.
            let condition = condition.trim_end().strip_suffix(':')?.trim();
            conditions.push((index, condition.to_string()));
            threshold = indent;
            continue;
        }
        if trimmed.starts_with("else") {
            // An `else` arm is taken whenever the chain is reached, so no
            // row can ever be provably excluded: fail open.
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix("while ") {
            let condition = rest.trim_end().strip_suffix(':')?.trim();
            conditions.push((index, condition.to_string()));
            threshold = indent;
            continue;
        }
        if is_block_opener(trimmed) {
            // `for` / `with` / `try` / `except` / `finally` / `def` / `class`
            // open blocks this reader adds no condition for; the walk
            // continues at their indent. The owner header bounds the walk.
            threshold = indent;
            continue;
        }
        // A dedented line that opens nothing readable: the source does not
        // nest the way this reader expects (a continuation, a broken
        // dedent), so no honest exclusion is possible.
        return None;
    }
    conditions.reverse();
    (!conditions.is_empty()).then_some(conditions)
}

/// Per line, whether it starts inside (or is wholly part of) a triple-quoted
/// string, so docstring text that looks like `if X:` / `return` is not read as
/// code. A line that closes a string and then holds code counts as inside;
/// the callers only skip such lines, which fails open.
fn lines_inside_triple_quotes(lines: &[&str]) -> Vec<bool> {
    let mut open: Option<&str> = None;
    let mut mask = Vec::with_capacity(lines.len());
    for line in lines {
        let mut inside = open.is_some();
        let mut rest: &str = line;
        loop {
            match open {
                Some(delimiter) => match rest.find(delimiter) {
                    Some(position) => {
                        rest = &rest[position + delimiter.len()..];
                        open = None;
                    }
                    None => break,
                },
                None => {
                    // Earliest triple quote outside a comment; single-line
                    // quotes are not tracked, so a quote character inside
                    // another string can only make the mask larger.
                    let comment = rest.find('#').unwrap_or(rest.len());
                    let double = rest.find("\"\"\"");
                    let single = rest.find("'''");
                    let start = match (double, single) {
                        (Some(d), Some(s)) if d <= s => Some((d, "\"\"\"")),
                        (Some(d), None) => Some((d, "\"\"\"")),
                        (_, Some(s)) => Some((s, "'''")),
                        (None, None) => None,
                    };
                    match start {
                        Some((position, delimiter)) if position < comment => {
                            inside = true;
                            rest = &rest[position + delimiter.len()..];
                            open = Some(delimiter);
                        }
                        _ => break,
                    }
                }
            }
        }
        mask.push(inside);
    }
    mask
}

/// Conditions of simple `if X:` statements in the changed line's own block
/// that precede it and always leave the owner (`return`/`raise` as the last
/// body statement, no `elif`/`else` chain). The changed line runs only for
/// inputs that fail every such condition. Anything the reader cannot place
/// honestly yields no entry, which keeps the existing verdict.
fn early_exit_conditions(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
) -> Vec<(usize, String)> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let Some(changed_index) = changed_line.checked_sub(1) else {
        return Vec::new();
    };
    let Some(changed) = lines.get(changed_index) else {
        return Vec::new();
    };
    if owner.start_line == 0 || changed.trim().is_empty() {
        return Vec::new();
    }
    let block_indent = leading_whitespace(changed);
    let in_string = lines_inside_triple_quotes(&lines);
    if in_string.get(changed_index).copied().unwrap_or(true) {
        return Vec::new();
    }
    let mut found = Vec::new();
    for index in (owner.start_line..changed_index).rev() {
        if in_string[index] {
            // Docstring or multi-line string text is not code.
            continue;
        }
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_whitespace(raw);
        if indent < block_indent {
            // Left the changed line's block; enclosing levels are not read.
            break;
        }
        if indent > block_indent {
            continue;
        }
        let Some(condition) = trimmed
            .strip_prefix("if ")
            .and_then(|rest| rest.trim_end().strip_suffix(':'))
        else {
            continue;
        };
        let mut body: Vec<&str> = Vec::new();
        let mut chain_continues = false;
        for line in &lines[index + 1..] {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if leading_whitespace(line) > block_indent {
                body.push(line);
                continue;
            }
            chain_continues = t.starts_with("elif ") || t.starts_with("else");
            break;
        }
        // The last body statement must sit at the body's own top level; a
        // nested `return` under an inner `if` does not always exit.
        let terminates = match (body.first(), body.last()) {
            (Some(first), Some(last)) => {
                let t = last.trim();
                leading_whitespace(first) == leading_whitespace(last)
                    && (t == "return"
                        || t.starts_with("return ")
                        || t == "raise"
                        || t.starts_with("raise "))
            }
            _ => false,
        };
        if terminates && !chain_continues {
            found.push((index, condition.trim().to_string()));
        }
    }
    found.reverse();
    found
}

fn leading_whitespace(text: &str) -> usize {
    text.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn is_block_opener(trimmed: &str) -> bool {
    [
        "for ",
        "with ",
        "try:",
        "except",
        "finally:",
        "def ",
        "async def ",
        "class ",
        "@",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

/// Whether one guarding condition holds for one call row: every `and`
/// conjunct must be a comparison static evidence can evaluate to true.
/// `Some(false)` means the row provably fails the guard; `None` means the
/// guard (or one conjunct) is unreadable for this row, which never excludes
/// the row.
fn condition_holds(
    condition: &str,
    condition_line: usize,
    row: &CallRow,
    constants: &[PythonModuleConstant],
    owner_source: &str,
    owner_start: usize,
) -> Option<bool> {
    // `or` and `not` change when the condition is true without changing the
    // single comparison this reader extracts: `total > 1000 or force` is
    // true through `force` alone, and `not total > 1000` reverses the
    // comparison. The check must run on the WHOLE condition before any
    // conjunct is evaluated: `total > 1000 and ready or force` parses as
    // `(total > 1000 and ready) or force`, and a row entering through
    // `force` must not be excluded by an early false conjunct (#7874).
    if has_boolean_operator(condition) {
        return None;
    }
    for conjunct in condition.split(" and ") {
        let conjunct = conjunct.trim();
        match eval_comparison(
            conjunct,
            condition_line,
            row,
            constants,
            owner_source,
            owner_start,
        ) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => return None,
        }
    }
    Some(true)
}

/// True when `text` applies `or` or `not` outside string literals.
fn has_boolean_operator(text: &str) -> bool {
    keyword_outside_quotes(text, "or") || keyword_outside_quotes(text, "not")
}

/// True when `keyword` appears in `text` outside quotes, bounded by
/// non-identifier characters (so `notary` does not match `not`).
fn keyword_outside_quotes(text: &str, keyword: &str) -> bool {
    let bytes = text.as_bytes();
    let keyword = keyword.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index + keyword.len() <= bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(keyword)
            && (index == 0 || !is_identifier_byte(bytes[index - 1]))
            && (index + keyword.len() == bytes.len()
                || !is_identifier_byte(bytes[index + keyword.len()]))
        {
            return true;
        }
        index += 1;
    }
    false
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True when `line` contains an assignment `=` — one not part of `==`,
/// `!=`, `<=`, or `>=`. Conservative text check used only to refuse
/// destructuring-target exclusions; it never awards credit.
fn line_is_assignment(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'=' {
            continue;
        }
        if bytes.get(index + 1) == Some(&b'=') {
            continue;
        }
        if index > 0 && matches!(bytes[index - 1], b'=' | b'!' | b'<' | b'>') {
            continue;
        }
        return true;
    }
    false
}

/// True when `line` names `name` as an assignment target anywhere on the
/// line: plain, annotated, augmented, destructuring, chained, or walrus
/// binding. Conservative (refuses on ambiguous shapes like a subscript
/// slice, which only over-refuses in the safe fail-open direction); it
/// never awards credit. Requires identifier-bounded occurrences so `totals`
/// or `subtotal` never match, and skips attribute access (`self.total`)
/// which names a different binding.
fn line_rebinds_parameter(line: &str, name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut search = 0;
    while let Some(offset) = line[search..].find(name) {
        let start = search + offset;
        let end = start + name.len();
        search = end;
        // Identifier bounds: reject `totals` and `subtotal`.
        if start > 0 && is_identifier_byte(bytes[start - 1]) {
            continue;
        }
        if bytes.get(end).is_some_and(|byte| is_identifier_byte(*byte)) {
            continue;
        }
        // An attribute (`self.total`) is a different binding.
        if start > 0 && bytes[start - 1] == b'.' {
            continue;
        }
        let rest = line[end..].trim_start();
        // `name = ..`; `name == ..` is a comparison, not a binding. This
        // also covers chained (`other = total = 2000`) and reversed
        // destructuring (`other, total = 2000, 0`) targets, where the name
        // is followed directly by the assignment operator.
        if let Some(after) = rest.strip_prefix('=')
            && !after.starts_with('=')
        {
            return true;
        }
        // Augmented assignments, single- and multi-character
        // (`+=`, `**=`, `//=`, `>>=`, `|=`, ..).
        if [
            "+=", "-=", "*=", "/=", "%=", "**=", "//=", ">>=", "<<=", "|=", "&=", "^=",
        ]
        .iter()
        .any(|op| rest.starts_with(op))
        {
            return true;
        }
        // Annotated assignment `name: T = ..` and walrus `name := ..`; an
        // `as name:` target (`with .. as name`, `except .. as name`) ends
        // the same way. A subscript slice (`x[name:]`) over-refuses, which
        // is the safe fail-open direction.
        if rest.starts_with(':') {
            return true;
        }
        // Destructuring targets later in the target list: `(total, other) =
        // ..`, `[total, other] = ..`, `total, other = ..`. Refuse when the
        // line is an assignment.
        if rest.starts_with(',') && line_is_assignment(line) {
            return true;
        }
    }
    false
}

/// Leading-whitespace width of `raw`, so a guard nested inside a loop can be
/// told apart from the loop header that encloses it.
fn indent_width(raw: &str) -> usize {
    raw.len() - raw.trim_start().len()
}

/// True when the owner body assigns the parameter a new value so a call
/// binding no longer predicts the value the guard reads. Scans the lines
/// from the owner header to the guard, plus the WHOLE owner when a `for` or
/// `while` header encloses the guard: a rebinding later in the loop body
/// changes what the guard reads on the next iteration, and linear scanning
/// cannot prove execution order there (#7874 follow-up). Deliberately
/// conservative; anything subtler keeps the existing limitation.
fn parameter_rebound_before(
    owner_source: &str,
    owner_start: usize,
    guard_line: usize,
    name: &str,
) -> bool {
    let lines: Vec<&str> = owner_source.lines().collect();
    let Some(guard_raw) = lines.get(guard_line) else {
        return false;
    };
    let guard_indent = indent_width(guard_raw);
    let loop_enclosed = lines
        .get(owner_start..guard_line)
        .unwrap_or(&[])
        .iter()
        .any(|raw| {
            let trimmed = raw.trim_start();
            (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                && indent_width(raw) < guard_indent
        });
    let scan_end = if loop_enclosed {
        // Execution order is not linear inside a loop: any rebinding
        // anywhere in the owner may run before a later iteration's guard.
        lines.len()
    } else {
        guard_line
    };
    for raw in lines.get(owner_start..scan_end).unwrap_or(&[]) {
        let trimmed = raw.trim();
        if line_rebinds_parameter(trimmed, name) {
            return true;
        }
        if let Some(targets) = trimmed
            .strip_prefix("for ")
            .and_then(|rest| rest.split(" in ").next())
        {
            // `for name in ..` / `for a, name in ..` rebinds the name, and
            // so do parenthesized or bracketed tuple targets and star
            // targets: Python binds every target before the body runs, so
            // decoration around the name must not hide it
            // (`for (other, total) in ..`).
            if targets.split(',').any(|target| {
                let normalized = target
                    .trim()
                    .trim_start_matches(['(', '[', '*'])
                    .trim_end_matches([')', ']']);
                normalized == name
            }) {
                return true;
            }
        }
    }
    false
}

/// One `left <op> right` comparison under a row's literal bindings, or None
/// when the comparison or an operand is not statically readable.
fn eval_comparison(
    text: &str,
    condition_line: usize,
    row: &CallRow,
    constants: &[PythonModuleConstant],
    owner_source: &str,
    owner_start: usize,
) -> Option<bool> {
    let (start, len) = comparison_operator(text)?;
    let (left, right) = simple_comparison_operands(text, start, len)?;
    for operand in [&left, &right] {
        // A bare name the owner rebinds before the guard reads a different
        // value from the one the call bound (`total = 2000` above
        // `if total > 1000`), so the call binding cannot exclude the row.
        if !operand.contains('.')
            && operand
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && parameter_rebound_before(owner_source, owner_start, condition_line, operand)
        {
            return None;
        }
    }
    let left = resolve_operand(&left, row, constants)?;
    let right = resolve_operand(&right, row, constants)?;
    compare_literal_values(&left, &text[start..start + len], &right)
}

/// The span of the single comparison operator in `text` (outside quotes),
/// or None for zero or several operators — several cannot be evaluated
/// honestly, and zero means there is no comparison to read.
fn comparison_operator(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut found: Option<(usize, usize)> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'=' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((index, 2));
                    index += 2;
                    continue;
                }
                // A bare `=` is an assignment, not a comparison.
                return None;
            }
            b'!' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((index, 2));
                    index += 2;
                    continue;
                }
                index += 1;
            }
            b'<' | b'>' => {
                if bytes.get(index + 1) == Some(&byte)
                    || index.checked_sub(1).is_some_and(|prev| bytes[prev] == b'-')
                {
                    // `<<` / `>>` shifts and the `->` arrow.
                    index += 2;
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                let len = usize::from(bytes.get(index + 1) == Some(&b'=')) + 1;
                found = Some((index, len));
                index += len;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    found
}

/// True when a literal's own text is an integer literal (optional `-` then
/// ASCII digits, no fraction), so Python orders it exactly and i128
/// comparison is faithful. Reads the original text, not the canonical
/// decimal: `100.0` canonicalizes to `100` but is a float literal that
/// Python rounds at parse time.
fn is_integer_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

/// Compare two resolved literal values with `op`. Equality uses Python
/// scalar semantics (`guard_literals_equal`: `True == 1`, `False == 0`);
/// ordering parses the canonical decimals and never orders across kinds (a
/// string against a number is unreadable, which fails open).
fn compare_literal_values(left: &str, op: &str, right: &str) -> Option<bool> {
    let (left_text, right_text) = (left, right);
    match op {
        "==" => Some(guard_literals_equal(left, right)),
        "!=" => Some(!guard_literals_equal(left, right)),
        "<" | "<=" | ">" | ">=" => {
            let left = canonical_decimal(left)?;
            let right = canonical_decimal(right)?;
            // Integer literals order exactly in Python beyond f64 precision
            // (`9007199254740993 > 9007199254740992` holds; both round to the
            // same f64), so compare integer-form operands as i128 and refuse
            // (fail open) on overflow rather than excluding through a wrong
            // rounded comparison (#7874). The integer check reads the
            // ORIGINAL literal text: `9007199254740993.0` is a float literal
            // that Python also rounds to ...992.0, so it keeps the f64 path
            // and compares equal there. Non-integer operands keep the f64
            // path: Python rounds float literals to f64 too, so the parsed
            // comparison matches the runtime.
            if is_integer_literal(left_text) && is_integer_literal(right_text) {
                let left: i128 = left.parse().ok()?;
                let right: i128 = right.parse().ok()?;
                return Some(match op {
                    "<" => left < right,
                    "<=" => left <= right,
                    ">" => left > right,
                    _ => left >= right,
                });
            }
            let left: f64 = left.parse().ok()?;
            let right: f64 = right.parse().ok()?;
            Some(match op {
                "<" => left < right,
                "<=" => left <= right,
                ">" => left > right,
                _ => left >= right,
            })
        }
        _ => None,
    }
}

/// Equality as Python evaluates it for scalar literals. The shared
/// [`literals_equal`] intentionally never equates mixed kinds ("fails closed
/// toward not observed") because the boundary rule uses it to award credit;
/// this gate uses the answer to EXCLUDE a call, which reverses the
/// conservative direction, so bool/numeric pairs follow Python semantics:
/// `True == 1` and `False == 0` hold, and `2 == True` does not.
fn guard_literals_equal(left: &str, right: &str) -> bool {
    match (bool_decimal(left), bool_decimal(right)) {
        (Some(left), Some(right)) => return left == right,
        (Some(bool_value), None) => {
            return canonical_decimal(right).is_some_and(|num| num == bool_value);
        }
        (None, Some(bool_value)) => {
            return canonical_decimal(left).is_some_and(|num| num == bool_value);
        }
        (None, None) => {}
    }
    literals_equal(left, right)
}

/// The decimal value Python equates a bool with, or None for anything else.
fn bool_decimal(text: &str) -> Option<String> {
    match text {
        "True" => Some("1".to_string()),
        "False" => Some("0".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::owners_tests::extract_owners;
    use super::*;

    fn quote_owner(source: &str) -> Result<PythonOwner, String> {
        extract_owners(std::path::Path::new("src/subject.py"), source)
            .into_iter()
            .find(|owner| owner.name == "quote")
            .ok_or_else(|| "quote owner must parse".to_string())
    }

    fn test_row(bindings: &[(&str, &str)]) -> CallRow {
        use super::super::boundary::Binding;
        CallRow {
            line: 1,
            text: "quote(..)".to_string(),
            bindings: bindings
                .iter()
                .map(|(parameter, value)| {
                    (
                        (*parameter).to_string(),
                        Binding {
                            value: (*value).to_string(),
                            from_default: false,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn guarding_conditions_reads_enclosing_if_arm() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 3)
            .ok_or_else(|| "guarded line must yield conditions".to_string())?;
        assert_eq!(conditions, vec![(1, "total > 1000".to_string())]);
        Ok(())
    }

    #[test]
    fn guarding_conditions_walks_nested_arms_outermost_first() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 0:\n        if total > 1000:\n            return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 4)
            .ok_or_else(|| "nested guarded line must yield conditions".to_string())?;
        assert_eq!(
            conditions,
            vec![
                (1, "total > 0".to_string()),
                (2, "total > 1000".to_string())
            ]
        );
        Ok(())
    }

    #[test]
    fn guarding_conditions_reads_elif_arm_without_its_chain_head() -> Result<(), String> {
        // The chain head at the same indent is a sibling line for the walk:
        // the elif arm's reach is approximated by its own condition only,
        // which never excludes a row the arm would genuinely take.
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    elif total > 0:\n        return round(total * 1.05, 2)\n    return total\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 5)
            .ok_or_else(|| "elif arm line must yield conditions".to_string())?;
        assert_eq!(conditions, vec![(3, "total > 0".to_string())]);
        Ok(())
    }

    #[test]
    fn guarding_conditions_fails_open_on_else_and_unguarded_lines() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    else:\n        return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        assert!(guarding_conditions(&owner, source, 3).is_some());
        // An `else` arm is taken whenever the chain is reached, so no row can
        // ever be provably excluded.
        assert!(guarding_conditions(&owner, source, 5).is_none());
        let unguarded = "def quote(total):\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(unguarded)?;
        assert!(guarding_conditions(&owner, unguarded, 2).is_none());
        Ok(())
    }

    #[test]
    fn compare_literal_values_uses_python_scalar_semantics() {
        assert_eq!(compare_literal_values("100", "==", "100.0"), Some(true));
        assert_eq!(compare_literal_values("100", ">", "1000"), Some(false));
        assert_eq!(compare_literal_values("2000", ">", "1000"), Some(true));
        assert_eq!(compare_literal_values("1", "<=", "1.0"), Some(true));
        assert_eq!(compare_literal_values("\"a\"", "==", "\"a\""), Some(true));
        assert_eq!(compare_literal_values("1", "!=", "2"), Some(true));
        // A number against a string is unreadable, which fails open.
        assert_eq!(compare_literal_values("100", ">", "\"a\""), None);
        assert_eq!(compare_literal_values("\"a\"", "<", "\"b\""), None);
    }

    #[test]
    fn compare_literal_values_equates_bool_like_python() {
        // Python compares `True == 1` and `False == 0` as true; the shared
        // literals_equal never mixes kinds, which is conservative for credit
        // but wrong for the exclusion direction this gate evaluates.
        assert_eq!(compare_literal_values("True", "==", "1"), Some(true));
        assert_eq!(compare_literal_values("1", "==", "True"), Some(true));
        assert_eq!(compare_literal_values("False", "==", "0"), Some(true));
        assert_eq!(compare_literal_values("2", "==", "True"), Some(false));
        assert_eq!(compare_literal_values("True", "!=", "1"), Some(false));
        assert_eq!(compare_literal_values("True", "==", "False"), Some(false));
    }

    #[test]
    fn keyword_outside_quotes_respects_word_boundaries() {
        assert!(keyword_outside_quotes("total > 1000 or force", "or"));
        assert!(keyword_outside_quotes("not total > 1000", "not"));
        assert!(!keyword_outside_quotes("notary > 100", "not"));
        assert!(!keyword_outside_quotes("total > \"or\"", "or"));
        assert!(!keyword_outside_quotes("force_flag > 0", "or"));
    }

    #[test]
    fn condition_holds_fails_open_on_or_guard() {
        // `total > 1000 or force` is entered through `force` alone; reading
        // only the comparison would exclude a row that entered the arm.
        let source = "def quote(total, force):\n    if total > 1000 or force:\n        return 42\n";
        let row = test_row(&[("total", "100.0"), ("force", "True")]);
        assert_eq!(
            condition_holds("total > 1000 or force", 1, &row, &[], source, 0),
            None
        );
    }

    #[test]
    fn condition_holds_fails_open_on_not_guard() {
        // `not total > 1000` reverses the comparison; a row the comparison
        // fails enters the arm, so the raw comparison must not exclude it.
        let source = "def quote(total):\n    if not total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "2000.0")]);
        assert_ne!(
            condition_holds("not total > 1000", 1, &row, &[], source, 0),
            Some(false)
        );
    }

    #[test]
    fn condition_holds_still_excludes_simple_false_comparison() {
        let source = "def quote(total):\n    if total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "100.0")]);
        assert_eq!(
            condition_holds("total > 1000", 1, &row, &[], source, 0),
            Some(false)
        );
    }

    #[test]
    fn eval_comparison_fails_open_on_rebound_parameter() {
        // `total = 2000` between the header and the guard means the guard
        // reads 2000 regardless of the call binding, so the binding cannot
        // exclude the row.
        let source =
            "def quote(total):\n    total = 2000\n    if total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "100.0")]);
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], source, 0),
            None
        );
        // Without the rebinding the same binding honestly excludes the row.
        let plain = "def quote(total):\n    if total > 1000:\n        return 42\n";
        assert_eq!(
            eval_comparison("total > 1000", 1, &row, &[], plain, 0),
            Some(false)
        );
    }

    #[test]
    fn condition_holds_fails_open_on_or_after_a_false_conjunct() {
        // #7874: `total > 1000 and ready or force` parses as
        // `(total > 1000 and ready) or force`. The whole-condition boolean
        // check must run before any conjunct is evaluated, so the first
        // conjunct returning false cannot exclude a row that enters through
        // `force`.
        let source = "def quote(total, force, ready):\n    if total > 1000 and ready or force:\n        return 42\n";
        let row = test_row(&[("total", "100.0"), ("force", "True"), ("ready", "False")]);
        assert_eq!(
            condition_holds("total > 1000 and ready or force", 1, &row, &[], source, 0),
            None
        );
    }

    #[test]
    fn condition_holds_fails_open_on_leading_or_operand_order() {
        // The reversed operand order names `force` first; it is the same
        // whole-condition `or` and must fail open the same way (#7874).
        let source = "def quote(total, force, ready):\n    if force or total > 1000 and ready:\n        return 42\n";
        let row = test_row(&[("total", "100.0"), ("force", "True"), ("ready", "False")]);
        assert_eq!(
            condition_holds("force or total > 1000 and ready", 1, &row, &[], source, 0),
            None
        );
    }

    #[test]
    fn condition_holds_confirms_simple_true_comparison() {
        // The passing sibling of the #7874 controls: a plain true guard
        // still holds, so a discriminating row keeps its credit.
        let source = "def quote(total):\n    if total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "2000.0")]);
        assert_eq!(
            condition_holds("total > 1000", 1, &row, &[], source, 0),
            Some(true)
        );
    }

    #[test]
    fn eval_comparison_fails_open_on_multi_character_augmented_rebinding() {
        // #7874: `**=`, `//=`, shifts and bitwise augmented assignments
        // rebind the parameter exactly like `+=`, so the call binding cannot
        // exclude the row.
        for assignment in [
            "total **= 2",
            "total //= 2",
            "total >>= 1",
            "total <<= 1",
            "total |= 1",
            "total &= 1",
            "total ^= 1",
        ] {
            let source = format!(
                "def quote(total):\n    {assignment}\n    if total > 1000:\n        return 42\n"
            );
            let row = test_row(&[("total", "100.0")]);
            assert_eq!(
                eval_comparison("total > 1000", 2, &row, &[], &source, 0),
                None,
                "{assignment} must fail open"
            );
        }
    }

    #[test]
    fn eval_comparison_fails_open_on_tuple_rebinding() {
        // #7874: `total, other = 2000, 0` rebinds `total` through a
        // destructuring target.
        let source = "def quote(total):\n    total, other = 2000, 0\n    if total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "100.0")]);
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], source, 0),
            None
        );
    }

    #[test]
    fn rebound_scan_keeps_comparison_lines_readable() {
        // Negative controls for the conservative scan: comparisons and
        // unrelated assignments never rebind the parameter.
        let source = "def quote(total):\n    seen = total != 2000\n    if total > 1000:\n        return 42\n";
        let row = test_row(&[("total", "100.0")]);
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], source, 0),
            Some(false)
        );
        // A destructuring comparison (`==`) is not an assignment either.
        let tuple_compare = "def quote(total, other):\n    if total > 1000:\n        return 42\n";
        assert_eq!(
            eval_comparison("total > 1000", 1, &row, &[], tuple_compare, 0),
            Some(false)
        );
    }

    #[test]
    fn compare_literal_values_orders_large_integers_exactly() {
        // #7874: integers beyond 2^53 keep exact Python ordering instead of
        // collapsing to the same f64.
        assert_eq!(
            compare_literal_values("9007199254740993", ">", "9007199254740992"),
            Some(true)
        );
        assert_eq!(
            compare_literal_values("9007199254740992", "<", "9007199254740993"),
            Some(true)
        );
        assert_eq!(
            compare_literal_values("9007199254740992", ">=", "9007199254740992"),
            Some(true)
        );
        // Float literals round at parse time in Python too: both become
        // 9007199254740992.0, so the f64 path keeps them unordered.
        assert_eq!(
            compare_literal_values("9007199254740993.0", ">", "9007199254740992.0"),
            Some(false)
        );
        // Mixed integer/float forms still order by value.
        assert_eq!(compare_literal_values("100", ">", "99.5"), Some(true));
    }

    #[test]
    fn rebinding_scan_finds_parameter_later_on_the_line() {
        // The parameter is an assignment target even when it is not the
        // first token: reversed destructuring, parenthesized/bracketed
        // targets, chained assignments, and the walrus operator all rebind
        // it before the guard reads it.
        let row = test_row(&[("total", "100.0")]);
        for assignment in [
            "other, total = 2000, 0",
            "(total, other) = (2000, 0)",
            "[total, other] = [2000, 0]",
            "other = total = 2000",
            "x = (total := 2000)",
        ] {
            let source = format!(
                "def quote(total):\n    {assignment}\n    if total > 1000:\n        return 42\n"
            );
            assert_eq!(
                eval_comparison("total > 1000", 2, &row, &[], &source, 0),
                None,
                "{assignment} must fail open"
            );
        }
    }

    #[test]
    fn rebinding_after_guard_inside_loop_fails_open() {
        // A rebinding later in the loop body changes what the guard reads
        // on the next iteration; linear scanning cannot prove execution
        // order there, so the whole owner is scanned and the row stays.
        let source = "def quote(total):\n    for _ in range(3):\n        if total > 1000:\n            return 42\n        total *= 20\n";
        let row = test_row(&[("total", "100.0")]);
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], source, 0),
            None
        );
    }

    #[test]
    fn eval_comparison_fails_open_on_parenthesized_loop_target() {
        // A parenthesized tuple loop target (`for (other, total) in ..`)
        // binds `total` before the guard runs; the closing parenthesis and
        // a star-prefixed target must not hide the rebinding from the scan.
        let row = test_row(&[("total", "100.0")]);
        for header in [
            "    for (other, total) in [(0, 2000)]:",
            "    for other, *total in [(0, 2000)]:",
        ] {
            let source = format!(
                "def quote(total):\n{header}\n        if total > 1000:\n            return 42\n"
            );
            assert_eq!(
                eval_comparison("total > 1000", 2, &row, &[], &source, 0),
                None,
                "{header} must fail open"
            );
        }
    }

    #[test]
    fn rebinding_scan_keeps_reads_and_attributes_readable() {
        // Reading the parameter, comparisons, and attribute access on a
        // same-named field never rebind the parameter.
        let row = test_row(&[("total", "100.0")]);
        let read =
            "def quote(total):\n    other = total + 1\n    if total > 1000:\n        return 42\n";
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], read, 0),
            Some(false)
        );
        let compare =
            "def quote(total):\n    total == 2000\n    if total > 1000:\n        return 42\n";
        assert_eq!(
            eval_comparison("total > 1000", 2, &row, &[], compare, 0),
            Some(false)
        );
    }
}
