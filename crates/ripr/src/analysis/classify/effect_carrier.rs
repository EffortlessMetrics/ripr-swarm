//! Effect-state carriers (RIPR-SPEC-0094, Part D): whether a whole-object
//! equality can hold the state a deleted `self.callee(..)` call writes.
//!
//! RIPR-SPEC-0094 Part C lets a whole-object equality confirm observation of
//! an effect-family probe without sharing a token with it, because comparing
//! the resulting state is the canonical way to observe a side effect. That is
//! only sound when the compared object can carry the effect. An
//! `assert_eq!(inv.history(), &[..])` cannot notice a deleted
//! `self.refresh_low_stock(sku)` when `refresh_low_stock` writes only
//! `self.low_stock` and `history` never reads it.
//!
//! The carrier is established once per probe and only for the narrow shape
//! where the written state is statically bounded:
//!
//! 1. The changed expression is exactly `self.callee(args)` with no `?`, no
//!    `&mut` argument, inside a method of an inherent or trait impl in a
//!    parser-backed file.
//! 2. `callee` resolves to exactly one `&mut self` method of the same self
//!    type with no return type and no `&mut` parameter, and no trait,
//!    unparsed or other-type definition shares its name.
//! 3. The callee, and every `self.method(..)` it calls transitively, touches
//!    state only through `self.<field>` accesses: no bare `self`, no free
//!    function call, no macro outside a pure list, no interior mutability
//!    (`borrow_mut`, `lock`, atomics, `Cell`), no `unsafe`, and every
//!    transitive `self.method(..)` resolves the same way.
//!
//! When any gate fails the probe keeps the Part C reading (any whole-object
//! equality confirms). That is the conservative direction here: refusing the
//! confirmation turns an `exposed` reading into an actionable gap, and a
//! wrong actionable signal is worse than a missed advisory.
//!
//! An established carrier refuses a whole-object equality only when every
//! identifier in it is provably unable to hold a written field: it names no
//! written field, no method of the self type that reads one (transitively),
//! no self-type path, and every binding it compares is a `let` whose
//! initializer is itself provably non-carrying. Unknown bindings, unresolved
//! methods on a binding, and method names defined on other types all count
//! as carriers. Mock expectations and snapshots are never refused.

use super::super::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionContainer;
use crate::domain::{OracleKind, Probe, ProbeFamily};
use std::collections::{BTreeMap, BTreeSet};

/// Bound on transitive `self.method(..)` resolution and on `let` chains.
const MAX_DEPTH: usize = 6;
const MAX_BINDING_DEPTH: usize = 3;

/// Methods on a field that only read it.
const READ_ONLY_METHODS: &[&str] = &[
    "as_ref",
    "as_slice",
    "as_str",
    "binary_search",
    "clone",
    "cloned",
    "cmp",
    "contains",
    "contains_key",
    "copied",
    "ends_with",
    "eq",
    "first",
    "get",
    "get_key_value",
    "is_empty",
    "is_none",
    "is_some",
    "iter",
    "keys",
    "last",
    "len",
    "partial_cmp",
    "range",
    "starts_with",
    "to_owned",
    "to_string",
    "to_vec",
    "values",
];

/// Identifiers that mark state a `self.<field>` scan cannot see.
const OPAQUE_STATE_MARKERS: &[&str] = &[
    "AtomicBool",
    "AtomicI32",
    "AtomicI64",
    "AtomicU32",
    "AtomicU64",
    "AtomicUsize",
    "Cell",
    "Mutex",
    "RefCell",
    "RwLock",
    "borrow_mut",
    "compare_exchange",
    "fetch_add",
    "fetch_and",
    "fetch_or",
    "fetch_sub",
    "lock",
    "store",
    "swap",
    "try_borrow_mut",
    "try_lock",
    "unsafe",
];

/// Macros that neither write state nor perform I/O.
const PURE_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "format",
    "matches",
    "panic",
    "unreachable",
    "vec",
];

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "move", "mut", "ref", "return", "static",
    "struct", "super", "true", "type", "use", "where", "while",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Punct(char),
}

