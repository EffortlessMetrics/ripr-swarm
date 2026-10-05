//! Unknown, not a gap, for seam reach (#5411).
//!
//! A seam with no related test reads `ungripped` only when ripr established
//! that no test path reaches its owner. Two analyzer limits leave that
//! negative unestablished, the same limits `ripr check` already names on
//! `no_static_path` findings:
//!
//! - a bounded transitive or macro path from a test toward the owner exists
//!   but is unresolved (RIPR-SPEC-0114, RIPR-SPEC-0118);
//! - the owner is a trait-impl method, which operators, formatting macros
//!   and generic calls run without naming it, test-reached code names the
//!   impl's self type, and, for a std trait with its own call syntax
//!   (`Display`, `Debug`, `PartialEq`, `PartialOrd`/`Ord`, `Hash`, `Clone`,
//!   `Default`, `FromStr`, serde, `Arbitrary`), test-reached code uses that
//!   syntax (#5577).
//!
//! Either one makes the reach stage `opaque` with the witness named, so the
//! seam classifies `opaque` (an unknown with a limit) instead of a gap. The
//! witness is a candidate path, never a related test: it adds no reach,
//! activation or oracle credit.

use super::related_tests::{CompactGripContext, strip_comments_and_strings};
use crate::analysis::classify::{
    MAX_TRANSITIVE_DEPTH, MacroReachWitness, TransitiveWitness, calls_after_definition,
    impl_self_type_name, is_trait_impl_method, macro_reach_limit_kind, transitive_reach_limit_kind,
};
use crate::analysis::rust_index::{FunctionSummary, TestSummary, is_test_file};
use crate::domain::{Confidence, StageEvidence, StageState};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Where an unresolved trait-dispatch path starts: a test that names the
/// self type, or a test-reached production function that does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeMention {
    Test {
        name: String,
        file: PathBuf,
        line: usize,
    },
    Function {
        name: String,
        file: PathBuf,
        line: usize,
        /// Reached only through another trait method, not a test's calls.
        via_dispatch: bool,
    },
}

/// A trait-impl method whose self type test-reached code names. Trait
/// dispatch may run it, and the functions it calls, without any call naming
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DispatchRoot {
    id: String,
    method: String,
    self_type: String,
    file: PathBuf,
    line: usize,
}

/// First mention of each type-shaped identifier by a test or by code a test
/// may run, and the functions reached only through trait dispatch. Built
/// once per run on first use.
#[derive(Default)]
pub(in crate::analysis::test_grip_evidence) struct TypeMentionIndex {
    /// Best mention per identifier and its rank (lower wins): a test that
    /// builds or calls through the type, then a test that only names it
    /// (`assert_send_sync::<T>()`), then test-reached production code.
    mentions: BTreeMap<String, (TypeMention, u8)>,
    /// Owner id to the trait method it may run from.
    dispatch_reached: BTreeMap<String, DispatchRoot>,
    /// First use of each gated trait's call syntax. The key's type is
    /// `None` for a test, a test-file helper or a generic function
    /// (`fn render<T: Display>`), whose use may run the trait for any type.
    /// Other test-reached production code counts only for a type the same
    /// body names: a `Display` impl writing a `char` with `{:?}` runs
    /// `char`'s `Debug`, not every type's. A gated trait with no matching
    /// entry never dispatches.
    trait_uses: BTreeMap<(GatedTrait, Option<String>), TraitUse>,
}

/// Where test-reached code first uses a gated trait's call syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TraitUse {
    syntax: &'static str,
    site: String,
    file: PathBuf,
    line: usize,
}

