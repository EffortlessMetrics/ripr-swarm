//! Expectations computed from the code under test pin nothing (RIPR-SPEC-0238
//! rule 1, extended).
//!
//! `assertEqual(f(2), f(2))` is already read as a tautology because both
//! operands are the same tokens. The same emptiness hides behind a binding or
//! a rewrite: `self.expected = f(2)` in `setUp`, a class attribute or module
//! constant `EXP = f(2)`, a local `e = f(2)`, `g(2)` where `g` forwards to
//! `f`, `f(2) + 0`, `f(1) * 2`, or `f(2) - f(2) == 0`. The expected side runs
//! the changed code too, so the assertion passes before and after the change.
//!
//! This pass runs once workspace owners and tests are both known. For each
//! exact-value comparison of two operands it expands both sides through the
//! bindings visible to the test (test-local assignments, `setUp`-style hooks
//! and class attributes of the test's class and its same-file bases, module
//! constants, same-file and conftest fixtures), and reads every call of a
//! workspace owner on either side, closed over the owners' own bodies
//! (forwarders). When the owner calls of one side reach the other side's, the
//! comparison relates the code under test to itself and is read as a weak
//! relational check.
//!
//! The scan is over source text, not a node allow-list, so no expression form
//! can hide an owner call from it: an unknown form that mentions an owner
//! demotes, and a form that mentions none (a literal, an independent name, a
//! call of a function unrelated to the other side) keeps its credit. A name
//! with several candidate bindings is expanded through all of them.

use super::approx_band::parse_expression;
use super::source_utils::text_for_range;
use super::{PythonAssertion, PythonOracleShape, PythonOwner, PythonTest};
use crate::domain::{OracleKind, OracleStrength};
use rayon::prelude::*;
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use super::source_facts::parse_module_result;

/// Hooks whose `self.attr = ...` writes a test of the class may read.
const SETUP_HOOKS: [&str; 4] = ["setUp", "setUpClass", "setup_method", "setup_class"];
/// Expansion depth through chained bindings (`a = b`, `b = f(2)`).
const MAX_EXPANSION_DEPTH: usize = 4;
/// Depth of the owner-body forwarder closure.
const MAX_FORWARD_DEPTH: usize = 4;

/// Demote every exact-value comparison whose two sides reach the same
/// workspace owner.
///
/// Each test is decided from its own assertions, its file's bindings and the
/// read-only owner index, so files are bound and tests decided in parallel;
/// the result does not depend on the order.
pub(super) fn demote_derived_expectations<'a>(
    tests: &mut [PythonTest],
    owners: &[PythonOwner],
    read_source: impl Fn(&Path) -> Option<&'a str> + Sync,
) {
    let index = OwnerIndex::new(owners, &read_source);
    if index.is_empty() {
        return;
    }
    // Bindings only widen what an operand reads, and a pair relates only when
    // both operands reach an owner call. An operand that names nothing (a
    // literal) reaches none, so a test whose every pair has one is decided
    // without parsing its file again.
    let wanted: BTreeSet<PathBuf> = tests
        .iter()
        .filter(|test| test.assertions.iter().any(is_exact_comparison) && needs_bindings(test))
        .map(|test| test.file.clone())
        .collect();
    let files: BTreeMap<PathBuf, Option<FileBindings>> = wanted
        .into_par_iter()
        .map(|file| {
            let bindings =
                read_source(&file).and_then(|source| match parse_module_result(&file, source) {
                    Ok(Mod::Module(module)) => Some(FileBindings::read(source, &module.body)),
                    _ => None,
                });
            (file, bindings)
        })
        .collect();
    tests.par_iter_mut().for_each(|test| {
        if !test.assertions.iter().any(is_exact_comparison) {
            return;
        }
        let bindings = files.get(&test.file).and_then(|bindings| bindings.as_ref());
        let scope = TestScope::new(test, bindings);
        let imports: BTreeMap<String, String> = test
            .imports
            .iter()
            .map(|import| (import.alias.clone(), import.imported.clone()))
            .collect();
        let flags: Vec<bool> = test
            .assertions
            .iter()
            .map(|assertion| {
                is_exact_comparison(assertion) && derived(assertion, &scope, &imports, &index)
            })
            .collect();
        for (assertion, flag) in test.assertions.iter_mut().zip(flags) {
            if flag {
                assertion.oracle_kind = OracleKind::RelationalCheck;
                assertion.oracle_strength = OracleStrength::Weak;
                if assertion.oracle_shape == PythonOracleShape::ExactAssertion {
                    assertion.oracle_shape = PythonOracleShape::BoundaryAssertion;
                }
            }
        }
    });
}

