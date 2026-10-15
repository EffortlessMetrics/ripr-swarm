//! Bounded execution context for owner-return pins, not a Rust resolver.
//!
//! Oracle extraction intentionally retains deferred assertions. This query
//! admits only uniquely identified assertions on ordinary statement paths,
//! optionally through one syntactically bound, directly invoked closure.
//! A test may also borrow the `assert_eq!` of one test-local check helper it
//! calls eagerly (#6482); see [`local_helper_assertions`].

mod fallible_check;
mod literal_json;
use literal_json::{
    json_namespace_rebound, literal_json_import_visible, literal_json_tokens, literal_json_tree,
};
mod slice_table;
pub(crate) use fallible_check::{decimal_u8, literal_check_syntax, literal_owner_syntax};
pub(crate) use slice_table::{constant_table_call_column, owner_reference_count};

use super::parse_clean_source_file;
use super::ra::{
    LineIndex, expect_test_receiver, is_expect_test_comparison, slice_macro_call_text, slice_text,
    std_macro_root_is_unrebound,
};
use crate::analysis::facts::cfg_predicates::attribute_test_build_availability;
use ra_ap_syntax::{
    AstNode, SyntaxNode, TextSize,
    ast::{
        self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasLoopBody, HasName,
        HasVisibility,
    },
};
use std::collections::{BTreeMap, BTreeSet};

type AssertionKey = (usize, String);
type FunctionKey = (usize, usize, String);

#[derive(Clone, Debug, Default)]
pub(crate) struct OwnerPinAssertions {
    parsed: bool,
    functions: BTreeMap<FunctionKey, FunctionAssertions>,
    module_declarations: BTreeMap<(usize, String), ModuleDeclarationAdmission>,
}

#[derive(Clone, Copy, Debug, Default)]
struct ModuleDeclarationAdmission {
    context: bool,
    source_preserved: bool,
}

#[derive(Clone, Debug, Default)]
struct FunctionAssertions {
    body: String,
    /// Why every assertion in this test is refused, when the refusal is a
    /// property of the whole test rather than of one invocation.
    refusal: Option<AssertionContextRefusal>,
    assertions: BTreeMap<AssertionKey, Result<(), AssertionContextRefusal>>,
    macros: BTreeSet<String>,
    /// Assertions of test-local check helpers this test calls on an eager
    /// path (#6482).
    helper_assertions: BTreeMap<AssertionKey, BorrowedAssertion>,
}

/// One assertion a test borrows from a check helper it calls.
#[derive(Clone, Debug, Default)]
struct BorrowedAssertion {
    /// The macros the helper invokes; they must be unambiguous too.
    macros: BTreeSet<String>,
    loan: HelperLoan,
}

/// The check helper a test borrows an `assert_eq!` from (#6482), and the
/// test's calls that lend it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperLoan {
    /// The helper's name.
    pub(crate) name: String,
    /// The helper's text from `fn` to its closing brace.
    pub(crate) helper: String,
    /// The helper's parameter names, in order, when every parameter is a
    /// plain identifier pattern (`score: u32`) that the helper's body never
    /// rebinds; empty otherwise, so no argument can be mapped through it.
    pub(crate) parameters: Vec<String>,
    /// Lines of the test's calls to the helper that sit on its eager
    /// statement path, each the only call expression on its line.
    pub(crate) call_lines: Vec<usize>,
}

/// Why one `assert_eq!` invocation is not on an established execution path.
/// Each variant is the first gate that failed; a later gate may also fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssertionContextRefusal {
    /// The file holding the test is not parser-clean.
    UnparsedFile,
    /// No unique test function with this name and line span was found.
    UnidentifiedTest,
    /// `async fn` tests run under an executor ripr does not model.
    AsyncTest,
    /// An attribute other than `#[test]` (for example `#[should_panic]`,
    /// `#[ignore]` or a `#[cfg(..)]`) may change whether or how it runs.
    TestAttribute(String),
    /// The test is nested in an executable body (a function or block).
    NestedItem,
    /// An enclosing module carries an attribute (usually a `cfg`) ripr cannot
    /// evaluate for test builds.
    GatedItem(String),
    /// The body invokes a macro whose expansion ripr cannot see, so a hidden
    /// `return` or `?` could skip the assertion.
    OpaqueMacro(String),
    /// An argument of this trusted macro holds a `return`, `?` or nested
    /// macro, which may leave the test before the comparison runs.
    MacroOperandExit(String),
    /// A closure in the body can exit early (`return`, `?`) or the body
    /// yields.
    ClosureExit,
    /// The same assertion text appears twice on one line.
    DuplicateSpelling,
    /// The invocation sits where it may not run: names the construct.
    ConditionalPath(&'static str),
    /// The test body's only `return` is the whole body of a root
    /// `if <condition> { return; }` before the invocation: whether the
    /// invocation runs depends on the condition, held here as written.
    GuardedReturn(String),
    /// The test uses a macro name some workspace file may rebind.
    MacroBinding(String),
    /// The indexed body no longer matches the parsed source.
    StaleSource,
}

/// An attribute that decides the test's outcome whatever its assertions do:
/// `#[ignore]` or a `cfg`/`cfg_attr` disabled in every test build keeps it
/// from running by default, and `#[should_panic]` absorbs a failing
/// assertion. Either is evidence of a gap, not an analyzer limit
/// (RIPR-SPEC-0240).
pub(crate) fn attribute_settles_test_outcome(attr_text: &str) -> bool {
    let compact: String = attr_text.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "#[ignore]"
        || compact.starts_with("#[ignore=")
        || compact == "#[should_panic]"
        || compact.starts_with("#[should_panic(")
        || attribute_test_build_availability(attr_text) == Some(false)
}

impl OwnerPinAssertions {
    pub(crate) fn admits_module_declaration(&self, line: usize, declaration: &str) -> bool {
        self.module_declarations
            .get(&(line, declaration.to_string()))
            .is_some_and(|admission| admission.context)
    }

    pub(crate) fn admits_targeted_module_declaration(
        &self,
        line: usize,
        declaration: &str,
    ) -> bool {
        self.module_declarations
            .get(&(line, declaration.to_string()))
            .is_some_and(|admission| admission.context && admission.source_preserved)
    }

    /// The first gate that refuses this assertion, or `None` when it is
    /// admitted on an established execution path.
    pub(crate) fn refusal(
        &self,
        function: (usize, usize, &str),
        body: &str,
        assertion: (usize, &str),
        ambiguous_macros: &BTreeSet<String>,
    ) -> Option<AssertionContextRefusal> {
        let Some(facts) = self
            .functions
            .get(&(function.0, function.1, function.2.to_string()))
        else {
            return Some(if self.parsed {
                AssertionContextRefusal::UnidentifiedTest
            } else {
                AssertionContextRefusal::UnparsedFile
            });
        };
        if let Some(refusal) = &facts.refusal {
            return Some(refusal.clone());
        }
        if facts.body != body {
            return Some(AssertionContextRefusal::StaleSource);
        }
        if let Some(name) = facts.macros.intersection(ambiguous_macros).next() {
            return Some(AssertionContextRefusal::MacroBinding(name.clone()));
        }
        let key = (assertion.0, assertion.1.to_string());
        match facts.assertions.get(&key) {
            Some(Ok(())) => None,
            Some(Err(refusal)) => Some(refusal.clone()),
            None => match facts.helper_assertions.get(&key) {
                Some(borrowed) => borrowed
                    .macros
                    .intersection(ambiguous_macros)
                    .next()
                    .map(|name| AssertionContextRefusal::MacroBinding(name.clone())),
                None => Some(AssertionContextRefusal::UnidentifiedTest),
            },
        }
    }

    /// The check helper `function` borrows `assertion` from (#6482), or
    /// `None` when the assertion is the test's own or unknown. Callers ask
    /// only after [`Self::refusal`] admitted the assertion.
    pub(crate) fn helper_loan(
        &self,
        function: (usize, usize, &str),
        assertion: (usize, &str),
    ) -> Option<&HelperLoan> {
        let facts = self
            .functions
            .get(&(function.0, function.1, function.2.to_string()))?;
        let key = (assertion.0, assertion.1.to_string());
        if facts.assertions.contains_key(&key) {
            return None;
        }
        facts
            .helper_assertions
            .get(&key)
            .map(|borrowed| &borrowed.loan)
    }
}

/// Visible bindings that may shadow trusted macros. Namespace is
/// intentionally not resolved; each test consults only names it uses.
/// Unknown macro imports affect all trusted names, including cross-file scope.
///
/// `module_resolved(line, "mod name;")` says whether this file's out-of-line
/// module declaration on `line` resolves to an indexed file. `#[macro_use]`
/// on such a module, or on an inline one, only widens the textual scope of
/// `macro_rules!` items whose definitions every scanned file already
/// reports, so it adds no unseen binding. Any other `#[macro_use]` (an
/// `extern crate`, an unresolved module) stays ambiguous for every name.
///
/// Every binding site the scan finds, in source order, including
/// definitions and private imports confined to one inline module, function
/// or block ([`MacroBindingSite::scope`]); callers apply those only to tests
/// inside it.
///
/// `drop_in_verified` says whether a drop-in crate name (`pretty_assertions`)
/// resolves to the registry package for this file's crate.
pub(crate) fn trusted_macro_binding_sites(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    module_resolved: &dyn Fn(usize, &str) -> bool,
    drop_in_verified: &dyn Fn(&str) -> bool,
) -> Vec<(String, MacroBindingSite)> {
    macro_binding_ambiguities(
        source,
        packages,
        trusted,
        &BTreeSet::new(),
        module_resolved,
        drop_in_verified,
    )
}

/// Apply the same binding/import/opaque-expansion authority to candidate empty
/// macros. Only the declaring file may exempt its exact local declaration.
pub(crate) fn empty_macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    names: &BTreeSet<String>,
    declaring_file: bool,
    module_resolved: &dyn Fn(usize, &str) -> bool,
) -> BTreeSet<String> {
    let trusted: Vec<_> = names.iter().map(String::as_str).collect();
    let allowed = if declaring_file {
        names.clone()
    } else {
        BTreeSet::new()
    };
    // Empty-macro names are local declarations, never a drop-in import.
    macro_binding_ambiguities(
        source,
        packages,
        &trusted,
        &allowed,
        module_resolved,
        &|_| false,
    )
    .into_iter()
    .map(|(name, _)| name)
    .collect()
}

/// Where a file may rebind a trusted macro name, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MacroBindingSite {
    /// 1-based line of the binding item; 0 when the file did not parse.
    pub(crate) line: usize,
    pub(crate) kind: MacroBindingKind,
    /// For a `macro_rules!` definition whose textual scope cannot leave an
    /// inline module or function body: that item's first and last line.
    /// `None` means the binding may reach any file in the workspace.
    pub(crate) scope: Option<(usize, usize)>,
    /// The binding cannot leave the crate whose module tree holds this file:
    /// a private import, `#[macro_use] extern crate`, `#![no_implicit_prelude]`
    /// or a non-exported definition. Exported definitions, `pub` imports,
    /// opaque macro arguments and unparsed files may reach other crates.
    pub(crate) crate_local: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MacroBindingKind {
    /// The file is not parser-clean, so any binding may hide in it.
    Unparsed,
    /// `#![no_implicit_prelude]` removes the standard macros.
    NoImplicitPrelude,
    /// `#[macro_use]` on an `extern crate` or a module ripr did not resolve
    /// to an indexed file; the text is the item it is attached to.
    MacroUse(String),
    /// A glob import from outside the workspace; the text is its path.
    ForeignGlob(String),
    /// A `macro_rules!` or `macro` definition with the trusted name.
    Definition,
    /// A `use` that brings the trusted name into scope.
    Import,
    /// A drop-in import (`use pretty_assertions::assert_eq`) whose crate no
    /// read manifest pins to the registry package; the text is the crate.
    UnverifiedDropIn(String),
    /// Another macro's arguments mention the name, so its expansion may
    /// define it.
    MacroArgument(String),
    /// Another macro's arguments carry `macro_use` or `no_implicit_prelude`,
    /// which its expansion may apply to an item.
    ArgumentAttribute { wrapper: String, attribute: String },
}

impl MacroBindingKind {
    /// Whether this site may shadow any macro name, not only the one it
    /// was recorded for.
    pub(crate) fn binds_any_name(&self) -> bool {
        matches!(
            self,
            Self::Unparsed
                | Self::NoImplicitPrelude
                | Self::MacroUse(_)
                | Self::ForeignGlob(_)
                | Self::ArgumentAttribute { .. }
        )
    }
}

/// One file's file-wide macro-binding sites (its scoped sites cannot
/// reach a test in another file). The diff scope (#5320) reads it for files
/// it withholds; no module declaration counts as resolved there, so a
/// `#[macro_use]` stays broad.
pub(crate) fn macro_binding_scan(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    drop_in_verified: &dyn Fn(&str) -> bool,
) -> Vec<(String, MacroBindingSite)> {
    macro_binding_ambiguities(
        source,
        packages,
        trusted,
        &BTreeSet::new(),
        &|_, _| false,
        drop_in_verified,
    )
    .into_iter()
    .filter(|(_, site)| site.scope.is_none())
    .collect()
}

/// Standard macros that cannot return a value from the enclosing function,
/// whose workspace rebinding the owner-pin scans look for.
pub(crate) const TRUSTED_MACRO_NAMES: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "cfg",
    "column",
    "concat",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "eprint",
    "eprintln",
    // expect-test's `expect!` builds a value and cannot return; only its
    // manifest-verified `use expect_test::expect` import keeps it trusted.
    "expect",
    "file",
    "format",
    "format_args",
    "line",
    "matches",
    "module_path",
    "panic",
    "print",
    "println",
    "stringify",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

/// The trusted macro names a file's binding sites may report, whatever the
/// workspace context: any trusted subset, package set, module resolution or
/// drop-in verdict. [`MacroBindingCandidates::may_bind`] false for a name
/// means [`trusted_macro_binding_sites`] reports no site for it in this
/// file, so the scans skip parsing it. Built from the producer's clean
/// parse and stored in the file facts (#5363).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum MacroBindingCandidates {
    /// A site may bind every name: an attribute or macro argument naming
    /// `macro_use` or `no_implicit_prelude`, a glob import not rooted at
    /// `crate`, `self` or `super`, or a `use` without a tree.
    Any,
    /// Only these trusted names have a definition, an import or a mention
    /// in some macro call's arguments.
    Names(BTreeSet<String>),
}

impl MacroBindingCandidates {
    /// Whether the scan may report a site for `name`. A name outside
    /// [`TRUSTED_MACRO_NAMES`] was never recorded, so it is always possible.
    pub(crate) fn may_bind(&self, name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Names(names) => names.contains(name) || !TRUSTED_MACRO_NAMES.contains(&name),
        }
    }

    /// Whether the scan may report a site for any trusted name. `Names`
    /// only ever holds trusted names, so this is its emptiness.
    pub(crate) fn may_bind_any_trusted(&self) -> bool {
        match self {
            Self::Any => true,
            Self::Names(names) => !names.is_empty(),
        }
    }
}

/// [`MacroBindingCandidates`] of a parser-clean file. It mirrors every site
/// [`macro_binding_ambiguities`] can record and widens each context check:
/// every macro call's arguments count (a trusted call is skipped only when
/// its name is in the requested subset), every `macro_use` counts as
/// unresolved (so a crate root with `#[macro_use] mod m;` is never skipped)
/// and every foreign-looking glob as foreign.
pub(crate) fn macro_binding_candidates(source: &ast::SourceFile) -> MacroBindingCandidates {
    let trusted = |text: &str| {
        let name = text.trim_start_matches("r#");
        (TRUSTED_MACRO_NAMES.contains(&name) || name == "json").then(|| name.to_string())
    };
    let binds_any = |text: &str| {
        matches!(
            text.trim_start_matches("r#"),
            "macro_use" | "no_implicit_prelude"
        )
    };
    let mut names = BTreeSet::new();
    for node in source.syntax().descendants() {
        if json_namespace_rebound(&node) {
            names.insert("json".to_string());
        }
        if let Some(name) = ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()))
        {
            names.extend(trusted(name.text()));
        }
        let tokens = || {
            node.descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
        };
        if ast::Attr::can_cast(node.kind()) && tokens().any(|token| binds_any(token.text())) {
            return MacroBindingCandidates::Any;
        }
        if let Some(tree) = ast::MacroCall::cast(node.clone()).and_then(|call| call.token_tree()) {
            for token in tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
            {
                if binds_any(token.text()) {
                    return MacroBindingCandidates::Any;
                }
                names.extend(trusted(token.text()));
            }
        }
        if let Some(import) = ast::Use::cast(node) {
            let Some(tree) = import.use_tree() else {
                return MacroBindingCandidates::Any;
            };
            let root = tree
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let root = root
                .trim_start_matches("::")
                .split("::")
                .next()
                .unwrap_or("")
                .trim();
            for item in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
                if item.star_token().is_some() && !matches!(root, "crate" | "self" | "super") {
                    return MacroBindingCandidates::Any;
                }
                // As the scan reads it: `as _` binds no name.
                if let Some(rename) = item.rename() {
                    if let Some(name) = rename.name() {
                        names.extend(trusted(name.text()));
                    }
                } else if item.use_tree_list().is_none()
                    && let Some(name) = item
                        .path()
                        .and_then(|path| path.segment())
                        .and_then(|segment| segment.name_ref())
                {
                    names.extend(trusted(name.text()));
                }
            }
        }
    }
    MacroBindingCandidates::Names(names)
}

