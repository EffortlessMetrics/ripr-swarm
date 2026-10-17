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
}