/// Whether some exact comparison of the test has a pair whose two operands
/// both read a name, the only pairs bindings can relate.
pub(super) fn needs_bindings(test: &PythonTest) -> bool {
    test.assertions
        .iter()
        .filter(|assertion| is_exact_comparison(assertion))
        .any(|assertion| {
            assertion.observed_operands.chunks(2).any(|pair| {
                matches!(pair, [(left, _), (right, _)] if reads_a_name(left) && reads_a_name(right))
            })
        })
}

/// Whether the operand text holds an identifier outside strings and comments.
pub(super) fn reads_a_name(operand: &str) -> bool {
    tokens(operand)
        .iter()
        .any(|token| matches!(token, Token::Chain(_) | Token::Ident { .. }))
}

fn is_exact_comparison(assertion: &PythonAssertion) -> bool {
    assertion.oracle_kind == OracleKind::ExactValue
        && assertion.oracle_strength == OracleStrength::Strong
        && assertion.observed_operands.len() >= 2
        && assertion.observed_operands.len().is_multiple_of(2)
}

/// Whether every compared pair of the assertion relates the owner to itself.
fn derived(
    assertion: &PythonAssertion,
    scope: &TestScope<'_>,
    imports: &BTreeMap<String, String>,
    index: &OwnerIndex<'_, '_>,
) -> bool {
    assertion.observed_operands.chunks(2).all(|pair| {
        let [(left, _), (right, _)] = pair else {
            return false;
        };
        cancels_itself(left) || cancels_itself(right) || scope.relates(left, right, imports, index)
    })
}

/// `f(2) - f(2)`, `f(2) / f(2)`, `f(2) // f(2)`, `f(2) % f(2)`, `f(2) ^ f(2)`:
/// a call minus (or over, or xor) itself is a constant whatever `f` returns.
fn cancels_itself(operand: &str) -> bool {
    // Cheap precheck before parsing: the shape needs a call on a side of one
    // of the cancelling operators (`//` is covered by `/`). Parsing every
    // operand of every exact comparison was a measurable share of a warm check.
    if !operand.contains('(') || !operand.contains(['-', '/', '%', '^']) {
        return false;
    }
    let Some(Expr::BinOp(binop)) = parse_expression(operand) else {
        return false;
    };
    if !matches!(
        binop.op,
        ast::Operator::Sub
            | ast::Operator::Div
            | ast::Operator::FloorDiv
            | ast::Operator::Mod
            | ast::Operator::BitXor
    ) {
        return false;
    }
    let source = wrapped(operand);
    let left = compact(&text_for_range(&source, binop.left.range()));
    let right = compact(&text_for_range(&source, binop.right.range()));
    left.contains('(') && left == right
}

/// `parse_expression` parses `_ = (\n{text}\n)\n`; its ranges index that source.
fn wrapped(text: &str) -> String {
    format!("_ = (\n{text}\n)\n")
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// The workspace owners by name, with the owner names each one's body calls
/// read on first use. Lexing every owner body of the workspace up front cost
/// more than the rest of a warm Python check on a large repository; a
/// comparison only ever reaches the owners its operands name.
struct OwnerIndex<'o, 'a> {
    /// Owner name to where its definitions live: file, first line, last line.
    sites: BTreeMap<&'o str, Vec<(&'o Path, usize, usize)>>,
    read_source: &'o (dyn Fn(&Path) -> Option<&'a str> + Sync),
    lines_by_file: Mutex<BTreeMap<&'o Path, Arc<Vec<&'a str>>>>,
    /// Owner name to the owner names its bodies call (memoized).
    callees: Mutex<BTreeMap<&'o str, Arc<BTreeSet<String>>>>,
}