fn tokenize(text: &str) -> Vec<Tok> {
    let masked = mask_comments_and_strings(text);
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in masked.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
            continue;
        }
        if !current.is_empty() {
            tokens.push(Tok::Ident(std::mem::take(&mut current)));
        }
        if !ch.is_whitespace() {
            tokens.push(Tok::Punct(ch));
        }
    }
    if !current.is_empty() {
        tokens.push(Tok::Ident(current));
    }
    tokens
}

fn ident(tokens: &[Tok], index: usize) -> Option<&str> {
    match tokens.get(index) {
        Some(Tok::Ident(name)) => Some(name.as_str()),
        _ => None,
    }
}

fn punct(tokens: &[Tok], index: usize, ch: char) -> bool {
    tokens.get(index) == Some(&Tok::Punct(ch))
}

/// Index of the token closing the bracket opened at `open`.
fn matching_close(tokens: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token {
            Tok::Punct('(' | '[' | '{') => depth += 1,
            Tok::Punct(')' | ']' | '}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn base_type_name(self_ty: &str) -> Option<String> {
    let without_generics = self_ty.trim().split('<').next()?.trim();
    let base = without_generics.rsplit("::").next()?.trim();
    let plain = base.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    plain.then(|| base.to_string())
}

fn container_self_type(container: &FunctionContainer) -> Option<String> {
    match container {
        FunctionContainer::Inherent { self_ty } | FunctionContainer::TraitImpl { self_ty, .. } => {
            base_type_name(self_ty)
        }
        _ => None,
    }
}

/// Split a function's text into signature tokens and body tokens (after the
/// first `{`).
fn split_signature(function: &FunctionSummary) -> Option<(Vec<Tok>, Vec<Tok>)> {
    let tokens = tokenize(&function.body);
    let open = tokens.iter().position(|token| *token == Tok::Punct('{'))?;
    let body = tokens[open..].to_vec();
    let mut signature = tokens;
    signature.truncate(open);
    Some((signature, body))
}

#[derive(Default)]
struct FieldUse {
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    self_calls: BTreeSet<String>,
    /// A bare `self` (passed, returned, dereferenced or cloned whole).
    whole_self: bool,
    /// A free function call, a non-pure macro, or opaque state.
    opaque: bool,
}

fn statement_start(tokens: &[Tok], index: usize) -> bool {
    index == 0 || matches!(tokens.get(index - 1), Some(Tok::Punct(';' | '{' | '}')))
}

fn assignment_at(tokens: &[Tok], index: usize) -> bool {
    match tokens.get(index) {
        Some(Tok::Punct('=')) => !punct(tokens, index + 1, '=') && !punct(tokens, index + 1, '>'),
        Some(Tok::Punct('+' | '-' | '*' | '/' | '%' | '|' | '&' | '^')) => {
            punct(tokens, index + 1, '=') && !punct(tokens, index + 2, '=')
        }
        Some(Tok::Punct('<')) => punct(tokens, index + 1, '<') && punct(tokens, index + 2, '='),
        Some(Tok::Punct('>')) => punct(tokens, index + 1, '>') && punct(tokens, index + 2, '='),
        _ => false,
    }
}

fn scan_field_use(body: &[Tok]) -> FieldUse {
    let mut uses = FieldUse::default();
    for (index, token) in body.iter().enumerate() {
        let Tok::Ident(name) = token else { continue };
        if OPAQUE_STATE_MARKERS.contains(&name.as_str()) {
            uses.opaque = true;
        }
        let previous_is_path =
            index > 0 && (punct(body, index - 1, '.') || punct(body, index - 1, ':'));
        if name == "self" {
            scan_self_access(body, index, &mut uses);
            continue;
        }
        if previous_is_path || KEYWORDS.contains(&name.as_str()) {
            continue;
        }
        if punct(body, index + 1, '!') {
            if !PURE_MACROS.contains(&name.as_str()) {
                uses.opaque = true;
            }
            continue;
        }
        let lowercase = name.starts_with(|c: char| c.is_lowercase() || c == '_');
        if lowercase && punct(body, index + 1, '(') {
            // A free function call can reach global state.
            uses.opaque = true;
        }
    }
    uses
}

fn scan_self_access(body: &[Tok], index: usize, uses: &mut FieldUse) {
    if !punct(body, index + 1, '.') {
        uses.whole_self = true;
        return;
    }
    let Some(name) = ident(body, index + 2) else {
        uses.whole_self = true;
        return;
    };
    if punct(body, index + 3, '(') {
        uses.self_calls.insert(name.to_string());
        return;
    }
    let field = name.to_string();
    if index >= 2 && ident(body, index - 1) == Some("mut") && punct(body, index - 2, '&') {
        uses.reads.insert(field.clone());
        uses.writes.insert(field);
        return;
    }
    // Walk nested field accesses (`self.a.b`) to the first method, index or
    // operator.
    let mut cursor = index + 3;
    while punct(body, cursor, '.')
        && ident(body, cursor + 1).is_some()
        && !punct(body, cursor + 2, '(')
    {
        cursor += 2;
    }
    if punct(body, cursor, '[') {
        uses.reads.insert(field.clone());
        uses.writes.insert(field);
        return;
    }
    if assignment_at(body, cursor) {
        uses.writes.insert(field);
        return;
    }
    if punct(body, cursor, '.')
        && let Some(method) = ident(body, cursor + 1)
        && punct(body, cursor + 2, '(')
    {
        let read_only = READ_ONLY_METHODS.contains(&method);
        let statement = statement_start(body, index)
            && matching_close(body, cursor + 2).is_some_and(|close| punct(body, close + 1, ';'));
        if read_only || !statement {
            uses.reads.insert(field.clone());
        }
        if !read_only {
            uses.writes.insert(field);
        }
        return;
    }
    uses.reads.insert(field);
}

/// Methods of the owner's self type, by name.
struct SelfTypeMethods<'a> {
    by_name: BTreeMap<&'a str, Vec<&'a FunctionSummary>>,
    /// Names also defined as a method on another type, in a trait, or in an
    /// unparsed file: a call by that name may not reach this type.
    contested: BTreeSet<&'a str>,
}

impl<'a> SelfTypeMethods<'a> {
    fn collect(self_ty: &str, index: &'a RustIndex) -> Self {
        let mut by_name: BTreeMap<&str, Vec<&FunctionSummary>> = BTreeMap::new();
        let mut contested = BTreeSet::new();
        for function in index.functions().iter() {
            let own = container_self_type(&function.item.container).as_deref() == Some(self_ty);
            if own && function.item.has_self_param {
                by_name
                    .entry(function.name.as_str())
                    .or_default()
                    .push(function);
            } else if function.item.has_self_param
                || matches!(
                    function.item.container,
                    FunctionContainer::Unknown | FunctionContainer::Trait { .. }
                )
            {
                contested.insert(function.name.as_str());
            }
        }
        Self { by_name, contested }
    }

    /// The one definition a `self.name(..)` call reaches, when established.
    fn resolve(&self, name: &str) -> Option<&'a FunctionSummary> {
        if self.contested.contains(name) {
            return None;
        }
        match self.by_name.get(name).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            _ => None,
        }
    }
}