// [`macro_binding_candidates`] is a stored over-approximation of this scan:
// a new site kind, or a wider existing one, must widen it too and bump
// `FILE_FACT_CACHE_SCHEMA_VERSION`, or cached candidates hide the new sites.
fn macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    allowed_empty: &BTreeSet<String>,
    module_resolved: &dyn Fn(usize, &str) -> bool,
    drop_in_verified: &dyn Fn(&str) -> bool,
) -> Vec<(String, MacroBindingSite)> {
    let mut ambiguous = Vec::new();
    if !(trusted.contains(&"json") && source.contains("serde_json")
        || source.contains("macro")
        || source.contains("use")
        || source.contains("no_implicit_prelude")
        || source.contains('!'))
    {
        return ambiguous;
    }
    let all = |line: usize, kind: MacroBindingKind, crate_local: bool| {
        trusted
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    MacroBindingSite {
                        line,
                        kind: kind.clone(),
                        scope: None,
                        crate_local,
                    },
                )
            })
            .collect::<Vec<_>>()
    };
    let Some(parse) = parse_clean_source_file(source) else {
        return all(0, MacroBindingKind::Unparsed, false);
    };
    // Built only when a binding site is found, which most files lack.
    let lines = std::cell::OnceCell::new();
    let line_of = |node: &SyntaxNode| {
        lines
            .get_or_init(|| LineIndex::new(source))
            .line(node.text_range().start())
    };
    let scope_of = |item: Option<SyntaxNode>| {
        item.map(|item| {
            let range = item.text_range();
            let lines = lines.get_or_init(|| LineIndex::new(source));
            (
                lines.line(range.start()),
                lines.line_for_range_end(range.end()),
            )
        })
    };
    // A workspace package, a module of this file, or an alias (`use x as ..`,
    // `extern crate x as ..`) that takes a drop-in crate's name can export a
    // different `assert_eq!` under that path. A leading `::` names an extern
    // crate, which no module can shadow; an `extern crate` alias still can.
    let drop_in_shadowed = |root: &str, external: bool| {
        let named = |name: ast::Name| name.text().trim_start_matches("r#") == root;
        packages
            .iter()
            .any(|package| package.replace('-', "_") == root)
            || parse.tree().syntax().descendants().any(|node| {
                ast::Rename::cast(node.clone())
                    .and_then(|rename| rename.name())
                    .is_some_and(named)
                    || !external
                        && ast::Module::cast(node)
                            .and_then(|module| module.name())
                            .is_some_and(named)
            })
    };
    for node in parse.tree().syntax().descendants() {
        // An extern-crate alias or module in another file can defeat this
        // import's root even when that file never mentions `json` itself.
        if trusted.contains(&"json") && json_namespace_rebound(&node) {
            ambiguous.push((
                "json".to_string(),
                MacroBindingSite {
                    line: line_of(&node),
                    kind: MacroBindingKind::Import,
                    scope: None,
                    crate_local: false,
                },
            ));
        }
        let definition = ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        if let Some(name) = definition {
            let name = name.text().to_string();
            let name = name.trim_start_matches("r#");
            let admitted_declaration = allowed_empty.contains(name)
                && ast::MacroRules::cast(node.clone()).is_some_and(|item| empty_catch_all(&item));
            if trusted.contains(&name) && !admitted_declaration {
                let line = line_of(&node);
                let scope = scope_of(textual_scope(&node));
                ambiguous.push((
                    name.to_string(),
                    MacroBindingSite {
                        line,
                        kind: MacroBindingKind::Definition,
                        scope,
                        crate_local: !is_exported_macro(&node),
                    },
                ));
            }
        }
        if let Some(attr) = ast::Attr::cast(node.clone()) {
            let words: Vec<_> = attr
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .collect();
            if words
                .iter()
                .any(|token| token.text().trim_start_matches("r#") == "no_implicit_prelude")
            {
                ambiguous.extend(all(
                    line_of(&node),
                    MacroBindingKind::NoImplicitPrelude,
                    true,
                ));
                continue;
            }
            if words
                .iter()
                .any(|token| token.text().trim_start_matches("r#") == "macro_use")
            {
                let module = attr.syntax().parent().and_then(ast::Module::cast);
                let resolved = module.as_ref().is_some_and(|module| {
                    module.item_list().is_some()
                        || module
                            .mod_token()
                            .zip(module.name())
                            .is_some_and(|(token, name)| {
                                let line = lines
                                    .get_or_init(|| LineIndex::new(source))
                                    .line(token.text_range().start());
                                module_resolved(line, &format!("mod {};", name.text()))
                            })
                });
                if !resolved {
                    let item = attr
                        .syntax()
                        .parent()
                        .map(|parent| item_head(&parent))
                        .unwrap_or_default();
                    let line = attr
                        .syntax()
                        .parent()
                        .map_or_else(|| line_of(&node), |parent| item_line(&parent, source));
                    // `#[macro_use] extern crate` only imports into this crate. An
                    // unresolved `#[macro_use] mod` may `#[macro_export]` its macros,
                    // which other crates reach through a glob, so it stays
                    // workspace-wide.
                    let extern_crate = attr
                        .syntax()
                        .parent()
                        .is_some_and(|parent| ast::ExternCrate::can_cast(parent.kind()));
                    ambiguous.extend(all(line, MacroBindingKind::MacroUse(item), extern_crate));
                    continue;
                }
            }
        }
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && let Some(path) = call.path()
            && !is_trusted_macro(&path.syntax().text().to_string(), trusted)
            && let Some(tree) = call.token_tree()
        {
            let tokens: Vec<_> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .collect();
            // An attribute in the arguments is only tokens to the parser,
            // but the expansion may apply it to an item (`wrap! {
            // #[no_implicit_prelude] mod tests; }`).
            if let Some(attribute) = tokens.iter().find_map(|token| {
                let word = token.text().trim_start_matches("r#");
                matches!(word, "macro_use" | "no_implicit_prelude").then_some(word)
            }) {
                return all(
                    line_of(&node),
                    MacroBindingKind::ArgumentAttribute {
                        wrapper: path.syntax().text().to_string(),
                        attribute: attribute.to_string(),
                    },
                    // The expansion may export what it defines.
                    false,
                );
            }
            // Every mention counts, a plain `assert_eq!(..)` included: to the
            // macro it is only tokens, and `define!(assert_eq!(mod tests;))`
            // can emit `macro_rules! assert_eq` together with the module whose
            // tests then use it. The one exception is a wrapper whose only
            // definition this file shows and whose expansion cannot bind a
            // name at all (#6648).
            if binding_free_local_wrapper(parse.tree().syntax(), &call, &tokens, trusted) {
                continue;
            }
            for token in &tokens {
                let name = token.text().trim_start_matches("r#");
                if trusted.contains(&name) {
                    let line = line_of(&node);
                    ambiguous.push((
                        name.to_string(),
                        MacroBindingSite {
                            line,
                            kind: MacroBindingKind::MacroArgument(path.syntax().text().to_string()),
                            scope: None,
                            crate_local: false,
                        },
                    ));
                }
            }
        }
        if let Some(import) = ast::Use::cast(node.clone()) {
            let Some(tree) = import.use_tree() else {
                return all(line_of(&node), MacroBindingKind::Unparsed, false);
            };
            let private = import.visibility().is_none();
            let root = tree
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let external = root.trim_start().starts_with("::");
            let root = root
                .trim_start_matches("::")
                .split("::")
                .next()
                .unwrap_or("")
                .trim();
            let own = matches!(root, "crate" | "self" | "super")
                || packages
                    .iter()
                    .any(|package| package.replace('-', "_") == root);
            for item in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
                if item.star_token().is_some() && !own {
                    let path = import.syntax().text().to_string();
                    let line = line_of(&node);
                    let Some(scope) = scope_of(import_scope(&import)) else {
                        ambiguous.extend(all(line, MacroBindingKind::ForeignGlob(path), private));
                        continue;
                    };
                    ambiguous.extend(trusted.iter().map(|name| {
                        (
                            (*name).to_string(),
                            MacroBindingSite {
                                line,
                                kind: MacroBindingKind::ForeignGlob(path.clone()),
                                scope: Some(scope),
                                crate_local: private,
                            },
                        )
                    }));
                    continue;
                }
                let name = if let Some(rename) = item.rename() {
                    rename.name().map(|name| name.text().to_string())
                } else if item.use_tree_list().is_none() {
                    item.path()
                        .and_then(|path| path.segment())
                        .and_then(|segment| segment.name_ref())
                        .map(|name| name.text().to_string())
                } else {
                    None
                };
                if let Some(name) = name {
                    let name = name.trim_start_matches("r#");
                    let kind = if !trusted.contains(&name) {
                        None
                    } else if !is_drop_in_assertion(&item, name) || drop_in_shadowed(root, external)
                    {
                        Some(MacroBindingKind::Import)
                    } else if !drop_in_verified(root) {
                        // Cargo can bind the crate name to another package.
                        Some(MacroBindingKind::UnverifiedDropIn(root.to_string()))
                    } else {
                        None
                    };
                    if let Some(kind) = kind {
                        let line = line_of(&node);
                        ambiguous.push((
                            name.to_string(),
                            MacroBindingSite {
                                line,
                                kind,
                                scope: scope_of(import_scope(&import)),
                                crate_local: private,
                            },
                        ));
                    }
                }
            }
        }
    }
    ambiguous
}

/// `pretty_assertions` exports `assert_eq!`/`assert_ne!` as drop-in
/// replacements that panic exactly when the standard macros do (they only
/// format the diff differently), so importing one under its own name keeps
/// the assertion's meaning. Renaming one onto another trusted name does not.
const DROP_IN_ASSERTION_CRATES: &[&str] = &["pretty_assertions"];

/// #6578: `expect_test` exports `expect!`, which only builds an `Expect`
/// value; importing it under its own name is the one binding that makes a
/// bare `expect!` expect-test's.
const EXPECT_TEST_CRATES: &[&str] = &["expect_test"];

fn is_drop_in_assertion(item: &ast::UseTree, name: &str) -> bool {
    let crates = match name {
        "assert_eq" | "assert_ne" => DROP_IN_ASSERTION_CRATES,
        "expect" => EXPECT_TEST_CRATES,
        "json" => &["serde_json"],
        _ => return false,
    };
    // Ancestor trees run inner to outer; reverse each path, then the whole.
    let mut segments: Vec<String> = item
        .syntax()
        .ancestors()
        .filter_map(ast::UseTree::cast)
        .filter_map(|tree| tree.path())
        .flat_map(|path| {
            let mut names: Vec<_> = path
                .syntax()
                .descendants()
                .filter_map(ast::NameRef::cast)
                .map(|name| name.text().to_string())
                .collect();
            names.reverse();
            names
        })
        .collect();
    segments.reverse();
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    matches!(
        segments.as_slice(),
        [root, imported] if crates.contains(root) && *imported == name
    )
}

/// Words that let tokens bind or import a name, or pull in unseen items,
/// once a macro expansion parses them as items.
const BINDING_WORDS: &[&str] = &[
    "macro_rules",
    "macro",
    "use",
    "extern",
    "mod",
    "include",
    "macro_use",
    "macro_export",
    "no_implicit_prelude",
];

/// Attributes a binding-free wrapper may carry through its tokens: built-in
/// test and lint attributes, none of which can emit an item.
const INERT_ATTRIBUTES: &[&str] = &[
    "test",
    "ignore",
    "should_panic",
    "allow",
    "warn",
    "deny",
    "expect",
    "cfg",
    "doc",
];

/// Whether `call` invokes a `macro_rules!` this file defines once, in
/// textual scope before the call, whose expansion cannot bind a macro name
/// (#6648). The definition and the call arguments hold no binding word
/// ([`BINDING_WORDS`]) and no attribute outside [`INERT_ATTRIBUTES`]; the
/// definition invokes no bang macro, and the arguments invoke only trusted
/// names by their bare name. No macro call between the definition and this
/// call may shadow it: each one is a bare trusted name or another inert
/// call of the same wrapper. A wrapper reached by path (`a::wrap!`), named
/// by a `use` in this file, defined twice, under an attribute other than a
/// doc comment, inside a function or block, or used before or outside its
/// definition's scope fails closed: it may be a macro ripr cannot see.
/// Attribute proc macros stay invisible here as everywhere else.
fn binding_free_local_wrapper(
    root: &SyntaxNode,
    call: &ast::MacroCall,
    argument_tokens: &[ra_ap_syntax::SyntaxToken],
    trusted: &[&str],
) -> bool {
    let Some(name) = bare_macro_name(call) else {
        return false;
    };
    if !inert_tokens(argument_tokens, trusted, false) {
        return false;
    }
    let named = |candidate: Option<ast::Name>| {
        candidate.is_some_and(|candidate| candidate.text().trim_start_matches("r#") == name)
    };
    let imported = root
        .descendants()
        .filter_map(ast::UseTree::cast)
        .any(|tree| {
            tree.rename().is_some_and(|rename| named(rename.name()))
                || tree
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|segment| segment.text().trim_start_matches("r#") == name)
        });
    if imported {
        return false;
    }
    let mut definitions = root
        .descendants()
        .filter_map(ast::MacroRules::cast)
        .filter(|rules| named(rules.name()));
    let (Some(definition), None) = (definitions.next(), definitions.next()) else {
        return false;
    };
    if definition.attrs().next().is_some() {
        return false;
    }
    let definition_end = definition.syntax().text_range().end();
    let call_start = call.syntax().text_range().start();
    if definition_end > call_start {
        return false;
    }
    // A file-level definition reaches the rest of the file. One in an
    // inline module reaches the call only when `textual_scope` can bound
    // it and the call sits inside; `None` there means the scope cannot be
    // bounded, not that it covers everything. Function and block bodies
    // fail closed.
    let in_scope = match definition.syntax().parent().map(|parent| parent.kind()) {
        Some(ra_ap_syntax::SyntaxKind::SOURCE_FILE) => true,
        Some(ra_ap_syntax::SyntaxKind::ITEM_LIST) => textual_scope(definition.syntax())
            .is_some_and(|scope| scope.text_range().contains(call_start)),
        _ => false,
    };
    if !in_scope {
        return false;
    }
    let Some(body) = definition.token_tree() else {
        return false;
    };
    let body_tokens = non_trivia_tokens(body.syntax());
    if !inert_tokens(&body_tokens, trusted, true) {
        return false;
    }
    // A macro expanded between the definition and the call could emit a
    // second `macro_rules!` of the same name, which this file never shows
    // but which shadows the first at the call. Trusted macros only build
    // expressions, so anything they could expand to stays block-local.
    root.descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|other| {
            let start = other.syntax().text_range().start();
            start >= definition_end && start < call_start
        })
        .all(|other| match bare_macro_name(&other) {
            Some(other_name) if trusted.contains(&other_name.as_str()) => true,
            Some(other_name) if other_name == name => other.token_tree().is_some_and(|tree| {
                inert_tokens(&non_trivia_tokens(tree.syntax()), trusted, false)
            }),
            _ => false,
        })
}

/// The name of a macro call reached by a bare, unqualified path.
fn bare_macro_name(call: &ast::MacroCall) -> Option<String> {
    let path = call.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    path.segment()
        .and_then(|segment| segment.name_ref())
        .map(|name| name.text().trim_start_matches("r#").to_string())
}