impl<'o, 'a> OwnerIndex<'o, 'a> {
    fn new(
        owners: &'o [PythonOwner],
        read_source: &'o (dyn Fn(&Path) -> Option<&'a str> + Sync),
    ) -> Self {
        let mut sites: BTreeMap<&str, Vec<(&Path, usize, usize)>> = BTreeMap::new();
        for owner in owners.iter().filter(|owner| owner.owner_kind.is_some()) {
            sites.entry(owner.name.as_str()).or_default().push((
                owner.file.as_path(),
                owner.start_line,
                owner.end_line,
            ));
        }
        Self {
            sites,
            read_source,
            lines_by_file: Mutex::new(BTreeMap::new()),
            callees: Mutex::new(BTreeMap::new()),
        }
    }

    fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Whether `name` is a workspace owner.
    fn has_owner(&self, name: &str) -> bool {
        self.sites.contains_key(name)
    }

    /// The owner names the bodies of the owners called `name` call.
    fn callees_of(&self, name: &str) -> Arc<BTreeSet<String>> {
        let Some((key, sites)) = self.sites.get_key_value(name) else {
            return Arc::default();
        };
        if let Some(found) = self
            .callees
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
        {
            return Arc::clone(found);
        }
        let mut callees = BTreeSet::new();
        for &(file, start_line, end_line) in sites {
            let Some(lines) = self.lines_of(file) else {
                continue;
            };
            let first = start_line.saturating_sub(1).min(lines.len());
            let last = (first + end_line.saturating_sub(start_line) + 1).min(lines.len());
            let body = lines[first..last].join("\n");
            callees.extend(
                called_names(&body)
                    .into_iter()
                    .filter(|called| self.has_owner(called)),
            );
        }
        let callees = Arc::new(callees);
        self.callees
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, Arc::clone(&callees));
        callees
    }

    /// The lines of `file`, split once.
    fn lines_of(&self, file: &'o Path) -> Option<Arc<Vec<&'a str>>> {
        if let Some(found) = self
            .lines_by_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(file)
        {
            return Some(Arc::clone(found));
        }
        let source = (self.read_source)(file)?;
        let lines = Arc::new(source.lines().collect::<Vec<_>>());
        self.lines_by_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(file, Arc::clone(&lines));
        Some(lines)
    }

    /// `names` and every owner they call, transitively.
    fn closure(&self, names: &BTreeSet<String>) -> BTreeSet<String> {
        let mut reached = names.clone();
        let mut frontier = names.clone();
        for _ in 0..MAX_FORWARD_DEPTH {
            let mut next = BTreeSet::new();
            for name in &frontier {
                for callee in self.callees_of(name).iter() {
                    if reached.insert(callee.clone()) {
                        next.insert(callee.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        reached
    }
}

/// The names `text` calls: every identifier directly followed by `(`, outside
/// string literals and comments.
fn called_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for token in tokens(text) {
        if let Token::Ident { name, called, .. } = token
            && called
        {
            names.insert(name);
        }
    }
    names
}

enum Token {
    /// A dotted chain's final segment is the `name`; `chain` is the whole
    /// dotted path.
    Ident {
        name: String,
        called: bool,
        /// The whole dotted path the name ends.
        chain: String,
        /// The chain follows a `.` on something that is not a name (a string,
        /// call or subscript result), so its receiver is unknown.
        attr: bool,
    },
    Chain(String),
    /// A bracket opens; `call` is the chain (and its `attr` flag) it directly
    /// calls.
    Open {
        call: Option<(String, bool)>,
    },
    Close,
}

/// Identifier tokens of `text` (and dotted chains), skipping strings and
/// comments.
fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;
    let mut pending_call: Option<(String, bool)> = None;
    let is_start = |ch: char| ch.is_alphabetic() || ch == '_';
    let is_part = |ch: char| ch.is_alphanumeric() || ch == '_';
    while index < chars.len() {
        let ch = chars[index];
        if ch == '#' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if (ch == '"' || ch == '\'')
            && index > 0
            && chars[index - 1].is_alphabetic()
            && chars[index.saturating_sub(2)..index]
                .iter()
                .any(|prefix| matches!(prefix, 'f' | 'F'))
        {
            // An f-string's replacement fields are code: read its text as code
            // (fail-closed: prose inside it may name a call too).
            index += 1;
        } else if ch == '"' || ch == '\'' {
            // Skip a (possibly triple-quoted, possibly prefixed) string.
            let triple = chars.get(index + 1) == Some(&ch) && chars.get(index + 2) == Some(&ch);
            index += if triple { 3 } else { 1 };
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                } else if chars[index] == ch
                    && (!triple
                        || (chars.get(index + 1) == Some(&ch) && chars.get(index + 2) == Some(&ch)))
                {
                    index += if triple { 3 } else { 1 };
                    break;
                } else {
                    index += 1;
                }
            }
        } else if is_start(ch) && (index == 0 || !is_part(chars[index - 1])) {
            let attr = chars[..index]
                .iter()
                .rev()
                .find(|prev| !prev.is_whitespace())
                == Some(&'.');
            let mut chain = String::new();
            let last = loop {
                let start = index;
                while index < chars.len() && is_part(chars[index]) {
                    index += 1;
                }
                let segment: String = chars[start..index].iter().collect();
                chain.push_str(&segment);
                let mut look = index;
                while look < chars.len() && chars[look].is_whitespace() {
                    look += 1;
                }
                if chars.get(look) == Some(&'.')
                    && chars.get(look + 1).is_some_and(|next| is_start(*next))
                {
                    chain.push('.');
                    index = look + 1;
                    continue;
                }
                break segment;
            };
            let mut look = index;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            let called = chars.get(look) == Some(&'(');
            pending_call = called.then(|| (chain.clone(), attr));
            out.push(Token::Chain(chain.clone()));
            out.push(Token::Ident {
                name: last,
                called,
                chain,
                attr,
            });
        } else {
            if matches!(ch, '(' | '[' | '{') {
                out.push(Token::Open {
                    call: if ch == '(' { pending_call.take() } else { None },
                });
            } else if matches!(ch, ')' | ']' | '}') {
                out.push(Token::Close);
            }
            if !ch.is_whitespace() {
                pending_call = None;
            }
            index += 1;
        }
    }
    out
}

/// Candidate binding texts, by the name a test reads them under.
#[derive(Default)]
struct Bindings {
    by_name: BTreeMap<String, Vec<String>>,
}

impl Bindings {
    fn add(&mut self, name: String, text: String) {
        self.by_name.entry(name).or_default().push(text);
    }
}

/// What one file binds outside any test: module constants, and per class its
/// attributes and hook-assigned attributes.
struct FileBindings {
    module: Bindings,
    /// Class name to its attribute bindings (keys are `self.x`, `cls.x`,
    /// `Class.x`) and its base class names.
    classes: BTreeMap<String, (Bindings, Vec<String>)>,
    /// Every function in the file: name to the text of its body, so a
    /// same-file fixture or helper expands.
    functions: BTreeMap<String, Vec<String>>,
    /// Test function body assignments, by qualified test name.
    locals: BTreeMap<String, Bindings>,
}

impl FileBindings {
    fn read(source: &str, body: &[Stmt]) -> Self {
        let mut this = Self {
            module: Bindings::default(),
            classes: BTreeMap::new(),
            functions: BTreeMap::new(),
            locals: BTreeMap::new(),
        };
        collect_assigns(source, body, &mut this.module);
        this.walk(source, body, &mut Vec::new());
        this
    }

    fn walk(&mut self, source: &str, body: &[Stmt], path: &mut Vec<String>) {
        for stmt in body {
            match stmt {
                Stmt::ClassDef(class) => {
                    let name = class.name.to_string();
                    let mut attrs = Bindings::default();
                    let mut own = Bindings::default();
                    collect_assigns(source, &class.body, &mut own);
                    for (attr, texts) in own.by_name {
                        for text in texts {
                            for receiver in ["self", "cls", name.as_str()] {
                                attrs.add(format!("{receiver}.{attr}"), text.clone());
                            }
                            attrs.add(attr.clone(), text);
                        }
                    }
                    for member in &class.body {
                        let Stmt::FunctionDef(function) = member else {
                            continue;
                        };
                        if SETUP_HOOKS.contains(&function.name.as_str()) {
                            let mut hooked = Bindings::default();
                            collect_assigns(source, &function.body, &mut hooked);
                            // `cls.x` in a class hook is read as `self.x` by a test.
                            for (key, texts) in hooked.by_name {
                                let attr = key
                                    .strip_prefix("self.")
                                    .or_else(|| key.strip_prefix("cls."))
                                    .map(str::to_string);
                                for text in texts {
                                    match &attr {
                                        Some(attr) => {
                                            for receiver in ["self", "cls", name.as_str()] {
                                                attrs.add(
                                                    format!("{receiver}.{attr}"),
                                                    text.clone(),
                                                );
                                            }
                                        }
                                        None => attrs.add(key.clone(), text),
                                    }
                                }
                            }
                        }
                    }
                    let bases: Vec<String> = class
                        .bases
                        .iter()
                        .filter_map(|base| match base {
                            Expr::Name(name) => Some(name.id.to_string()),
                            _ => None,
                        })
                        .collect();
                    let entry = self
                        .classes
                        .entry(name.clone())
                        .or_insert_with(|| (Bindings::default(), Vec::new()));
                    for (key, texts) in attrs.by_name {
                        for text in texts {
                            entry.0.add(key.clone(), text);
                        }
                    }
                    entry.1.extend(bases);
                    path.push(name);
                    self.walk(source, &class.body, path);
                    path.pop();
                }
                Stmt::FunctionDef(function) => {
                    let name = function.name.to_string();
                    self.record_function(source, &name, &function.body, path);
                }
                Stmt::AsyncFunctionDef(function) => {
                    let name = function.name.to_string();
                    self.record_function(source, &name, &function.body, path);
                }
                _ => {}
            }
        }
    }

    fn record_function(&mut self, source: &str, name: &str, body: &[Stmt], path: &[String]) {
        let mut locals = Bindings::default();
        collect_assigns(source, body, &mut locals);
        let qualified = path
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(name))
            .collect::<Vec<_>>()
            .join(".");
        let slot = self.locals.entry(qualified).or_default();
        for (key, texts) in locals.by_name {
            for text in texts {
                slot.add(key.clone(), text);
            }
        }
        let text = body
            .iter()
            .map(|stmt| text_for_range(source, stmt.range()))
            .collect::<Vec<_>>()
            .join("\n");
        self.functions
            .entry(name.to_string())
            .or_default()
            .push(text);
    }
}

