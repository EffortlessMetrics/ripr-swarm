//! Whether a related test's owner call enters the `if` that encloses a
//! changed side effect (RIPR-SPEC-0263, #7132).
//!
//! A guarded `v.push(x)` runs only for inputs that make its `if` condition
//! true. When every related test calls the owner with a literal that makes
//! the condition false, no mutant of the push executes under the tests, so
//! an assertion over the collection cannot reveal it, however exactly it
//! pins the collection.
//!
//! The reading is deliberately narrow and fails closed. The condition must
//! be one comparison between an immutable owner parameter and an integer
//! bound (`x > 10`, `10 <= x`), the `if` must be the innermost block
//! around the changed line, and nothing before it in the owner may rebind
//! or reassign the parameter or loop back to it. The bound is an integer
//! literal, or an upper-case name the caller resolves to the one
//! file-level `const` of that name in the indexed files (#7964). Every
//! value must fit the parameter's primitive integer type. Each call must pass an integer literal at the
//! parameter's position, or a test-local name bound once by an immutable
//! `let name = <integer literal>;` and used only as owner-call arguments.
//! The owner must not name itself past its signature (recursion can enter
//! the guard with a value no test passes), and the changed line must hold
//! no brace of its own. Any other shape is unknown, and unknown never names
//! the guard.

use super::{
    function_parameters, integer_value, is_identifier, line_starts_of, owner_calls_in,
    parameter_declaration, parameter_name, whole_word_offsets,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, TestSummary};
use crate::domain::Probe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

impl Comparison {
    /// The comparison read with its operands swapped: `10 < x` is `x > 10`.
    fn flipped(self) -> Self {
        match self {
            Comparison::Less => Comparison::Greater,
            Comparison::LessEqual => Comparison::GreaterEqual,
            Comparison::Greater => Comparison::Less,
            Comparison::GreaterEqual => Comparison::LessEqual,
            Comparison::Equal | Comparison::NotEqual => self,
        }
    }

    fn holds(self, value: i128, bound: i128) -> bool {
        match self {
            Comparison::Less => value < bound,
            Comparison::LessEqual => value <= bound,
            Comparison::Greater => value > bound,
            Comparison::GreaterEqual => value >= bound,
            Comparison::Equal => value == bound,
            Comparison::NotEqual => value != bound,
        }
    }
}

/// The guard around a changed side effect, read as `<parameter> <op>
/// <bound>`, and how a related test's owner call passes that parameter.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct GuardEntry {
    owner: String,
    method: bool,
    local_roots: Vec<String>,
    parameter: String,
    index: usize,
    comparison: Comparison,
    bound: i128,
    range: (i128, i128),
    condition: String,
}

