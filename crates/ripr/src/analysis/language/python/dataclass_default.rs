//! A changed numeric default of a plain `@dataclass` field, read through a test
//! that omits the field (#9619).
//!
//! `tax_rate: float = 0.2` -> `tax_rate: float = 0.25` changes what
//! `Invoice(100).total()` returns, so `assert Invoice(100).total() == 120`
//! fails after the edit. The class decorator is what keeps every dataclass
//! behind the `decorator_indirection` abstention: it can regenerate `__init__`,
//! freeze the instance or rewrite fields. This pass credits one shape and
//! evaluates the old and the new class itself instead of trusting the name:
//!
//! - the diff of the file is this one line, so the method that reads the field
//!   is unchanged between the old and the new source;
//! - the class is `class C:` under a bare `@dataclass` imported from
//!   `dataclasses`, with no base class, no other decorator, and only annotated
//!   fields (numeric literal defaults or `field(default_factory=..)`) and
//!   undecorated methods in its body, none of them `__init__`, `__post_init__`,
//!   `__new__` or an attribute hook;
//! - the edit replaces one numeric literal default with another on the same
//!   annotated field;
//! - the test is a plain function whose only statement is
//!   `assert C(<numeric literals>).<method>() == <numeric literal>`, imported
//!   from the owner's module, that omits the edited field, so the default is
//!   what the instance holds;
//! - the method is a single `return` over `+ - * /`, numeric literals,
//!   `self.<field>` and `round(..)`, and the asserted value is computed under
//!   the old default (the assertion held) and the new one (it no longer does).
//!
//! Anything else keeps the abstention.

use super::related_tests::{PythonRelatedCandidate, import_source_module_matches_owner};
use super::{PythonAssertion, PythonOwner, PythonTest};

/// The limit evidence this credit replaces.
pub(super) const DATACLASS_LIMIT_EVIDENCE: &str = "static_limit decorator_indirection: `dataclass`";

/// One assertion that fails once the default is edited.
pub(super) struct DataclassDefaultCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
}

/// Methods that run on construction or intercept attribute access, so the
/// field read in the asserted method may not be the stored default.
const HOOK_METHODS: &[&str] = &[
    "__init__",
    "__post_init__",
    "__new__",
    "__init_subclass__",
    "__set_name__",
    "__getattr__",
    "__getattribute__",
    "__setattr__",
    "__delattr__",
    "__slots__",
];

/// Annotation bases that are plain stored fields, never constructor markers.
const BUILTIN_ANNOTATIONS: &[&str] = &[
    "int", "float", "str", "bool", "bytes", "complex", "list", "dict", "set", "tuple",
];

/// The only spellings of the import: the whole statement is the line.
const IMPORT_LINES: &[&str] = &[
    "from dataclasses import dataclass",
    "from dataclasses import dataclass, field",
    "from dataclasses import field, dataclass",
];

/// Builtins the class body may use whose meaning a module could replace, each
/// checked by `builtins_are_unreplaced`.
const WATCHED_BUILTINS: &[&str] = &[
    "int",
    "float",
    "str",
    "bool",
    "bytes",
    "complex",
    "list",
    "dict",
    "set",
    "tuple",
    "round",
    "property",
    "classmethod",
    "staticmethod",
];

/// Class-body decorators that return the function or a standard descriptor.
const SAFE_DECORATORS: &[&str] = &["@property", "@classmethod", "@staticmethod"];

const MAX_EXPRESSION: usize = 200;
const MAX_DEPTH: usize = 24;

/// Python integers inside the range where `f64` arithmetic is exact.
const EXACT_LIMIT: i64 = 1 << 53;

