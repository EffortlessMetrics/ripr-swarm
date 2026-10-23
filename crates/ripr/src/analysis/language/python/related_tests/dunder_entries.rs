//! Statically certain operator and `with` entries of a dunder owner (#9421).
//!
//! A dunder method runs through syntax, not a call the adapter can read by
//! name: `Money(5) + Money(7)` runs `Money.__add__`, `with Recorder() as rec`
//! runs `Recorder.__enter__` and `__exit__`. The syntax binds to the owner only
//! when both the receiver's class and the operator are certain:
//!
//! - the receiver is built in the test from the owner's own class, inline
//!   (`Money(5)`) or through a local bound once to such a construction, so no
//!   subclass can override the method;
//! - the operator maps to exactly the owner dunder: the receiver is the left
//!   operand (a right operand runs the reflected method), the other operand is
//!   a literal or another instance of the same class (a foreign or subclass
//!   operand may take priority), and a `!=` runs `__eq__` only when the class
//!   defines no `__ne__`;
//! - the operands are atoms, so the operator is not a part of a larger
//!   expression whose receiver the text does not name.
//!
//! Anything else keeps the `dunder_protocol` relation, which abstains.

use super::{
    PythonOwner, PythonTest, assignment_count, binding_target_for_construction,
    binds_in_target_list, bracket_depth_at, construction_is_whole_value, dunder_method_class,
    is_python_identifier_char, keyword_or_operator_binds_text, oracle_operand_depth,
    owner_class_locals, python_callee_start_has_boundary, python_text_hides_code, text_binds_name,
};
use crate::domain::OwnerKind;

/// Class-level facts the entry binding reads, filled once per workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::analysis::language::python) struct PythonClassFacts {
    /// Every method name defined in the owner's class body.
    pub(in crate::analysis::language::python) methods: Vec<String>,
    /// Whether the class's `__enter__` is a bare `return self`.
    pub(in crate::analysis::language::python) enter_returns_self: bool,
    /// The class has no decorators, metaclass or other class keywords, a body
    /// of only methods and plain attributes (no dunder assigned or defined
    /// conditionally), and is the only definition of its path in the file.
    /// Anything else may run a regenerated or replaced method. Set at
    /// extraction; `false` when unknown.
    pub(in crate::analysis::language::python) plain: bool,
    /// The class lists no base class. A base may define `__ne__`.
    pub(in crate::analysis::language::python) no_bases: bool,
    /// Trimmed text of the single-line expression statements written directly
    /// in this method's body (not nested under a guard). Set at extraction.
    pub(in crate::analysis::language::python) direct_statements: Vec<String>,
}

/// Fills [`PythonClassFacts`] on each method owner from the owners that share
/// its file and class.
pub(in crate::analysis::language::python) fn apply_class_facts(owners: &mut [PythonOwner]) {
    let mut grouped: Vec<(std::path::PathBuf, String, PythonClassFacts)> = Vec::new();
    for owner in owners.iter() {
        if owner.class_path.is_empty()
            || !matches!(
                owner.owner_kind,
                Some(OwnerKind::Method | OwnerKind::ClassMethod)
            )
        {
            continue;
        }
        let at = match grouped
            .iter()
            .position(|(file, class, _)| *file == owner.file && *class == owner.class_path)
        {
            Some(at) => at,
            None => {
                grouped.push((
                    owner.file.clone(),
                    owner.class_path.clone(),
                    PythonClassFacts::default(),
                ));
                grouped.len() - 1
            }
        };
        let entry = &mut grouped[at].2;
        if !entry.methods.contains(&owner.name) {
            entry.methods.push(owner.name.clone());
        }
        if owner.name == "__enter__"
            && owner.decorators.is_empty()
            && owner
                .parameters
                .first()
                .is_some_and(|first| owner.term_facts.returned_parameter() == Some(&first.name))
        {
            entry.enter_returns_self = true;
        }
    }
    for owner in owners.iter_mut() {
        if let Some((_, _, class_facts)) = grouped
            .iter()
            .find(|(file, class, _)| *file == owner.file && *class == owner.class_path)
        {
            owner.class_facts.methods = class_facts.methods.clone();
            owner.class_facts.enter_returns_self = class_facts.enter_returns_self;
        }
    }
}