impl GuardEntry {
    /// The guard of the probe's line, or `None` when the innermost block
    /// around it is not an `if` this module can read (see the module
    /// documentation for the accepted shape). A named bound (`x > LIMIT`)
    /// is read only when `resolve_bound` returns its integer value.
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        resolve_bound: impl FnOnce(&str) -> Option<i128>,
    ) -> Option<Self> {
        if owner.name.is_empty() {
            return None;
        }
        let relative = probe.location.line.checked_sub(owner.start_line)?;
        let masked = mask_comments_and_strings(&owner.body);
        let line_offset = masked
            .split_inclusive('\n')
            .take(relative)
            .map(str::len)
            .sum::<usize>();
        if line_offset == 0 || line_offset > masked.len() {
            return None;
        }
        // A brace on the changed line itself (`} else { small(v); }`,
        // `if y { v.push(x) }`) means the line opens or closes a block the
        // scan below does not see, so the enclosing `if` is not read.
        let probe_line = masked
            .get(line_offset..)?
            .lines()
            .next()
            .unwrap_or_default();
        if probe_line.contains(['{', '}']) {
            return None;
        }
        // An owner that names itself past its signature may call itself
        // (`walk(v, depth + 1)`), and a recursive call can enter the guard
        // with a value no test passes.
        if whole_word_offsets(&masked, &owner.name).len() > 1 {
            return None;
        }
        let before = masked.get(..line_offset)?;
        let brace = innermost_open_brace(before)?;
        let head = before.get(..brace)?;
        let keyword = *whole_word_offsets(head, "if").last()?;
        let condition = head.get(keyword + "if".len()..)?.trim();
        // The `if` must open this very block: text with a statement end,
        // a block, or an arm arrow between them belongs to something else.
        if condition.is_empty() || condition.contains(['{', '}', ';']) || condition.contains("=>") {
            return None;
        }
        let (parameter, comparison, bound) = parameter_comparison(condition)?;
        let parameters = function_parameters(owner);
        let index = parameters
            .iter()
            .position(|declared| parameter_name(declared) == parameter)?;
        let declared = parameter_declaration(owner, &parameter)?;
        let body_start = masked.find('\n')? + 1;
        let prelude = masked.get(body_start..keyword)?;
        if declared.mutable || may_rebind_before(prelude, &parameter) {
            return None;
        }
        // Every value is judged in the parameter's own integer type: with
        // `overflowing_literals` allowed, `260` passed as a `u8` wraps to 4
        // at runtime, so a value outside the type's range is unread.
        if declared.qualified {
            return None;
        }
        let range = integer_range(declared.type_name.as_deref()?)?;
        let bound = match bound {
            Bound::Literal(value) => value,
            Bound::Named(name) => {
                // The guard must be the owner's only mention of the name: a
                // parameter, local `let` or nested `const` of that name
                // would shadow the item the caller resolves.
                // A `use` in the owner's body (`use std::u8::*;`) may bind
                // the name closer than the file-level item.
                if whole_word_offsets(&masked, &name).len() != 1
                    || !whole_word_offsets(&masked, "use").is_empty()
                {
                    return None;
                }
                resolve_bound(&name)?
            }
        };
        if !fits(bound, range) {
            return None;
        }
        let method = owner.item.has_self_param || super::owner_has_self_parameter(owner);
        let mut local_roots = ["crate", "self", "super", "Self"]
            .map(str::to_string)
            .to_vec();
        if let crate::analysis::facts::FunctionImplContext::Impl { self_type } = &owner.impl_context
            && let Some(type_name) = self_type
                .split('<')
                .next()
                .and_then(|path| path.rsplit("::").next())
                .map(str::trim)
                .filter(|name| !name.is_empty())
        {
            local_roots.push(type_name.to_string());
        }
        Some(Self {
            owner: owner.name.clone(),
            method,
            local_roots,
            parameter,
            index,
            comparison,
            bound,
            range,
            condition: condition.split_whitespace().collect::<Vec<_>>().join(" "),
        })
    }

    /// The guard's condition as written, whitespace collapsed.
    pub(in crate::analysis) fn condition(&self) -> &str {
        &self.condition
    }

    /// The guarded parameter's name.
    pub(in crate::analysis) fn parameter(&self) -> &str {
        &self.parameter
    }

    /// The literal inputs the test passes, when the test calls the owner
    /// and every call provably leaves the guard false. `None` when the test
    /// does not name the owner, some mention is not a readable direct call,
    /// some input is not an integer literal, or some input enters the guard.
    pub(in crate::analysis) fn unentered_inputs(&self, test: &TestSummary) -> Option<Vec<String>> {
        let masked = mask_comments_and_strings(&test.body);
        let mentions = whole_word_offsets(&masked, &self.owner);
        if mentions.is_empty() {
            return None;
        }
        let mut inputs = Vec::new();
        let mut named_arguments = std::collections::BTreeMap::<&str, usize>::new();
        for line_start in line_starts_of(&test.body, &mentions) {
            let line_end = test.body[line_start..]
                .find('\n')
                .map_or(test.body.len(), |offset| line_start + offset);
            let line = &test.body[line_start..line_end];
            let calls = owner_calls_in(line, &self.owner, self.method, &self.local_roots);
            let line_mentions = mentions
                .iter()
                .filter(|offset| (line_start..line_end).contains(*offset))
                .count();
            if calls.len() != line_mentions {
                return None;
            }
            for call in calls {
                let input = call.arguments.get(self.index)?.trim();
                let (value, shown) = match integer_value(input) {
                    Some(value) => (value, input.to_string()),
                    None => {
                        let value = let_bound_integer(test, &masked, input)?;
                        *named_arguments.entry(input).or_default() += 1;
                        (value, format!("{input} = {value}"))
                    }
                };
                if !fits(value, self.range) || self.comparison.holds(value, self.bound) {
                    return None;
                }
                inputs.push(shown);
            }
        }
        // A let-bound name is read only when its `let` and these owner-call
        // arguments are its only mentions: any other use may rebind it in a
        // pattern, closure parameter or shadowing block.
        for (name, arguments) in named_arguments {
            if whole_word_offsets(&masked, name).len() != arguments + 1 {
                return None;
            }
        }
        Some(inputs)
    }
}