impl TypeMentionIndex {
    pub(in crate::analysis::test_grip_evidence) fn build(context: &CompactGripContext<'_>) -> Self {
        let mut index = Self::default();
        let mut tests: Vec<_> = context.tests.iter().map(|indexed| indexed.test).collect();
        tests.sort_by(|a, b| {
            (&a.file, a.start_line, &a.name).cmp(&(&b.file, b.start_line, &b.name))
        });
        let mut test_files: BTreeSet<&std::path::Path> = BTreeSet::new();
        for test in tests {
            index.add_test_mentions(test);
            index.add_trait_uses(&test.body, &test.name, &test.file, test.start_line, true);
            test_files.insert(test.file.as_path());
        }
        // A test file's imports (`use serde_json::to_string;`) say what its
        // bare calls run.
        for file in test_files {
            if let Some(facts) = context.index.files().get(file) {
                let imports = use_items(&facts.data().source);
                index.add_trait_uses(&imports, "use", file, 1, true);
            }
        }
        let reach = &context.transitive_reach;
        let mut reached: BTreeSet<&str> = BTreeSet::new();
        let mut functions = reach.test_reached_functions();
        sort_functions(&mut functions);
        for function in functions {
            reached.insert(function.id.0.as_str());
            index.add_function_mentions(function, false);
            index.add_function_trait_uses(function);
        }

        // A dispatch root's callees may name further types whose trait
        // methods are roots in turn. Each round is one more hop, bounded
        // like the transitive walk.
        let mut trait_methods: Vec<&FunctionSummary> = reach
            .production_functions()
            .filter(|function| is_trait_impl_method(function))
            .collect();
        sort_functions(&mut trait_methods);
        // Gated callees a root reached before their trait's syntax was seen;
        // retried each round, since later roots may add that syntax.
        let mut deferred: Vec<(&FunctionSummary, DispatchRoot)> = Vec::new();
        for _ in 0..MAX_TRANSITIVE_DEPTH {
            let mut grew = false;
            for (callee, root) in std::mem::take(&mut deferred) {
                if !index.may_dispatch(&callee.id.0) {
                    deferred.push((callee, root));
                } else if reached.insert(callee.id.0.as_str()) {
                    index.add_dispatch_reached(callee, root);
                    grew = true;
                }
            }
            for &method in &trait_methods {
                if reached.contains(method.id.0.as_str()) {
                    continue;
                }
                let Some(self_type) = impl_self_type_name(&method.id.0) else {
                    continue;
                };
                if !index.self_type_dispatches(&method.id.0, &self_type) {
                    continue;
                }
                let root = DispatchRoot {
                    id: method.id.0.clone(),
                    method: method.name.clone(),
                    self_type,
                    file: method.file.clone(),
                    line: method.start_line,
                };
                reached.insert(method.id.0.as_str());
                index.add_function_mentions(method, true);
                index.add_function_trait_uses(method);
                let calls = calls_after_definition(&method.name, method.start_line, &method.calls);
                let mut callees = reach.functions_reached_from(calls, MAX_TRANSITIVE_DEPTH);
                sort_functions(&mut callees);
                for callee in callees {
                    // Name matching reaches every same-named method; a gated
                    // trait's method still needs its syntax (`Display::fmt`
                    // calling `.fmt(f)` must not reach an unused `Debug::fmt`).
                    if is_trait_impl_method(callee) && !index.may_dispatch(&callee.id.0) {
                        if !reached.contains(callee.id.0.as_str()) {
                            deferred.push((callee, root.clone()));
                        }
                        continue;
                    }
                    if reached.insert(callee.id.0.as_str()) {
                        index.add_dispatch_reached(callee, root.clone());
                    }
                }
                grew = true;
            }
            if !grew {
                break;
            }
        }
        index
    }

    fn add_dispatch_reached(&mut self, callee: &FunctionSummary, root: DispatchRoot) {
        self.dispatch_reached.insert(callee.id.0.clone(), root);
        self.add_function_mentions(callee, true);
        self.add_function_trait_uses(callee);
    }

    fn add_function_mentions(&mut self, function: &FunctionSummary, via_dispatch: bool) {
        for (token, _) in identifiers(&function.body) {
            self.mentions.entry(token).or_insert_with(|| {
                let mention = TypeMention::Function {
                    name: function.name.clone(),
                    file: function.file.clone(),
                    line: function.start_line,
                    via_dispatch,
                };
                (mention, 2)
            });
        }
    }

    fn add_test_mentions(&mut self, test: &TestSummary) {
        for (token, used_as_value) in identifiers(&test.body) {
            let rank = if used_as_value { 0 } else { 1 };
            if self
                .mentions
                .get(&token)
                .is_some_and(|(_, existing)| *existing <= rank)
            {
                continue;
            }
            let mention = TypeMention::Test {
                name: test.name.clone(),
                file: test.file.clone(),
                line: test.start_line,
            };
            self.mentions.insert(token, (mention, rank));
        }
    }

    fn add_function_trait_uses(&mut self, function: &FunctionSummary) {
        self.add_trait_uses(
            &function.body,
            &function.name,
            &function.file,
            function.start_line,
            is_test_file(&function.file) || is_generic_signature(&function.body),
        );
    }

