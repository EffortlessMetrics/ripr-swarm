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
//!    or path-qualified function call outside `Self`, the self type and std
//!    roots, no macro outside a pure list, no `ref mut` pattern, no interior
//!    mutability (`borrow_mut`, `lock`, atomics, `Cell`), no `unsafe`, and
//!    every transitive `self.method(..)` resolves the same way. A std path
//!    into I/O, the environment, processes or threads, and a mutating call
//!    on a parameter or local, also leave the object.
//! 4. The owner itself, apart from the changed call, reads no written field,
//!    uses no bare `self` and calls no other reading self method.
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
//! as carriers. A test that calls a `&mut self` reader other than the owner
//! (`inv.reorder()`) may move the written state into any field before it
//! asserts, so every whole-object equality in it is admitted. Mock
//! expectations and snapshots are never refused.

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
    "abs",
    "as_bytes",
    "as_str",
    "binary_search",
    "checked_add",
    "checked_sub",
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
    "map",
    "max",
    "min",
    "partial_cmp",
    "range",
    "saturating_add",
    "saturating_sub",
    "starts_with",
    "to_owned",
    "to_string",
    "to_vec",
    "trim",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_or_else",
    "values",
];

/// Methods that, called as a discarded statement, only store into the field.
const WRITE_ONLY_METHODS: &[&str] = &[
    "clear",
    "extend",
    "insert",
    "pop",
    "pop_back",
    "pop_front",
    "push",
    "push_back",
    "push_front",
    "push_str",
    "remove",
    "truncate",
];

/// Shared-ownership handles whose value may alias the object's state.
const SHARED_HANDLES: &[&str] = &["Arc", "Rc", "Weak"];

/// Methods that only finish an in-object chain (`entry(..).or_insert(0)`).
const CHAIN_METHODS: &[&str] = &[
    "and_modify",
    "collect",
    "expect",
    "is_err",
    "is_ok",
    "ok",
    "or_default",
    "or_insert",
    "or_insert_with",
    "unwrap",
];

/// Uppercase std types whose constructors cannot return the receiver.
const STD_VALUE_TYPES: &[&str] = &[
    "BTreeMap", "BTreeSet", "Box", "HashMap", "HashSet", "Option", "Result", "Vec", "VecDeque",
];

/// Collection methods that change only the collection they are called on.
const COLLECTION_MUTATORS: &[&str] = &[
    "append",
    "clear",
    "dedup",
    "drain",
    "entry",
    "extend",
    "insert",
    "pop",
    "pop_back",
    "pop_front",
    "push",
    "push_back",
    "push_front",
    "push_str",
    "remove",
    "reserve",
    "resize",
    "retain",
    "reverse",
    "shrink_to_fit",
    "sort",
    "sort_by",
    "sort_by_key",
    "sort_unstable",
    "swap_remove",
    "truncate",
];

/// std modules whose functions reach state outside the object.
const STD_EFFECT_MODULES: &[&str] = &["env", "fs", "io", "net", "process", "sync", "thread"];