/// The value of `name` when the test binds it exactly once, before any
/// other mention, with an immutable `let name = <integer literal>;` (an
/// optional plain type annotation allowed) as a statement of the test's own
/// body block. A test with a nested `fn` refuses: an item there resolves
/// `name` without the local. The caller checks the binding's other
/// mentions are all read owner-call arguments.
fn let_bound_integer(test: &TestSummary, masked: &str, name: &str) -> Option<i128> {
    if !is_identifier(name) || !test.nested_fn_names.is_empty() {
        return None;
    }
    let first = *whole_word_offsets(masked, name).first()?;
    let before = masked.get(..first)?;
    let statement = before.trim_end().strip_suffix("let")?;
    // `let` must begin a statement of the test's own block: the text before
    // it ends a statement or opens that block, at brace depth one.
    if !statement.trim_end().ends_with([';', '{', '}']) {
        return None;
    }
    let depth = before.matches('{').count();
    if depth.checked_sub(before.matches('}').count())? != 1 {
        return None;
    }
    let rest = masked.get(first + name.len()..)?;
    let declaration = rest.get(..rest.find(';')?)?.trim();
    let value = match declaration.strip_prefix(':') {
        Some(typed) => {
            let (ty, value) = typed.split_once('=')?;
            if !is_identifier(ty.trim()) {
                return None;
            }
            value
        }
        None => declaration.strip_prefix('=')?,
    };
    // `==` after the name would be a comparison, not a binding.
    if value.starts_with('=') {
        return None;
    }
    integer_value(value.trim())
}

/// The values a primitive integer type holds, or `None` for any other type
/// name (an alias such as `type Small = u32` is not read).
fn integer_range(type_name: &str) -> Option<(i128, i128)> {
    Some(match type_name {
        "u8" => (0, u8::MAX.into()),
        "u16" => (0, u16::MAX.into()),
        "u32" => (0, u32::MAX.into()),
        "u64" => (0, u64::MAX.into()),
        // `integer_value` reads at most `i128::MAX`.
        "u128" => (0, i128::MAX),
        "usize" => (0, u32::MAX.into()),
        "i8" => (i8::MIN.into(), i8::MAX.into()),
        "i16" => (i16::MIN.into(), i16::MAX.into()),
        "i32" => (i32::MIN.into(), i32::MAX.into()),
        "i64" => (i64::MIN.into(), i64::MAX.into()),
        "i128" => (i128::MIN, i128::MAX),
        "isize" => (i32::MIN.into(), i32::MAX.into()),
        _ => return None,
    })
}

/// Whether `value` is representable in `range` without wrapping. `usize`
/// and `isize` use the 32-bit range, which every supported target holds.
fn fits(value: i128, (min, max): (i128, i128)) -> bool {
    (min..=max).contains(&value)
}