fn non_trivia_tokens(node: &SyntaxNode) -> Vec<ra_ap_syntax::SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect()
}

/// Whether macro tokens can bind nothing once expanded: no binding word, no
/// attribute outside [`INERT_ATTRIBUTES`], and no bang macro other than a
/// bare trusted name (none at all in a definition body, whose `$name!`
/// could be anything).
fn inert_tokens(tokens: &[ra_ap_syntax::SyntaxToken], trusted: &[&str], definition: bool) -> bool {
    use ra_ap_syntax::SyntaxKind::{BANG, COLON, COLON2, IDENT, L_BRACK, POUND};
    let word =
        |token: &ra_ap_syntax::SyntaxToken| token.text().trim_start_matches("r#").to_string();
    tokens.iter().enumerate().all(|(index, token)| {
        if BINDING_WORDS.contains(&word(token).as_str()) {
            return false;
        }
        match token.kind() {
            BANG => {
                let previous = index.checked_sub(1).and_then(|before| tokens.get(before));
                let Some(previous) = previous.filter(|previous| previous.kind() == IDENT) else {
                    // A unary `!` follows an operator, a keyword or an
                    // opening delimiter, never a name.
                    return true;
                };
                let qualified = index
                    .checked_sub(2)
                    .and_then(|before| tokens.get(before))
                    .is_some_and(|before| matches!(before.kind(), COLON | COLON2));
                !definition && !qualified && trusted.contains(&word(previous).as_str())
            }
            POUND => {
                tokens
                    .get(index + 1)
                    .is_some_and(|open| open.kind() == L_BRACK)
                    && tokens.get(index + 2).is_some_and(|attribute| {
                        attribute.kind() == IDENT
                            && INERT_ATTRIBUTES.contains(&word(attribute).as_str())
                    })
                    && tokens
                        .get(index + 3)
                        .is_some_and(|next| !matches!(next.kind(), COLON | COLON2))
            }
            _ => true,
        }
    })
}

/// The inline module or function body that bounds a `macro_rules!`
/// definition's textual scope, when nothing can carry it further: no
/// `#[macro_use]` on that module or any enclosing one in the file, and no
/// out-of-line `mod name;` inside it (whose file would inherit the scope).
/// `None` for a file-level definition or a `macro` 2.0 item.
fn textual_scope(definition: &SyntaxNode) -> Option<SyntaxNode> {
    ast::MacroRules::cast(definition.clone())?;
    // `#[macro_export]` (also under `cfg_attr`) puts the macro at crate-root
    // path scope, so a bare `assert_eq!` anywhere in the crate root resolves
    // to it whatever item encloses the definition.
    if is_exported_macro(definition) {
        return None;
    }
    let scope = definition.ancestors().skip(1).find(|node| {
        ast::Fn::can_cast(node.kind())
            || ast::Module::cast(node.clone()).is_some_and(|module| module.item_list().is_some())
    })?;
    let carries_out = |module: &ast::Module| {
        module.attrs().any(|attr| {
            attr.syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| token.text().trim_start_matches("r#") == "macro_use")
        })
    };
    if definition
        .ancestors()
        .filter_map(ast::Module::cast)
        .any(|module| carries_out(&module))
    {
        return None;
    }
    if scope
        .descendants()
        .filter_map(ast::Module::cast)
        .any(|module| module.item_list().is_none())
    {
        return None;
    }
    Some(scope)
}

/// The inline module or block that bounds a private `use`: the import is
/// visible there and in its descendants only. `None` (workspace-wide) for a
/// file-level or `pub`-visible import, and when the scope holds an
/// out-of-line `mod name;` whose file could reach it through `super`.
fn import_scope(import: &ast::Use) -> Option<SyntaxNode> {
    if import.visibility().is_some() {
        return None;
    }
    let scope = import.syntax().ancestors().skip(1).find(|node| {
        ast::BlockExpr::can_cast(node.kind())
            || ast::Module::cast(node.clone()).is_some_and(|module| module.item_list().is_some())
    })?;
    if scope
        .descendants()
        .filter_map(ast::Module::cast)
        .any(|module| module.item_list().is_none())
    {
        return None;
    }
    Some(scope)
}

/// Whether a macro definition can be named from another crate:
/// `#[macro_export]` (raw or under `cfg_attr`) on `macro_rules!`, or a
/// visibility on a `macro` 2.0 item.
fn is_exported_macro(definition: &SyntaxNode) -> bool {
    if let Some(rules) = ast::MacroRules::cast(definition.clone()) {
        return rules.attrs().any(|attr| {
            attr.syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| token.text().trim_start_matches("r#") == "macro_export")
        });
    }
    ast::MacroDef::cast(definition.clone()).is_none_or(|item| item.visibility().is_some())
}

/// The item's text without attributes, doc comments or body: `mod name;` or
/// `extern crate name;` for the `#[macro_use]` sites this names.
fn item_head(item: &SyntaxNode) -> String {
    let text: String = item
        .children_with_tokens()
        .filter(|element| {
            !matches!(
                element.kind(),
                ra_ap_syntax::SyntaxKind::ATTR | ra_ap_syntax::SyntaxKind::COMMENT
            )
        })
        .map(|element| element.to_string())
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.split('{').next().unwrap_or(&text).trim().to_string()
}

/// Line of the item's first non-attribute token (the `mod`/`extern`
/// keyword), so a multi-attribute item points at the declaration.
fn item_line(item: &SyntaxNode, source: &str) -> usize {
    let offset = item
        .children_with_tokens()
        .find(|element| {
            !matches!(
                element.kind(),
                ra_ap_syntax::SyntaxKind::ATTR | ra_ap_syntax::SyntaxKind::COMMENT
            ) && !element.kind().is_trivia()
        })
        .map_or_else(
            || item.text_range().start(),
            |element| element.text_range().start(),
        );
    LineIndex::new(source).line(offset)
}

/// This recognizes one bounded syntax form, not a macro evaluator: a sole
/// `($($name:tt)*) => {}` rule consumes any invocation and emits no tokens.
/// Other matchers, arms, attributes and nonempty transcribers stay opaque.
fn empty_catch_all(item: &ast::MacroRules) -> bool {
    if item.attrs().next().is_some() {
        return false;
    }
    let Some(tree) = item.token_tree() else {
        return false;
    };
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let mut text: Vec<_> = tokens.iter().map(|token| token.text()).collect();
    if text.len() == 17 && text.get(15) == Some(&";") {
        text.remove(15);
    }
    // Token-tree parsing retains `=` and `>` as separate punctuation tokens;
    // unlike expression grammar it does not combine them into FAT_ARROW.
    text.len() == 16
        && tokens
            .get(5)
            .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::IDENT)
        && text[..5] == ["{", "(", "$", "(", "$"]
        && text[6..] == [":", "tt", ")", "*", ")", "=", ">", "{", "}", "}"]
}

fn local_empty_macros(root: &SyntaxNode) -> BTreeMap<String, ast::MacroRules> {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut candidates = BTreeMap::new();
    for node in root.descendants() {
        let rule = ast::MacroRules::cast(node.clone());
        let name = rule
            .as_ref()
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        let Some(name) = name else {
            continue;
        };
        let name = name.text().to_string();
        *counts.entry(name.clone()).or_default() += 1;
        if !name.starts_with("r#")
            && let Some(rule) = rule
            && empty_catch_all(&rule)
            && rule.syntax().parent().is_some_and(|parent| {
                ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
            })
        {
            candidates.insert(name, rule);
        }
    }
    candidates.retain(|name, _| counts.get(name) == Some(&1));
    candidates
}

/// Whether every `return` in `item` (one function's text) leaves that
/// function: the text parses cleanly and no `return` sits inside a closure,
/// an `async` or `const` block, or a nested `fn` item, where it would end
/// only that inner body.
pub(crate) fn returns_leave_the_function(item: &str) -> bool {
    parse_clean_source_file(item).is_some_and(|parse| {
        parse
            .tree()
            .syntax()
            .descendants()
            .filter(|node| ast::ReturnExpr::can_cast(node.kind()))
            .all(|node| {
                node.ancestors()
                    .filter(|parent| ast::Fn::can_cast(parent.kind()))
                    .count()
                    <= 1
                    && !node.ancestors().any(|parent| {
                        ast::ClosureExpr::can_cast(parent.kind())
                            || ast::BlockExpr::cast(parent).is_some_and(|block| {
                                block.async_token().is_some() || block.const_token().is_some()
                            })
                    })
            })
    })
}

pub(crate) fn local_empty_macro_names(source: &str) -> BTreeSet<String> {
    parse_clean_source_file(source)
        .map(|parse| {
            local_empty_macros(parse.tree().syntax())
                .into_keys()
                .collect()
        })
        .unwrap_or_default()
}

fn resolves_empty_local(call: &ast::MacroCall, empty: &BTreeMap<String, ast::MacroRules>) -> bool {
    let Some(path) = call.path() else {
        return false;
    };
    let Some(definition) = empty.get(&path.syntax().text().to_string()) else {
        return false;
    };
    let Some(scope) = definition.syntax().parent() else {
        return false;
    };
    definition.syntax().text_range().end() <= call.syntax().text_range().start()
        && call.syntax().ancestors().any(|ancestor| ancestor == scope)
}

/// Discarded call-argument spans from the same bounded local resolver used by
/// execution admission. This only removes evidence; workspace macro ambiguity
/// still independently refuses positive assertion admission.
pub(crate) fn empty_local_macro_invocation_ranges(
    root: &SyntaxNode,
) -> Vec<std::ops::Range<usize>> {
    let empty = local_empty_macros(root);
    if empty.is_empty() {
        return Vec::new();
    }
    root.descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| resolves_empty_local(call, &empty))
        .map(|call| {
            let range = call.syntax().text_range();
            u32::from(range.start()) as usize..u32::from(range.end()) as usize
        })
        .collect()
}

pub(crate) fn owner_pin_assertions(source: &str, trusted: &[&str]) -> OwnerPinAssertions {
    let mut result = OwnerPinAssertions::default();
    let Some(parse) = parse_clean_source_file(source) else {
        return result;
    };
    result.parsed = true;
    let lines = LineIndex::new(source);
    let empty_macros = local_empty_macros(parse.tree().syntax());
    // A guard twin's discrimination assumes its `return Err(..)` is the
    // prelude variant: only then does a changed owner fail the test. A file
    // that binds the value name `Err` anywhere may route that return to an
    // `Ok`, so its guards are never twin candidates (#7130 review).
    let err_bound = file_binds_value_name(parse.tree().syntax(), "Err");
    for module in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
    {
        if module.item_list().is_some() {
            continue;
        }
        let (Some(token), Some(name)) = (module.mod_token(), module.name()) else {
            continue;
        };
        let key = (
            lines.line(token.text_range().start()),
            format!("mod {};", name.text()),
        );
        let admitted = ModuleDeclarationAdmission {
            context: supported_item_context(module.syntax()),
            source_preserved: supported_targeted_item_context(module.syntax()),
        };
        result
            .module_declarations
            .entry(key)
            .and_modify(|previous| *previous = ModuleDeclarationAdmission::default())
            .or_insert(admitted);
    }
    let mut identities = BTreeMap::<FunctionKey, usize>::new();
    let mut fn_names = BTreeMap::<String, Vec<ast::Fn>>::new();
    for function in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
    {
        if let Some(name) = function.name() {
            fn_names
                .entry(name.text().to_string())
                .or_default()
                .push(function);
        }
    }
    for function in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
    {
        let (Some(name), Some(token), Some(body)) =
            (function.name(), function.fn_token(), function.body())
        else {
            continue;
        };
        let key = (
            lines.line(token.text_range().start()),
            lines.line_for_range_end(function.syntax().text_range().end()),
            name.text().to_string(),
        );
        *identities.entry(key.clone()).or_default() += 1;
        // An outcome-settling attribute is named first wherever it sits: it
        // is evidence of a gap, which RIPR-SPEC-0240 must not withhold.
        let refusal = if let Some(attr) = function
            .attrs()
            .find(|attr| attribute_settles_test_outcome(&attr.syntax().text().to_string()))
        {
            Some(AssertionContextRefusal::TestAttribute(
                attr.syntax().text().to_string(),
            ))
        } else if function.async_token().is_some()
            && !function
                .attrs()
                .any(|attr| tokio_test_runs_body(&attr, function.syntax()))
        {
            Some(AssertionContextRefusal::AsyncTest)
        } else if let Some(refusal) = has_escape(body.syntax(), trusted, &empty_macros) {
            Some(refusal)
        } else if let Some(attr) = function
            .attrs()
            .find(|attr| !runs_body_unchanged(attr, function.syntax()))
        {
            Some(AssertionContextRefusal::TestAttribute(
                attr.syntax().text().to_string(),
            ))
        } else {
            item_context_refusal(function.syntax())
        };
        if let Some(refusal) = refusal {
            // Only test functions are queried (`#[test]`, `#[tokio::test]`);
            // a refused helper still counts toward duplicate detection.
            if is_test_function(&function) {
                insert_function(
                    &mut result.functions,
                    key,
                    FunctionAssertions {
                        refusal: Some(refusal),
                        ..FunctionAssertions::default()
                    },
                );
            }
            continue;
        }
        let body_source = slice_text(
            source,
            token.text_range().start(),
            function.syntax().text_range().end(),
        );
        let mut candidates = MacroCandidates::new();
        for call in function
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            // `assert!` is collected for the bool-owner pin; the classify
            // side decides which spelling a pin may use.
            if call.path().is_none_or(|path| {
                !names_candidate_assertion(
                    &path.syntax().text().to_string(),
                    &["assert_eq", "assert"],
                )
            }) {
                continue;
            }
            let range = call.syntax().text_range();
            let assertion = (
                lines.line(range.start()),
                slice_macro_call_text(source, range.start(), range.end()),
            );
            candidates
                .entry(assertion)
                .or_default()
                .push(call.syntax().clone());
        }
        // #6578: an inline expect-test comparison
        // (`expect![["alice: 42"]].assert_eq(&render_row("alice", 41))`) is
        // a method call on the bare `expect!` value; the classify side
        // decides which spelling pins.
        for call in function
            .syntax()
            .descendants()
            .filter_map(ast::MethodCallExpr::cast)
        {
            if !is_expect_test_comparison(&call)
                || expect_test_receiver(&call).as_deref() != Some("expect")
                || call
                    .name_ref()
                    .is_none_or(|name| name.text() != "assert_eq")
            {
                continue;
            }
            let range = call.syntax().text_range();
            let assertion = (
                lines.line(range.start()),
                slice_macro_call_text(source, range.start(), range.end()),
            );
            candidates
                .entry(assertion)
                .or_default()
                .push(call.syntax().clone());
        }
        // RIPR-SPEC-0197: a terminal Err-return guard is the assertion twin
        // of its negated condition (RIPR-SPEC-0154), keyed by its `if` line
        // and condition as [`err_return_guard_key`] spells it.
        if !err_bound {
            for guard in function
                .syntax()
                .descendants()
                .filter_map(ast::IfExpr::cast)
            {
                let Some(condition) = terminal_err_return_guard_condition(&guard) else {
                    continue;
                };
                let Some(if_token) = guard.if_token() else {
                    continue;
                };
                let key = (
                    lines.line(if_token.text_range().start()),
                    err_return_guard_key(&condition.syntax().text().to_string()),
                );
                candidates
                    .entry(key)
                    .or_default()
                    .push(guard.syntax().clone());
            }
        }
        let macros = invoked_macro_names(function.syntax());
        // Compute the conservative prefix boundary once per function, rather
        // than rescanning its body for every candidate assertion/invocation.
        // A nested helper or async block has its own return context.
        // The earlier escape gate still refuses returns in any closure.
        let first_return = body
            .syntax()
            .descendants()
            .filter_map(ast::ReturnExpr::cast)
            .filter(|expression| {
                expression
                    .syntax()
                    .ancestors()
                    .find(|node| {
                        ast::Fn::can_cast(node.kind())
                            || ast::BlockExpr::cast(node.clone())
                                .is_some_and(|block| block.async_token().is_some())
                    })
                    .is_some_and(|owner| owner == *function.syntax())
            })
            .map(|expression| expression.syntax().text_range().start())
            .min();
        let assertions = admitted_assertions(candidates, &function, first_return);
        let helper_assertions = if is_test_function(&function) {
            local_helper_assertions(
                &function,
                first_return,
                &fn_names,
                (source, &lines),
                trusted,
                &empty_macros,
            )
        } else {
            BTreeMap::new()
        };
        insert_function(
            &mut result.functions,
            key,
            FunctionAssertions {
                body: body_source,
                refusal: None,
                assertions,
                macros,
                helper_assertions,
            },
        );
    }
    result
        .functions
        .retain(|key, _| identities.get(key) == Some(&1));
    result
}