/// The Python operators that run a binary dunder on their left operand, with
/// the dunders each can run. `!=` runs `__ne__`, which defaults to inverting
/// `__eq__`.
const OPERATOR_DUNDERS: [(&str, &[&str]); 12] = [
    ("+", &["__add__"]),
    ("-", &["__sub__"]),
    ("*", &["__mul__"]),
    ("/", &["__truediv__"]),
    ("//", &["__floordiv__"]),
    ("%", &["__mod__"]),
    ("==", &["__eq__"]),
    ("!=", &["__ne__", "__eq__"]),
    ("<", &["__lt__"]),
    ("<=", &["__le__"]),
    (">", &["__gt__"]),
    (">=", &["__ge__"]),
];

fn is_comparison(operator: &str) -> bool {
    matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=")
}

/// Locals the test binds once, as a whole value, to a construction of the
/// owner's class: `a = Money(1)`.
fn bound_instances(test: &PythonTest, owner: &PythonOwner, class: &str) -> Vec<String> {
    let body = test.body_text.as_str();
    // `with rec:` reads like an annotation (`rec: int`) to the target-list
    // scan; a context-manager header binds nothing.
    let scrubbed = scrub_with_headers(body);
    let mut locals = Vec::new();
    for local in owner_class_locals(test, owner, class) {
        let construct = format!("{local}(");
        for (idx, _) in body.match_indices(&construct) {
            if !python_callee_start_has_boundary(body, idx)
                || python_text_hides_code(body, idx)
                || !construction_is_whole_value(body, idx, &construct)
            {
                continue;
            }
            if let Some(var) = binding_target_for_construction(body, idx)
                && assignment_count(body, &var) == 1
                && !keyword_or_operator_binds_text(body, &var, true)
                && !binds_in_target_list(&scrubbed, &var)
                && !test.fixtures.contains(&var)
                && !is_callable_parameter(body, &var)
                && !locals.contains(&var)
            {
                locals.push(var);
            }
        }
    }
    locals
}

/// Whether `name` is a parameter of a nested `def` or a `lambda` in `body`
/// (`def check(a):`, `lambda a: ...`), which rebinds it inside that scope. A
/// header whose parameter list cannot be read on its line fails closed.
fn is_callable_parameter(body: &str, name: &str) -> bool {
    let has_word = |text: &str| {
        text.match_indices(name).any(|(idx, _)| {
            !text[..idx]
                .chars()
                .next_back()
                .is_some_and(is_python_identifier_char)
                && !text[idx + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_python_identifier_char)
        })
    };
    body.lines().any(|line| {
        let trimmed = line.trim_start();
        let header = trimmed.strip_prefix("async ").unwrap_or(trimmed);
        if header.starts_with("def ") {
            return match (header.find('('), header.rfind(')')) {
                (Some(open), Some(close)) if close > open => has_word(&header[open..close]),
                _ => true,
            };
        }
        line.match_indices("lambda").any(|(idx, _)| {
            let rest = &line[idx + "lambda".len()..];
            has_word(rest.split(':').next().unwrap_or(rest))
        })
    })
}

/// The body with each bare `with <name>:` header rewritten to `with _:`,
/// keeping every offset.
fn scrub_with_headers(body: &str) -> String {
    body.split_inclusive('\n')
        .map(|line| {
            let bare = line
                .trim()
                .strip_prefix("with ")
                .and_then(|rest| rest.strip_suffix(':'))
                .map(str::trim)
                .filter(|name| !name.is_empty() && name.chars().all(is_python_identifier_char));
            match bare {
                Some(name) => {
                    let at = line.find(name).unwrap_or(0);
                    let mut scrubbed = line.to_string();
                    scrubbed.replace_range(at..at + name.len(), &"_".repeat(name.len()));
                    scrubbed
                }
                None => line.to_string(),
            }
        })
        .collect()
}