    fn add_trait_uses(
        &mut self,
        body: &str,
        site: &str,
        file: &std::path::Path,
        line: usize,
        test_code: bool,
    ) {
        let uses = gated_trait_uses(body);
        if uses.is_empty() {
            return;
        }
        let types: Vec<Option<String>> = if test_code {
            vec![None]
        } else {
            identifiers(body)
                .into_iter()
                .map(|(token, _)| Some(token))
                .collect()
        };
        for (gated, syntax) in uses {
            for type_name in &types {
                self.trait_uses
                    .entry((gated, type_name.clone()))
                    .or_insert_with(|| TraitUse {
                        syntax,
                        site: site.to_string(),
                        file: file.to_path_buf(),
                        line,
                    });
            }
        }
    }

    /// Whether trait dispatch may run the trait-impl method `owner_id`: its
    /// trait is not gated, or test-reached code uses the trait's syntax.
    fn may_dispatch(&self, owner_id: &str) -> bool {
        self.trait_use(owner_id).is_ok()
    }

    /// The gated trait use that lets `owner_id` dispatch (`Ok(None)` for an
    /// ungated trait), or `Err` when its gated trait is never used.
    fn trait_use(&self, owner_id: &str) -> Result<Option<(GatedTrait, &TraitUse)>, ()> {
        let Some(gated) = GatedTrait::of_owner(owner_id) else {
            return Ok(None);
        };
        let self_type = impl_self_type_name(owner_id);
        self.trait_uses
            .get(&(gated, None))
            .or_else(|| self.trait_uses.get(&(gated, self_type)))
            .map(|found| Some((gated, found)))
            .ok_or(())
    }

    /// Whether test-reached code may run the trait-impl method `owner_id`
    /// of `self_type` through dispatch. A primitive self type such as `u32`
    /// is named almost everywhere, so its impl also needs the trait itself
    /// named (a `T: Encode` bound, `Encode::encode`).
    fn self_type_dispatches(&self, owner_id: &str, self_type: &str) -> bool {
        if !self.mentions.contains_key(self_type) || !self.may_dispatch(owner_id) {
            return false;
        }
        !is_primitive_type(self_type)
            || impl_trait_name(owner_id).is_some_and(|name| self.mentions.contains_key(&name))
    }

    fn mention(&self, type_name: &str) -> Option<&TypeMention> {
        self.mentions.get(type_name).map(|(mention, _)| mention)
    }

    fn dispatch_root(&self, owner_id: &str) -> Option<&DispatchRoot> {
        self.dispatch_reached.get(owner_id)
    }
}

fn sort_functions(functions: &mut [&FunctionSummary]) {
    functions
        .sort_by(|a, b| (&a.file, a.start_line, &a.id.0).cmp(&(&b.file, b.start_line, &b.id.0)));
}

/// Type-shaped identifiers (capitalized, or a primitive type such as `u32`)
/// outside comments and strings, each with whether any occurrence builds or
/// calls through it (`T::`, `T(`, `T {`) rather than only naming it in a type
/// position.
fn identifiers(body: &str) -> Vec<(String, bool)> {
    let code = body
        .lines()
        .map(strip_comments_and_strings)
        .collect::<Vec<_>>()
        .join("\n");
    let mut out: BTreeMap<String, bool> = BTreeMap::new();
    let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut start = None;
    for (index, ch) in code.char_indices().chain([(code.len(), ' ')]) {
        if is_ident(ch) {
            start.get_or_insert(index);
            continue;
        }
        let Some(begin) = start.take() else {
            continue;
        };
        let token = &code[begin..index];
        if !token.chars().next().is_some_and(char::is_uppercase) && !is_primitive_type(token) {
            continue;
        }
        let rest = code[index..].trim_start();
        let used_as_value =
            rest.starts_with("::") || rest.starts_with('(') || rest.starts_with('{');
        let entry = out.entry(token.to_string()).or_insert(false);
        *entry |= used_as_value;
    }
    out.into_iter().collect()
}

/// The reach stage for a seam with no related test: `opaque` with the named
/// limit when a candidate path is unresolved, else the established `no`.
pub(in crate::analysis::test_grip_evidence) fn reach_without_related_tests(
    owner: &str,
    owner_fn: Option<&FunctionSummary>,
    context: &CompactGripContext<'_>,
) -> StageEvidence {
    if let Some(owner_fn) = owner_fn
        && let Some(summary) = unresolved_reach_summary(owner_fn, context)
    {
        return StageEvidence::new(StageState::Opaque, Confidence::Low, summary);
    }
    StageEvidence::new(
        StageState::No,
        Confidence::Medium,
        format!("No static test path found for seam owner `{owner}`"),
    )
}