/// Every assignment in `stmts` (recursing through compound statements, not
/// into nested definitions) as target-path to right-hand-side text.
fn collect_assigns(source: &str, stmts: &[Stmt], out: &mut Bindings) {
    let target_names = |target: &Expr, text: &str, out: &mut Bindings| {
        fn names(target: &Expr, found: &mut Vec<String>) {
            match target {
                Expr::Name(name) => found.push(name.id.to_string()),
                Expr::Attribute(_) => {
                    if let Some(full) = super::expr_full_name(target) {
                        found.push(full);
                    }
                }
                Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| names(elt, found)),
                Expr::List(list) => list.elts.iter().for_each(|elt| names(elt, found)),
                Expr::Starred(starred) => names(&starred.value, found),
                _ => {}
            }
        }
        let mut found = Vec::new();
        names(target, &mut found);
        for name in found {
            out.add(name, text.to_string());
        }
    };
    for stmt in stmts {
        match stmt {
            Stmt::Assign(assign) => {
                let text = text_for_range(source, assign.value.range());
                for target in &assign.targets {
                    target_names(target, &text, out);
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    target_names(&assign.target, &text_for_range(source, value.range()), out);
                }
            }
            Stmt::AugAssign(assign) => target_names(
                &assign.target,
                &text_for_range(source, assign.value.range()),
                out,
            ),
            Stmt::For(stmt) => {
                target_names(
                    &stmt.target,
                    &text_for_range(source, stmt.iter.range()),
                    out,
                );
                collect_assigns(source, &stmt.body, out);
                collect_assigns(source, &stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                collect_assigns(source, &stmt.body, out);
                collect_assigns(source, &stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                collect_assigns(source, &stmt.body, out);
                collect_assigns(source, &stmt.orelse, out);
            }
            Stmt::With(stmt) => {
                for item in &stmt.items {
                    if let Some(vars) = &item.optional_vars {
                        target_names(
                            vars,
                            &text_for_range(source, item.context_expr.range()),
                            out,
                        );
                    }
                }
                collect_assigns(source, &stmt.body, out);
            }
            Stmt::Try(stmt) => {
                collect_assigns(source, &stmt.body, out);
                collect_assigns(source, &stmt.orelse, out);
                collect_assigns(source, &stmt.finalbody, out);
                for handler in &stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_assigns(source, &handler.body, out);
                }
            }
            _ => {}
        }
    }
}