/// `#[test]`, `#[tokio::test]` and other attributes whose last segment is
/// `test`: the functions the admission is queried for.
fn is_test_function(function: &ast::Fn) -> bool {
    function.attrs().any(|attr| {
        attr.path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text() == "test" || name.text() == "rstest")
    })
}

type MacroCandidates = BTreeMap<AssertionKey, Vec<SyntaxNode>>;

/// Each candidate's admission: unique at its coordinate and on an eager
/// statement path of `function`.
fn admitted_assertions(
    candidates: MacroCandidates,
    function: &ast::Fn,
    first_return: Option<TextSize>,
) -> BTreeMap<AssertionKey, Result<(), AssertionContextRefusal>> {
    candidates
        .into_iter()
        .map(|(key, calls)| {
            // OracleFact has line/text, not an offset. No identical spelling
            // on the same line may borrow another invocation's context.
            let admitted = if calls.len() == 1 {
                eager_path(calls[0].clone(), function, false, first_return).map_err(|construct| {
                    match (construct == EARLIER_RETURN)
                        .then(|| sole_return_guard(function))
                        .flatten()
                    {
                        Some(condition) => AssertionContextRefusal::GuardedReturn(condition),
                        None => AssertionContextRefusal::ConditionalPath(construct),
                    }
                })
            } else {
                Err(AssertionContextRefusal::DuplicateSpelling)
            };
            (key, admitted)
        })
        .collect()
}

/// Assertions a test borrows from test-local check helpers (#6482):
///
/// ```text
/// fn check_tip(bill: u64, tip: u64, want: u64) { assert_eq!(with_tip(bill, tip), want); }
/// #[test]
/// fn tip_is_added() { check_tip(40, 6, 46); }
/// ```
///
/// The same execution argument as for the test's own assertions, applied
/// across exactly one call: the test calls the helper as a bare
/// single-segment function on its own eager statement path, the call can
/// only name that helper, and the `assert_eq!` sits on the helper's eager
/// path. Every condition fails closed:
///
/// - the name is defined by exactly one `fn` anywhere in the file, and that
///   `fn` is a direct item of the test's own module (a module item cannot
///   coexist with a same-named import and wins over a glob);
/// - the test binds or imports nothing of that name: no pattern, parameter,
///   nested item or `use` item anywhere in its body;
/// - the helper is a plain function: no attributes other than
///   `#[track_caller]`, no generics or `where` clause, not `async`, `const`
///   or `unsafe`, no `self` parameter, and no `return` or `?`
///   in its body, so the assertion cannot be skipped by an early exit;
/// - the helper passes the test's own escape gate (only trusted macros, no
///   closure exits or `yield`) and its supported item context.
///
/// A call inside a loop, branch, argument, deferred closure or macro
/// operand is not eager, so it lends nothing.
fn local_helper_assertions(
    test: &ast::Fn,
    first_return: Option<TextSize>,
    fn_names: &BTreeMap<String, Vec<ast::Fn>>,
    (source, lines): (&str, &LineIndex),
    trusted: &[&str],
    empty_macros: &BTreeMap<String, ast::MacroRules>,
) -> BTreeMap<AssertionKey, BorrowedAssertion> {
    let mut borrowed = BTreeMap::new();
    let Some(module) = test.syntax().parent() else {
        return borrowed;
    };
    // A `use` anywhere in the test may import a same-named function over
    // the module's helper.
    if test
        .syntax()
        .descendants()
        .any(|node| ast::Use::can_cast(node.kind()))
    {
        return borrowed;
    }
    let bound: BTreeSet<String> = test
        .syntax()
        .descendants()
        .filter_map(ast::Name::cast)
        .map(|name| name.text().to_string())
        .collect();
    let mut called = BTreeMap::<String, Vec<usize>>::new();
    let mut calls_per_line = BTreeMap::<usize, usize>::new();
    // Method calls and macro invocations count too: a macro's operand is
    // an unparsed token tree that may hold another call.
    for node in test.syntax().descendants().filter(|node| {
        ast::CallExpr::can_cast(node.kind())
            || ast::MethodCallExpr::can_cast(node.kind())
            || ast::MacroCall::can_cast(node.kind())
    }) {
        *calls_per_line
            .entry(lines.line(node.text_range().start()))
            .or_default() += 1;
    }
    for call in test.syntax().descendants().filter_map(ast::CallExpr::cast) {
        let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
            continue;
        };
        let Some(path) = callee.path() else {
            continue;
        };
        let Some(segment) = path.segment() else {
            continue;
        };
        let Some(ast::PathSegmentKind::Name(name)) = segment.kind() else {
            continue;
        };
        if path.qualifier().is_some() || segment.generic_arg_list().is_some() {
            continue;
        }
        let name = name.text().to_string();
        if !bound.contains(&name)
            && eager_path(call.syntax().clone(), test, false, first_return).is_ok()
        {
            let line = lines.line(call.syntax().text_range().start());
            let lines_of_name = called.entry(name).or_default();
            // A line holding another call cannot say which call an
            // activation fact on it describes.
            if calls_per_line.get(&line) == Some(&1)
                && lines.line(call.syntax().text_range().end()) == line
            {
                lines_of_name.push(line);
            }
        }
    }
    for (name, call_lines) in called {
        let Some([helper]) = fn_names.get(&name).map(Vec::as_slice) else {
            continue;
        };
        if helper.syntax() == test.syntax()
            || helper.syntax().parent().as_ref() != Some(&module)
            || !plain_check_helper(helper, trusted, empty_macros)
        {
            continue;
        }
        let Some(token) = helper.fn_token() else {
            continue;
        };
        let macros = invoked_macro_names(helper.syntax());
        let text = slice_text(
            source,
            token.text_range().start(),
            helper.syntax().text_range().end(),
        );
        let mut candidates = MacroCandidates::new();
        for call in helper
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            if call.path().is_none_or(|path| {
                !names_candidate_assertion(&path.syntax().text().to_string(), &["assert_eq"])
            }) {
                continue;
            }
            let range = call.syntax().text_range();
            candidates
                .entry((
                    lines.line(range.start()),
                    slice_macro_call_text(source, range.start(), range.end()),
                ))
                .or_default()
                .push(call.syntax().clone());
        }
        let loan = HelperLoan {
            name: name.clone(),
            helper: text,
            parameters: plain_parameter_names(helper),
            call_lines,
        };
        for (key, admitted) in admitted_assertions(candidates, helper, None) {
            if admitted.is_ok() {
                borrowed.insert(
                    key,
                    BorrowedAssertion {
                        macros: macros.clone(),
                        loan: loan.clone(),
                    },
                );
            }
        }
    }
    borrowed
}

/// `helper`'s parameter names in order, or none when any parameter is not
/// a plain identifier pattern (`(a, b): (u32, u32)`, `mut x`, `ref x`) or a
/// name repeats.
fn plain_parameter_names(helper: &ast::Fn) -> Vec<String> {
    let Some(params) = helper.param_list() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for param in params.params() {
        let Some(ast::Pat::IdentPat(pattern)) = param.pat() else {
            return Vec::new();
        };
        if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some()
        {
            return Vec::new();
        }
        let Some(name) = pattern.name() else {
            return Vec::new();
        };
        let name = name.text().to_string();
        if names.contains(&name) {
            return Vec::new();
        }
        names.push(name);
    }
    // A `let`, pattern, closure parameter or nested item of the same name
    // in the body may rebind a parameter before the assertion reads it.
    let rebound = helper.body().is_some_and(|body| {
        body.syntax()
            .descendants()
            .filter_map(ast::Name::cast)
            .any(|name| names.iter().any(|param| name.text() == param.as_str()))
    });
    if rebound { Vec::new() } else { names }
}

/// A synchronous helper with no early exit: see [`local_helper_assertions`].
/// Its return type does not swallow a panic from the borrowed `assert_eq!`,
/// even if the caller discards a returned `Result`. Fallible macros and `?`
/// remain refused; their errors could otherwise skip the assertion unnoticed.
fn plain_check_helper(
    helper: &ast::Fn,
    trusted: &[&str],
    empty_macros: &BTreeMap<String, ast::MacroRules>,
) -> bool {
    let Some(body) = helper.body() else {
        return false;
    };
    helper
        .attrs()
        .all(|attr| attr.simple_name().as_deref() == Some("track_caller"))
        && helper.generic_param_list().is_none()
        && helper.where_clause().is_none()
        && helper.async_token().is_none()
        && helper.const_token().is_none()
        && helper.unsafe_token().is_none()
        && helper
            .param_list()
            .is_some_and(|params| params.self_param().is_none())
        && !body.syntax().descendants().any(|node| {
            ast::ReturnExpr::can_cast(node.kind()) || ast::TryExpr::can_cast(node.kind())
        })
        && has_escape(body.syntax(), trusted, empty_macros).is_none()
        && supported_item_context(helper.syntax())
}

/// The assertion key of a terminal Err-return guard: its condition with
/// whitespace removed, so the parsed guard and the scanned oracle text
/// (which may join continuation lines) name the same guard.
pub(crate) fn err_return_guard_key(condition: &str) -> String {
    let compact: String = condition.split_whitespace().collect();
    format!("if {compact}")
}

const EARLIER_RETURN: &str = "a block that an earlier `return` can skip";

/// The condition of `if <condition> { return; }` when that `return` is the
/// only one the test body holds outside closures and nested items, the
/// `if` has no `else`, its block holds nothing else, and it is a statement
/// of the body itself.
fn sole_return_guard(function: &ast::Fn) -> Option<String> {
    let body = function.body()?;
    let mut returns = body
        .syntax()
        .descendants()
        .filter_map(ast::ReturnExpr::cast)
        .filter(|expression| {
            !expression.syntax().ancestors().any(|node| {
                ast::ClosureExpr::can_cast(node.kind())
                    || (node != *function.syntax() && ast::Fn::can_cast(node.kind()))
            })
        });
    let returned = returns.next()?;
    if returns.next().is_some() || returned.expr().is_some() {
        return None;
    }
    let only = |list: &ast::StmtList, expression: &SyntaxNode| match (
        list.statements().collect::<Vec<_>>().as_slice(),
        list.tail_expr(),
    ) {
        ([ast::Stmt::ExprStmt(statement)], None) => statement
            .expr()
            .is_some_and(|inner| inner.syntax() == expression),
        ([], Some(tail)) => tail.syntax() == expression,
        _ => false,
    };
    let block = returned
        .syntax()
        .ancestors()
        .find_map(ast::BlockExpr::cast)?;
    let guard = ast::IfExpr::cast(block.syntax().parent()?)?;
    if guard.else_branch().is_some()
        || block.modifier().is_some()
        || block.label().is_some()
        || !only(&block.stmt_list()?, returned.syntax())
    {
        return None;
    }
    let statement = ast::ExprStmt::cast(guard.syntax().parent()?)?;
    (statement.syntax().parent()? == *body.stmt_list()?.syntax())
        .then(|| guard.condition())
        .flatten()
        .map(|condition| condition.syntax().text().to_string())
}

/// The condition of `if <condition> { return Err(..); .. }` with no `else`:
/// the first statement of the body returns an `Err(..)` call. Whether the
/// condition has an assertion twin is the oracle scan's decision.
fn terminal_err_return_guard_condition(guard: &ast::IfExpr) -> Option<ast::Expr> {
    if guard.else_branch().is_some() {
        return None;
    }
    let condition = guard.condition()?;
    let body = guard.then_branch()?.stmt_list()?;
    let first = match body.statements().next() {
        Some(ast::Stmt::ExprStmt(statement)) => statement.expr()?,
        Some(_) => return None,
        None => body.tail_expr()?,
    };
    let ast::Expr::ReturnExpr(returned) = first else {
        return None;
    };
    let ast::Expr::CallExpr(call) = returned.expr()? else {
        return None;
    };
    let ast::Expr::PathExpr(callee) = call.expr()? else {
        return None;
    };
    (callee.syntax().text() == "Err").then_some(condition)
}

/// Whether the file binds the value name `name` anywhere, so an unqualified
/// call of that name could resolve to the binding instead of the prelude:
/// a `fn`, `const`, `static` or struct constructor of the name (an extern
/// block's declaration parses as a `fn`), a pattern binding or parameter
/// of the name, an explicit import of the name or into the name, or a glob
/// import the file cannot see through. Enum and type declarations live in
/// the type namespace and a `mod` in the module namespace, so none of them
/// can capture a call; a `macro_rules!` of the name needs `!` and never
/// intercepts one either.
///
/// A glob can only introduce the name behind this file's scan when it
/// reaches outside the file. One rooted at `super` inside an inline module
/// (the idiomatic `use super::*;` of a test module) re-imports exactly the
/// file-local definitions the whole-file scan already refuses, and one
/// rooted at `self` targets a module of this file; every other glob
/// (`crate::..::*`, an external crate, a `super::sub::*` that may be an
/// out-of-line module, a file-root `super::*` that reaches the parent
/// file) stays opaque and refuses. Since a glob is not re-exported without
/// `pub`, an in-file glob cannot relay an opaque glob's names inward.
///
/// Bindings of inline modules, functions and blocks do not leak outward,
/// but the answer is deliberately file-wide: it only refuses the Err-return
/// guard twin, so a coarser no never mis-credits, and the parser-backed
/// name equality keeps the whole-word rule (#7130 review: identity over
/// token coincidence).
fn file_binds_value_name(root: &SyntaxNode, name: &str) -> bool {
    file_binds_value_name_with(root, name, true)
}

/// [`file_binds_value_name`], with `opaque_globs` deciding whether a glob
/// the scan cannot see through counts as a binding.
fn file_binds_value_name_with(root: &SyntaxNode, name: &str, opaque_globs: bool) -> bool {
    let spells = |text: &str| text.trim_start_matches("r#") == name;
    // A bare `None =>` arm or `if let None = ..` parses as an identifier
    // pattern, but while the prelude variant is in scope it names that
    // variant (#7712 review). Only `#![no_implicit_prelude]` takes the
    // variant out of scope. `let` statements and parameters, where a
    // binding is the usual reading, still count.
    let prelude_variant = matches!(name, "None" | "Some" | "Ok" | "Err");
    // The attribute itself, never a comment or string that mentions it.
    if prelude_variant
        && root
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attribute| {
                attribute
                    .path()
                    .is_some_and(|path| path.syntax().text() == "no_implicit_prelude")
            })
    {
        return true;
    }
    let names_the_variant = |pattern: &ast::IdentPat| {
        prelude_variant
            && pattern
                .syntax()
                .ancestors()
                .find(|node| {
                    ast::MatchArm::can_cast(node.kind())
                        || ast::LetExpr::can_cast(node.kind())
                        || ast::LetStmt::can_cast(node.kind())
                        || ast::Param::can_cast(node.kind())
                        || ast::ClosureExpr::can_cast(node.kind())
                })
                .is_some_and(|node| {
                    ast::MatchArm::can_cast(node.kind()) || ast::LetExpr::can_cast(node.kind())
                })
    };
    root.descendants().any(|node| {
        let named = |item_name: Option<ast::Name>| {
            item_name.is_some_and(|item_name| spells(item_name.text()))
        };
        ast::Fn::cast(node.clone()).is_some_and(|item| named(item.name()))
            || ast::Const::cast(node.clone()).is_some_and(|item| named(item.name()))
            || ast::Static::cast(node.clone()).is_some_and(|item| named(item.name()))
            || ast::Struct::cast(node.clone()).is_some_and(|item| named(item.name()))
            || ast::IdentPat::cast(node.clone())
                .is_some_and(|pattern| named(pattern.name()) && !names_the_variant(&pattern))
            || ast::UseTree::cast(node.clone()).is_some_and(|tree| {
                if let Some(rename) = tree.rename() {
                    return rename.name().is_some_and(|rename| spells(rename.text()));
                }
                if tree.star_token().is_some() {
                    return opaque_globs && !glob_stays_within_the_file(&tree);
                }
                if tree.use_tree_list().is_some() {
                    // The nested trees are their own `UseTree` nodes; a
                    // prefix path here names the imported module, not a
                    // binding of the name.
                    return false;
                }
                tree.path().is_some_and(|path| {
                    path.segments().last().is_some_and(|segment| {
                        segment
                            .name_ref()
                            .is_some_and(|name_ref| spells(name_ref.text()))
                    })
                })
            })
    })
}