fn unresolved_reach_summary(
    owner_fn: &FunctionSummary,
    context: &CompactGripContext<'_>,
) -> Option<String> {
    let owner_name = owner_fn.name.as_str();
    if owner_name.is_empty() {
        return None;
    }
    // The witness walks match by name, so every owner sharing a name shares
    // their answer; only the trait-dispatch checks need the owner id.
    if let Some(summary) = cached(context, format!("name:{owner_name}"), || {
        witness_summary(owner_name, context)
    }) {
        return Some(summary);
    }
    cached(context, format!("id:{}", owner_fn.id.0), || {
        trait_dispatch_reach_summary(owner_fn, context)
    })
}

fn cached(
    context: &CompactGripContext<'_>,
    key: String,
    compute: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if let Some(cached) = context.unresolved_reach_cached(&key) {
        return cached;
    }
    let summary = compute();
    context.cache_unresolved_reach(key, summary.clone());
    summary
}

fn witness_summary(owner_name: &str, context: &CompactGripContext<'_>) -> Option<String> {
    if let Some(witness) = context.transitive_reach.transitive_witness(owner_name) {
        return Some(transitive_summary(owner_name, &witness));
    }
    let witness = context.transitive_reach.macro_reach_witness(owner_name)?;
    Some(macro_summary(owner_name, &witness))
}

fn trait_dispatch_reach_summary(
    owner_fn: &FunctionSummary,
    context: &CompactGripContext<'_>,
) -> Option<String> {
    let owner_name = owner_fn.name.as_str();
    let mentions = context.type_mentions();
    if is_trait_impl_method(owner_fn)
        && let Some(self_type) = impl_self_type_name(&owner_fn.id.0)
        && mentions.self_type_dispatches(&owner_fn.id.0, &self_type)
        && let Some(mention) = mentions.mention(&self_type)
    {
        let trait_use = mentions.trait_use(&owner_fn.id.0).ok().flatten();
        return Some(trait_dispatch_summary(
            owner_name, &self_type, mention, trait_use,
        ));
    }
    // A trait method whose own type nothing names may still run from another
    // trait method that delegates to it (`self.inner.fmt(f)`).
    let root = mentions.dispatch_root(&owner_fn.id.0)?;
    let mention = mentions.mention(&root.self_type)?;
    let trait_use = mentions.trait_use(&root.id).ok().flatten();
    Some(dispatch_reached_summary(
        owner_name, root, mention, trait_use,
    ))
}

/// A std trait whose methods run only through call syntax ripr can see
/// (#5577). A test that only builds a value does not format, compare, hash
/// or clone it, so an impl of one of these traits dispatches only when a
/// test uses the trait's syntax, or test-reached code does in a body that
/// names the type. Other traits (`Drop`, `From`, `Iterator`, operator
/// traits, a crate's own traits) run from syntax too common or too implicit
/// to gate, so naming the self type is enough for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GatedTrait {
    Display,
    Debug,
    PartialEq,
    Ord,
    Hash,
    Clone,
    Default,
    FromStr,
    Serde,
    Arbitrary,
}

impl GatedTrait {
    const ALL: [Self; 10] = [
        Self::Display,
        Self::Debug,
        Self::PartialEq,
        Self::Ord,
        Self::Hash,
        Self::Clone,
        Self::Default,
        Self::FromStr,
        Self::Serde,
        Self::Arbitrary,
    ];