/// The bindings visible to one test.
struct TestScope<'a> {
    file: Option<&'a FileBindings>,
    test: &'a PythonTest,
    /// The test function's own assignments.
    locals: Option<&'a Bindings>,
    /// Attribute bindings of the test's class and its same-file bases, found
    /// once per test rather than on every name lookup.
    classes: Vec<&'a Bindings>,
}

impl<'a> TestScope<'a> {
    fn new(test: &'a PythonTest, file: Option<&'a FileBindings>) -> Self {
        let mut classes = Vec::new();
        if let Some(file) = file {
            let mut pending: Vec<String> = test
                .qualified_name
                .rsplit_once('.')
                .map(|(class, _)| class.rsplit('.').next().unwrap_or(class).to_string())
                .into_iter()
                .collect();
            let mut visited: BTreeSet<String> = BTreeSet::new();
            while let Some(class) = pending.pop() {
                if !visited.insert(class.clone()) {
                    continue;
                }
                if let Some((attrs, bases)) = file.classes.get(&class) {
                    classes.push(attrs);
                    pending.extend(bases.iter().cloned());
                }
            }
        }
        let locals = file.and_then(|file| file.locals.get(&test.qualified_name));
        Self {
            file,
            test,
            locals,
            classes,
        }
    }

    /// Whether the owner calls of one operand reach the other's.
    fn relates(
        &self,
        left: &str,
        right: &str,
        imports: &BTreeMap<String, String>,
        index: &OwnerIndex<'_, '_>,
    ) -> bool {
        let left_calls = self.owner_calls_of(left, imports, index);
        if left_calls.is_empty() {
            return false;
        }
        let right_calls = self.owner_calls_of(right, imports, index);
        if right_calls.is_empty() {
            return false;
        }
        let left_reach = index.closure(&left_calls);
        let right_reach = index.closure(&right_calls);
        !left_calls.is_disjoint(&right_reach) || !right_calls.is_disjoint(&left_reach)
    }