/// The owner-side half: the fields the deleted call may write and the
/// self-type methods that read one of them.
pub(in crate::analysis) struct EffectStateCarrier {
    self_ty: String,
    written_fields: BTreeSet<String>,
    reader_methods: BTreeSet<String>,
    /// Self-type methods, uncontested by name, that read no written field.
    non_reader_methods: BTreeSet<String>,
    /// Every name defined as a method anywhere in the workspace.
    any_method: BTreeSet<String>,
}

impl EffectStateCarrier {
    /// `None` unless every gate in the module docs holds.
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
    ) -> Option<Self> {
        if !matches!(
            probe.family,
            ProbeFamily::CallDeletion | ProbeFamily::SideEffect
        ) || !owner.item.has_self_param
        {
            return None;
        }
        let parser_backed = index
            .files()
            .get(&owner.file)
            .is_some_and(|facts| !facts.used_lexical_fallback);
        if !parser_backed {
            return None;
        }
        let self_ty = container_self_type(&owner.item.container)?;
        let callee_name = self_call_name(&probe.expression)?;
        let methods = SelfTypeMethods::collect(&self_ty, index);
        let callee = methods.resolve(&callee_name)?;
        let (signature, _) = split_signature(callee)?;
        if !mut_self_without_return(&signature) {
            return None;
        }
        let written_fields = transitive_writes(callee, &methods)?;
        if written_fields.is_empty() {
            return None;
        }
        let reader_methods = reader_methods(&methods, &written_fields);
        let non_reader_methods = methods
            .by_name
            .keys()
            .filter(|name| !methods.contested.contains(*name) && !reader_methods.contains(**name))
            .map(|name| (*name).to_string())
            .collect();
        let any_method = index
            .functions()
            .iter()
            .filter(|function| {
                function.item.has_self_param
                    || matches!(
                        function.item.container,
                        FunctionContainer::Unknown | FunctionContainer::Trait { .. }
                    )
            })
            .map(|function| function.name.clone())
            .collect();
        Some(Self {
            self_ty,
            written_fields,
            reader_methods,
            non_reader_methods,
            any_method,
        })
    }

    /// Whether `assertion` may observe the written state. Only a whole-object
    /// equality is ever refused.
    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        if !matches!(assertion.kind, OracleKind::WholeObjectEquality) {
            return true;
        }
        let body = tokenize(&test.body);
        self.may_carry(&tokenize(&assertion.text), &body, 0)
    }

    fn may_carry(&self, tokens: &[Tok], test_body: &[Tok], depth: usize) -> bool {
        for (index, token) in tokens.iter().enumerate() {
            let Tok::Ident(name) = token else { continue };
            let name = name.as_str();
            if index > 0 && punct(tokens, index - 1, '.') {
                if punct(tokens, index + 1, '(') {
                    // A method: readers carry; a name defined on another
                    // type or in a trait may read; std/derive methods are
                    // decided at their receiver.
                    if self.reader_methods.contains(name)
                        || (self.any_method.contains(name)
                            && !self.non_reader_methods.contains(name))
                    {
                        return true;
                    }
                } else if self.written_fields.contains(name) {
                    return true;
                }
                continue;
            }
            if name == self.self_ty || name == "Self" || name == "self" {
                return true;
            }
            if index > 0 && punct(tokens, index - 1, ':') {
                continue;
            }
            if punct(tokens, index + 1, '!') || punct(tokens, index + 1, ':') {
                // A macro name, a path root, or a struct field label.
                continue;
            }
            if KEYWORDS.contains(&name)
                || name.starts_with(|c: char| c.is_uppercase() || c.is_ascii_digit())
                || punct(tokens, index + 1, '(')
            {
                continue;
            }
            // A binding root. A resolved non-reading method call on it yields
            // a value that cannot hold the written fields.
            if punct(tokens, index + 1, '.')
                && let Some(method) = ident(tokens, index + 2)
                && punct(tokens, index + 3, '(')
                && self.non_reader_methods.contains(method)
            {
                continue;
            }
            if punct(tokens, index + 1, '.')
                && ident(tokens, index + 2).is_some()
                && !punct(tokens, index + 3, '(')
            {
                // A field read on the binding: decided at the field token.
                continue;
            }
            if depth >= MAX_BINDING_DEPTH {
                return true;
            }
            let Some(initializer) = single_let_initializer(test_body, name) else {
                return true;
            };
            if self.may_carry(&initializer, test_body, depth + 1) {
                return true;
            }
        }
        false
    }
}