    /// The gated trait an impl owner id implements, if any. serde's
    /// `Visitor<'de>` counts; another trait named `Visitor` does not.
    fn of_owner(owner_id: &str) -> Option<Self> {
        let name = impl_trait_name(owner_id)?;
        if name == "Visitor" {
            return owner_id.contains("Visitor<'de>").then_some(Self::Serde);
        }
        Self::from_name(&name)
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Display" => Self::Display,
            "Debug" => Self::Debug,
            "PartialEq" => Self::PartialEq,
            // A `PartialOrd` impl usually delegates to `cmp`, and both run
            // from the same comparisons.
            "PartialOrd" | "Ord" => Self::Ord,
            "Hash" => Self::Hash,
            "Clone" => Self::Clone,
            "Default" => Self::Default,
            "FromStr" => Self::FromStr,
            "Serialize" | "Deserialize" => Self::Serde,
            // `arbitrary` and `proptest` generate values only in fuzz and
            // property harnesses.
            "Arbitrary" => Self::Arbitrary,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Display => "Display",
            Self::Debug => "Debug",
            Self::PartialEq => "PartialEq",
            Self::Ord => "Ord",
            Self::Hash => "Hash",
            Self::Clone => "Clone",
            Self::Default => "Default",
            Self::FromStr => "FromStr",
            Self::Serde => "serde",
            Self::Arbitrary => "Arbitrary",
        }
    }

    /// Identifiers (outside comments and strings) whose use runs the trait.
    fn identifiers(self) -> &'static [&'static str] {
        match self {
            Self::Display => &["to_string", "assert_display_snapshot", "assert_snapshot"],
            // `assert_eq!` and `unwrap` format `Debug` only when they fail,
            // so a passing test never runs `Debug::fmt` through them.
            Self::Debug => &["dbg", "assert_debug_snapshot", "assert_debug_eq"],
            Self::PartialEq => &[
                "assert_eq",
                "assert_ne",
                "debug_assert_eq",
                "debug_assert_ne",
                "eq",
                "ne",
                "contains",
                "dedup",
                "dedup_by_key",
                "position",
            ],
            Self::Ord => &[
                "cmp",
                "partial_cmp",
                "lt",
                "le",
                "gt",
                "ge",
                "max",
                "min",
                "clamp",
                "sort",
                "sort_unstable",
                "sort_by_key",
                "sort_unstable_by_key",
                "max_by_key",
                "min_by_key",
                "binary_search",
                "is_sorted",
                "BTreeMap",
                "BTreeSet",
                "BinaryHeap",
            ],
            Self::Hash => &[
                "hash",
                "hash_one",
                "Hasher",
                "BuildHasher",
                "HashMap",
                "HashSet",
                "IndexMap",
                "IndexSet",
            ],
            Self::Clone => &[
                "clone",
                "cloned",
                "clone_from",
                "to_owned",
                "to_vec",
                "resize",
                "extend_from_slice",
            ],
            Self::Default => &[
                "default",
                "unwrap_or_default",
                "or_default",
                "Default",
                "take",
            ],
            Self::FromStr => &["parse", "from_str", "FromStr"],
            // A format crate or serde itself; a crate whose tests never
            // serialize (serde behind a feature, say) never runs its impls.
            Self::Serde => &[
                "serde",
                "serde_json",
                "serde_yaml",
                "serde_test",
                "toml",
                "bincode",
                "postcard",
                "ron",
                "rmp_serde",
                "ciborium",
                "Serializer",
                "Deserializer",
                "assert_tokens",
                "assert_ser_tokens",
                "assert_de_tokens",
                "assert_json_snapshot",
                "assert_yaml_snapshot",
                "assert_ron_snapshot",
                "assert_toml_snapshot",
                "assert_csv_snapshot",
                "assert_compact_json_snapshot",
            ],
            Self::Arbitrary => &[
                "arbitrary",
                "Unstructured",
                "fuzz_target",
                "proptest",
                "prop_compose",
                "arbitrary_with",
                "quickcheck",
            ],
        }
    }

    /// Operators and macro syntax (outside comments and strings) whose use
    /// runs the trait. `<` and `>` count only with a space on each side, so
    /// generics and `->` do not. Any `vec![` counts for `Clone`, since
    /// `vec![x; n]` clones `x`.
    fn operators(self) -> &'static [&'static str] {
        match self {
            Self::PartialEq => &["==", "!="],
            Self::Ord => &[" < ", " > ", "<=", ">="],
            Self::Clone => &["vec!["],
            _ => &[],
        }
    }
}

/// Each gated trait whose call syntax `body` uses, with the first syntax
/// found. A format string counts only outside a failure message
/// (`assert!`, `panic!`, `expect`), which formats only when the test fails.
fn gated_trait_uses(body: &str) -> Vec<(GatedTrait, &'static str)> {
    let code = body
        .lines()
        .map(strip_comments_and_strings)
        .collect::<Vec<_>>()
        .join("\n");
    let words = words(&code);
    let (display_placeholder, debug_placeholder) = format_placeholders(body);
    GatedTrait::ALL
        .into_iter()
        .filter_map(|gated| {
            let placeholder = match gated {
                GatedTrait::Display if display_placeholder => Some("{}"),
                GatedTrait::Debug if debug_placeholder => Some("{:?}"),
                _ => None,
            };
            let syntax = placeholder
                .or_else(|| {
                    gated
                        .identifiers()
                        .iter()
                        .copied()
                        .find(|ident| words.contains(ident))
                })
                .or_else(|| {
                    gated
                        .operators()
                        .iter()
                        .copied()
                        .find(|op| code.contains(op))
                        .map(str::trim)
                })?;
            Some((gated, syntax))
        })
        .collect()
}