    /// The owner calls that are the root of the operand's value, directly or
    /// through any binding the value reads. A call inside the arguments of an
    /// owner call is an input of that call, not part of the value: it is
    /// neither a root nor expanded through bindings, so an input both sides
    /// read (`n = make_n(); assert f(n) == n * 2`) does not relate them.
    fn owner_calls_of(
        &self,
        operand: &str,
        imports: &BTreeMap<String, String>,
        index: &OwnerIndex<'_, '_>,
    ) -> BTreeSet<String> {
        let mut roots = BTreeSet::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut frontier = vec![operand.to_string()];
        for _ in 0..=MAX_EXPANSION_DEPTH {
            let mut next = Vec::new();
            for text in &frontier {
                let chains = self.scan(text, imports, index, &mut roots);
                for chain in chains {
                    next.extend(self.expansions(&chain, &mut seen));
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        roots
    }

    /// Add the owner calls at the root of `text` to `roots`; return the
    /// dotted chains read outside every owner call's arguments.
    fn scan(
        &self,
        text: &str,
        imports: &BTreeMap<String, String>,
        index: &OwnerIndex<'_, '_>,
        roots: &mut BTreeSet<String>,
    ) -> Vec<String> {
        let mut chains = Vec::new();
        // One entry per open bracket: whether it opened an owner call's args.
        let mut stack: Vec<bool> = Vec::new();
        let mut hidden = 0usize;
        for token in tokens(text) {
            match token {
                Token::Chain(chain) if hidden == 0 => chains.push(chain),
                Token::Ident {
                    name,
                    called: true,
                    chain,
                    attr,
                } if hidden == 0 => {
                    if let Some(owner) = self.owner_of(&name, &chain, attr, imports, index) {
                        roots.insert(owner);
                    }
                }
                Token::Open { call } => {
                    let owner_args = hidden == 0
                        && call.is_some_and(|(chain, attr)| {
                            let name = chain.rsplit('.').next().unwrap_or(&chain).to_string();
                            self.owner_of(&name, &chain, attr, imports, index).is_some()
                        });
                    hidden += usize::from(owner_args);
                    stack.push(owner_args);
                }
                Token::Close if stack.pop() == Some(true) => {
                    hidden -= 1;
                }
                Token::Close => {}
                _ => {}
            }
        }
        chains
    }

    /// The workspace owner a call names, receiver-aware: a bare name or an
    /// imported/`self`/`cls` receiver resolves; a receiver of unknown type
    /// (`d.get(..)`, `''.join(..)`, `f(x).get(..)`) does not, unless the
    /// receiver was itself built by an owner call.
    fn owner_of(
        &self,
        name: &str,
        chain: &str,
        attr: bool,
        imports: &BTreeMap<String, String>,
        index: &OwnerIndex<'_, '_>,
    ) -> Option<String> {
        if attr {
            return None;
        }
        let first = chain.split('.').next().unwrap_or(chain);
        if chain.contains('.')
            && !imports.contains_key(first)
            && !matches!(first, "self" | "cls")
            && !self.built_by_owner(first, index)
        {
            return None;
        }
        let resolved = imports
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string());
        index.has_owner(&resolved).then_some(resolved)
    }

    /// Whether a name the test binds is assigned from a call of an owner.
    fn built_by_owner(&self, name: &str, index: &OwnerIndex<'_, '_>) -> bool {
        self.bound(name).iter().any(|text| {
            called_names(text)
                .iter()
                .any(|called| index.has_owner(called))
        })
    }

    /// The binding texts a dotted chain reads, longest bound prefix first.
    fn expansions(&self, chain: &str, seen: &mut BTreeSet<String>) -> Vec<String> {
        let mut found = Vec::new();
        let segments: Vec<&str> = chain.split('.').collect();
        for length in (1..=segments.len()).rev() {
            let key = segments[..length].join(".");
            if !seen.insert(key.clone()) {
                continue;
            }
            found.extend(self.bound(&key));
        }
        found
    }

    fn bound(&self, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(file) = self.file {
            if let Some(locals) = self.locals {
                out.extend(locals.by_name.get(key).cloned().unwrap_or_default());
            }
            out.extend(file.module.by_name.get(key).cloned().unwrap_or_default());
            // The test's class and its same-file bases.
            for attrs in &self.classes {
                out.extend(attrs.by_name.get(key).cloned().unwrap_or_default());
            }
            // A same-file fixture or helper read by name.
            if !key.contains('.') {
                out.extend(file.functions.get(key).cloned().unwrap_or_default());
            }
        }
        if !key.contains('.') {
            out.extend(
                self.test
                    .fixture_defs
                    .iter()
                    .filter(|fixture| fixture.name == key)
                    .map(|fixture| fixture.body_text.clone()),
            );
        }
        out
    }
}