pub(super) fn changed_dataclass_default_credit<'a>(
    owner: &PythonOwner,
    old_line_text: Option<&str>,
    line_text: &str,
    line: usize,
    sole_line_replacement: bool,
    owner_source: &str,
    related_candidates: &[PythonRelatedCandidate<'a>],
) -> Option<DataclassDefaultCredit<'a>> {
    if !sole_line_replacement || !is_bare_dataclass_owner(owner, owner_source) {
        return None;
    }
    let class = parse_class(owner, owner_source)?;
    let (name, old_value, new_value) =
        changed_default(old_line_text?, line_text, &class, owner_source, line)?;
    let index = class.fields.iter().position(|field| field.name == name)?;
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .find_map(|candidate| {
            let assertion = discriminating_assertion(
                candidate.test,
                owner,
                &class,
                index,
                (old_value, new_value),
            )?;
            Some(DataclassDefaultCredit {
                test: candidate.test,
                assertion,
            })
        })
}

/// The file spells `dataclass` only as `@dataclass` and in the one
/// `from dataclasses import ..` line, and has no star import, so the decorator
/// is the standard library's. A lexical, fail-closed screen: any other
/// occurrence of the word (a rebinding, a comment, another import) refuses.
fn dataclass_name_is_the_import(source: &str) -> bool {
    let mut imports = 0;
    let Some(code) = code_text(source) else {
        return false;
    };
    for (line, code_line) in source.lines().zip(&code) {
        let text = code_line.trim();
        if text.contains(" import *") {
            return false;
        }
        if IMPORT_LINES.contains(&line) {
            imports += 1;
        } else if mentions_word(code_line, "dataclass") && text != "@dataclass" {
            // Includes a trailing statement on the import line
            // (`from dataclasses import dataclass; dataclass = fake`).
            return false;
        }
    }
    imports == 1
}

/// Each line with the text of triple-quoted strings and trailing `#` comments
/// removed, so a docstring that names `property` is not read as a binding of
/// it. `None` when a quote precedes a triple quote or a comment on the same
/// line (`x = '#'; round = f`): the scan cannot tell code from text there.
fn code_text(source: &str) -> Option<Vec<String>> {
    let mut open: Option<&str> = None;
    let mut out = Vec::new();
    for line in source.lines() {
        let mut rest = line;
        let mut code = String::new();
        loop {
            if let Some(quote) = open {
                match rest.find(quote) {
                    Some(at) => {
                        rest = &rest[at + quote.len()..];
                        open = None;
                    }
                    None => break,
                }
                continue;
            }
            let triple = ["\"\"\"", "'''"]
                .into_iter()
                .filter_map(|quote| rest.find(quote).map(|at| (at, quote)))
                .min();
            let comment = rest.find('#');
            let cut = match (triple, comment) {
                (Some((at, _)), Some(c)) => at.min(c),
                (Some((at, _)), None) => at,
                (None, Some(c)) => c,
                (None, None) => {
                    code.push_str(rest);
                    break;
                }
            };
            if rest[..cut].contains(['"', '\'']) {
                return None;
            }
            code.push_str(&rest[..cut]);
            match triple {
                Some((at, quote)) if at == cut => {
                    rest = &rest[at + quote.len()..];
                    open = Some(quote);
                }
                _ => break,
            }
        }
        out.push(code);
    }
    Some(out)
}

fn mentions_word(line: &str, word: &str) -> bool {
    line.split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .any(|candidate| candidate == word)
}

/// Nothing else in the file can replace the evaluated class or a builtin the
/// class body relies on: the class name appears only in its own definition;
/// each watched builtin (`round`, `list`, `property`, ..) the class uses
/// appears nowhere outside it, and no field or method takes its name; `field`
/// appears outside only in the import. A module-level rebinding (an
/// assignment, a loop target, an import, a patch after the class) refuses.
fn class_and_builtins_are_unreplaced(
    lines: &[&str],
    code: &[String],
    span: std::ops::Range<usize>,
    name: &str,
) -> bool {
    let inside = |word: &str| {
        code[span.clone()]
            .iter()
            .any(|line| mentions_word(line, word))
    };
    let watched: Vec<&str> = WATCHED_BUILTINS
        .iter()
        .copied()
        .filter(|word| inside(word))
        .collect();
    lines.iter().enumerate().all(|(index, line)| {
        if span.contains(&index) {
            // `round` is only the call in a `return`.
            !mentions_word(line, "round")
                || (line.trim_start().starts_with("return ") && line.contains("round("))
        } else {
            let line_code = &code[index];
            !mentions_word(line_code, name)
                && !watched.iter().any(|word| mentions_word(line_code, word))
                && (!mentions_word(line_code, "field") || IMPORT_LINES.contains(line))
        }
    })
}