fn words(code: &str) -> BTreeSet<&str> {
    code.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether `body` has a `Display` placeholder (`{}`, `{name}`, `{:>8}`) and
/// a `Debug` one (`{:?}`, `{x:#?}`) in a string literal that is not a
/// failure message.
fn format_placeholders(body: &str) -> (bool, bool) {
    let mut display = false;
    let mut debug = false;
    for (start, literal) in string_literals(body) {
        if is_failure_message(&body[..start]) {
            continue;
        }
        let mut rest = literal;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            if let Some(after) = rest.strip_prefix('{') {
                rest = after;
                continue;
            }
            let Some(close) = rest.find('}') else {
                break;
            };
            let inner = &rest[..close];
            rest = &rest[close + 1..];
            if inner.contains(['{', '"', '\\', ' ']) {
                continue;
            }
            match inner.split_once(':') {
                Some((_, spec)) if spec.contains('?') => debug = true,
                _ => display = true,
            }
        }
    }
    (display, debug)
}

/// Each `"..."` literal's start offset and contents, skipping `//` and
/// `/* */` comments and char literals.
fn string_literals(body: &str) -> Vec<(usize, &str)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i += 2;
            }
            // A char literal such as '"' must not open a string.
            b'\'' if bytes.get(i + 2) == Some(&b'\'') => i += 3,
            b'\'' if bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'') => {
                i += 4;
            }
            // A raw string (`r"…"`, `r#"…"#`, `br"…"`) has no escapes and
            // ends at a quote followed by as many `#` as it opened with.
            b'r' if raw_string_open(bytes, i).is_some() => {
                let Some((hashes, quote)) = raw_string_open(bytes, i) else {
                    i += 1;
                    continue;
                };
                let start = quote;
                let mut end = quote + 1;
                while end < bytes.len()
                    && !(bytes[end] == b'"'
                        && bytes[end + 1..]
                            .iter()
                            .take(hashes)
                            .filter(|b| **b == b'#')
                            .count()
                            == hashes)
                {
                    end += 1;
                }
                if let Some(literal) = body.get(start + 1..end.min(bytes.len())) {
                    out.push((start, literal));
                }
                i = end + 1 + hashes;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                let end = i.min(bytes.len());
                if let Some(literal) = body.get(start + 1..end) {
                    out.push((start, literal));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// The `#` count and opening-quote offset of a raw string starting at the
/// `r` at `at`, if one does: `r` must not end an identifier (`br` may).
fn raw_string_open(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    if at > 0 && is_ident(bytes[at - 1]) {
        let byte_prefix = bytes[at - 1] == b'b' && (at < 2 || !is_ident(bytes[at - 2]));
        if !byte_prefix {
            return None;
        }
    }
    let hashes = bytes[at + 1..].iter().take_while(|b| **b == b'#').count();
    let quote = at + 1 + hashes;
    (bytes.get(quote) == Some(&b'"')).then_some((hashes, quote))
}

/// Whether the call that encloses the end of `before` formats its message
/// only on failure.
fn is_failure_message(before: &str) -> bool {
    let bytes = before.as_bytes();
    let mut depth = 0usize;
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' if depth == 0 => {
                let name = before[..i].trim_end().trim_end_matches('!');
                let start = name
                    .rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .map_or(0, |at| at + 1);
                return matches!(
                    &name[start..],
                    "assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "debug_assert"
                        | "debug_assert_eq"
                        | "debug_assert_ne"
                        | "panic"
                        | "unreachable"
                        | "todo"
                        | "unimplemented"
                        | "expect"
                        | "expect_err"
                );
            }
            b'(' => depth -= 1,
            b';' | b'{' | b'}' if depth == 0 => return false,
            _ => {}
        }
    }
    false
}