/// `callee` for an expression that is exactly `self.callee(args)`.
fn self_call_name(expression: &str) -> Option<String> {
    let trimmed = expression.trim().trim_end_matches(';').trim_end();
    let tokens = tokenize(trimmed);
    if ident(&tokens, 0) != Some("self") || !punct(&tokens, 1, '.') || !punct(&tokens, 3, '(') {
        return None;
    }
    let name = ident(&tokens, 2)?;
    if matching_close(&tokens, 3)? + 1 != tokens.len() {
        return None;
    }
    let args = &tokens[4..tokens.len() - 1];
    let mutable_borrow = args
        .windows(2)
        .any(|pair| pair[0] == Tok::Punct('&') && pair[1] == Tok::Ident("mut".to_string()));
    (!mutable_borrow).then(|| name.to_string())
}

/// `&mut self` (or `mut self`) receiver, no `&mut` parameter, no `->`.
fn mut_self_without_return(signature: &[Tok]) -> bool {
    let mut mut_self = false;
    for (index, token) in signature.iter().enumerate() {
        if *token == Tok::Ident("mut".to_string()) {
            if ident(signature, index + 1) == Some("self") {
                mut_self = true;
            } else if index > 0 && punct(signature, index - 1, '&') {
                return false;
            }
        }
        if punct(signature, index, '-') && punct(signature, index + 1, '>') {
            return false;
        }
    }
    mut_self
}