/// The owner class's spellings in the test that are still the imported class,
/// and its instances bound once.
fn bound_receivers(
    test: &PythonTest,
    owner: &PythonOwner,
    class: &str,
) -> (Vec<String>, Vec<String>) {
    let class_locals: Vec<String> = owner_class_locals(test, owner, class)
        .into_iter()
        .filter(|local| {
            let head = local.split('.').next().unwrap_or(local);
            !super::super::sink_alignment::function_scope_stand_in(test, head, owner)
        })
        .collect();
    if class_locals.is_empty() {
        return (class_locals, Vec::new());
    }
    (class_locals, bound_instances(test, owner, class))
}

/// Whether the operator `operator` runs the owner dunder.
fn operator_runs_owner(operator: &str, owner: &PythonOwner) -> bool {
    OPERATOR_DUNDERS
        .iter()
        .find(|(token, _)| *token == operator)
        .is_some_and(|(_, dunders)| {
            dunders.iter().any(|dunder| {
                if *dunder != owner.name || !owner.class_facts.plain {
                    return false;
                }
                // `!=` reaches `__eq__` only through the inherited
                // `object.__ne__`; a class that defines its own `__ne__` runs
                // that instead. The owner's own class facts must be filled.
                operator != "!="
                    || owner.name == "__ne__"
                    || (owner.class_facts.no_bases
                        && owner
                            .class_facts
                            .methods
                            .iter()
                            .any(|name| name == "__eq__")
                        && !owner
                            .class_facts
                            .methods
                            .iter()
                            .any(|name| name == "__ne__"))
            })
        })
}