/// [`file_binds_value_name`] over a whole source file. A file that does not
/// parse cleanly may bind anything, so it answers yes.
pub(crate) fn source_binds_value_name(source: &str, name: &str) -> bool {
    parse_clean_source_file(source)
        .is_none_or(|parse| file_binds_value_name(parse.tree().syntax(), name))
}

/// Whether a source file binds `name` as a value by a declaration, pattern
/// or named import of its own. Unlike [`source_binds_value_name`], a glob
/// whose names the scan cannot see does not count: a test module's
/// `use super::sub::*;` is routine, and refusing on it withdrew real
/// credits (#7712 review).
pub(crate) fn source_declares_value_name(source: &str, name: &str) -> bool {
    parse_clean_source_file(source)
        .is_none_or(|parse| file_binds_value_name_with(parse.tree().syntax(), name, false))
}

/// Whether a glob import can only carry names this file's own scan covers:
/// its path must be exactly `self` (a module of this file) or exactly
/// `super` while the import sits inside at least one inline module, so
/// `super` names an enclosing module of this file. A file-root `super::*`
/// reaches the parent file, and any longer path may leave the file at its
/// first non-`super` segment.
fn glob_stays_within_the_file(glob: &ast::UseTree) -> bool {
    let Some(path) = glob.path() else {
        return false;
    };
    let segments: Vec<String> = path
        .segments()
        .filter_map(|segment| segment.name_ref().map(|name| name.text().to_string()))
        .collect();
    match segments.as_slice() {
        [single] if single == "self" => true,
        [single] if single == "super" => glob
            .syntax()
            .ancestors()
            .skip(1)
            .take_while(|node| !ast::SourceFile::can_cast(node.kind()))
            .any(|node| ast::Module::cast(node).is_some_and(|module| module.item_list().is_some())),
        _ => false,
    }
}

/// Duplicate function identities are ambiguous: the second insert refuses
/// both, and the identity count later drops the key entirely.
fn insert_function(
    functions: &mut BTreeMap<FunctionKey, FunctionAssertions>,
    key: FunctionKey,
    facts: FunctionAssertions,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) = functions.entry(key.clone()) {
        entry.insert(facts);
    } else {
        functions.insert(
            key,
            FunctionAssertions {
                refusal: Some(AssertionContextRefusal::UnidentifiedTest),
                ..FunctionAssertions::default()
            },
        );
    }
}

/// A libtest item cannot be nested in an executable body. Module/source
/// attributes include inner attributes on ItemList, not only outer attrs.
pub(super) fn supported_item_context(item: &SyntaxNode) -> bool {
    item_context_refusal(item).is_none()
}

/// Targeted call syntax also needs container attributes to preserve source
/// items. The selected function's own harness attributes are outside this
/// check; its parent and the resolved module declarations supply the context.
pub(crate) fn supported_targeted_item_context(item: &SyntaxNode) -> bool {
    supported_item_context(item)
        && item.ancestors().all(|node| {
            node.children().filter_map(ast::Attr::cast).all(|attr| {
                crate::analysis::facts::cfg_predicates::attribute_preserves_source_items(
                    &attr.syntax().text().to_string(),
                )
            })
        })
}

/// Why `item` is not a plain item reachable from the file root under test
/// builds: nested in an executable body, or under a gating attribute.
fn item_context_refusal(item: &SyntaxNode) -> Option<AssertionContextRefusal> {
    let mut source_file = false;
    for (depth, node) in item.ancestors().enumerate() {
        if depth > 0
            && !ast::ItemList::can_cast(node.kind())
            && !ast::Module::can_cast(node.kind())
            && !ast::SourceFile::can_cast(node.kind())
        {
            return Some(AssertionContextRefusal::NestedItem);
        }
        if let Some(attr) = node.children().filter_map(ast::Attr::cast).find(|attr| {
            attribute_test_build_availability(&attr.syntax().text().to_string()) != Some(true)
        }) {
            return Some(AssertionContextRefusal::GatedItem(
                attr.syntax().text().to_string(),
            ));
        }
        source_file |= ast::SourceFile::can_cast(node.kind());
    }
    (!source_file).then_some(AssertionContextRefusal::NestedItem)
}

/// Attributes a pinned test may carry: `#[test]` itself, and the
/// `serial_test` locks, which run the unchanged body while holding a mutex or
/// file lock. Lock arguments are admitted only as bare key names
/// (`#[serial(env, db)]`): `inner_attrs = [..]` hands the body to other
/// attribute macros and `crate = ..` swaps the runtime that runs it, so any
/// other argument refuses. Every other attribute (`ignore`, `should_panic`,
/// an async runtime or a parameterizing macro) may skip, invert or rewrite
/// the body, so it keeps the test out of the pin.
fn runs_body_unchanged(attr: &ast::Attr, test: &SyntaxNode) -> bool {
    if attr.simple_name().as_deref() == Some("test") {
        return true;
    }
    if attr.excl_token().is_some() {
        return false;
    }
    let Some(path) = attr.path() else {
        return false;
    };
    let path = path.syntax().text().to_string().replace(' ', "");
    let Some(module) = test.parent() else {
        return false;
    };
    if is_tokio_test_path(&path) {
        return tokio_test_runs_body(attr, test);
    }
    if let Some(kind) = rstest_attribute(&path, &module) {
        return rstest_runs_body(attr, kind, test, &module);
    }
    let admitted = if path.contains("::") {
        serial_test_lock_leaf(&path).is_some() && serial_test_path_is_the_crate(&module)
    } else {
        bare_lock_bound_in_module(&path, &module) && serial_test_path_is_the_crate(&module)
    };
    admitted && lock_arguments_are_bare_keys(attr)
}

/// `tokio::test`, rooted or not. Only the crate-qualified path: a bare
/// `#[test]` on an `async fn` binds whatever `test` the module imports.
fn is_tokio_test_path(path: &str) -> bool {
    path.strip_prefix("::").unwrap_or(path) == "tokio::test"
}

/// Whether `attr` is `#[tokio::test]` on `test` and runs the async body to
/// completion on a fresh runtime, failing the test when the body panics,
/// as libtest does for a plain `#[test]` body. `tokio` must be the crate in
/// the test's module (the serial_test check, for `tokio`). Arguments are
/// admitted only as `flavor`, `worker_threads` and `start_paused`, which
/// pick the runtime and its clock: `crate = ..` swaps the macro's runtime
/// path and `unhandled_panic = ..` changes what a panic does, so any other
/// argument refuses. A sync `fn` under it does not compile, so it refuses.
fn tokio_test_runs_body(attr: &ast::Attr, test: &SyntaxNode) -> bool {
    if attr.excl_token().is_some() {
        return false;
    }
    if ast::Fn::cast(test.clone()).is_none_or(|function| function.async_token().is_none()) {
        return false;
    }
    let Some(path) = attr.path() else {
        return false;
    };
    if !is_tokio_test_path(&path.syntax().text().to_string().replace(' ', "")) {
        return false;
    }
    let Some(module) = test.parent() else {
        return false;
    };
    extern_crate_path_holds(&module, "tokio") && tokio_test_arguments_pick_runtime(attr)
}

/// No argument list, or `key = literal` pairs whose keys only pick the
/// runtime flavor, its worker count or a paused clock.
fn tokio_test_arguments_pick_runtime(attr: &ast::Attr) -> bool {
    let tree = match attr.meta() {
        Some(ast::Meta::PathMeta(_)) => return true,
        Some(ast::Meta::TokenTreeMeta(meta)) => match meta.token_tree() {
            Some(tree) => tree,
            None => return false,
        },
        _ => return false,
    };
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let Some((first, rest)) = tokens.split_first() else {
        return false;
    };
    let Some((last, inner)) = rest.split_last() else {
        return false;
    };
    if first.text() != "(" || last.text() != ")" {
        return false;
    }
    inner
        .split(|token| token.text() == ",")
        .all(|pair| match pair {
            [] => true,
            [key, eq, value] => {
                matches!(key.text(), "flavor" | "worker_threads" | "start_paused")
                    && eq.text() == "="
                    && (value.kind().is_literal() || matches!(value.text(), "true" | "false"))
            }
            _ => false,
        })
}

/// An rstest attribute on a test function.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RstestAttribute {
    /// `#[rstest]`, which expands the function into one test per case row.
    Rstest,
    /// `#[case(..)]` or `#[case::name(..)]`: one case row, inert on its own
    /// and consumed by an `#[rstest]` above it.
    Case,
}

/// Which rstest attribute `path` names in `module`: `rstest::rstest`
/// (rooted or not), a bare `rstest` bound to it by an explicit unrenamed
/// `use`, or a case row. A rename (`use rstest::rstest as table`) is not
/// discovered as a test, so it is not admitted here either. The caller
/// still checks that `rstest` is the crate.
fn rstest_attribute(path: &str, module: &SyntaxNode) -> Option<RstestAttribute> {
    if path.strip_prefix("::").unwrap_or(path) == "rstest::rstest"
        || (path == "rstest" && rstest_leaf_bound(path, module) == Some("rstest"))
    {
        return Some(RstestAttribute::Rstest);
    }
    let row = match path.split_once("::") {
        None => path == "case",
        Some(("case", name)) => is_identifier(name),
        _ => false,
    };
    // A `use` binding `case` makes the name another attribute macro.
    let rebound = || {
        module_use_leaves(module).iter().any(
            |leaf| matches!(&leaf.binding, UseBinding::Name(binding) if unraw(binding) == "case"),
        )
    };
    (row && !rebound()).then_some(RstestAttribute::Case)
}

/// Whether `attr` is an rstest attribute that expands to items named after
/// the function it decorates and to nothing else: `#[rstest]`,
/// `#[fixture]`, or a case row on a function whose `#[rstest]` above it
/// consumes the row. A bare `#[case]` anywhere else is an unknown attribute
/// macro and may emit anything.
fn rstest_attribute_trusted(attr: &ast::Attr, module: &SyntaxNode) -> bool {
    let Some(path) = attr.path() else {
        return false;
    };
    let path = path.syntax().text().to_string().replace(' ', "");
    match rstest_attribute(&path, module) {
        Some(RstestAttribute::Rstest) => true,
        Some(RstestAttribute::Case) => {
            attr.syntax()
                .parent()
                .and_then(ast::Fn::cast)
                .is_some_and(|function| {
                    function.attrs().any(|other| {
                        other.syntax().text_range().start() < attr.syntax().text_range().start()
                            && other.path().is_some_and(|other_path| {
                                rstest_attribute(
                                    &other_path.syntax().text().to_string().replace(' ', ""),
                                    module,
                                ) == Some(RstestAttribute::Rstest)
                            })
                    })
                })
        }
        None => {
            path.strip_prefix("::").unwrap_or(&path) == "rstest::fixture"
                || (path == "fixture" && rstest_leaf_bound(&path, module) == Some("fixture"))
        }
    }
}

/// The rstest attribute macro (`rstest` or `fixture`) that a bare name is
/// bound to in `module` by exactly one explicit `use rstest::<macro>`
/// (renames included); globs and any second binding of the name refuse.
fn rstest_leaf_bound(name: &str, module: &SyntaxNode) -> Option<&'static str> {
    let name = unraw(name);
    let mut bound = None;
    for leaf in module_use_leaves(module) {
        let UseBinding::Name(binding) = &leaf.binding else {
            continue;
        };
        if unraw(binding) != name {
            continue;
        }
        let target = match leaf.path.as_slice() {
            [krate, target] if krate == "rstest" && target == "rstest" => "rstest",
            [krate, target] if krate == "rstest" && target == "fixture" => "fixture",
            _ => return None,
        };
        if bound.is_some() {
            return None;
        }
        bound = Some(target);
    }
    bound
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// Whether an rstest attribute on `test` leaves each generated test running
/// the body unchanged with literal case values. rstest turns
/// `#[rstest] #[case(2, 4)] fn t(#[case] input: u32, #[case] expected: u32)`
/// into one `#[test]` per row that binds the row's values to the parameters
/// and runs the body. Admitted only when `rstest` is the crate in the test's
/// module, `#[rstest]` takes no arguments and appears once, every parameter
/// is a plain binding marked `#[case]` or an unmarked fixture, and every row
/// follows `#[rstest]` and holds one scalar literal per `#[case]` parameter.
/// `#[values]` matrices, `#[future]`, other parameter attributes, timeouts
/// and computed case values (which could call the owner) refuse.
fn rstest_runs_body(
    attr: &ast::Attr,
    kind: RstestAttribute,
    test: &SyntaxNode,
    module: &SyntaxNode,
) -> bool {
    if attr.excl_token().is_some() {
        return false;
    }
    let Some(function) = ast::Fn::cast(test.clone()) else {
        return false;
    };
    if !extern_crate_path_holds(module, "rstest") {
        return false;
    }
    let kinds: Vec<(TextSize, RstestAttribute)> = function
        .attrs()
        .filter_map(|attr| {
            let path = attr.path()?.syntax().text().to_string().replace(' ', "");
            Some((
                attr.syntax().text_range().start(),
                rstest_attribute(&path, module)?,
            ))
        })
        .collect();
    let rstest_at: Vec<TextSize> = kinds
        .iter()
        .filter(|(_, kind)| *kind == RstestAttribute::Rstest)
        .map(|(at, _)| *at)
        .collect();
    let [rstest_at] = rstest_at.as_slice() else {
        return false;
    };
    let Some(parameters) = case_parameters(&function) else {
        return false;
    };
    match kind {
        RstestAttribute::Rstest => {
            matches!(attr.meta(), Some(ast::Meta::PathMeta(_)))
                && (parameters == 0 || kinds.iter().any(|(_, kind)| *kind == RstestAttribute::Case))
        }
        RstestAttribute::Case => {
            attr.syntax().text_range().start() > *rstest_at
                && case_row_literals(attr) == Some(parameters)
        }
    }
}

/// The number of `#[case]` parameters when every parameter is a plain
/// `ident: Type` binding that is either marked by exactly one `#[case]`
/// attribute or carries no attribute at all; `None` otherwise. #6578: an
/// unmarked parameter is a fixture, which rstest fills by calling the
/// same-named `#[fixture]` function once before the body runs (a missing
/// fixture does not compile). Any other parameter attribute (`#[values]`,
/// `#[from]`, `#[with]`, `#[future]`, `#[files]`, `#[by_ref]`) changes how
/// often or with what the body runs, so it refuses, as does a generic test.
fn case_parameters(function: &ast::Fn) -> Option<usize> {
    // A generic test lets rstest infer a type parameter from a fixture, so
    // a parameter annotation may name a type parameter, not a type.
    if function.generic_param_list().is_some() || function.where_clause().is_some() {
        return None;
    }
    let list = function.param_list()?;
    if list.self_param().is_some() {
        return None;
    }
    let mut count = 0;
    for param in list.params() {
        let ast::Pat::IdentPat(pat) = param.pat()? else {
            return None;
        };
        if pat.ref_token().is_some() || pat.pat().is_some() {
            return None;
        }
        // An `impl Trait` parameter is an anonymous type parameter, which
        // rstest may infer from a fixture as it does a named one.
        if matches!(param.ty(), Some(ast::Type::ImplTraitType(_))) {
            return None;
        }
        let attrs: Vec<_> = param.attrs().collect();
        match attrs.as_slice() {
            [] => {}
            // A `mut` case value may be reassigned in the body
            // (`expected = want;`), which no binding scan sees, so a case
            // row's literal reaches the assertion only through an immutable
            // parameter (#6578 review).
            [case]
                if case.excl_token().is_none()
                    && pat.mut_token().is_none()
                    && matches!(case.meta(), Some(ast::Meta::PathMeta(_)))
                    && case.simple_name().as_deref() == Some("case") =>
            {
                count += 1;
            }
            _ => return None,
        }
    }
    Some(count)
}