/// The `use` items of a file, one per line.
fn use_items(source: &str) -> String {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("use ") || line.starts_with("pub use "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a function's signature takes a type parameter or an `impl` /
/// `dyn` argument, so a trait it uses may run for whatever type a caller
/// passes. Lifetime-only generics (`fn f<'a>`) do not count.
fn is_generic_signature(body: &str) -> bool {
    let signature = body.split('{').next().unwrap_or(body);
    let Some(after_fn) = signature.split_once("fn ").map(|(_, rest)| rest) else {
        return false;
    };
    let name_end = after_fn
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(after_fn.len());
    let rest = &after_fn[name_end..];
    if let Some(params) = rest.strip_prefix('<') {
        let params = params.split('>').next().unwrap_or(params);
        if params
            .split(',')
            .map(str::trim)
            .any(|param| !param.is_empty() && !param.starts_with('\''))
        {
            return true;
        }
    }
    let arguments = rest.split_once('(').map_or("", |(_, args)| args);
    let words = words(arguments);
    words.contains("impl") || words.contains("dyn")
}

/// The trait of a trait-impl owner id: `Display` from
/// `src/lib.rs::impl fmt::Display for Version::fmt`.
fn impl_trait_name(owner_id: &str) -> Option<String> {
    let impl_rest = owner_id.split("::impl ").nth(1)?;
    let impl_body = impl_rest.rsplit_once("::")?.0;
    let (trait_path, _) = impl_body.rsplit_once(" for ")?;
    let trait_path = trait_path.split('<').next()?.trim();
    let name = trait_path.rsplit("::").next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn is_primitive_type(token: &str) -> bool {
    matches!(
        token,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
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
            | "f32"
            | "f64"
    )
}

fn location(file: &std::path::Path, line: usize) -> String {
    format!("{}:{line}", file.display().to_string().replace('\\', "/"))
}

fn transitive_summary(owner_name: &str, witness: &TransitiveWitness) -> String {
    format!(
        "Reach unresolved ({}): `{}` ({}) calls `{}`, which may lead to `{owner_name}` through a call path ripr does not fully trace; no gap is reported",
        transitive_reach_limit_kind(&witness.test_file).as_str(),
        witness.test_name,
        location(&witness.test_file, witness.test_line),
        witness.entry_symbol,
    )
}

fn macro_summary(owner_name: &str, witness: &MacroReachWitness) -> String {
    format!(
        "Reach unresolved ({}): `{}` ({}) calls `{}`, which reaches macro `{}!` ({}) that names `{owner_name}`; ripr does not expand macros, so no gap is reported",
        macro_reach_limit_kind(&witness.macro_host).as_str(),
        witness.test_name,
        location(&witness.test_file, witness.test_line),
        witness.entry_symbol,
        witness.macro_name,
        location(&witness.macro_file, witness.macro_line),
    )
}

fn mention_text(self_type: &str, mention: &TypeMention) -> String {
    match mention {
        TypeMention::Test { name, file, line } => {
            format!(
                "test `{name}` ({}) uses `{self_type}`",
                location(file, *line)
            )
        }
        TypeMention::Function {
            name,
            file,
            line,
            via_dispatch,
        } => format!(
            "`{name}` ({}), which {}, uses `{self_type}`",
            location(file, *line),
            if *via_dispatch {
                "may itself run through trait dispatch"
            } else {
                "tests may call"
            }
        ),
    }
}

/// `, and `site` (file:line) uses `syntax`, which runs `Trait`` for a gated
/// trait; nothing for an ungated one.
fn trait_use_text(trait_use: Option<(GatedTrait, &TraitUse)>) -> String {
    let Some((gated, found)) = trait_use else {
        return String::new();
    };
    format!(
        ", and `{}` ({}) uses `{}`, which runs `{}`",
        found.site,
        location(&found.file, found.line),
        found.syntax,
        gated.name(),
    )
}

fn trait_dispatch_summary(
    owner_name: &str,
    self_type: &str,
    mention: &TypeMention,
    trait_use: Option<(GatedTrait, &TraitUse)>,
) -> String {
    format!(
        "Reach unresolved (trait dispatch): `{owner_name}` is a trait method of `{self_type}`, which operators, formatting and generic calls run without naming it; {}{}. ripr does not trace trait dispatch, so no gap is reported",
        mention_text(self_type, mention),
        trait_use_text(trait_use),
    )
}

fn dispatch_reached_summary(
    owner_name: &str,
    root: &DispatchRoot,
    mention: &TypeMention,
    trait_use: Option<(GatedTrait, &TraitUse)>,
) -> String {
    format!(
        "Reach unresolved (trait dispatch): `{owner_name}` may run from `{}` ({}), a trait method of `{}` that operators, formatting and generic calls run without naming it; {}{}. ripr does not trace trait dispatch, so no gap is reported",
        root.method,
        location(&root.file, root.line),
        root.self_type,
        mention_text(&root.self_type, mention),
        trait_use_text(trait_use),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_type_shaped_tokens_outside_comments_and_strings() {
        let tokens = identifiers(
            "let v = Version::parse(\"Prerelease\"); // BuildMetadata\nassert_send_sync::<Op>();",
        );
        assert_eq!(
            tokens,
            vec![("Op".to_string(), false), ("Version".to_string(), true)]
        );
    }

    #[test]
    fn identifiers_keep_primitive_types() {
        let tokens = identifiers("let n: u32 = width(3usize);");
        assert_eq!(tokens, vec![("u32".to_string(), false)]);
    }

    #[test]
    fn format_strings_count_outside_failure_messages_only() {
        assert_eq!(
            format_placeholders("let s = format!(\"{}\", v);"),
            (true, false)
        );
        assert_eq!(format_placeholders("println!(\"{v:#?}\");"), (false, true));
        assert_eq!(
            format_placeholders("write!(f, \"{:>8}\", v)"),
            (true, false)
        );
        assert_eq!(
            format_placeholders("assert!(ok(v), \"bad {:?}\", v); x.expect(\"{}\");"),
            (false, false)
        );
        assert_eq!(
            format_placeholders("assert_eq!(f(x), 1, \"bad {}\", v);"),
            (false, false)
        );
        // A format string passed to another call inside an assertion runs.
        assert_eq!(
            format_placeholders("assert_eq!(format!(\"{:?}\", v), \"V\");"),
            (false, true)
        );
        // Escaped braces and a char literal quote do not open a placeholder.
        assert_eq!(
            format_placeholders("let q = '\"'; let s = \"{{x}}\";"),
            (false, false)
        );
    }

    #[test]
    fn raw_strings_are_read_whole() {
        // The inner quote does not end `r#"…"#`, so the later `{:?}` is
        // still read as its own literal.
        assert_eq!(
            format_placeholders("let a = r#\"say \"x\" {}\"#; let b = format!(\"{:?}\", a);"),
            (true, true)
        );
        assert_eq!(
            format_placeholders("let p = r\"C:\\\"; let s = format!(\"{:?}\", p);"),
            (false, true)
        );
        assert_eq!(format_placeholders("let b = br\"{x}\";"), (true, false));
    }

    #[test]
    fn block_comments_hold_no_format_strings() {
        assert_eq!(
            format_placeholders("let a = 1; /* \"{:?}\" */ let b = 2;"),
            (false, false)
        );
        assert_eq!(
            format_placeholders("/* x */ let s = format!(\"{}\", a);"),
            (true, false)
        );
    }

    #[test]
    fn generic_signatures_take_type_parameters_or_impl_arguments() {
        assert!(is_generic_signature(
            "fn render<T: Display>(t: &T) -> String { t.to_string() }"
        ));
        assert!(is_generic_signature("pub fn show(t: &impl Debug) { }"));
        assert!(is_generic_signature("fn show(t: &dyn Debug) { }"));
        assert!(!is_generic_signature(
            "fn fmt<'a>(&self, f: &mut Formatter<'a>) -> Result { }"
        ));
        assert!(!is_generic_signature(
            "fn fmt(&self, f: &mut Formatter<'_>) -> Result { }"
        ));
    }

    #[test]
    fn gated_trait_uses_read_operators_and_identifiers() {
        let uses =
            gated_trait_uses("if a < b && v == w { list.sort(); }\nlet x: Vec<u8> = Vec::new();");
        assert_eq!(
            uses,
            vec![(GatedTrait::PartialEq, "=="), (GatedTrait::Ord, "sort"),]
        );
        // `->` and generics are not comparisons.
        assert_eq!(
            gated_trait_uses("fn f() -> Option<Vec<u8>> { None }"),
            vec![]
        );
    }

    #[test]
    fn impl_trait_name_reads_the_trait_segment() {
        assert_eq!(
            impl_trait_name("src/display.rs::impl fmt::Display for Version::fmt").as_deref(),
            Some("Display")
        );
        assert_eq!(
            impl_trait_name("src/serde.rs::impl Deserialize<'de> for Version::deserialize")
                .as_deref(),
            Some("Deserialize")
        );
        assert_eq!(impl_trait_name("src/lib.rs::impl Version::parse"), None);
        assert_eq!(
            GatedTrait::of_owner("src/serde.rs::impl Visitor<'de> for V::visit_str"),
            Some(GatedTrait::Serde)
        );
        assert_eq!(
            GatedTrait::of_owner("src/ast.rs::impl Visitor for Walk::visit"),
            None
        );
    }
}