/// Unclosed bracket positions before `idx`, outermost first.
fn enclosing_brackets(text: &str, idx: usize) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut stack = Vec::new();
    let mut cursor = 0usize;
    while cursor < idx.min(bytes.len()) {
        match bytes[cursor] {
            b'#' => {
                cursor += bytes[cursor..]
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .unwrap_or(bytes.len() - cursor);
            }
            quote @ (b'\'' | b'"') => {
                let triple = bytes[cursor..].starts_with(&[quote; 3]);
                let delimiter: &[u8] = if triple { &[quote; 3] } else { &[quote] };
                cursor += delimiter.len();
                while cursor < bytes.len() && !bytes[cursor..].starts_with(delimiter) {
                    cursor += if bytes[cursor] == b'\\' { 2 } else { 1 };
                }
                cursor += delimiter.len();
            }
            b'(' | b'[' | b'{' => {
                stack.push(cursor);
                cursor += 1;
            }
            b')' | b']' | b'}' => {
                stack.pop();
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    stack
}

/// The index of the bracket that closes the opener at `open`.
fn closing_bracket(text: &str, open: usize) -> Option<usize> {
    super::super::no_behavior::matching_call_paren(text, open)
}

/// The word that ends `text`, when `text` ends in an identifier.
fn trailing_word(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_python_identifier_char(*ch))
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    &text[start..]
}

/// The start of the atom that ends at `end`: a construction `Local(...)` of
/// the owner class or a bound instance local.
fn instance_atom_ending_at(
    text: &str,
    end: usize,
    class_locals: &[String],
    instances: &[String],
) -> Option<usize> {
    let head = text[..end].trim_end();
    if head.ends_with(')') {
        let close = head.len() - 1;
        let open = (0..close)
            .rev()
            .find(|&at| text.as_bytes()[at] == b'(' && closing_bracket(text, at) == Some(close))?;
        let callee_text = &text[..open];
        let callee_start = callee_text
            .char_indices()
            .rev()
            .find(|(_, ch)| !(is_python_identifier_char(*ch) || *ch == '.'))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        let callee = &callee_text[callee_start..];
        (class_locals.iter().any(|local| local == callee)
            && python_callee_start_has_boundary(text, callee_start))
        .then_some(callee_start)
    } else {
        let name = trailing_word(head);
        let start = head.len() - name.len();
        (!name.is_empty()
            && instances.iter().any(|instance| instance == name)
            && !text[..start].ends_with(['.', '"', '\'']))
        .then_some(start)
    }
}

/// Whether the text before an operand starts an operand position: the text
/// start, an opening bracket or comma, `not`/`assert`, a comparison or boolean
/// operator. Any other predecessor means the operand is part of a larger
/// expression (`x * Money(2) + ...`, `obj.Money(1)`).
fn operand_position_before(text: &str, start: usize) -> bool {
    let before = text[..start].trim_end();
    if before.is_empty() || before.ends_with(['(', ',', '[']) {
        return true;
    }
    if before.ends_with("==")
        || before.ends_with("!=")
        || before.ends_with("<=")
        || before.ends_with(">=")
    {
        return true;
    }
    if (before.ends_with('<') && !before.ends_with("<<"))
        || (before.ends_with('>') && !before.ends_with(">>") && !before.ends_with("->"))
    {
        return true;
    }
    matches!(trailing_word(before), "assert" | "not" | "and" | "or")
}

/// The end of the right operand atom starting at `start`: a number, a string
/// literal, `True`/`False`/`None`, an owner-class construction or a bound
/// instance. `None` when the operand is anything else.
fn right_atom_end(
    text: &str,
    start: usize,
    class_locals: &[String],
    instances: &[String],
) -> Option<usize> {
    let rest = &text[start..];
    let first = rest.chars().next()?;
    if first.is_ascii_digit() {
        let len = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_'))
            .unwrap_or(rest.len());
        return Some(start + len);
    }
    if first == '\'' || first == '"' {
        if rest.starts_with(&first.to_string().repeat(3)) {
            return None;
        }
        let mut escaped = false;
        for (at, ch) in rest.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == first {
                return Some(start + at + 1);
            }
        }
        return None;
    }
    let word_len = rest
        .find(|ch: char| !(is_python_identifier_char(ch) || ch == '.'))
        .unwrap_or(rest.len());
    let word = &rest[..word_len];
    if matches!(word, "True" | "False" | "None")
        || instances.iter().any(|instance| instance == word)
    {
        return Some(start + word_len);
    }
    if class_locals.iter().any(|local| local == word) && rest[word_len..].starts_with('(') {
        let close = closing_bracket(text, start + word_len)?;
        return Some(close + 1);
    }
    None
}

/// Whether the text after the right operand leaves it a whole operand: only
/// the end, a closing bracket, a comma or a comparison/boolean operator may
/// follow (an arithmetic operator, attribute, call or subscript would bind it
/// into a larger expression).
fn operand_ends_before(text: &str, end: usize, operator: &str) -> bool {
    let after = text[end..].trim_start();
    if after.is_empty() || after.starts_with([')', ',', ']']) {
        return true;
    }
    let comparison_follows = after.starts_with("==")
        || after.starts_with("!=")
        || after.starts_with("<=")
        || after.starts_with(">=")
        || (after.starts_with(['<', '>']) && !after.starts_with("<<") && !after.starts_with(">>"));
    if comparison_follows {
        return !is_comparison(operator);
    }
    ["and", "or"].iter().any(|word| {
        after.starts_with(word)
            && !after[word.len()..]
                .chars()
                .next()
                .is_some_and(is_python_identifier_char)
    })
}