/// The number of values in a case row when each is a non-string literal,
/// `-literal`, `true` or `false`; `None` for anything else.
fn case_row_literals(attr: &ast::Attr) -> Option<usize> {
    let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
        return None;
    };
    let tokens: Vec<_> = meta
        .token_tree()?
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let (first, rest) = tokens.split_first()?;
    let (last, inner) = rest.split_last()?;
    if first.text() != "(" || last.text() != ")" {
        return None;
    }
    let mut count = 0;
    for value in inner.split(|token| token.text() == ",") {
        // A string literal reaches the parameter through `FromStr`, which
        // runs user code that may call the owner, so only scalar literals.
        let scalar = |token: &ra_ap_syntax::SyntaxToken| {
            token.kind().is_literal()
                && !matches!(
                    token.kind(),
                    ra_ap_syntax::SyntaxKind::STRING
                        | ra_ap_syntax::SyntaxKind::BYTE_STRING
                        | ra_ap_syntax::SyntaxKind::C_STRING
                )
        };
        let literal = match value {
            [token] => scalar(token) || matches!(token.text(), "true" | "false"),
            [minus, token] => minus.text() == "-" && scalar(token),
            _ => false,
        };
        if !literal {
            return None;
        }
        count += 1;
    }
    Some(count)
}

const SERIAL_TEST_LOCKS: &[&str] = &["serial", "parallel", "file_serial", "file_parallel"];

/// The lock of a crate-qualified `serial_test::<lock>` attribute path,
/// rooted (`::serial_test::serial`) or not. A rooted path still names the
/// same crate, so it earns the same admission as the unrooted one.
fn serial_test_lock_leaf(path: &str) -> Option<&str> {
    let path = path.strip_prefix("::").unwrap_or(path);
    match path.split_once("::") {
        Some(("serial_test", leaf)) if SERIAL_TEST_LOCKS.contains(&leaf) => Some(leaf),
        _ => None,
    }
}

/// Whether a bare lock attribute `name` resolves to a serial_test lock in the
/// test's own module (`module` is its item list or source file). Only an
/// explicit `use serial_test::<lock>` (optionally renamed) that is a direct
/// item of that module counts: an explicit import shadows globs and the
/// `#[macro_use]` prelude, and a second explicit binding of the name there
/// would not compile, so another one refuses. Imports in function bodies,
/// sibling modules or macro token trees bind nothing for this attribute. The
/// caller still checks that `serial_test` in that path is the crate.
fn bare_lock_bound_in_module(name: &str, module: &SyntaxNode) -> bool {
    let name = unraw(name);
    let mut bound = false;
    for leaf in module_use_leaves(module) {
        let UseBinding::Name(binding) = &leaf.binding else {
            continue;
        };
        if unraw(binding) != name {
            continue;
        }
        let is_lock = leaf.path.len() == 2
            && leaf.path[0] == "serial_test"
            && SERIAL_TEST_LOCKS.contains(&leaf.path[1].as_str());
        if !is_lock || bound {
            return false;
        }
        bound = true;
    }
    bound
}

/// Whether `serial_test::..` in the test's module resolves to the extern
/// crate: the module binds no item named `serial_test` (`mod`, `use .. as`,
/// `extern crate .. as`), has no item-position macro call or other
/// attribute or derive macro on a direct item that could emit one, and
/// imports no glob except `super::*` into an enclosing module of this file
/// that satisfies the same rule. Any other glob may bring a `serial_test`
/// that shadows the extern prelude, so it refuses.
fn serial_test_path_is_the_crate(module: &SyntaxNode) -> bool {
    extern_crate_path_holds(module, "serial_test")
}

/// [`serial_test_path_is_the_crate`] for any extern crate name `krate`.
///
/// A `#[serial]` lock and `#[tokio::test]` in one module each name the other
/// crate's attribute, so the check runs jointly: every trusted crate whose
/// attribute appears there must hold under the same rule. Neither can then
/// be shadowed, and neither real crate's attribute emits an item.
fn extern_crate_path_holds(module: &SyntaxNode, krate: &str) -> bool {
    let mut crates = vec![krate];
    for other in TRUSTED_ATTRIBUTE_CRATES {
        if *other != krate && crate_attribute_appears(module, other) {
            crates.push(other);
        }
    }
    crates
        .iter()
        .all(|checked| extern_crate_holds_trusting(module, checked, &crates))
}

/// Crates whose own attributes a module check may trust (see
/// [`extern_crate_path_holds`]).
const TRUSTED_ATTRIBUTE_CRATES: &[&str] = &["serial_test", "tokio", "rstest"];

/// Whether a direct item of `module` carries an attribute of `krate`: a
/// `krate::..` path, or for `serial_test` a bare name bound to a lock.
fn crate_attribute_appears(module: &SyntaxNode, krate: &str) -> bool {
    module
        .children()
        .filter_map(ast::Item::cast)
        .flat_map(|item| item.attrs().collect::<Vec<_>>())
        .any(|attr| {
            let Some(path) = attr.path() else {
                return false;
            };
            let path = path.syntax().text().to_string().replace(' ', "");
            let unrooted = path.strip_prefix("::").unwrap_or(&path);
            unrooted.starts_with(&format!("{krate}::"))
                || (krate == "serial_test"
                    && !path.contains("::")
                    && bare_lock_bound_in_module(&path, module))
                || (krate == "rstest" && rstest_attribute_trusted(&attr, module))
        })
}