/// Identifiers that mark state a `self.<field>` scan cannot see.
const OPAQUE_STATE_MARKERS: &[&str] = &[
    "AtomicBool",
    "AtomicI32",
    "AtomicI64",
    "AtomicU32",
    "AtomicU64",
    "AtomicUsize",
    "Cell",
    "borrow",
    "get_mut",
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

/// `scan_field_use` over a function's body. An inline format capture of the
/// receiver (`format!("{self:?}")`) sits in a masked string, so it is read
/// from the raw text and counts as a bare `self` (#7046 review).
fn function_field_use(function: &FunctionSummary) -> Option<FieldUse> {
    let (_, body) = split_signature(function)?;
    let mut uses = scan_field_use(&body);
    if inline_format_captures(&function.body)
        .iter()
        .any(|capture| capture == "self")
    {
        uses.whole_self = true;
    }
    Some(uses)
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
        if name == "ref" && ident(body, index + 1) == Some("mut") {
            // A `ref mut` pattern binding writes through a place the
            // `self.<field>` scan does not attribute.
            uses.opaque = true;
        }
        let previous_is_path =
            index > 0 && (punct(body, index - 1, '.') || punct(body, index - 1, ':'));
        if name == "self" {
            scan_self_access(body, index, &mut uses);
            continue;
        }
        if index > 0
            && punct(body, index - 1, ':')
            && punct(body, index + 1, '(')
            && name.starts_with(|c: char| c.is_lowercase() || c == '_')
            && !path_root(body, index).is_some_and(|root| STD_ROOTS.contains(&root))
        {
            // A path-qualified function (`Audit::record(..)`,
            // `crate::audit::record(..)`) can reach global state like a free
            // call. An associated function of the self type
            // (`Self::record()`) is not traversed, so it counts too (#7046
            // review); only std roots stay bounded.
            uses.opaque = true;
        }
        if index > 0
            && punct(body, index - 1, ':')
            && path_segments(body, index)
                .iter()
                .any(|segment| STD_EFFECT_MODULES.contains(segment))
        {
            // A std path into I/O, the environment, processes or threads
            // (`std::fs::write(..)`) leaves the object (#7046 review).
            uses.opaque = true;
        }
        if index >= 2
            && punct(body, index - 1, '.')
            && punct(body, index + 1, '(')
            && ident(body, index - 2) != Some("self")
            && !READ_ONLY_METHODS.contains(&name.as_str())
            && !COLLECTION_MUTATORS.contains(&name.as_str())
            && !CHAIN_METHODS.contains(&name.as_str())
        {
            // A later method in a chain (`self.f.as_ref().write_all(..)`,
            // `self.tx.clone().send(..)`) may publish state outside the
            // receiver (#7046 review).
            uses.opaque = true;
        }
        if index >= 1
            && punct(body, index - 1, '.')
            && punct(body, index + 1, ':')
            && punct(body, index + 2, ':')
            && punct(body, index + 3, '<')
            && !READ_ONLY_METHODS.contains(&name.as_str())
            && !COLLECTION_MUTATORS.contains(&name.as_str())
            && !CHAIN_METHODS.contains(&name.as_str())
        {
            // A turbofish method call (`self.events.record::<Low>(..)`,
            // `self.flush::<T>()`) hides its `(` behind generic arguments,
            // so neither the field walk nor the self-call scan sees it
            // (#7046 review).
            uses.opaque = true;
        }
        if previous_is_path || KEYWORDS.contains(&name.as_str()) {
            continue;
        }
        if punct(body, index + 1, '.')
            && let Some(method) = ident(body, index + 2)
            && punct(body, index + 3, '(')
            && !READ_ONLY_METHODS.contains(&method)
        {
            // A mutating call on a parameter or local (`sink.record(..)`,
            // `w.write_all(..)`) can write through a shared handle the
            // `self.<field>` scan does not attribute (#7046 review).
            uses.opaque = true;
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
        if !read_only && !COLLECTION_MUTATORS.contains(&method) {
            // A field method outside the known in-object operations
            // (`self.file.write_all(..)`, `self.sink.publish(..)`) may
            // publish state outside the receiver (#7046 review).
            uses.opaque = true;
        }
        let statement = statement_start(body, index)
            && matching_close(body, cursor + 2).is_some_and(|close| punct(body, close + 1, ';'));
        // A discarded call reads the field unless it only stores into it
        // (`self.low_stock.clone_into(&mut self.log)` reads `low_stock`).
        if read_only || !statement || !WRITE_ONLY_METHODS.contains(&method) {
            uses.reads.insert(field.clone());
        }
        if !read_only {
            uses.writes.insert(field);
        }
        return;
    }
    uses.reads.insert(field);
}

/// The `(` opening the argument list that holds `index`.
fn enclosing_call_open(tokens: &[Tok], index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        match tokens.get(cursor) {
            Some(Tok::Punct(')')) => depth += 1,
            Some(Tok::Punct('(')) if depth == 0 => return Some(cursor),
            Some(Tok::Punct('(')) => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The declared field types of the self type's one `struct` definition in
/// the workspace, by field name; `None` when it is missing, defined more than
/// once, generic or not a braced struct.
fn self_type_field_types(self_ty: &str, index: &RustIndex) -> Option<BTreeMap<String, String>> {
    let mut found = None;
    for facts in index.files().values() {
        let source =
            crate::analysis::language::mask_rust_comments_and_strings(&facts.data().source);
        let needle = format!("struct {self_ty}");
        for (start, matched) in source.match_indices(&needle) {
            let before_ok = !source[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            let rest = source[start + matched.len()..].trim_start();
            if !before_ok || rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
                continue;
            }
            if found.is_some() {
                return None;
            }
            let body = rest.strip_prefix('{')?;
            let mut depth = 0usize;
            let mut end = None;
            for (offset, ch) in body.char_indices() {
                match ch {
                    '{' | '(' | '[' | '<' => depth += 1,
                    '}' if depth == 0 => {
                        end = Some(offset);
                        break;
                    }
                    '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            found = Some(parse_struct_fields(&body[..end?])?);
        }
    }
    found
}

fn parse_struct_fields(body: &str) -> Option<BTreeMap<String, String>> {
    let mut fields = BTreeMap::new();
    let mut depth = 0usize;
    let mut entry_start = 0;
    for (index, ch) in body.char_indices().chain([(body.len(), ',')]) {
        match ch {
            '(' | '[' | '<' | '{' => depth += 1,
            ')' | ']' | '>' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let entry = body[entry_start..index].trim();
                entry_start = index + 1;
                if entry.is_empty() {
                    continue;
                }
                let (name, ty) = entry.split_once(':')?;
                let name = name
                    .split_whitespace()
                    .last()?
                    .trim_start_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
                fields.insert(name.to_string(), ty.trim().to_string());
            }
            _ => {}
        }
    }
    Some(fields)
}

/// Whether every type name in `ty` is a std collection, wrapper or
/// primitive, so field methods and operators run no user code.
fn std_only_type(ty: &str) -> bool {
    ty.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|segment| !segment.is_empty())
        .all(|segment| STD_FIELD_TYPES.contains(&segment) || STD_ROOTS.contains(&segment))
}

/// Methods of the owner's self type, by name.
struct SelfTypeMethods<'a> {
    by_name: BTreeMap<&'a str, Vec<&'a FunctionSummary>>,
    /// Names also defined as a method on another type, in a trait, or in an
    /// unparsed file: a call by that name may not reach this type.
    contested: BTreeSet<&'a str>,
}

impl<'a> SelfTypeMethods<'a> {
    fn collect(self_ty: &'a str, index: &'a RustIndex) -> Self {
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
    /// Fields any self-type method touches through `self.<field>`. A field
    /// read on a test binding outside this set (`app.inventory`) may hold
    /// the receiver itself.
    self_fields: BTreeSet<String>,
    /// Every name defined as a method anywhere in the workspace.
    any_method: BTreeSet<String>,
    /// Reader methods other than the owner that take `&mut self`: a test
    /// that calls one can move the written state into any field before it
    /// asserts (`inv.reorder()` turning `low_stock` into log entries).
    mutating_readers: BTreeSet<String>,
}

impl EffectStateCarrier {
    /// `None` unless every gate in the module docs holds.
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        workspace_complete: bool,
    ) -> Option<Self> {
        // An omitted file may hold a same-name method, a `Drop` impl or the
        // self type's definition, so a partial index proves nothing bounded
        // (#7046 review).
        if !workspace_complete {
            return None;
        }
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
        // A user `Drop` impl runs on values a collection mutator removes or
        // replaces, so an in-object store may still reach outside the object
        // (#7046 review).
        if index.functions().iter().any(|function| {
            function.name == "drop"
                && matches!(
                    &function.item.container,
                    FunctionContainer::TraitImpl { trait_path, .. }
                        if trait_path.rsplit("::").next() == Some("Drop")
                )
        }) {
            return None;
        }
        let written_fields = transitive_writes(callee, &methods)?;
        if written_fields.is_empty() {
            return None;
        }
        // A field method or operator dispatches through the field's type: a
        // user type's `push` or `AddAssign` may publish state, so every
        // written field must be declared with std and primitive types only
        // (#7046 review).
        let field_types = self_type_field_types(&self_ty, index)?;
        if !written_fields.iter().all(|field| {
            field_types
                .get(field.as_str())
                .is_some_and(|ty| std_only_type(ty))
        }) {
            return None;
        }
        let reader_methods = reader_methods(&methods, &written_fields);
        // The owner itself may read the written state after the call and
        // move it into a field a whole-object equality does compare
        // (`if self.low_stock.contains(..) { self.log.push(..) }`).
        let owner_uses = function_field_use(owner)?;
        if owner_uses.whole_self
            || owner_uses
                .reads
                .iter()
                .any(|field| written_fields.contains(field))
            || owner_uses
                .self_calls
                .iter()
                .any(|call| *call != callee_name && reader_methods.contains(call))
        {
            return None;
        }
        let self_fields = methods
            .by_name
            .values()
            .flatten()
            .filter_map(|definition| function_field_use(definition))
            .flat_map(|uses| uses.reads.into_iter().chain(uses.writes))
            .collect();
        let non_reader_methods = methods
            .by_name
            .keys()
            .filter(|name| !methods.contested.contains(*name) && !reader_methods.contains(**name))
            .map(|name| (*name).to_string())
            .collect();
        let mutating_readers = reader_methods
            .iter()
            .filter(|name| **name != owner.name)
            .filter(|name| {
                methods
                    .by_name
                    .get(name.as_str())
                    .is_some_and(|definitions| {
                        // A `&self` reader that also reaches outside the
                        // object (`publish_low_state` writing a file) moves
                        // the written state just as a `&mut self` one does
                        // (#7046 review).
                        definitions.iter().any(|definition| {
                            split_signature(definition)
                                .is_none_or(|(signature, _)| takes_mut_self(&signature))
                                || function_field_use(definition).is_none_or(|uses| uses.opaque)
                        })
                    })
            })
            .cloned()
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
            self_fields,
            any_method,
            mutating_readers,
        })
    }

    /// Whether `assertion` may observe the written state. Only a whole-object
    /// equality is ever refused.
    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        if !matches!(assertion.kind, OracleKind::WholeObjectEquality) {
            return true;
        }
        let body = tokenize(&test.body);
        if self.calls_mutating_reader(&body) {
            return true;
        }
        // An inline format capture (`format!("{inv:?}")`) names its value only
        // inside the string literal the tokenizer masks.
        if inline_format_captures(&assertion.text)
            .into_iter()
            .any(|name| self.may_carry(&[Tok::Ident(name)], &body, 0))
        {
            return true;
        }
        self.may_carry(&tokenize(&assertion.text), &body, 0)
    }

    /// Whether the test calls a `&mut self` reader (`inv.reorder()`,
    /// `Inventory::reorder(&mut inv)`). The owner may be reached indirectly,
    /// so call order is not established and any such call admits.
    fn calls_mutating_reader(&self, test_body: &[Tok]) -> bool {
        // A `&mut binding` argument hands the receiver to code this scan
        // does not follow (`restock(&mut inv)` calling `inv.reorder()`),
        // unless the call is a resolved non-reader of the self type.
        // A `&mut` borrow anywhere (`let r = &mut inv;`, `vec![&mut inv]`)
        // or a reassigned binding (`inv = restocked(inv);`) hands the
        // receiver on the same way (#7046 review).
        let lends_mut = test_body.iter().enumerate().any(|(index, token)| {
            *token == Tok::Punct('&')
                && ident(test_body, index + 1) == Some("mut")
                && ident(test_body, index + 2).is_some()
                && !self.lent_to_non_reader(test_body, index)
        });
        let reassigns = test_body.iter().enumerate().any(|(index, token)| {
            matches!(token, Tok::Ident(name) if !KEYWORDS.contains(&name.as_str()))
                && statement_start(test_body, index)
                && punct(test_body, index + 1, '=')
                && !punct(test_body, index + 2, '=')
        });
        // A binding passed by value or shared reference to a call
        // (`publish(&inv)`, `audit.record(inv)`) may reach code that reads
        // the written field and publishes it (#7046 review); only a resolved
        // non-reader of the self type is known not to.
        let passes_binding = test_body.iter().enumerate().any(|(index, token)| {
            let Tok::Ident(name) = token else {
                return false;
            };
            if KEYWORDS.contains(&name.as_str())
                || name == "self"
                || !name.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
            {
                return false;
            }
            let start = if index > 0 && punct(test_body, index - 1, '&') {
                index - 1
            } else {
                index
            };
            start > 0
                && (punct(test_body, start - 1, '(') || punct(test_body, start - 1, ','))
                && (punct(test_body, index + 1, ')') || punct(test_body, index + 1, ','))
                && enclosing_call_open(test_body, index).is_some_and(|open| {
                    open > 0
                        && ident(test_body, open - 1).is_some()
                        && !self.lent_to_non_reader(test_body, index)
                })
        });
        lends_mut
            || reassigns
            || passes_binding
            || test_body.iter().enumerate().any(|(index, token)| {
                matches!(token, Tok::Ident(name) if self.mutating_readers.contains(name))
                    && index > 0
                    && (punct(test_body, index - 1, '.') || punct(test_body, index - 1, ':'))
                    && punct(test_body, index + 1, '(')
            })
    }

    /// Whether the call whose argument list holds `index` is a resolved
    /// non-reader of the self type (`Inventory::ship(&mut inv, ..)`).
    fn lent_to_non_reader(&self, tokens: &[Tok], index: usize) -> bool {
        let mut depth = 0usize;
        let mut cursor = index;
        while cursor > 0 {
            cursor -= 1;
            match tokens.get(cursor) {
                Some(Tok::Punct(')')) => depth += 1,
                Some(Tok::Punct('(')) if depth == 0 => {
                    return cursor > 0
                        && ident(tokens, cursor - 1)
                            .is_some_and(|name| self.non_reader_methods.contains(name))
                        && path_root(tokens, cursor - 1)
                            .is_some_and(|root| root == self.self_ty || root == "Self");
                }
                Some(Tok::Punct('(')) => depth -= 1,
                _ => {}
            }
        }
        false
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
            if SHARED_HANDLES.contains(&name) {
                // A shared handle (`Rc::new(Sink::default())`) may alias
                // state the object also holds (#7046 review).
                return true;
            }
            if punct(tokens, index + 1, '(')
                && !KEYWORDS.contains(&name)
                && name.starts_with(|c: char| c.is_lowercase() || c == '_')
                && match path_root(tokens, index) {
                    // A workspace type's associated function
                    // (`Fixtures::stocked()`, `TestBed::with_inventory()`)
                    // may return a value holding the receiver; only std
                    // roots and std value types are decided by their
                    // arguments (#7046 review).
                    Some(root) => !STD_ROOTS.contains(&root) && !STD_VALUE_TYPES.contains(&root),
                    // A turbofish path (`Vec::<Event>::new()`) has no
                    // resolvable root but is path-qualified, not a free call
                    // (#7046 review).
                    None => !(index >= 2 && punct(tokens, index - 1, ':')),
                }
            {
                // A free or module-path function result is not provably
                // non-carrying: a fixture helper such as `setup()` returns
                // the receiver itself.
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
                && ident(tokens, index + 2).is_some_and(|field| self.self_fields.contains(field))
                && !punct(tokens, index + 3, '(')
            {
                // A field of the self type read on the binding: decided at
                // the field token. Any other field (`app.inventory`) may hold
                // the receiver, so the binding itself is resolved below
                // (#7046 review).
                continue;
            }
            if depth >= MAX_BINDING_DEPTH {
                return true;
            }
            let Some((annotation, initializer)) = single_let_initializer(test_body, name) else {
                return true;
            };
            if annotation
                .iter()
                .any(|token| matches!(token, Tok::Ident(ty) if *ty == self.self_ty || ty == "Self"))
            {
                // `let inv: Inventory = Default::default()` holds the receiver type.
                return true;
            }
            if initializer.iter().enumerate().any(|(at, token)| {
                matches!(token, Tok::Ident(_)) && punct(&initializer, at + 1, '!')
            }) {
                // A macro initializer (`format!("{inv:?}")`) may capture the
                // receiver inside a string the tokenizer masks (#7046
                // review).
                return true;
            }
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

/// A `mut self` receiver (`&mut self`, `mut self`, `self: &mut Self`).
fn takes_mut_self(signature: &[Tok]) -> bool {
    signature.iter().enumerate().any(|(index, token)| {
        *token == Tok::Ident("mut".to_string()) && ident(signature, index + 1) == Some("self")
    }) || signature.windows(4).any(|window| {
        window[0] == Tok::Ident("self".to_string())
            && window[1] == Tok::Punct(':')
            && window[2] == Tok::Punct('&')
            && window[3] == Tok::Ident("mut".to_string())
    })
}

/// `&mut self` receiver, no `&mut` parameter, no `->`. A by-value
/// `mut self` receiver is refused: the gate names a borrowed receiver only.
fn mut_self_without_return(signature: &[Tok]) -> bool {
    let mut mut_self = false;
    for (index, token) in signature.iter().enumerate() {
        if *token == Tok::Ident("mut".to_string()) {
            let borrowed = index > 0 && punct(signature, index - 1, '&');
            if ident(signature, index + 1) == Some("self") && borrowed {
                mut_self = true;
            } else if borrowed {
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
        let uses = function_field_use(function)?;
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
            let uses = function_field_use(definition);
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
fn single_let_initializer(body: &[Tok], name: &str) -> Option<(Vec<Tok>, Vec<Tok>)> {
    let mut found = None;
    let mut count = 0usize;
    for (index, token) in body.iter().enumerate() {
        if *token != Tok::Ident("let".to_string()) {
            continue;
        }
        let mut cursor = index + 1;
        if ident(body, cursor) == Some("mut") {
            if ident(body, cursor + 1) == Some(name) {
                // Later writes to a `mut` binding are not tracked: unknown.
                return None;
            }
            cursor += 1;
        }
        if ident(body, cursor) != Some(name) {
            continue;
        }
        cursor += 1;
        let annotation_start = cursor;
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
        let annotation = body
            .get(annotation_start..cursor)
            .unwrap_or_default()
            .to_vec();
        found = Some((annotation, body[start..end].to_vec()));
    }
    if count == 1 { found } else { None }
}

/// Path roots whose functions are std library calls, decided by their
/// arguments rather than treated as fixture helpers.
/// Std types whose methods and operators on a field stay in the object.
const STD_FIELD_TYPES: &[&str] = &[
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "HashMap",
    "HashSet",
    "Option",
    "Vec",
    "VecDeque",
    "collections",
    "std",
    "alloc",
];

const STD_ROOTS: &[&str] = &[
    "std", "core", "alloc", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128",
    "usize", "isize", "f32", "f64", "bool", "char", "str", "String",
];

/// The segments before `name` in the `a::b::name` path ending at `index`.
fn path_segments(tokens: &[Tok], index: usize) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut cursor = index;
    while cursor >= 3 && punct(tokens, cursor - 1, ':') && punct(tokens, cursor - 2, ':') {
        let Some(segment) = ident(tokens, cursor - 3) else {
            break;
        };
        segments.push(segment);
        cursor -= 3;
    }
    segments
}

/// The leftmost segment of the `a::b::name` path ending at `index`, or `None`
/// when `name` is not path-qualified.
fn path_root(tokens: &[Tok], index: usize) -> Option<&str> {
    let mut root = None;
    let mut cursor = index;
    while cursor >= 3 && punct(tokens, cursor - 1, ':') && punct(tokens, cursor - 2, ':') {
        let Some(segment) = ident(tokens, cursor - 3) else {
            break;
        };
        root = Some(segment);
        cursor -= 3;
    }
    root
}

/// Identifiers captured inline by a format string (`"{inv}"`, `"{inv:?}"`).
/// `{{` is an escaped brace, not a capture.
fn inline_format_captures(text: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(text);
    let (raw, masked) = (text.as_bytes(), masked.as_bytes());
    if raw.len() != masked.len() {
        return Vec::new();
    }
    let mut captures = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'{' && masked[index] != b'{' {
            if raw.get(index + 1) == Some(&b'{') {
                index += 2;
                continue;
            }
            let start = index + 1;
            let mut end = start;
            while end < raw.len() && (raw[end].is_ascii_alphanumeric() || raw[end] == b'_') {
                end += 1;
            }
            if end > start && !raw[start].is_ascii_digit() {
                captures.push(String::from_utf8_lossy(&raw[start..end]).into_owned());
            }
            index = end.max(index + 1);
            continue;
        }
        index += 1;
    }
    captures
}

#[cfg(test)]
mod tests;