/// Whether every bracket between the assertion's operand depth and `idx` is a
/// grouping parenthesis whose value is read as an operand
/// (`(a + b).cents == 12`), not a call argument that a wrapper may ignore.
fn grouping_chain_is_plain(text: &str, idx: usize, base: usize) -> bool {
    let chain = enclosing_brackets(text, idx);
    if chain.len() < base {
        return false;
    }
    chain[base..].iter().all(|&open| {
        if text.as_bytes()[open] != b'(' {
            return false;
        }
        let before = text[..open].trim_end();
        let grouping = before.is_empty()
            || before.ends_with(['(', ',', '=', '<', '>', '!'])
            || matches!(trailing_word(before), "assert" | "not" | "and" | "or");
        if !grouping {
            return false;
        }
        let Some(close) = closing_bracket(text, open) else {
            return false;
        };
        let after = text[close + 1..].trim_start();
        match after.strip_prefix('.') {
            Some(attribute) => {
                let len = attribute
                    .find(|ch: char| !is_python_identifier_char(ch))
                    .unwrap_or(attribute.len());
                len > 0 && !attribute[len..].trim_start().starts_with(['(', '['])
            }
            None => true,
        }
    })
}

/// The operator token that starts at `idx`, when it is a whole binary operator
/// (not part of `->`, `**`, `<<`, or an augmented assignment).
fn operator_at(text: &str, idx: usize) -> Option<&'static str> {
    let rest = &text[idx..];
    let previous = text[..idx].chars().next_back();
    let token = [
        "==", "!=", "<=", ">=", "//", "+", "-", "*", "/", "%", "<", ">",
    ]
    .into_iter()
    .find(|token| rest.starts_with(token))?;
    let after = rest[token.len()..].chars().next();
    if after == Some('=') && !matches!(token, "==" | "!=" | "<=" | ">=") {
        return None;
    }
    if after == Some('=') {
        // `===`-like or `<==`: not Python.
        return None;
    }
    if previous.is_some_and(|prev| matches!(prev, '=' | '!' | '<' | '>' | '*' | '/' | '-' | '+')) {
        return None;
    }
    if matches!(token, "*" | "<" | ">") && after == token.chars().next() {
        return None;
    }
    if token == "-" && after == Some('>') {
        return None;
    }
    Some(token)
}

/// Whether an assertion's text runs the owner dunder through an operator whose
/// receiver is a statically bound instance of the owner class and whose result
/// is an asserted operand (`assert Money(5) + Money(7) == Money(12)`,
/// `assert (a + b).cents == 12`, `assert a == b`).
pub(in crate::analysis::language::python) fn operator_entry_in_text(
    text: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> bool {
    let Some(class) = dunder_method_class(owner) else {
        return false;
    };
    if !OPERATOR_DUNDERS
        .iter()
        .any(|(_, dunders)| dunders.contains(&owner.name.as_str()))
    {
        return false;
    }
    let Some(base) = oracle_operand_depth(text) else {
        return false;
    };
    if super::super::test_activation::activation_control(test).is_some() {
        return false;
    }
    let (class_locals, instances) = bound_receivers(test, owner, class);
    if class_locals.is_empty() {
        return false;
    }
    for (at, _) in text.char_indices() {
        let Some(operator) = operator_at(text, at) else {
            continue;
        };
        if python_text_hides_code(text, at)
            || bracket_depth_at(text, at) < base
            || !operator_runs_owner(operator, owner)
            || !grouping_chain_is_plain(text, at, base)
        {
            continue;
        }
        let Some(left_start) = instance_atom_ending_at(text, at, &class_locals, &instances) else {
            continue;
        };
        if !operand_position_before(text, left_start) {
            continue;
        }
        let after_operator = at + operator.len();
        let right_start = after_operator + text[after_operator..].len()
            - text[after_operator..].trim_start().len();
        let Some(right_end) = right_atom_end(text, right_start, &class_locals, &instances) else {
            continue;
        };
        if operand_ends_before(text, right_end, operator) {
            return true;
        }
    }
    false
}

/// A `with` statement that enters a statically bound instance of the owner's
/// class and so runs its `__enter__` and `__exit__`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::language::python) struct WithEntry {
    /// The local that holds the manager object after the block: the bound
    /// instance, or the `as` target of a class whose `__enter__` returns
    /// `self`. Its state is what an assertion after the block reads.
    pub(in crate::analysis::language::python) manager: String,
    /// Offset in the test body of the first line after the `with` block.
    pub(in crate::analysis::language::python) after_block_offset: usize,
    /// Offset in the test body of the `with` statement.
    pub(in crate::analysis::language::python) statement_offset: usize,
}