/// The offset of the innermost `{` still open at the end of `before`, or
/// `None` when the innermost open delimiter is a `(` or `[`.
fn innermost_open_brace(before: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in before.bytes().enumerate().rev() {
        match byte {
            b'}' | b')' | b']' => depth += 1,
            b'{' | b'(' | b'[' => {
                if depth == 0 {
                    return (byte == b'{').then_some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The other side of a guard's comparison: an integer literal, or an
/// upper-case name (`LIMIT`) the caller may resolve to a `const`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Bound {
    Literal(i128),
    Named(String),
}

/// Whether `text` is named like a constant: upper-case letters, digits and
/// underscores, starting with a letter. A lower-case side is read as the
/// parameter, so `x > y` keeps refusing.
fn is_constant_name(text: &str) -> bool {
    is_identifier(text)
        && text.starts_with(|ch: char| ch.is_ascii_uppercase())
        && text
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

/// `x > 10`, `10 < x` or `x > LIMIT` (optionally parenthesized) as the
/// parameter, the comparison with the parameter on the left, and the bound.
fn parameter_comparison(condition: &str) -> Option<(String, Comparison, Bound)> {
    let mut condition = condition.trim();
    while let Some(inner) = condition
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    {
        condition = inner.trim();
    }
    if condition.contains(['(', ')', '&', '|', '!']) && !condition.contains("!=") {
        return None;
    }
    let (at, width, comparison) = single_comparison(condition)?;
    let left = condition.get(..at)?.trim();
    let right = condition.get(at + width..)?.trim();
    // A literal bound takes any identifier as the parameter, upper-case
    // included (`X > 10`); only between two names does the upper-case one
    // read as the bound.
    let named = |bound: &str, parameter: &str| {
        is_constant_name(bound) && is_identifier(parameter) && !is_constant_name(parameter)
    };
    match (integer_value(left), integer_value(right)) {
        (None, Some(bound)) if is_identifier(left) => {
            Some((left.to_string(), comparison, Bound::Literal(bound)))
        }
        (Some(bound), None) if is_identifier(right) => Some((
            right.to_string(),
            comparison.flipped(),
            Bound::Literal(bound),
        )),
        (None, None) if named(right, left) => Some((
            left.to_string(),
            comparison,
            Bound::Named(right.to_string()),
        )),
        (None, None) if named(left, right) => Some((
            right.to_string(),
            comparison.flipped(),
            Bound::Named(left.to_string()),
        )),
        _ => None,
    }
}

/// The integer value of the bound `name` in a guard of `owner` (#7964):
/// exactly one `const`/`static` of that name in the indexed files, a
/// file-level `const name: <integer type> = <integer literal>;` in the
/// owner's own file, with no import binding the name there, the owner
/// outside any inline `mod`, and no macro in that file that may declare
/// items the owner sees. Anything else is unread.
pub(in crate::analysis) fn named_integer_bound(
    index: &crate::analysis::rust_index::RustIndex,
    owner: &FunctionSummary,
    name: &str,
) -> Option<i128> {
    use ra_ap_syntax::{AstNode, ast, ast::HasAttrs, ast::HasName};
    let mut declarations = 0usize;
    let mut owner_source = None;
    for (path, file) in index.files().iter() {
        let masked = mask_comments_and_strings(&file.source);
        declarations += whole_word_offsets(&masked, name)
            .into_iter()
            .filter(|offset| {
                let before = masked[..*offset].trim_end();
                let before = before.strip_suffix("mut").map_or(before, str::trim_end);
                before.ends_with("const") || before.ends_with("static")
            })
            .count();
        if *path == owner.file {
            owner_source = Some(file.source.to_string());
        }
    }
    let source = owner_source?;
    let source = source.as_str();
    if declarations != 1
        || super::may_declare_items_in_owner_scope(source, &owner.name)
        || super::flattened_use_paths(source).iter().any(|import| {
            import.alias.as_deref() == Some(name)
                || (import.alias.is_none() && import.path.rsplit("::").next() == Some(name))
        })
    {
        return None;
    }
    let parse = crate::analysis::syntax::parse_clean_source_file(source)?;
    let mut value = None;
    for node in parse.tree().syntax().descendants() {
        if let Some(function) = ast::Fn::cast(node.clone())
            && function
                .name()
                .is_some_and(|fn_name| fn_name.text() == owner.name)
            && function
                .syntax()
                .ancestors()
                .any(|ancestor| ast::Module::can_cast(ancestor.kind()))
        {
            return None;
        }
        let Some(constant) = ast::Const::cast(node) else {
            continue;
        };
        if constant
            .name()
            .is_none_or(|const_name| const_name.text() != name)
        {
            continue;
        }
        // A `#[cfg(..)]` may remove the `const` under test, leaving a
        // glob-imported item of the same name in its place, so any
        // attribute on it leaves the value unread.
        if !constant
            .syntax()
            .parent()
            .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()))
            || constant.attrs().next().is_some()
        {
            return None;
        }
        let ty = constant.ty()?.syntax().text().to_string();
        if !matches!(
            ty.trim(),
            "u8" | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
        ) {
            return None;
        }
        value = Some(integer_value(
            constant.body()?.syntax().text().to_string().trim(),
        )?);
    }
    value
}

/// The one comparison operator in `condition`: its offset, width and kind.
/// `None` when there is none, more than one, or another operator such as
/// `&&`, `||`, `!` or a shift.
fn single_comparison(condition: &str) -> Option<(usize, usize, Comparison)> {
    let bytes = condition.as_bytes();
    let mut found = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let pair = bytes.get(index..index + 2);
        let (width, comparison) = match (bytes[index], pair) {
            (_, Some(b"<=")) => (2, Comparison::LessEqual),
            (_, Some(b">=")) => (2, Comparison::GreaterEqual),
            (_, Some(b"==")) => (2, Comparison::Equal),
            (_, Some(b"!=")) => (2, Comparison::NotEqual),
            (_, Some(b"<<" | b">>")) => return None,
            (b'<', _) => (1, Comparison::Less),
            (b'>', _) => (1, Comparison::Greater),
            (b'=' | b'!', _) => return None,
            _ => {
                index += 1;
                continue;
            }
        };
        if found.is_some() {
            return None;
        }
        found = Some((index, width, comparison));
        index += width;
    }
    found
}

/// Whether the owner text before the guard may give the parameter another
/// value by the time the guard reads it: any loop (its body may run the
/// guard again after a reassignment), or any mention of the parameter other
/// than a plain read in an expression. Telling a call argument (`f(x)`)
/// from a pattern position (`Some(x) =`) needs a parser, so both refuse.
fn may_rebind_before(prelude: &str, parameter: &str) -> bool {
    if ["loop", "while", "for"]
        .iter()
        .any(|keyword| !whole_word_offsets(prelude, keyword).is_empty())
    {
        return true;
    }
    whole_word_offsets(prelude, parameter)
        .into_iter()
        .any(|offset| {
            let prefix = prelude[..offset].trim_end();
            let suffix = prelude[offset + parameter.len()..].trim_start();
            ["let", "mut", "ref", "|", "(", ",", "{", "@", "[", "&"]
                .iter()
                .any(|marker| prefix.ends_with(marker))
                || (prefix.ends_with(':') && !prefix.ends_with("::"))
                || (suffix.starts_with('=') && !suffix.starts_with("=="))
                || [
                    "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>=", "@", ":",
                ]
                .iter()
                .any(|marker| suffix.starts_with(marker))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_comparison_reads_one_literal_bound_either_way_round() {
        assert_eq!(
            parameter_comparison("x > 10"),
            Some(("x".to_string(), Comparison::Greater, Bound::Literal(10)))
        );
        assert_eq!(
            parameter_comparison("(10 <= x)"),
            Some((
                "x".to_string(),
                Comparison::GreaterEqual,
                Bound::Literal(10)
            ))
        );
        assert_eq!(
            parameter_comparison("x != 0"),
            Some(("x".to_string(), Comparison::NotEqual, Bound::Literal(0)))
        );
        assert_eq!(
            parameter_comparison("X > 10"),
            Some(("X".to_string(), Comparison::Greater, Bound::Literal(10)))
        );
        assert_eq!(
            parameter_comparison("LIMIT_2 <= x"),
            Some((
                "x".to_string(),
                Comparison::GreaterEqual,
                Bound::Named("LIMIT_2".to_string())
            ))
        );
        for unread in [
            "x > y",
            "x > 10 && y",
            "x + 1 > 10",
            "!x",
            "x.len() > 10",
            "x >> 2 > 1",
            "let Some(v) = x",
            "LIMIT > MAX",
            "x > Limit",
        ] {
            assert_eq!(parameter_comparison(unread), None, "{unread}");
        }
    }

    #[test]
    fn comparison_holds_like_rust() {
        assert!(!Comparison::Greater.holds(4, 10));
        assert!(Comparison::Greater.holds(11, 10));
        assert!(!Comparison::Greater.holds(10, 10));
        assert!(Comparison::GreaterEqual.holds(10, 10));
        assert!(Comparison::Less.flipped() == Comparison::Greater);
    }

    #[test]
    fn a_loop_or_rebinding_before_the_guard_refuses() {
        assert!(!may_rebind_before("    let y = x + 1;\n    ", "x"));
        assert!(may_rebind_before("    let x = x + 1;\n    ", "x"));
        assert!(may_rebind_before("    x = 3;\n    ", "x"));
        assert!(may_rebind_before("    x += 3;\n    ", "x"));
        assert!(may_rebind_before("    for i in 0..3 {\n    ", "x"));
        assert!(may_rebind_before(
            "    let Some(x) = other else { return };\n",
            "x"
        ));
    }

    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation, SymbolId};
    use std::path::{Path, PathBuf};

    fn facts_for(source: &str) -> crate::analysis::facts::FileFacts {
        crate::analysis::rust_index::summarize_file(PathBuf::from("src/lib.rs"), source.to_string())
    }

    fn probe_at(line: usize) -> Probe {
        Probe {
            id: ProbeId("guard".into()),
            location: SourceLocation {
                file: "src/lib.rs".into(),
                line,
                column: 1,
            },
            owner: Some(SymbolId("src/lib.rs::add".into())),
            family: ProbeFamily::SideEffect,
            delta: DeltaKind::Effect,
            before: None,
            after: None,
            expression: "v.push(x);".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    /// The guard of line `line` of the `add` owner in `source`.
    fn guard_of(source: &str, line: usize) -> Option<GuardEntry> {
        let facts = facts_for(source);
        let owner = facts.functions.iter().find(|f| f.name == "add")?;
        GuardEntry::establish(&probe_at(line), owner, |_| None)
    }

    const PUSH_IF_GT_10: &str =
        "pub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > 10 {\n        v.push(x);\n    }\n}\n";

    /// The inputs `guard` reads from one test body calling `add`.
    fn inputs_of(guard: &GuardEntry, body: &str) -> Option<Vec<String>> {
        let facts = facts_for(&format!("#[test]\nfn t() {{\n{body}\n}}\n"));
        guard.unentered_inputs(facts.tests.first()?)
    }

    fn established(source: &str, line: usize) -> Result<GuardEntry, String> {
        guard_of(source, line).ok_or_else(|| format!("no guard established for:\n{source}"))
    }

    #[test]
    fn establish_refuses_every_unreadable_enclosing_shape() {
        // The changed line is outside any `if` block.
        assert!(
            guard_of(
                "pub fn add(v: &mut Vec<u32>, x: u32) {\n    v.push(x);\n}\n",
                2
            )
            .is_none()
        );
        // The signature line has no preceding block.
        assert!(guard_of(PUSH_IF_GT_10, 1).is_none());
        // A line before the owner's own start.
        let facts = facts_for(&format!("\n\n{PUSH_IF_GT_10}"));
        let owner = facts.functions.iter().find(|f| f.name == "add");
        assert!(
            owner
                .is_some_and(|owner| GuardEntry::establish(&probe_at(1), owner, |_| None).is_none())
        );
        // An `else` block is not the `if` it follows.
        assert!(
            guard_of(
                "pub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > 10 {\n        v.clear();\n    } else {\n        v.push(x);\n    }\n}\n",
                5
            )
            .is_none()
        );
        // A `loop` block around the line is not an `if`.
        assert!(
            guard_of(
                "pub fn add(v: &mut Vec<u32>, x: u32) {\n    loop {\n        v.push(x);\n    }\n}\n",
                3
            )
            .is_none()
        );
        // A brace on the changed line itself.
        assert!(
            guard_of(
                "pub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > 10 {\n        if true { v.push(x); }\n    }\n}\n",
                3
            )
            .is_none()
        );
        // A compound or non-parameter condition.
        for condition in [
            "x > 10 && x < 20",
            "y > 10",
            "x.is_positive()",
            "x + 1 > 10",
        ] {
            let source = format!(
                "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    let y = 1u32;\n    if {condition} {{\n        v.push(x);\n    }}\n}}\n"
            );
            assert!(guard_of(&source, 4).is_none(), "{condition}");
        }
    }

    #[test]
    fn establish_refuses_a_parameter_that_may_change_before_the_guard() {
        for (parameter, prelude) in [
            ("mut x: u32", ""),
            ("x: u32", "    let x = x + 1;\n"),
            ("x: u32", "    for _ in 0..3 {}\n"),
            (
                "x: u32",
                "    let mut y = x;\n    y += 1;\n    while y < 3 { y += 1; }\n",
            ),
        ] {
            let source = format!(
                "pub fn add(v: &mut Vec<u32>, {parameter}) {{\n{prelude}    if x > 10 {{\n        v.push(x);\n    }}\n}}\n"
            );
            let line = source.lines().count() - 2;
            assert!(guard_of(&source, line).is_none(), "{parameter} / {prelude}");
        }
        // A plain read before the guard is fine.
        let source = "pub fn add(v: &mut Vec<u32>, x: u32) {\n    let y = x + 1;\n    if x > 10 {\n        v.push(y);\n    }\n}\n";
        assert!(guard_of(source, 4).is_some());
    }

    #[test]
    fn establish_judges_the_bound_in_the_parameters_own_integer_type() {
        let source = |ty: &str, bound: &str| {
            format!(
                "pub fn add(v: &mut Vec<u32>, x: {ty}) {{\n    if x > {bound} {{\n        v.push(1);\n    }}\n}}\n"
            )
        };
        assert!(guard_of(&source("u8", "255"), 3).is_some());
        // 256 wraps in a `u8`, so the comparison is unread.
        assert!(guard_of(&source("u8", "256"), 3).is_none());
        assert!(guard_of(&source("u8", "-1"), 3).is_none());
        assert!(guard_of(&source("i8", "-128"), 3).is_some());
        assert!(guard_of(&source("i8", "-129"), 3).is_none());
        // Aliases and non-integer parameter types are unread.
        assert!(guard_of(&source("Small", "1"), 3).is_none());
        assert!(guard_of(&source("std::primitive::u32", "1"), 3).is_none());
    }

    #[test]
    fn unentered_inputs_requires_every_call_to_miss_the_guard_exactly() -> Result<(), String> {
        let guard = established(PUSH_IF_GT_10, 3)?;
        let call =
            |argument: &str| format!("    let mut v = Vec::new();\n    add(&mut v, {argument});");
        // `x > 10`: 10 misses, 11 enters.
        assert_eq!(inputs_of(&guard, &call("10")), Some(vec!["10".to_string()]));
        assert_eq!(inputs_of(&guard, &call("0")), Some(vec!["0".to_string()]));
        assert_eq!(inputs_of(&guard, &call("11")), None);
        // A value outside the `u32` range wraps at runtime: unread.
        assert_eq!(inputs_of(&guard, &call("-1")), None);
        assert_eq!(inputs_of(&guard, &call("4294967296")), None);
        // Not an integer literal, and no argument at the parameter's index.
        assert_eq!(inputs_of(&guard, &call("n")), None);
        assert_eq!(
            inputs_of(&guard, "    let mut v = Vec::new();\n    add(&mut v);"),
            None
        );
        // The owner is not named at all, or is named without a call.
        assert_eq!(inputs_of(&guard, "    let v: Vec<u32> = Vec::new();"), None);
        assert_eq!(
            inputs_of(&guard, "    let f = add;\n    f(&mut Vec::new(), 1);"),
            None
        );
        // One entering call among missing ones spoils the test.
        assert_eq!(
            inputs_of(
                &guard,
                "    let mut v = Vec::new();\n    add(&mut v, 1);\n    add(&mut v, 20);"
            ),
            None
        );
        assert_eq!(
            inputs_of(
                &guard,
                "    let mut v = Vec::new();\n    add(&mut v, 1);\n    add(&mut v, 2);"
            ),
            Some(vec!["1".to_string(), "2".to_string()])
        );
        Ok(())
    }

    #[test]
    fn unentered_inputs_reads_a_let_bound_name_only_when_it_is_used_as_nothing_else()
    -> Result<(), String> {
        let guard = established(PUSH_IF_GT_10, 3)?;
        let with_let = |binding: &str, extra: &str| {
            format!("    {binding}\n    let mut v = Vec::new();\n    add(&mut v, n);\n{extra}")
        };
        assert_eq!(
            inputs_of(&guard, &with_let("let n = 3;", "")),
            Some(vec!["n = 3".to_string()])
        );
        assert_eq!(
            inputs_of(&guard, &with_let("let n: u32 = 4;", "")),
            Some(vec!["n = 4".to_string()])
        );
        // A binding that enters the guard is refused like a literal.
        assert_eq!(inputs_of(&guard, &with_let("let n = 11;", "")), None);
        // Mutable, non-literal and call bindings are unread.
        assert_eq!(inputs_of(&guard, &with_let("let mut n = 3;", "")), None);
        assert_eq!(inputs_of(&guard, &with_let("let n = 1 + 2;", "")), None);
        assert_eq!(inputs_of(&guard, &with_let("let n = other();", "")), None);
        // Any further mention of the name may rebind or shadow it.
        assert_eq!(
            inputs_of(&guard, &with_let("let n = 3;", "    drop(n);")),
            None
        );
        assert_eq!(
            inputs_of(
                &guard,
                &with_let("let n = 3;", "    { let n = 20; add(&mut v, n); }")
            ),
            None
        );
        // A `let` inside a nested block is not a statement of the test's own block.
        assert_eq!(
            inputs_of(&guard, "    { let n = 3; add(&mut Vec::new(), n); }"),
            None
        );
        // The name is bound after its first use.
        assert_eq!(
            inputs_of(&guard, "    add(&mut Vec::new(), n);\n    let n = 3;"),
            None
        );
        Ok(())
    }

    #[test]
    fn let_bound_integer_refuses_a_test_with_a_nested_fn() -> Result<(), String> {
        let facts =
            facts_for("#[test]\nfn t() {\n    let n = 3;\n    fn n() {}\n    drop(n);\n}\n");
        let test = facts.tests.first().ok_or("test missing")?;
        let masked = mask_comments_and_strings(&test.body);
        assert_eq!(let_bound_integer(test, &masked, "n"), None);
        let facts = facts_for("#[test]\nfn t() {\n    let n = 3;\n    drop(n);\n}\n");
        let test = facts.tests.first().ok_or("test missing")?;
        let masked = mask_comments_and_strings(&test.body);
        assert_eq!(let_bound_integer(test, &masked, "n"), Some(3));
        assert_eq!(let_bound_integer(test, &masked, "not an identifier"), None);
        assert_eq!(let_bound_integer(test, &masked, "absent"), None);
        Ok(())
    }

    static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn named_bound(files: &[(&str, &str)], owner_file: &str, name: &str) -> Option<i128> {
        let root = std::env::temp_dir().join(format!(
            "ripr-guard-entry-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).ok()?;
        struct CleanupGuard(PathBuf);
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = CleanupGuard(root.clone());
        let mut paths = Vec::new();
        for (relative, source) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent()?).ok()?;
            std::fs::write(&path, source).ok()?;
            paths.push(PathBuf::from(*relative));
        }
        let index = crate::analysis::rust_index::build_index(&root, &paths).ok()?;
        let owner = index
            .functions()
            .iter()
            .find(|f| f.name == "add" && f.file == Path::new(owner_file))?;
        named_integer_bound(&index, owner, name)
    }

    #[test]
    fn named_integer_bound_reads_one_file_level_integer_const_and_refuses_the_rest() {
        let owner = "pub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > LIMIT {\n        v.push(x);\n    }\n}\n";
        let with = |declaration: &str| format!("{declaration}\n{owner}");
        let read = |declaration: &str| {
            named_bound(&[("src/lib.rs", &with(declaration))], "src/lib.rs", "LIMIT")
        };
        assert_eq!(read("const LIMIT: u32 = 10;"), Some(10));
        assert_eq!(read("const LIMIT: i64 = 0x10;"), Some(16));
        assert_eq!(read("const LIMIT: u8 = 1_0;"), Some(10));
        // Not an integer type, not a literal, or attributed: unread.
        assert_eq!(read("type Small = u32;\nconst LIMIT: Small = 10;"), None);
        assert_eq!(read("const LIMIT: u32 = 5 + 5;"), None);
        assert_eq!(read("const LIMIT: f32 = 10.0;"), None);
        assert_eq!(read("#[cfg(unix)]\nconst LIMIT: u32 = 10;"), None);
        // A renaming or same-name import may bind the name instead.
        assert_eq!(
            read("use other::MAX as LIMIT;\nconst LIMIT: u32 = 10;"),
            None
        );
        // No declaration at all.
        assert_eq!(read("const OTHER: u32 = 10;"), None);
        // A second declaration of the name anywhere in the index is ambiguous.
        assert_eq!(
            named_bound(
                &[
                    ("src/lib.rs", &with("const LIMIT: u32 = 10;")),
                    ("src/other.rs", "pub const LIMIT: u32 = 99;\n"),
                ],
                "src/lib.rs",
                "LIMIT"
            ),
            None
        );
        // An owner inside an inline module is outside the file-level scope.
        let nested = format!("const LIMIT: u32 = 10;\nmod inner {{\n{owner}}}\n");
        assert_eq!(
            named_bound(&[("src/lib.rs", &nested)], "src/lib.rs", "LIMIT"),
            None
        );
    }
}