fn transitive_writes(
    callee: &FunctionSummary,
    methods: &SelfTypeMethods<'_>,
) -> Option<BTreeSet<String>> {
    let mut written = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut pending = vec![(callee, 0usize)];
    while let Some((function, depth)) = pending.pop() {
        if !visited.insert(function.id.0.clone()) {
            continue;
        }
        if depth > MAX_DEPTH {
            return None;
        }
        let (_, body) = split_signature(function)?;
        let uses = scan_field_use(&body);
        if uses.whole_self || uses.opaque {
            return None;
        }
        written.extend(uses.writes);
        for call in &uses.self_calls {
            pending.push((methods.resolve(call)?, depth + 1));
        }
    }
    Some(written)
}

/// Self-type method names whose body reads a written field, passes the
/// whole receiver, or calls a reader or an unresolved self method.
fn reader_methods(methods: &SelfTypeMethods<'_>, written: &BTreeSet<String>) -> BTreeSet<String> {
    let mut direct = BTreeMap::new();
    for (name, definitions) in &methods.by_name {
        for definition in definitions {
            let uses = split_signature(definition).map(|(_, body)| scan_field_use(&body));
            direct
                .entry((*name).to_string())
                .or_insert_with(Vec::new)
                .push(uses);
        }
    }
    let mut readers: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut changed = false;
        for (name, definitions) in &direct {
            if readers.contains(name) {
                continue;
            }
            let reads = definitions.iter().any(|uses| match uses {
                None => true,
                Some(uses) => {
                    uses.whole_self
                        || uses.reads.iter().any(|field| written.contains(field))
                        || uses
                            .self_calls
                            .iter()
                            .any(|call| readers.contains(call) || methods.resolve(call).is_none())
                }
            });
            if reads {
                readers.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    readers
}

/// The initializer tokens of the one `let [mut] name [: T] = ..;` in the test
/// body. `None` when there is no such `let` or more than one.
fn single_let_initializer(body: &[Tok], name: &str) -> Option<Vec<Tok>> {
    let mut found = None;
    let mut count = 0usize;
    for (index, token) in body.iter().enumerate() {
        if *token != Tok::Ident("let".to_string()) {
            continue;
        }
        let mut cursor = index + 1;
        if ident(body, cursor) == Some("mut") {
            cursor += 1;
        }
        if ident(body, cursor) != Some(name) {
            continue;
        }
        cursor += 1;
        // Skip a type annotation up to the `=` at depth zero.
        let mut depth = 0isize;
        while let Some(token) = body.get(cursor) {
            match token {
                Tok::Punct('<' | '(' | '[') => depth += 1,
                Tok::Punct('>' | ')' | ']') => depth -= 1,
                Tok::Punct('=') if depth <= 0 => break,
                Tok::Punct(';') => break,
                _ => {}
            }
            cursor += 1;
        }
        if !punct(body, cursor, '=') {
            continue;
        }
        let start = cursor + 1;
        let mut end = start;
        let mut depth = 0isize;
        while let Some(token) = body.get(end) {
            match token {
                Tok::Punct('(' | '[' | '{') => depth += 1,
                Tok::Punct(')' | ']' | '}') => depth -= 1,
                Tok::Punct(';') if depth <= 0 => break,
                _ => {}
            }
            if depth < 0 {
                break;
            }
            end += 1;
        }
        count += 1;
        found = Some(body[start..end].to_vec());
    }
    if count == 1 { found } else { None }
}

#[cfg(test)]
mod tests;