/// The single `with` statement of the test that enters an instance of the
/// owner's class, for an `__enter__` / `__exit__` owner.
pub(in crate::analysis::language::python) fn with_entry(
    test: &PythonTest,
    owner: &PythonOwner,
) -> Option<WithEntry> {
    if !matches!(owner.name.as_str(), "__enter__" | "__exit__") || !owner.class_facts.plain {
        return None;
    }
    let class = dunder_method_class(owner)?;
    if super::super::test_activation::activation_control(test).is_some() {
        return None;
    }
    let body = test.body_text.as_str();
    let (class_locals, instances) = bound_receivers(test, owner, class);
    if class_locals.is_empty() {
        return None;
    }
    let mut found: Option<WithEntry> = None;
    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let Some(header) = line
            .trim()
            .strip_prefix("with ")
            .and_then(|item| item.strip_suffix(':'))
        else {
            continue;
        };
        if python_text_hides_code(body, line_start + line.len() - line.trim_start().len()) {
            continue;
        }
        let (subject, target) = match header.split_once(" as ") {
            Some((subject, target)) => (subject.trim(), Some(target.trim())),
            None => (header.trim(), None),
        };
        // One item only: `with a, b` or a parenthesized group may enter other
        // objects first, and an item that is not exactly the manager hides it.
        if subject.contains(',') || subject.starts_with('(') {
            continue;
        }
        let is_inline = class_locals.iter().any(|local| {
            subject
                .strip_prefix(local.as_str())
                .is_some_and(|rest| rest.starts_with('('))
                && closing_bracket(subject, local.len()) == Some(subject.len() - 1)
        });
        let is_bound = instances.iter().any(|instance| instance == subject);
        if !is_inline && !is_bound {
            continue;
        }
        let manager = match (is_bound, target) {
            (true, None) => subject.to_string(),
            // `as` rebinds the name to whatever `__enter__` returns, which
            // this adapter does not read for a bound instance.
            (true, Some(_)) | (false, None) => continue,
            (false, Some(target)) => {
                if !owner.class_facts.enter_returns_self
                    || target.is_empty()
                    || !target.chars().all(is_python_identifier_char)
                {
                    continue;
                }
                // The manager local must be bound by this statement only.
                let stripped = body.replacen(&format!("as {target}"), "as _", 1);
                if text_binds_name(&stripped, target) || binds_in_target_list(&stripped, target) {
                    continue;
                }
                target.to_string()
            }
        };
        if found.is_some() {
            // Two entries: the effect cannot be attributed to one of them.
            return None;
        }
        let indent = line.len() - line.trim_start().len();
        let mut end = offset;
        // Triple-quoted text can hold lines at any indent; the block end
        // cannot be read through it.
        if body[offset..].contains("\"\"\"") || body[offset..].contains("'''") {
            return None;
        }
        for next in body[offset..].split_inclusive('\n') {
            let next_indent = next.len() - next.trim_start().len();
            let skipped = next.trim().is_empty() || next.trim_start().starts_with('#');
            if !skipped && next_indent <= indent {
                break;
            }
            end += next.len();
        }
        found = Some(WithEntry {
            manager,
            after_block_offset: end,
            statement_offset: line_start,
        });
    }
    found
}