fn is_bare_dataclass_owner(owner: &PythonOwner, source: &str) -> bool {
    owner.owner_kind.is_none()
        && !owner.is_module_owner()
        && owner.decorators == ["dataclass"]
        && dataclass_name_is_the_import(source)
        && owner
            .imports
            .iter()
            .filter(|import| bound_name(import) == "dataclass")
            .count()
            == 1
        && owner.imports.iter().any(|import| {
            import.imported == "dataclass"
                && import.alias == import.imported
                && import.source_module == "dataclasses"
        })
}

fn bound_name(import: &super::PythonImport) -> &str {
    import.alias.as_str()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn as_float(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    fn equals(self, other: Self) -> bool {
        match (self, other) {
            (Self::Int(left), Self::Int(right)) => left == right,
            // Exact IEEE equality, as Python compares an `int` with a `float`.
            _ => self.as_float().partial_cmp(&other.as_float()) == Some(std::cmp::Ordering::Equal),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Default {
    Required,
    Literal(Num),
    Factory,
}

struct Field {
    name: String,
    default: Default,
}

struct Method {
    name: String,
    /// The expression of a lone, undecorated `def name(self): return <expr>`.
    returns: Option<String>,
}

struct Class {
    indent: usize,
    fields: Vec<Field>,
    methods: Vec<Method>,
}

/// An integer or decimal literal: digits with at most one point, no exponent,
/// separator, sign suffix or leading zero. An optional leading `-` is allowed.
fn parse_literal(text: &str) -> Option<Num> {
    let text = text.trim();
    let body = text.strip_prefix('-').unwrap_or(text);
    let first = body.bytes().next()?;
    if body.len() > 17
        || !(first.is_ascii_digit() || first == b'.')
        || !body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        || body.matches('.').count() > 1
        || body == "."
    {
        return None;
    }
    if body.contains('.') {
        let value: f64 = text.parse().ok()?;
        return value.is_finite().then_some(Num::Float(value));
    }
    if body.len() > 1 && first == b'0' {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    (-EXACT_LIMIT..=EXACT_LIMIT)
        .contains(&value)
        .then_some(Num::Int(value))
}

fn is_identifier(text: &str) -> bool {
    text.bytes()
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && text
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

/// `name: annotation` or `name: annotation = default`, the annotation a plain
/// type spelling (a `ClassVar` or `InitVar` marker is not a stored field).
fn parse_field_line(text: &str) -> Option<(String, String, Default)> {
    let (name, rest) = text.trim().split_once(':')?;
    let name = name.trim();
    let (annotation, default) = match rest.split_once(" = ") {
        Some((annotation, value)) => (annotation.trim(), Some(value.trim())),
        None => (rest.trim(), None),
    };
    if !is_identifier(name)
        || name.starts_with("__")
        || annotation.is_empty()
        || !annotation
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric() || b"[], .".contains(&byte))
        // Only a builtin type spelling: an alias or marker (`ClassVar as CV`,
        // `InitVar`, `KW_ONLY`) may exclude the name from the constructor.
        || !BUILTIN_ANNOTATIONS.contains(&annotation.split('[').next().unwrap_or("").trim())
    {
        return None;
    }
    let default = match default {
        None => Default::Required,
        Some(value) => match parse_literal(value) {
            Some(number) => Default::Literal(number),
            None => {
                let factory = value
                    .strip_prefix("field(default_factory=")?
                    .strip_suffix(')')?;
                // Only a builtin container: any other callable can raise or
                // have effects each time an instance is built.
                if !["list", "dict", "set", "tuple"].contains(&factory) {
                    return None;
                }
                Default::Factory
            }
        },
    };
    Some((name.to_string(), annotation.to_string(), default))
}

fn leading_spaces(line: &str) -> Option<usize> {
    let lead = line.len() - line.trim_start().len();
    line[..lead]
        .bytes()
        .all(|byte| byte == b' ')
        .then_some(lead)
}

/// The class body read as fields and methods, or `None` for any statement
/// outside the shapes above (an assignment, nested class, `pass`, decorator
/// stack, second definition, or an unreadable layout).
fn parse_class(owner: &PythonOwner, source: &str) -> Option<Class> {
    let lines: Vec<&str> = source.lines().collect();
    let header = format!("class {}:", owner.name);
    let header_index = (owner.start_line.saturating_sub(1)..owner.end_line.min(lines.len()))
        .find(|&index| lines[index] == header)?;
    // `@dataclass` directly above, and no other definition of the class name.
    if header_index == 0 || lines[header_index - 1] != "@dataclass" {
        return None;
    }
    let definitions = lines
        .iter()
        .filter(|text| {
            text.trim_start()
                .strip_prefix("class ")
                .and_then(|rest| rest.strip_prefix(owner.name.as_str()))
                .is_some_and(|rest| {
                    !rest.starts_with(|ch: char| ch == '_' || ch.is_ascii_alphanumeric())
                })
        })
        .count();
    if definitions != 1 {
        return None;
    }
    let end = owner.end_line.min(lines.len());
    if !class_and_builtins_are_unreplaced(
        &lines,
        &code_text(source)?,
        header_index - 1..end,
        &owner.name,
    ) {
        return None;
    }
    let mut class = Class {
        indent: 0,
        fields: Vec::new(),
        methods: Vec::new(),
    };
    let mut indent = None;
    let mut decorated = false;
    let mut index = header_index + 1;
    while index < end {
        let text = lines[index].trim();
        if text.is_empty() || text.starts_with('#') {
            index += 1;
            continue;
        }
        let lead = leading_spaces(lines[index])?;
        if lead != *indent.get_or_insert(lead) || lead == 0 {
            return None;
        }
        if text.starts_with('@') {
            // Any other decorator may run code at class creation (a
            // descriptor's `__set_name__` can rewrite a default).
            if !SAFE_DECORATORS.contains(&text) {
                return None;
            }
            decorated = true;
            index += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("def ") {
            let (name, signature) = rest.split_once('(')?;
            // Parameter defaults and annotations run at class creation.
            let parameters = signature.strip_suffix("):")?;
            if !is_identifier(name)
                || !parameters.bytes().all(|byte| {
                    byte == b'_' || byte == b',' || byte == b' ' || byte.is_ascii_alphanumeric()
                })
            {
                return None;
            }
            let mut body = Vec::new();
            index += 1;
            while index < end {
                let inner = lines[index].trim();
                if !inner.is_empty() && !inner.starts_with('#') {
                    if leading_spaces(lines[index])? <= lead {
                        break;
                    }
                    body.push(inner);
                }
                index += 1;
            }
            let returns = match body.as_slice() {
                [only] if signature == "self):" && !decorated => {
                    only.strip_prefix("return ").map(str::to_string)
                }
                _ => None,
            };
            class.methods.push(Method {
                name: name.to_string(),
                returns,
            });
            decorated = false;
            continue;
        }
        index += 1;
        let docstring = text.len() >= 6
            && ((text.starts_with("\"\"\"") && text.ends_with("\"\"\""))
                || (text.starts_with("'''") && text.ends_with("'''")));
        if docstring && class.fields.is_empty() && class.methods.is_empty() {
            continue;
        }
        if decorated {
            return None;
        }
        let (name, _, default) = parse_field_line(text)?;
        class.fields.push(Field { name, default });
    }
    class.indent = indent?;
    let defaulted_before = |position: usize| {
        class.fields[..position]
            .iter()
            .any(|field| field.default != Default::Required)
    };
    if class
        .fields
        .iter()
        .enumerate()
        .any(|(position, field)| field.default == Default::Required && defaulted_before(position))
    {
        return None;
    }
    let mut names: Vec<&str> = class
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .chain(class.methods.iter().map(|method| method.name.as_str()))
        .collect();
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1])
        || names.iter().any(|name| {
            HOOK_METHODS.contains(name) || WATCHED_BUILTINS.contains(name) || *name == "field"
        })
    {
        return None;
    }
    Some(class)
}

/// The edited field and its old and new literal, when the edit replaces one
/// numeric default with another and the new source line says so.
fn changed_default(
    old: &str,
    new: &str,
    class: &Class,
    source: &str,
    line: usize,
) -> Option<(String, Num, Num)> {
    let (old_name, old_annotation, old_default) = parse_field_line(old)?;
    let (new_name, new_annotation, new_default) = parse_field_line(new)?;
    let (Default::Literal(old_value), Default::Literal(new_value)) = (old_default, new_default)
    else {
        return None;
    };
    let in_source = source.lines().nth(line.checked_sub(1)?)?;
    let field = class.fields.iter().find(|field| field.name == new_name)?;
    (old_name == new_name
        && old_annotation == new_annotation
        && old.trim() != new.trim()
        && leading_spaces(old)? == class.indent
        && leading_spaces(new)? == class.indent
        && in_source == new
        && field.default == Default::Literal(new_value))
    .then_some((new_name, old_value, new_value))
}

/// The test's single assertion, when it fails under the new default and held
/// under the old one.
fn discriminating_assertion<'a>(
    test: &'a PythonTest,
    owner: &PythonOwner,
    class: &Class,
    changed: usize,
    (old_value, new_value): (Num, Num),
) -> Option<&'a PythonAssertion> {
    if !test_is_plain(test, owner) {
        return None;
    }
    let mut body = test
        .body_text
        .lines()
        .map(str::trim)
        .filter(|text| !text.is_empty() && !text.starts_with('#'));
    let head = body.next()?;
    let statement = body.next()?;
    if body.next().is_some() || !head.starts_with("def ") || !head.ends_with("():") {
        return None;
    }
    let call = parse_assertion(statement, &owner.name)?;
    // The test module may replace the imported class or patch its attributes,
    // and code outside the test may run at import or implicitly.
    let facts = &test.constant_rebinding;
    if facts.rebinds(&owner.name)
        || facts.implicit_reach
        || facts.header_reach
        || facts.edited_in_diff
        || facts.assigns_attribute(&owner.name)
        || facts.assigns_attribute(&call.method)
        || class
            .fields
            .iter()
            .any(|field| facts.assigns_attribute(&field.name))
    {
        return None;
    }
    let assertion = test
        .assertions
        .iter()
        .find(|assertion| assertion.gate_text.trim() == statement)?;
    // The edited field is omitted and every required field is supplied.
    let supplied = call.arguments.len();
    if supplied > class.fields.len()
        || supplied > changed
        || class
            .fields
            .iter()
            .enumerate()
            .any(|(position, field)| field.default == Default::Required && position >= supplied)
    {
        return None;
    }
    let method = class
        .methods
        .iter()
        .find(|method| method.name == call.method)?;
    let expression = method.returns.as_deref()?;
    let evaluate = |default: Num| {
        let lookup = |name: &str| {
            let position = class.fields.iter().position(|field| field.name == name)?;
            if position < supplied {
                return call.arguments.get(position).copied();
            }
            match class.fields[position].default {
                Default::Literal(_) if position == changed => Some(default),
                Default::Literal(value) => Some(value),
                _ => None,
            }
        };
        Evaluator::new(expression, &lookup).run()
    };
    let before = evaluate(old_value)?;
    let after = evaluate(new_value)?;
    (before.equals(call.expected) && !after.equals(call.expected)).then_some(assertion)
}

/// A plain function test: no decorator, parameter, fixture or control that
/// could skip, repeat or replace the call, and no construct that could catch
/// the failure.
fn test_is_plain(test: &PythonTest, owner: &PythonOwner) -> bool {
    let imports: Vec<_> = test
        .imports
        .iter()
        .filter(|import| bound_name(import) == owner.name)
        .collect();
    test.decorators.is_empty()
        && test.fixtures.is_empty()
        && test.fixture_defs.is_empty()
        && test.generated_inputs.is_empty()
        && test.setup_receivers.is_empty()
        && test.activation_controls.is_empty()
        && !test.parametrized
        && test.assertions.len() == 1
        && matches!(imports.as_slice(), [import]
            if import.imported == owner.name
                && import.alias == import.imported
                && import_source_module_matches_owner(import, owner, &test.file))
        && ![
            "try:",
            "except",
            "with ",
            "raises",
            "suppress",
            "contextlib",
        ]
        .iter()
        .any(|marker| test.body_text.contains(marker))
}

struct ParsedAssertion {
    arguments: Vec<Num>,
    method: String,
    expected: Num,
}

/// `assert Class(<numeric literals>).method() == <numeric literal>`.
fn parse_assertion(statement: &str, class: &str) -> Option<ParsedAssertion> {
    let text = statement.strip_prefix("assert ")?.trim();
    if text.matches("==").count() != 1 {
        return None;
    }
    let (left, right) = text.split_once("==")?;
    let expected = parse_literal(right)?;
    let rest = left.trim().strip_prefix(class)?.strip_prefix('(')?;
    let (arguments, rest) = rest.split_once(')')?;
    let method = rest.trim().strip_prefix('.')?.strip_suffix("()")?;
    if !is_identifier(method) {
        return None;
    }
    let arguments = if arguments.trim().is_empty() {
        Vec::new()
    } else {
        arguments
            .split(',')
            .map(parse_literal)
            .collect::<Option<Vec<_>>>()?
    };
    Some(ParsedAssertion {
        arguments,
        method: method.to_string(),
        expected,
    })
}

/// The closed expression grammar: `+ - * /`, unary minus, parentheses,
/// numeric literals, `self.<field>` and `round(<expression>)`.
struct Evaluator<'a> {
    text: &'a [u8],
    position: usize,
    depth: usize,
    field: &'a dyn Fn(&str) -> Option<Num>,
}

impl<'a> Evaluator<'a> {
    fn new(text: &'a str, field: &'a dyn Fn(&str) -> Option<Num>) -> Self {
        Self {
            text: text.as_bytes(),
            position: 0,
            depth: 0,
            field,
        }
    }

    fn run(mut self) -> Option<Num> {
        if self.text.len() > MAX_EXPRESSION {
            return None;
        }
        let value = self.expression()?;
        self.skip_space();
        (self.position == self.text.len()).then_some(value)
    }

    fn skip_space(&mut self) {
        while self.text.get(self.position) == Some(&b' ') {
            self.position += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.text.get(self.position).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.position += 1;
        }
        found
    }

    fn expression(&mut self) -> Option<Num> {
        let mut value = self.term()?;
        while let Some(operator @ (b'+' | b'-')) = self.peek() {
            self.position += 1;
            let right = self.term()?;
            value = arithmetic(operator, value, right)?;
        }
        Some(value)
    }

    fn term(&mut self) -> Option<Num> {
        let mut value = self.factor()?;
        while let Some(operator @ (b'*' | b'/')) = self.peek() {
            self.position += 1;
            // `**` and `//` are other operators.
            if self.text.get(self.position) == Some(&operator) {
                return None;
            }
            let right = self.factor()?;
            value = arithmetic(operator, value, right)?;
        }
        Some(value)
    }

    fn factor(&mut self) -> Option<Num> {
        // Nesting (`((((..`, `----..`) is bounded so a long input cannot
        // exhaust the stack.
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let value = self.factor_inner();
        self.depth -= 1;
        value
    }

    fn factor_inner(&mut self) -> Option<Num> {
        match self.peek()? {
            b'-' => {
                self.position += 1;
                match self.factor()? {
                    Num::Int(value) => bounded(value.checked_neg()?),
                    Num::Float(value) => Some(Num::Float(-value)),
                }
            }
            b'(' => {
                self.position += 1;
                let value = self.expression()?;
                self.eat(b')').then_some(value)
            }
            byte if byte.is_ascii_digit() || byte == b'.' => {
                let start = self.position;
                while self
                    .text
                    .get(self.position)
                    .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
                {
                    self.position += 1;
                }
                if self
                    .text
                    .get(self.position)
                    .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
                {
                    return None;
                }
                parse_literal(std::str::from_utf8(&self.text[start..self.position]).ok()?)
            }
            _ => self.name(),
        }
    }

    fn name(&mut self) -> Option<Num> {
        let rest = std::str::from_utf8(&self.text[self.position..]).ok()?;
        if let Some(after) = rest.strip_prefix("round(") {
            self.position += rest.len() - after.len();
            let value = self.expression()?;
            if !self.eat(b')') {
                return None;
            }
            return round(value);
        }
        let after = rest.strip_prefix("self.")?;
        let length = after
            .bytes()
            .take_while(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
            .count();
        let name = &after[..length];
        if !is_identifier(name) {
            return None;
        }
        self.position += "self.".len() + length;
        // A call or a further attribute is not a stored field read.
        if matches!(self.peek(), Some(b'(' | b'.' | b'[')) {
            return None;
        }
        (self.field)(name)
    }
}

fn bounded(value: i64) -> Option<Num> {
    (-EXACT_LIMIT..=EXACT_LIMIT)
        .contains(&value)
        .then_some(Num::Int(value))
}

fn finite(value: f64) -> Option<Num> {
    value.is_finite().then_some(Num::Float(value))
}

fn arithmetic(operator: u8, left: Num, right: Num) -> Option<Num> {
    match (operator, left, right) {
        (b'/', _, _) => {
            let divisor = right.as_float();
            (divisor != 0.0)
                .then(|| left.as_float() / divisor)
                .and_then(finite)
        }
        (b'+', Num::Int(a), Num::Int(b)) => bounded(a.checked_add(b)?),
        (b'-', Num::Int(a), Num::Int(b)) => bounded(a.checked_sub(b)?),
        (b'*', Num::Int(a), Num::Int(b)) => bounded(a.checked_mul(b)?),
        (b'+', _, _) => finite(left.as_float() + right.as_float()),
        (b'-', _, _) => finite(left.as_float() - right.as_float()),
        (b'*', _, _) => finite(left.as_float() * right.as_float()),
        _ => None,
    }
}

/// `round(x)` with one argument: an `int`, ties to even.
fn round(value: Num) -> Option<Num> {
    match value {
        Num::Int(_) => Some(value),
        Num::Float(float) => {
            let rounded = float.round_ties_even();
            (rounded.abs() <= EXACT_LIMIT as f64).then_some(Num::Int(rounded as i64))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate(text: &str, amount: i64, rate: f64) -> Option<Num> {
        let lookup = |name: &str| match name {
            "amount" => Some(Num::Int(amount)),
            "rate" => Some(Num::Float(rate)),
            _ => None,
        };
        Evaluator::new(text, &lookup).run()
    }

    #[test]
    fn the_evaluator_follows_python_float_and_round_semantics() {
        assert_eq!(
            evaluate("round(self.amount * (1 + self.rate))", 100, 0.2),
            Some(Num::Int(120))
        );
        assert_eq!(
            evaluate("round(self.amount * (1 + self.rate))", 100, 0.25),
            Some(Num::Int(125))
        );
        // Ties go to the even integer, as in Python.
        assert_eq!(evaluate("round(2.5)", 0, 0.0), Some(Num::Int(2)));
        assert_eq!(evaluate("round(3.5)", 0, 0.0), Some(Num::Int(4)));
        assert_eq!(evaluate("round(-0.5)", 0, 0.0), Some(Num::Int(0)));
        // True division always yields a float.
        assert_eq!(evaluate("self.amount / 4", 10, 0.0), Some(Num::Float(2.5)));
        assert_eq!(evaluate("-self.amount + 1", 3, 0.0), Some(Num::Int(-2)));
    }

    #[test]
    fn the_evaluator_refuses_anything_outside_the_closed_grammar() {
        for text in [
            "self.amount // 2",
            "self.amount ** 2",
            "self.amount % 2",
            "self.amount / 0",
            "self.other",
            "self.amount.real",
            "self.amount()",
            "round(self.amount, 2)",
            "round(self.amount",
            "abs(self.amount)",
            "self.amount if 1 else 2",
            "1e3",
            "1_000",
            "'a'",
            "",
            "self.amount +",
            "(self.amount",
            "self.amount)",
        ] {
            assert_eq!(evaluate(text, 5, 0.5), None, "{text}");
        }
    }

    #[test]
    fn integers_outside_the_exact_range_are_refused_without_panicking() {
        assert_eq!(bounded(i64::MIN), None);
        assert_eq!(bounded(i64::MAX), None);
        assert_eq!(bounded(-EXACT_LIMIT - 1), None);
        assert_eq!(bounded(EXACT_LIMIT + 1), None);
        assert_eq!(bounded(-EXACT_LIMIT), Some(Num::Int(-EXACT_LIMIT)));
        assert_eq!(bounded(EXACT_LIMIT), Some(Num::Int(EXACT_LIMIT)));
        // `2**31 * -(2**32)` is `i64::MIN`; the product is out of range.
        assert_eq!(
            evaluate("-(2147483648 * -4294967296) / 4294967296", 0, 0.0),
            None
        );
        assert_eq!(parse_literal("9223372036854775808"), None);
    }

    #[test]
    fn docstrings_and_comments_are_not_code() {
        let source = "\"\"\"Dataclass, property shapes.\n\nmore property text\n\"\"\"\nimport os  # round\nx = 1\n";
        assert_eq!(
            code_text(source),
            Some(vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "import os  ".to_string(),
                "x = 1".to_string(),
            ])
        );
        // A quote before a comment or a triple quote hides what is code.
        assert_eq!(code_text("x = '#'; round = f\n"), None);
        assert_eq!(code_text("x = '\"\"\"'; round = f\n"), None);
    }

    #[test]
    fn nesting_and_length_are_bounded() {
        let deep = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        assert_eq!(evaluate(&deep, 0, 0.0), None);
        let negations = format!("{}1", "-".repeat(5000));
        assert_eq!(evaluate(&negations, 0, 0.0), None);
        let shallow = format!("{}1{}", "(".repeat(10), ")".repeat(10));
        assert_eq!(evaluate(&shallow, 0, 0.0), Some(Num::Int(1)));
        let long = vec!["1"; 150].join(" + ");
        assert_eq!(evaluate(&long, 0, 0.0), None);
    }

    #[test]
    fn literals_are_plain_decimal_numbers() {
        assert_eq!(parse_literal("120"), Some(Num::Int(120)));
        assert_eq!(parse_literal("-3"), Some(Num::Int(-3)));
        assert_eq!(parse_literal("0.25"), Some(Num::Float(0.25)));
        assert_eq!(parse_literal(".5"), Some(Num::Float(0.5)));
        for text in [
            "007", "1e3", "1_0", "0x10", "1j", "--1", ".", "", "1.2.3", "True",
        ] {
            assert_eq!(parse_literal(text), None, "{text}");
        }
    }
}