fn extern_crate_holds_trusting(module: &SyntaxNode, krate: &str, trusted: &[&str]) -> bool {
    for item in module.children().filter_map(ast::Item::cast) {
        if item
            .attrs()
            .any(|attr| !attribute_emits_no_items(&attr, module, trusted))
        {
            return false;
        }
        // `#[rstest]` and `#[fixture]` emit items named after the function
        // they decorate (a module of cases, a fixture struct).
        if let ast::Item::Fn(function) = &item
            && function
                .name()
                .is_some_and(|name| unraw(name.text()) == krate)
            && function
                .attrs()
                .any(|attr| rstest_attribute_trusted(&attr, module))
        {
            return false;
        }
        match &item {
            ast::Item::MacroCall(_) => return false,
            ast::Item::Module(inner)
                if inner.name().is_some_and(|name| unraw(name.text()) == krate) =>
            {
                return false;
            }
            ast::Item::ExternCrate(extern_crate) => {
                let binding = match extern_crate.rename() {
                    Some(rename) => rename.name().map(|name| name.text().to_string()),
                    None => extern_crate.name_ref().map(|name| name.text().to_string()),
                };
                let Some(binding) = binding else {
                    return false;
                };
                let real = extern_crate
                    .name_ref()
                    .is_some_and(|name| unraw(name.text()) == krate);
                if (unraw(&binding) == krate && !real) || PRELUDE_MACROS.contains(&unraw(&binding))
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    for leaf in module_use_leaves(module) {
        match &leaf.binding {
            UseBinding::Name(binding) => {
                // `use rstest::rstest` binds the attribute macro, which
                // leaves the crate's path in the type namespace untouched.
                let real = (leaf.path.len() == 1 && leaf.path[0] == krate)
                    || (krate == "rstest" && leaf.path == ["rstest", "rstest"]);
                if unraw(binding) == krate && !real {
                    return false;
                }
                // `derive`, the std derives and `test` are prelude macros an
                // import can rebind, which voids the allowlist below.
                if PRELUDE_MACROS.contains(&unraw(binding)) {
                    return false;
                }
            }
            UseBinding::Glob => {
                let into_parent = leaf.path.len() == 1 && leaf.path[0] == "super";
                let parent = module
                    .parent()
                    .filter(|owner| ast::Module::can_cast(owner.kind()))
                    .and_then(|owner| owner.parent());
                match parent {
                    Some(parent) if into_parent => {
                        if !extern_crate_path_holds(&parent, krate) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
    }
    true
}

/// Built-in attributes and std derives expand to no new items, provided the
/// caller has refused any import rebinding a `PRELUDE_MACROS` name. A
/// `serial_test` lock is trusted here because it is the crate's only when the
/// surrounding check passes; a bare name must be bound to a lock (renames
/// included).
/// Anything else (`cfg_attr` included) may emit a `serial_test` module.
fn attribute_emits_no_items(attr: &ast::Attr, module: &SyntaxNode, trusted: &[&str]) -> bool {
    const BUILTIN: &[&str] = &[
        "test",
        "cfg",
        "allow",
        "warn",
        "deny",
        "forbid",
        "expect",
        "doc",
        "inline",
        "cold",
        "must_use",
        "non_exhaustive",
        "repr",
        "path",
        "ignore",
        "should_panic",
        "track_caller",
        "deprecated",
        "macro_use",
        "macro_export",
    ];
    let Some(path) = attr.path() else {
        // `cfg(..)` parses as its own meta kind with no path.
        return attr.simple_name().as_deref() == Some("cfg");
    };
    let path = path.syntax().text().to_string().replace(' ', "");
    let unrooted = path.strip_prefix("::").unwrap_or(&path);
    // A crate's own attribute is trusted only while that crate is under
    // check too; in any other module check it could emit the item.
    if unrooted.starts_with("serial_test::") {
        return trusted.contains(&"serial_test") && serial_test_lock_leaf(&path).is_some();
    }
    if unrooted.starts_with("tokio::") {
        return trusted.contains(&"tokio") && is_tokio_test_path(&path);
    }
    if rstest_attribute_trusted(attr, module) {
        return trusted.contains(&"rstest");
    }
    if path == "derive" {
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        return meta.token_tree().is_some_and(|tree| {
            let text = tree.syntax().text().to_string();
            text.trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .all(|name| STD_DERIVES.contains(&name))
        });
    }
    BUILTIN.contains(&path.as_str())
        || (trusted.contains(&"serial_test")
            && !path.contains("::")
            && bare_lock_bound_in_module(&path, module))
}

const STD_DERIVES: &[&str] = &[
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

/// Prelude macro names `attribute_emits_no_items` trusts that an import can
/// rebind. Built-in attributes (`cfg`, `inline`, ..) cannot be rebound: an
/// import of one is an ambiguity error. A `#[macro_use]` extern crate can
/// also rebind these names with no import; `macro_binding_scan` refuses any
/// `macro_use` in the scanned files, which this check relies on.
const PRELUDE_MACROS: &[&str] = &[
    "derive",
    "test",
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// The use leaves of the module's own direct `use` items.
/// Whether any `use` item anywhere under `file` binds `name` to something
/// other than the crate `name` itself (`use name;`, `use ::name;`,
/// `use {name}` and `use name::{self}` bind the crate). Renames, `self`
/// leaves and nested trees resolve through `flatten_use_tree`, the same
/// reading the serial_test lock check uses.
pub(crate) fn use_rebinds_crate_name(file: &SyntaxNode, name: &str) -> bool {
    let mut leaves = Vec::new();
    for item in file.descendants().filter_map(ast::Use::cast) {
        if let Some(tree) = item.use_tree() {
            flatten_use_tree(&tree, &[], &mut leaves);
        }
    }
    leaves.iter().any(|leaf| {
        let UseBinding::Name(bound) = &leaf.binding else {
            return false;
        };
        let target = match leaf.path.split_last() {
            Some((last, parent)) if last == "self" => parent,
            _ => leaf.path.as_slice(),
        };
        unraw(bound) == name && target != [name]
    })
}

fn module_use_leaves(module: &SyntaxNode) -> Vec<UseLeaf> {
    let mut leaves = Vec::new();
    for item in module.children().filter_map(ast::Use::cast) {
        if let Some(tree) = item.use_tree() {
            flatten_use_tree(&tree, &[], &mut leaves);
        }
    }
    leaves
}

/// No argument list, or one holding only identifiers and commas.
fn lock_arguments_are_bare_keys(attr: &ast::Attr) -> bool {
    let tree = match attr.meta() {
        Some(ast::Meta::PathMeta(_)) => return true,
        Some(ast::Meta::TokenTreeMeta(meta)) => match meta.token_tree() {
            Some(tree) => tree,
            None => return false,
        },
        _ => return false,
    };
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let Some((first, rest)) = tokens.split_first() else {
        return false;
    };
    let Some((last, inner)) = rest.split_last() else {
        return false;
    };
    first.text() == "("
        && last.text() == ")"
        && inner.iter().all(|token| {
            token.text() == ","
                || (token.kind() == ra_ap_syntax::SyntaxKind::IDENT
                    && !matches!(token.text(), "inner_attrs" | "crate" | "path"))
        })
}

struct UseLeaf {
    path: Vec<String>,
    binding: UseBinding,
}

enum UseBinding {
    Glob,
    Name(String),
}

/// Expand one use tree into its leaves with their full paths and the name
/// each binds (`as` rename, or the last segment). `as _` binds nothing.
fn flatten_use_tree(tree: &ast::UseTree, prefix: &[String], out: &mut Vec<UseLeaf>) {
    let mut path = prefix.to_vec();
    if let Some(own) = tree.path() {
        path.extend(
            own.syntax()
                .text()
                .to_string()
                .split("::")
                .map(|segment| unraw(segment.trim()).to_string())
                .filter(|segment| !segment.is_empty()),
        );
    }
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            flatten_use_tree(&child, &path, out);
        }
        return;
    }
    if tree.star_token().is_some() {
        out.push(UseLeaf {
            path,
            binding: UseBinding::Glob,
        });
        return;
    }
    let name = match tree.rename() {
        Some(rename) => match rename.name() {
            Some(name) => name.text().to_string(),
            None => return,
        },
        None => match path.last() {
            Some(last) if last != "self" => last.clone(),
            Some(_) => match path.iter().rev().nth(1) {
                Some(parent) => parent.clone(),
                None => return,
            },
            None => return,
        },
    };
    out.push(UseLeaf {
        path,
        binding: UseBinding::Name(name),
    });
}

fn has_escape(
    body: &SyntaxNode,
    trusted: &[&str],
    empty: &BTreeMap<String, ast::MacroRules>,
) -> Option<AssertionContextRefusal> {
    body.descendants().find_map(|node| {
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            // Discarded arguments are not executed. Cross-file/import/shadow
            // ambiguity is checked by the shared binding authority at admission.
            if resolves_empty_local(&call, empty) {
                return None;
            }
            let path = call
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            if (!is_trusted_macro(&path, trusted)
                && !is_standard_qualified_macro(&path, &call, trusted))
                || path == "json"
                    && (!literal_json_import_visible(call.syntax())
                        || !call
                            .token_tree()
                            .is_some_and(|tree| literal_json_tree(&tree)))
            {
                return Some(AssertionContextRefusal::OpaqueMacro(path));
            }
            // Macro operands are opaque to AST descendant walks. Refuse
            // hidden exits and nested expansion, but not boolean negation.
            if call.token_tree().is_some_and(|tree| {
                opaque_macro_operand_with_json(
                    &tree,
                    trusted.contains(&"json") && literal_json_import_visible(call.syntax()),
                )
            }) {
                return Some(AssertionContextRefusal::MacroOperandExit(path));
            }
        }

        // Root returns are checked against the actual invocation's statement
        // prefix below. A later return cannot undo an earlier assertion.
        // `break`/`continue` are checked the same way against each enclosing
        // `loop` in `eager_path`: outside a loop that holds the assertion they
        // only leave a loop or labeled block the assertion is not inside.
        // Closure returns retain the existing conservative refusal, including
        // returns in closures other than the selected one.
        ((ast::ReturnExpr::can_cast(node.kind())
            && node
                .ancestors()
                .any(|parent| ast::ClosureExpr::can_cast(parent.kind())))
            || (ast::TryExpr::can_cast(node.kind())
                && node
                    .ancestors()
                    .any(|parent| ast::ClosureExpr::can_cast(parent.kind())))
            || ast::YieldExpr::can_cast(node.kind()))
        .then_some(AssertionContextRefusal::ClosureExit)
    })
}

fn opaque_macro_operand(tree: &ast::TokenTree) -> bool {
    opaque_macro_operand_with_json(tree, false)
}

fn opaque_macro_operand_with_json(tree: &ast::TokenTree, literal_json: bool) -> bool {
    let tokens = operand_tokens(tree);
    tokens.iter().any(|token| {
        matches!(
            token.text(),
            "return" | "break" | "continue" | "yield" | "?"
        )
    }) || nested_invocations(&tokens).any(|(name, plain)| {
        !(plain && (NESTED_EXPANSIONS.contains(&name) || name == "json" && literal_json))
    }) || tokens.windows(3).enumerate().any(|(at, window)| {
        window[0].text() == "json"
            && window[1].text() == "!"
            && !literal_json_tokens(&tokens[at + 2..])
    })
}

/// A bare `matches!`, `format!` or `vec!` in a macro operand expands to one
/// `match`, call or vector over its own operands (RIPR-SPEC-0197 literal
/// `matches!`, rendered scalar pins and single-element indexes), which the
/// token scan above already reads for exits; its binding is checked with the
/// invoking macros'.
const NESTED_EXPANSIONS: &[&str] = &["matches", "format", "vec"];

fn operand_tokens(tree: &ast::TokenTree) -> Vec<ra_ap_syntax::SyntaxToken> {
    tree.syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect()
}

/// Each `name!(`, `name![` or `name!{` inside an operand, with whether the
/// name is unqualified (no `::` before it).
fn nested_invocations(
    tokens: &[ra_ap_syntax::SyntaxToken],
) -> impl Iterator<Item = (&str, bool)> + '_ {
    tokens
        .windows(3)
        .enumerate()
        .filter_map(move |(at, window)| {
            let [name, bang, open] = window else {
                return None;
            };
            if name.kind() != ra_ap_syntax::SyntaxKind::IDENT
                || bang.text() != "!"
                || !matches!(open.text(), "(" | "[" | "{")
            {
                return None;
            }
            // Inside a token tree `::` lexes as two `:` tokens; a single
            // `:` is a field or type annotation, not a path.
            let before = |back: usize| {
                at.checked_sub(back)
                    .and_then(|index| tokens.get(index))
                    .map(|token| token.text())
            };
            let qualified =
                before(1) == Some("::") || (before(1) == Some(":") && before(2) == Some(":"));
            Some((name.text(), !qualified))
        })
}

/// The macros `node` invokes: every macro call's path, plus every name
/// invoked inside a macro operand, so the binding check covers it too.
fn invoked_macro_names(node: &SyntaxNode) -> BTreeSet<String> {
    node.descendants()
        .filter_map(ast::MacroCall::cast)
        .flat_map(|call| {
            let nested: Vec<String> = call
                .token_tree()
                .map(|tree| {
                    nested_invocations(&operand_tokens(&tree))
                        .map(|(name, _)| name.to_string())
                        .collect()
                })
                .unwrap_or_default();
            call.path()
                .map(|path| path.syntax().text().to_string())
                .into_iter()
                .chain(nested)
        })
        .collect()
}

fn is_trusted_macro(path: &str, trusted: &[&str]) -> bool {
    // Qualified roots can themselves be rebound. Without name resolution,
    // only bare names with the workspace binding check are established.
    trusted.contains(&path)
}

/// A macro path that may be one of the `leaves` assertions: the bare name,
/// or `assert_eq` under a `std`/`core` root (#7857). A qualified candidate
/// is admitted only when [`has_escape`] finds its root unrebound, so a
/// rebinding refuses the whole test with the path named.
fn names_candidate_assertion(path: &str, leaves: &[&str]) -> bool {
    let path: String = path.split_whitespace().collect();
    let unrooted = path.strip_prefix("::").unwrap_or(&path);
    match unrooted.split_once("::") {
        Some(("std" | "core", "assert_eq")) => leaves.contains(&"assert_eq"),
        Some(_) => false,
        None => leaves.contains(&unrooted),
    }
}

/// `std::assert_eq!` or `::core::assert_eq!` under a root nothing in the
/// file can rebind runs as the standard macro, so it is no escape (#7857).
/// Only `assert_eq` qualifies: the pin reads no other qualified macro, and
/// the classify side adds the workspace root proof for the credited one.
fn is_standard_qualified_macro(path: &str, call: &ast::MacroCall, trusted: &[&str]) -> bool {
    // The parser keeps the spacing of `std :: assert_eq`.
    let path: String = path.split_whitespace().collect();
    path.rsplit("::").next() == Some("assert_eq")
        && trusted.contains(&"assert_eq")
        && std_macro_root_is_unrebound(&path, call.syntax())
}

/// `Ok` when the invocation runs on every execution of the test body;
/// otherwise the construct that may skip it, phrased for a reader.
fn eager_path(
    mut node: SyntaxNode,
    function: &ast::Fn,
    through_closure: bool,
    first_return: Option<TextSize>,
) -> Result<(), &'static str> {
    // When this query follows a bound closure, recursion below resets this
    // coordinate to the real invocation, not the earlier closure definition.
    let execution_start = node.text_range().start();
    loop {
        if node
            .children()
            .any(|child| ast::Attr::can_cast(child.kind()))
        {
            return Err("an attribute on an enclosing statement or expression");
        }
        let Some(parent) = node.parent() else {
            return Err("a context outside the test body");
        };
        if parent == *function.syntax() {
            if function.body().is_none_or(|body| body.syntax() != &node) {
                return Err("a context outside the test body");
            }
            if first_return.is_some_and(|position| position < execution_start) {
                return Err(EARLIER_RETURN);
            }
            return Ok(());
        }
        if let Some(closure) = ast::ClosureExpr::cast(parent.clone()) {
            // `thread::scope` calls its closure once on this thread and
            // re-raises its panic, so it adds no closure of its own to count.
            if let Some(call) = scope_invocation(&closure, function) {
                return eager_path(
                    call.syntax().clone(),
                    function,
                    through_closure,
                    first_return,
                );
            }
            if through_closure {
                return Err("a nested closure");
            }
            if let Some(call) = closure_invocation(&closure, function, first_return) {
                return eager_path(call.syntax().clone(), function, true, first_return);
            }
            let Some(spawn) = spawned_thread_reaching_test(&closure, function) else {
                return Err("a closure ripr cannot see invoked exactly once");
            };
            return eager_path(spawn, function, true, first_return);
        }
        if let Some(block) = ast::BlockExpr::cast(parent.clone()) {
            if block.async_token().is_some() {
                return Err("an `async` block");
            }
            if block.const_token().is_some()
                || block.gen_token().is_some()
                || block.try_block_modifier().is_some()
                || block.label().is_some()
            {
                return Err("a labeled, `const`, `gen` or `try` block");
            }
        } else if let Some(body) = ast::LoopExpr::cast(parent.clone()) {
            // `loop` runs its body at least once, so the first iteration
            // reaches the invocation unless an earlier `break` or `continue`
            // in the body can skip it. Nested loops count conservatively.
            // `for` and `while` may run zero times and stay refused.
            if body.syntax().descendants().any(|node| {
                (ast::BreakExpr::can_cast(node.kind()) || ast::ContinueExpr::can_cast(node.kind()))
                    && node.text_range().start() < execution_start
            }) {
                return Err("a `loop` after a `break` or `continue` that can skip it");
            }
        } else if let Some(table) = ast::ForExpr::cast(parent.clone()) {
            // A `for` over a non-empty table of constant rows runs its body
            // at least once, like `loop` above (RIPR-SPEC-0197 rule 8). Any
            // other iterable may be empty and stays refused.
            if table.loop_body().is_none_or(|body| body.syntax() != &node)
                || table.label().is_some()
                || !constant_row_table(&table, function)
            {
                return Err(conditional_construct(&parent));
            }
            if table.syntax().descendants().any(|node| {
                (ast::BreakExpr::can_cast(node.kind()) || ast::ContinueExpr::can_cast(node.kind()))
                    && node.text_range().start() < execution_start
            }) {
                return Err("a `for` loop after a `break` or `continue` that can skip it");
            }
        } else if let Some(binding) = ast::LetStmt::cast(parent.clone()) {
            if binding.let_else().is_some()
                || binding
                    .initializer()
                    .is_none_or(|expr| expr.syntax() != &node)
            {
                return Err("a `let ... else` or pattern binding");
            }
        } else if !(ast::MacroExpr::can_cast(parent.kind())
            || ast::ExprStmt::can_cast(parent.kind())
            || ast::StmtList::can_cast(parent.kind())
            || ast::ParenExpr::can_cast(parent.kind()))
        {
            return Err(conditional_construct(&parent));
        }
        node = parent;
    }
}

/// Reader-facing name for a construct that may skip the code inside it.
fn conditional_construct(node: &SyntaxNode) -> &'static str {
    if ast::ForExpr::can_cast(node.kind()) {
        "a `for` loop, which may run zero times"
    } else if ast::WhileExpr::can_cast(node.kind()) {
        "a `while` loop, which may run zero times"
    } else if ast::IfExpr::can_cast(node.kind()) {
        "an `if` branch"
    } else if ast::MatchArm::can_cast(node.kind())
        || ast::MatchArmList::can_cast(node.kind())
        || ast::MatchExpr::can_cast(node.kind())
    {
        "a `match` arm"
    } else if ast::BinExpr::can_cast(node.kind()) {
        "an operand of `&&` or `||`"
    } else if ast::CallExpr::can_cast(node.kind())
        || ast::MethodCallExpr::can_cast(node.kind())
        || ast::ArgList::can_cast(node.kind())
    {
        "an argument of a call"
    } else {
        "an expression ripr cannot see evaluated on every run"
    }
}

/// Whether `table` iterates a non-empty array of constant rows, written
/// inline (`for row in [..]`, `&[..]`) or bound once by a plain `let` in the
/// statement list that holds the loop and named nowhere else in the test.
/// A row is constant when every leaf is a literal, a variant or tuple-struct
/// constructor, or `vec![..]` of literal tokens: it cannot call the owner,
/// so an expected value in the row is never the owner's own output.
fn constant_row_table(table: &ast::ForExpr, function: &ast::Fn) -> bool {
    constant_row_array(table, function).is_some()
}

/// The constant-row array `table` iterates, per [`constant_row_table`].
fn constant_row_array(table: &ast::ForExpr, function: &ast::Fn) -> Option<ast::ArrayExpr> {
    let mut iterable = table.iterable()?;
    while let ast::Expr::RefExpr(reference) = &iterable {
        if reference.mut_token().is_some() || reference.raw_token().is_some() {
            return None;
        }
        iterable = reference.expr()?;
    }
    match iterable {
        ast::Expr::ArrayExpr(rows) => constant_rows(&rows).then_some(rows),
        ast::Expr::PathExpr(path) => {
            let name = path
                .path()
                .filter(|path| path.qualifier().is_none())
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .map(|name| name.text().to_string())?;
            bound_constant_rows(table, function, &name)
        }
        _ => None,
    }
}

fn bound_constant_rows(
    table: &ast::ForExpr,
    function: &ast::Fn,
    name: &str,
) -> Option<ast::ArrayExpr> {
    let scope = table
        .syntax()
        .ancestors()
        .find(|node| ast::StmtList::can_cast(node.kind()))?;
    // The binding and the loop's iterable are the name's only two tokens:
    // no shadowing, mutation, alias or second use can change the rows.
    if name.starts_with("r#")
        || function
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.text().trim_start_matches("r#") == name)
            .count()
            != 2
    {
        return None;
    }
    scope
        .children()
        .filter_map(ast::LetStmt::cast)
        .filter(|binding| {
            binding.syntax().text_range().end() <= table.syntax().text_range().start()
                && binding.let_else().is_none()
                && !binding
                    .syntax()
                    .children()
                    .any(|node| ast::Attr::can_cast(node.kind()))
                && matches!(
                    binding.pat(),
                    Some(ast::Pat::IdentPat(pattern))
                        if pattern.mut_token().is_none()
                            && pattern.ref_token().is_none()
                            && pattern.at_token().is_none()
                            && pattern.name().is_some_and(|bound| bound.text() == name)
                )
        })
        .find_map(|binding| match binding.initializer() {
            Some(ast::Expr::ArrayExpr(rows)) if constant_rows(&rows) => Some(rows),
            _ => None,
        })
}

/// The values a constant-row table gives `name` (RIPR-SPEC-0186, #5328):
/// one cell per row when `name`'s only binding in the test function
/// `fn_text` is a plain identifier in the pattern of an unlabeled `for`
/// over a constant-row table, either the whole pattern (`for x in [..]`) or
/// one field of a flat tuple pattern (`for (x, want) in [..]`). Each row
/// must be a tuple of the pattern's arity. `None` when any part is not
/// established.
pub(crate) fn constant_table_column(fn_text: &str, name: &str) -> Option<Vec<String>> {
    table_column(fn_text, name).map(|(column, _)| column)
}

/// [`constant_table_column`], but only when the table's loop body holds
/// byte offset `at` of `fn_text` (#7969): a call outside the loop that
/// names the column reads some other `name`, such as a module `const`.
pub(crate) fn constant_table_column_around(
    fn_text: &str,
    name: &str,
    at: usize,
) -> Option<Vec<String>> {
    let (column, body) = table_column(fn_text, name)?;
    body.contains(&at).then_some(column)
}

/// The column and the byte range of its loop body.
fn table_column(fn_text: &str, name: &str) -> Option<(Vec<String>, std::ops::Range<usize>)> {
    // Most tests hold no `for` at all; skip the parse for them.
    if !fn_text.contains("for") || !fn_text.contains(name) {
        return None;
    }
    let parse = parse_clean_source_file(fn_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let mut bindings = function
        .syntax()
        .descendants()
        .filter(|node| node != function.syntax())
        .filter_map(|node| ast::AnyHasName::cast(node).and_then(|named| named.name()))
        .filter(|bound| bound.text() == name);
    let bound = bindings.next()?;
    if bindings.next().is_some() {
        return None;
    }
    let pattern = ast::IdentPat::cast(bound.syntax().parent()?)?;
    if pattern.mut_token().is_some()
        || pattern.ref_token().is_some()
        || pattern.at_token().is_some()
    {
        return None;
    }
    let parent = pattern.syntax().parent()?;
    let (table, position) = if let Some(table) = ast::ForExpr::cast(parent.clone()) {
        (table, None)
    } else {
        let tuple = ast::TuplePat::cast(parent)?;
        let fields: Vec<_> = tuple.fields().collect();
        if fields
            .iter()
            .any(|field| !matches!(field, ast::Pat::IdentPat(_) | ast::Pat::WildcardPat(_)))
        {
            return None;
        }
        let position = fields
            .iter()
            .position(|field| field.syntax() == pattern.syntax())?;
        let table = ast::ForExpr::cast(tuple.syntax().parent()?)?;
        (table, Some((position, fields.len())))
    };
    if table.pat().is_none_or(|pat| {
        !pat.syntax()
            .text_range()
            .contains_range(pattern.syntax().text_range())
    }) || table.label().is_some()
    {
        return None;
    }
    // Every row reaches the call only when nothing in the body leaves an
    // iteration early or runs code for some rows alone:
    // `{ assert_eq!(gate(cents), 99); break; }` runs the first row alone,
    // and `if let Some(want) = want { assert_eq!(gate(cents), want) }` skips
    // the `None` rows. Tokens, so control flow inside a macro counts too; a
    // `|` may open a closure that never runs, and `&&` or `||` may skip the
    // call on its right.
    let body = table.loop_body()?;
    if body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| {
            matches!(
                token.text(),
                "break"
                    | "continue"
                    | "return"
                    | "if"
                    | "match"
                    | "while"
                    | "loop"
                    | "for"
                    | "|"
                    | "||"
                    | "&&"
            )
        })
    {
        return None;
    }
    // The parser does not see bindings inside macro arguments, so a macro
    // that names the column next to a closure, `let`, `for`, `fn` or match
    // arm may rebind it there (`[50].map(|cents| gate(cents))`).
    if function
        .syntax()
        .descendants()
        .filter_map(ast::TokenTree::cast)
        .any(|tree| {
            let tokens: Vec<String> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .map(|token| token.text().to_string())
                .collect();
            tokens
                .iter()
                .any(|token| token.trim_start_matches("r#") == name)
                && tokens
                    .iter()
                    .any(|token| matches!(token.as_str(), "|" | "let" | "for" | "fn" | "=>"))
        })
    {
        return None;
    }
    let rows = constant_row_array(&table, &function)?;
    let range = body.syntax().text_range();
    let body_range = usize::from(range.start())..usize::from(range.end());
    let column = rows
        .exprs()
        .map(|row| match position {
            None => Some(row.syntax().text().to_string()),
            Some((index, arity)) => {
                let ast::Expr::TupleExpr(tuple) = row else {
                    return None;
                };
                let cells: Vec<_> = tuple.fields().collect();
                if cells.len() != arity {
                    return None;
                }
                cells
                    .get(index)
                    .map(|cell| cell.syntax().text().to_string())
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some((column, body_range))
}

fn constant_rows(rows: &ast::ArrayExpr) -> bool {
    // `[#[cfg(any())] (4, 0)]` is an empty array: an attribute anywhere in
    // the rows may remove one, so none is allowed.
    !rows
        .syntax()
        .descendants()
        .any(|node| ast::Attr::can_cast(node.kind()))
        && rows.semicolon_token().is_none()
        && rows.exprs().next().is_some()
        && rows.exprs().all(|row| constant_value(&row))
}

fn constant_value(value: &ast::Expr) -> bool {
    match value {
        ast::Expr::Literal(_) => true,
        ast::Expr::PrefixExpr(prefix) => {
            prefix.op_kind() == Some(ast::UnaryOp::Neg)
                && matches!(prefix.expr(), Some(ast::Expr::Literal(_)))
        }
        ast::Expr::ParenExpr(inner) => inner.expr().is_some_and(|expr| constant_value(&expr)),
        ast::Expr::RefExpr(reference) => {
            reference.mut_token().is_none()
                && reference.raw_token().is_none()
                && reference.expr().is_some_and(|expr| constant_value(&expr))
        }
        ast::Expr::TupleExpr(tuple) => tuple.fields().all(|field| constant_value(&field)),
        ast::Expr::ArrayExpr(items) => {
            items.semicolon_token().is_none() && items.exprs().all(|item| constant_value(&item))
        }
        ast::Expr::PathExpr(path) => path.path().is_some_and(|path| variant_path(&path)),
        ast::Expr::CallExpr(call) => {
            matches!(call.expr(), Some(ast::Expr::PathExpr(callee))
                if callee.path().is_some_and(|path| variant_path(&path)))
                && call
                    .arg_list()
                    .is_some_and(|args| args.args().all(|arg| constant_value(&arg)))
        }
        // `vec!` is matched by name; a workspace rebinding of it is refused
        // by the trusted-macro binding check every admitted test passes.
        ast::Expr::MacroExpr(expression) => expression.macro_call().is_some_and(|call| {
            call.path()
                .is_some_and(|path| path.syntax().text() == "vec")
                && call.token_tree().is_some_and(|tree| {
                    tree.syntax()
                        .descendants_with_tokens()
                        .filter_map(|element| element.into_token())
                        .filter(|token| !token.kind().is_trivia())
                        .all(|token| {
                            token.kind().is_literal()
                                || matches!(token.text(), "[" | "]" | "(" | ")" | "," | "-" | "&")
                        })
                })
        }),
        _ => false,
    }
}

/// `None`, `Some`, `Ok`, `Err`, or a qualified `Type::Variant` whose
/// segments are all CamelCase: a variant or constructor by convention. A
/// bare CamelCase name may be a `fn` or `const` that calls the owner, and a
/// SCREAMING_CASE `const` may be computed by the owner, so both are refused.
fn variant_path(path: &ast::Path) -> bool {
    let segments: Vec<_> = path.segments().collect();
    let names: Option<Vec<String>> = segments
        .iter()
        .map(|segment| {
            (segment.generic_arg_list().is_none())
                .then(|| segment.name_ref().map(|name| name.text().to_string()))
                .flatten()
        })
        .collect();
    let Some(names) = names else {
        return false;
    };
    let camel = |text: &str| {
        text.starts_with(|c: char| c.is_ascii_uppercase())
            && text.chars().any(|c| c.is_ascii_lowercase())
            && text != "Self"
    };
    match names.as_slice() {
        [single] => matches!(single.as_str(), "None" | "Some" | "Ok" | "Err"),
        [] => false,
        qualified => qualified.iter().all(|name| camel(name)),
    }
}

/// A closure that takes no arguments and is not `async`, `const`, `gen` or
/// attributed, so calling it runs its body once.
fn plain_closure(closure: &ast::ClosureExpr) -> bool {
    closure_parameters(closure) == Some(0)
}

/// The parameter count of a closure that is not `async`, `const`, `gen` or
/// attributed; `None` for any of those.
fn closure_parameters(closure: &ast::ClosureExpr) -> Option<usize> {
    (closure.async_token().is_none()
        && closure.const_token().is_none()
        && closure.gen_token().is_none()
        && !closure
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind())))
    .then(|| closure.param_list().map(|params| params.params().count()))
    .flatten()
}

/// The closure expression with any wrapping parentheses.
fn unparenthesized(closure: &ast::ClosureExpr) -> SyntaxNode {
    let mut expression = closure.syntax().clone();
    while let Some(parent) = expression
        .parent()
        .filter(|node| ast::ParenExpr::can_cast(node.kind()))
    {
        expression = parent;
    }
    expression
}

/// The item a `std::thread::<item>` path names, when nothing in this file
/// can make the path mean something else. Names are not resolved here, so
/// the file refuses when it holds any of:
/// - an item, alias or binding named `std` or `thread`;
/// - a `use` whose last segment is `std` or `thread`, other than exactly
///   `use std::thread;`;
/// - a glob `use` other than `use super::*;` (a glob-imported `std` module
///   beats the extern prelude);
/// - `use`, `mod` or `extern` inside a macro's tokens, `include!`, or an
///   item- or statement-position macro other than a std statement macro,
///   any of which may expand to the above;
/// - a `use` list holding `self` under a `std` or `thread` prefix.
///
/// A `::thread::` path names an extern crate and never matches.
/// `thread::<item>` also needs `use std::thread;` directly in the test
/// function's own module. A `std` or `thread` module that `use super::*;`
/// brings in from another file is not seen (RIPR-SPEC-0197).
fn std_thread_item(path: &ast::PathExpr, function: &ast::Fn) -> Option<String> {
    let text: String = path
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    // `::thread::` names an extern crate `thread`, never the imported std
    // module, so only `::std::thread::` keeps its leading `::`.
    let (item, needs_import) = if let Some(item) = text
        .strip_prefix("::std::thread::")
        .or_else(|| text.strip_prefix("std::thread::"))
    {
        (item, false)
    } else {
        (text.strip_prefix("thread::")?, true)
    };
    if !item
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_')
    {
        return None;
    }
    let file = function.syntax().ancestors().last()?;
    let module = function.syntax().parent()?;
    let mut imported = false;
    for element in file.descendants_with_tokens() {
        let node = match element {
            ra_ap_syntax::NodeOrToken::Token(token) => {
                if matches!(
                    token.kind(),
                    ra_ap_syntax::SyntaxKind::USE_KW
                        | ra_ap_syntax::SyntaxKind::MOD_KW
                        | ra_ap_syntax::SyntaxKind::EXTERN_KW
                ) && token
                    .parent_ancestors()
                    .any(|node| ast::TokenTree::can_cast(node.kind()))
                {
                    return None;
                }
                continue;
            }
            ra_ap_syntax::NodeOrToken::Node(node) => node,
        };
        if let Some(name) = ast::Name::cast(node.clone()) {
            if matches!(name.text().trim_start_matches("r#"), "std" | "thread") {
                return None;
            }
            continue;
        }
        // `extern crate thread;` names a dependency through a `NameRef`, not
        // a `Name`, and a block-level one shadows the module's
        // `use std::thread;` (#6966 review).
        if let Some(krate) = ast::ExternCrate::cast(node.clone()) {
            if krate.rename().is_none()
                && krate
                    .name_ref()
                    .is_some_and(|name| name.text().trim_start_matches("r#") == "thread")
            {
                return None;
            }
            continue;
        }
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            // A macro defined elsewhere, or `include!`, can expand to a `use`
            // whose tokens this file never shows. Expression macros cannot
            // bring an item into scope.
            let name = call
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let statement = macro_call_is_statement(&call);
            if name == "include" || statement && !STATEMENT_MACROS.contains(&name.as_str()) {
                return None;
            }
            continue;
        }
        let Some(tree) = ast::UseTree::cast(node) else {
            continue;
        };
        let path_text = tree.path().map(|path| {
            path.syntax()
                .text()
                .to_string()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
        });
        if tree.star_token().is_some() {
            // `use super::*;` is read only inside an inline module, where
            // the parent's items sit in this file and the scan sees them. At
            // the top of an out-of-line module file it globs a parent in
            // another file, which may declare its own `std` (#7022 review).
            let inline = tree
                .syntax()
                .ancestors()
                .any(|ancestor| ast::Module::can_cast(ancestor.kind()));
            if path_text.as_deref() != Some("super") || !inline {
                return None;
            }
            continue;
        }
        if tree.use_tree_list().is_some() {
            continue;
        }
        let path_text = path_text?;
        let path_text = path_text.trim_start_matches("::");
        let last = path_text.rsplit("::").next().unwrap_or(path_text);
        let last = last.trim_start_matches("r#");
        // Only a whole `use std::thread;` imports std's module: the same
        // text nested in a list (`use crate::fake::{std::thread};`) names
        // another path and falls through to the refusal below.
        let direct_use = tree
            .syntax()
            .parent()
            .filter(|parent| ast::Use::can_cast(parent.kind()));
        if path_text == "std::thread" && tree.rename().is_none() && direct_use.is_some() {
            imported |= direct_use
                .and_then(|parent| parent.parent())
                .is_some_and(|container| container == module);
        } else if matches!(last, "std" | "thread")
            || last == "self"
                && tree
                    .syntax()
                    .parent()
                    .and_then(|list| list.parent())
                    .and_then(ast::UseTree::cast)
                    .and_then(|parent| parent.path())
                    .and_then(|path| path.segment())
                    .is_none_or(|segment| {
                        segment.syntax().text().to_string().trim_start_matches("r#") == "std"
                            || segment.syntax().text().to_string().trim_start_matches("r#")
                                == "thread"
                    })
        {
            return None;
        }
    }
    (imported || !needs_import).then(|| item.to_string())
}

/// Item/statement macro expansions may introduce caller-visible bindings.
/// Expression expansions remain confined to their expression's scope.
pub(crate) fn macro_call_is_statement(call: &ast::MacroCall) -> bool {
    call.syntax().parent().is_none_or(|parent| {
        !ast::MacroExpr::can_cast(parent.kind())
            || parent.parent().is_none_or(|grandparent| {
                ast::ExprStmt::can_cast(grandparent.kind())
                    || ast::StmtList::can_cast(grandparent.kind())
            })
    })
}

/// Standard macros that may stand as a statement without bringing an item
/// into scope.
const STATEMENT_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "dbg",
    "eprint",
    "eprintln",
    "panic",
    "print",
    "println",
    "todo",
    "unimplemented",
    "unreachable",
    "write",
    "writeln",
];

/// The `std::thread::scope(..)` call that runs `closure`, its only argument.
fn scope_invocation(closure: &ast::ClosureExpr, function: &ast::Fn) -> Option<ast::CallExpr> {
    if closure_parameters(closure) != Some(1) {
        return None;
    }
    let expression = unparenthesized(closure);
    let args = expression.parent().and_then(ast::ArgList::cast)?;
    let call = args.syntax().parent().and_then(ast::CallExpr::cast)?;
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    (args.args().count() == 1 && std_thread_item(&path, function).as_deref() == Some("scope"))
        .then_some(call)
}

/// Where a spawned thread's panic reaches the test thread, for a plain
/// closure that is the only argument of a spawn:
/// - `std::thread::spawn(..).join().unwrap()` (or `.expect(..)`);
/// - `s.spawn(..)` as a statement directly in the closure of a
///   `std::thread::scope(|s| ..)`, which re-raises an unjoined thread's
///   panic, or with the same `.join().unwrap()` chain.
///
/// Returns the expression from which the test thread continues. A detached
/// thread, a handle kept in a binding, and `.join()` whose result is dropped
/// or matched lose the panic and stay refused.
fn spawned_thread_reaching_test(
    closure: &ast::ClosureExpr,
    function: &ast::Fn,
) -> Option<SyntaxNode> {
    if !plain_closure(closure) {
        return None;
    }
    let expression = unparenthesized(closure);
    let args = expression.parent().and_then(ast::ArgList::cast)?;
    if args.args().count() != 1 {
        return None;
    }
    let owner = args.syntax().parent()?;
    if let Some(call) = ast::CallExpr::cast(owner.clone()) {
        let ast::Expr::PathExpr(path) = call.expr()? else {
            return None;
        };
        if std_thread_item(&path, function).as_deref() != Some("spawn") {
            return None;
        }
        return joined_and_unwrapped(call.syntax());
    }
    let spawn = ast::MethodCallExpr::cast(owner)?;
    if spawn.name_ref()?.text() != "spawn" || spawn.generic_arg_list().is_some() {
        return None;
    }
    let ast::Expr::PathExpr(receiver) = spawn.receiver()? else {
        return None;
    };
    let receiver = receiver.path()?;
    if receiver.qualifier().is_some() || receiver.segment()?.generic_arg_list().is_some() {
        return None;
    }
    let name = receiver.segment()?.name_ref()?.text().to_string();
    // The receiver must be the parameter of the nearest enclosing closure,
    // and that closure the one `thread::scope` runs. No other binding in it
    // may reuse the name.
    let scope = spawn
        .syntax()
        .ancestors()
        .skip(1)
        .find_map(ast::ClosureExpr::cast)?;
    scope_invocation(&scope, function)?;
    let ast::Pat::IdentPat(parameter) = scope.param_list()?.params().next()?.pat()? else {
        return None;
    };
    if parameter.ref_token().is_some()
        || parameter.mut_token().is_some()
        || parameter.at_token().is_some()
        || parameter.name()?.text() != name
        || name.starts_with("r#")
        || scope
            .body()?
            .syntax()
            .descendants()
            .filter_map(ast::Name::cast)
            .any(|binding| binding.text().trim_start_matches("r#") == name)
    {
        return None;
    }
    if spawn
        .syntax()
        .parent()
        .is_some_and(|parent| ast::ExprStmt::can_cast(parent.kind()))
    {
        return Some(spawn.syntax().clone());
    }
    joined_and_unwrapped(spawn.syntax())
}

/// `<spawned>.join().unwrap()` or `<spawned>.join().expect(..)`: the outer
/// call, which panics on the test thread when the spawned thread panicked.
fn joined_and_unwrapped(spawned: &SyntaxNode) -> Option<SyntaxNode> {
    let method = |node: &SyntaxNode, receiver: &SyntaxNode, names: &[&str], arguments: usize| {
        ast::MethodCallExpr::cast(node.clone()).filter(|call| {
            call.receiver()
                .is_some_and(|expr| expr.syntax() == receiver)
                && call.generic_arg_list().is_none()
                && call
                    .name_ref()
                    .is_some_and(|name| names.iter().any(|wanted| name.text() == *wanted))
                && call
                    .arg_list()
                    .is_some_and(|args| args.args().count() == arguments)
        })
    };
    let join = method(&spawned.parent()?, spawned, &["join"], 0)?;
    let parent = join.syntax().parent()?;
    method(&parent, join.syntax(), &["unwrap"], 0)
        .or_else(|| method(&parent, join.syntax(), &["expect"], 1))
        .map(|call| call.syntax().clone())
}

fn closure_invocation(
    closure: &ast::ClosureExpr,
    function: &ast::Fn,
    first_return: Option<TextSize>,
) -> Option<ast::CallExpr> {
    if !plain_closure(closure) {
        return None;
    }
    let expression = unparenthesized(closure);
    if let Some(call) = expression.parent().and_then(ast::CallExpr::cast) {
        return (call.expr().is_some_and(|expr| expr.syntax() == &expression)
            && no_arguments(&call))
        .then_some(call);
    }
    let binding = expression.parent().and_then(ast::LetStmt::cast)?;
    if binding
        .initializer()
        .is_none_or(|expr| expr.syntax() != &expression)
        || binding.let_else().is_some()
        || binding
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    if eager_path(binding.syntax().clone(), function, true, first_return).is_err() {
        return None;
    }
    let scope = binding.syntax().parent()?;
    if !ast::StmtList::can_cast(scope.kind()) {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some()
        || pattern.ref_token().is_some()
        || pattern.at_token().is_some()
    {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    if name.starts_with("r#") {
        return None;
    }
    // Count tokens, including token trees, not only call expressions: aliases,
    // shadowing, macro arguments, mutation and capture are all unestablished.
    if function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text().trim_start_matches("r#") == name)
        .count()
        != 2
    {
        return None;
    }
    function
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .find(|call| {
            call.syntax().text_range().start() >= binding.syntax().text_range().end()
                && call
                    .syntax()
                    .ancestors()
                    .skip(1)
                    .find(|node| ast::StmtList::can_cast(node.kind()))
                    .as_ref()
                    == Some(&scope)
                && no_arguments(call)
                && call.expr().is_some_and(|expr| {
                    matches!(expr, ast::Expr::PathExpr(_))
                        && expr.syntax().text().to_string() == name
                })
        })
}

fn no_arguments(call: &ast::CallExpr) -> bool {
    call.arg_list()
        .is_some_and(|args| args.args().next().is_none())
}
