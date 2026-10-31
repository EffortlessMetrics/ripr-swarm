//! Whether a Python source patches, mocks or monkeypatches a given name (#10391).
//!
//! The bare-bool credit needs the test's callee to stay bound to the function
//! it imported. A file-wide `mock` / `patch` substring disqualified nearly
//! every real pytest file, so this pass keeps that veto and exempts only a
//! closed allow-list of occurrences, read from the token stream:
//!
//! - literal-argument `monkeypatch.setenv(` / `delenv(` / `chdir(` calls on an injected
//!   test/fixture parameter (no default, rebinding, or explicit supplying call);
//! - directly assigned zero-argument constructors from unshadowed `unittest.mock` imports;
//! - literal calls and standard inspection/reset methods on their directly
//!   assigned, unrebound objects, with discarded results and no escaping references
//!   or configuration;
//! - `import` / `from .. import` statements (a statement start, no `as`);
//! - a formal parameter (`def test(monkeypatch)`) or a recognized fixture type;
//! - an inert string statement (never a formatted string).
//!
//! Any other name containing `patch`, `mock`, `Mock`, `setattr`, `delattr` or
//! `monkey`, any string containing one of those (or `spy`, `setitem`,
//! `delitem`) outside a docstring, any name that reaches a patcher indirectly
//! (`getattr`, `getfixturevalue`, `__import__`, `__self__`, `builtins`,
//! `partial`; and `setitem`, `delitem`, `spy`, `context`, `syspath_prepend` or an
//! f-string when the file also names a patcher),
//! and any allowed call whose arguments name the callee or owner, contain a
//! backslash, an f-string, adjacent literals, `*args` / `**kwargs` or
//! `autospec`, disqualifies.
//!
//! Accepted residuals: conftest autouse fixtures and a collaborator patched
//! out of sight (both outside the file); a direct `rules.f = fake` assignment
//! is caught by the binding scan, not here.

use rustpython_parser::{
    Mode, Tok,
    ast::{self, Expr, Stmt},
    lexer::lex,
};

const PATCH_MARKERS: [&str; 5] = ["patch", "mock", "Mock", "setattr", "delattr"];

/// Names that reach or apply a patcher without containing a marker.
const INDIRECT_NAMES: [&str; 8] = [
    "getattr",
    "__getattribute__",
    "getfixturevalue",
    "__import__",
    "__builtins__",
    "builtins",
    "__self__",
    "partial",
];

/// Ordinary names that only reach a patcher when the file also names one
/// (`monkeypatch.context()`, `mocker.spy`). Without any patcher in the source
/// they are plain identifiers, so they do not disqualify on their own.
const CONTEXTUAL_NAMES: [&str; 5] = ["setitem", "delitem", "spy", "context", "syspath_prepend"];

/// Constructors that build a mock object without patching a binding.
const MOCK_OBJECT_NAMES: [&str; 7] = [
    "Mock",
    "MagicMock",
    "AsyncMock",
    "PropertyMock",
    "NonCallableMock",
    "NonCallableMagicMock",
    "mock_open",
];

/// These methods on the pytest fixture change the environment or working
/// directory. A locally rebound receiver does not inherit that exemption.
const ENVIRONMENT_VERBS: [&str; 3] = ["setenv", "delenv", "chdir"];

fn has_marker(text: &str) -> bool {
    PATCH_MARKERS.iter().any(|marker| text.contains(marker))
        || text.to_ascii_lowercase().contains("monkey")
}

/// Text that names a patcher, in a string or an f-string.
fn string_names_patcher(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "patch", "mock", "setattr", "delattr", "setitem", "delitem", "spy", "monkey",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

/// Whether `text` holds `name` as a whole identifier.
fn mentions_identifier(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(idx, _)| {
        let word = |ch: char| ch.is_alphanumeric() || ch == '_';
        text[..idx].chars().next_back().is_none_or(|ch| !word(ch))
            && text[idx + name.len()..]
                .chars()
                .next()
                .is_none_or(|ch| !word(ch))
    })
}

/// Whether `source` may patch, mock or rebind any of `names`, or does so in a
/// way this reading cannot follow. A source that does not lex is not trusted.
pub(super) fn may_patch_names(source: &str, names: &[&str], module: &[Stmt]) -> bool {
    let mut tokens: Vec<Tok> = Vec::new();
    for token in lex(source, Mode::Module) {
        match token {
            Ok((token, _)) => tokens.push(token),
            Err(_) => return true,
        }
    }
    let regions = Regions::of(&tokens);
    // An f-string hides expressions inside one token. It can only hide a
    // patcher when the source names one, or an indirect route to one.
    let mentions_patcher = string_names_patcher(source)
        || source.contains("syspath_prepend")
        || INDIRECT_NAMES.iter().any(|name| source.contains(name));
    let bindings = Bindings {
        source,
        module,
        tokens: &tokens,
    };
    let mock_receivers = bindings.mock_receivers();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            // CPython normalizes identifiers with NFKC; this lexer preserves
            // their spelling. Refuse unsupported identity instead of borrowing
            // the ASCII import/receiver proof for a different token spelling.
            Tok::Name { name } if !name.is_ascii() => return true,
            Tok::Name { name } if INDIRECT_NAMES.contains(&name.as_str()) => return true,
            Tok::Name { name } if mentions_patcher && CONTEXTUAL_NAMES.contains(&name.as_str()) => {
                return true;
            }
            Tok::String { value, kind, .. } => {
                // Their expressions are hidden inside a string token, including
                // fixture calls and normalized references to tracked receivers.
                if kind.is_any_fstring() && mentions_patcher {
                    return true;
                }
                let docstring = is_statement_start(&tokens, index)
                    && matches!(tokens.get(index + 1), None | Some(Tok::Newline));
                if !docstring && string_names_patcher(value) {
                    return true;
                }
            }
            _ => {}
        }
        if import_alias_hides_marker(&tokens, &regions, index) {
            return true;
        }
        let Some(chain) = Chain::at(&tokens, index) else {
            continue;
        };
        // A class-held mock also has a direct assignment, but qualified uses
        // are outside the local receiver model. Do not lose it after a dot.
        if chain
            .segments
            .iter()
            .any(|name| mock_receivers.contains(name))
            && !mock_receivers.contains(chain.segments[0])
        {
            return true;
        }
        // A known mock can execute user equality or side-effect protocols if
        // configured, called with arbitrary objects, or passed to a helper.
        // Inspect its uses even when the variable name has no patch marker.
        if let Some(receiver) = chain.segments.first()
            && mock_receivers.contains(receiver)
        {
            if !bindings.mock_binding(&tokens, receiver) {
                return true;
            }
            let assignment = chain.segments.len() == 1
                && is_statement_start(&tokens, index)
                && matches!(tokens.get(chain.end + 1), Some(Tok::Equal));
            let call = matches!(tokens.get(chain.end + 1), Some(Tok::Lpar))
                && discarded_call(&tokens, index, chain.end + 1)
                && (chain.segments.len() == 1 || bindings.mock_inspection(&tokens, &chain))
                && inert_call_arguments(&tokens, chain.end + 1)
                && !call_args_hide_a_target(&tokens, chain.end + 1, names);
            if assignment || call {
                continue;
            }
            return true;
        }
        if !chain.segments.iter().any(|segment| has_marker(segment)) {
            continue;
        }
        if chain_may_patch(&tokens, &regions, &bindings, index, &chain, names) {
            return true;
        }
    }
    false
}

/// Whether the token at `index` begins a statement.
fn is_statement_start(tokens: &[Tok], index: usize) -> bool {
    index == 0
        || matches!(
            tokens[index - 1],
            Tok::Newline | Tok::Indent | Tok::Dedent | Tok::Semi
        )
}

/// Token positions that are parameter lists or import statements.
struct Regions {
    parameters: Vec<bool>,
    annotations: Vec<bool>,
    imports: Vec<bool>,
    declarations: Vec<bool>,
}

impl Regions {
    fn of(tokens: &[Tok]) -> Self {
        let mut regions = Self {
            parameters: vec![false; tokens.len()],
            annotations: vec![false; tokens.len()],
            imports: vec![false; tokens.len()],
            declarations: vec![false; tokens.len()],
        };
        for (index, token) in tokens.iter().enumerate() {
            match token {
                Tok::Def | Tok::Class => {
                    if let Some(slot) = regions.declarations.get_mut(index + 1) {
                        *slot = true;
                    }
                    if matches!(token, Tok::Def)
                        && matches!(tokens.get(index + 2), Some(Tok::Lpar))
                        && let Some(close) = matching_close(tokens, index + 2)
                    {
                        regions.mark_parameters(tokens, index + 3, close);
                    }
                }
                Tok::Lambda => {
                    let end = tokens[index..]
                        .iter()
                        .position(|token| matches!(token, Tok::Colon))
                        .map_or(tokens.len(), |offset| index + offset);
                    regions.mark_parameters(tokens, index + 1, end);
                }
                Tok::Import | Tok::From if is_statement_start(tokens, index) => {
                    let end = tokens[index..]
                        .iter()
                        .position(|token| matches!(token, Tok::Newline | Tok::Semi))
                        .map_or(tokens.len(), |offset| index + offset);
                    regions.imports[index..end].fill(true);
                }
                _ => {}
            }
        }
        regions
    }

    // Only a formal parameter name is exempt. Defaults and annotations are
    // expressions and can retain a patch factory for a later unmarked call.
    fn mark_parameters(&mut self, tokens: &[Tok], start: usize, end: usize) {
        let mut depth = 0usize;
        let mut parameter_start = true;
        for index in start..end {
            match &tokens[index] {
                Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
                Tok::Rpar | Tok::Rsqb | Tok::Rbrace => depth = depth.saturating_sub(1),
                Tok::Comma if depth == 0 => parameter_start = true,
                Tok::Name { .. } if depth == 0 && parameter_start => {
                    self.parameters[index] = true;
                    parameter_start = false;
                    if matches!(tokens.get(index + 1), Some(Tok::Colon))
                        && let Some(slot) = self.annotations.get_mut(index + 2)
                    {
                        *slot = true;
                    }
                }
                _ => {}
            }
        }
    }
}

struct Bindings<'a> {
    source: &'a str,
    module: &'a [Stmt],
    tokens: &'a [Tok],
}

impl Bindings<'_> {
    fn mock_receivers(&self) -> std::collections::BTreeSet<&str> {
        self.tokens
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                let Tok::Name { name } = token else {
                    return None;
                };
                if !is_statement_start(self.tokens, index)
                    || !matches!(self.tokens.get(index + 1), Some(Tok::Equal))
                {
                    return None;
                }
                let constructor = Chain::at(self.tokens, index + 2)?;
                (self.mock_constructor(&constructor)
                    && matches!(self.tokens.get(constructor.end + 1), Some(Tok::Lpar))
                    && matches!(self.tokens.get(constructor.end + 2), Some(Tok::Rpar)))
                .then_some(name.as_str())
            })
            .collect()
    }
    fn imported(&self, root: &str, module: &str, member: Option<&str>) -> bool {
        !for_target_binds(self.tokens, root)
            && super::count_bindings(self.module, root) == 1
            && !super::binds_by_walrus(self.source, root)
            && self
                .module
                .iter()
                .any(|statement| match (statement, member) {
                    (Stmt::ImportFrom(import), Some(member)) => {
                        import.level.is_none_or(|level| level.to_u32() == 0)
                            && import
                                .module
                                .as_ref()
                                .is_some_and(|name| name.as_str() == module)
                            && import.names.iter().any(|alias| {
                                alias.name.as_str() == member && alias.asname.is_none()
                            })
                    }
                    (Stmt::Import(import), None) => import
                        .names
                        .iter()
                        .any(|alias| alias.name.as_str() == module && alias.asname.is_none()),
                    _ => false,
                })
    }

    fn mock_constructor(&self, chain: &Chain<'_>) -> bool {
        let Some(verb) = chain.segments.last() else {
            return false;
        };
        MOCK_OBJECT_NAMES.contains(verb)
            && match chain.segments.as_slice() {
                [name] => self.imported(name, "unittest.mock", Some(name)),
                ["mock", _] => self.imported("mock", "unittest", Some("mock")),
                ["unittest", "mock", _] => self.imported("unittest", "unittest.mock", None),
                _ => false,
            }
    }

    fn fixture_annotation(&self, chain: &Chain<'_>) -> bool {
        match chain.segments.as_slice() {
            ["pytest", "MonkeyPatch"] => self.imported("pytest", "pytest", None),
            ["pytest_mock", "MockerFixture"] => self.imported("pytest_mock", "pytest_mock", None),
            _ => false,
        }
    }

    fn mock_binding(&self, tokens: &[Tok], receiver: &str) -> bool {
        !for_target_binds(tokens, receiver)
            && super::count_bindings(self.module, receiver) == 1
            && !super::binds_by_walrus(self.source, receiver)
            && tokens.iter().enumerate().any(|(index, token)| {
                if !is_statement_start(tokens, index)
                    || !matches!(token, Tok::Name { name } if name == receiver)
                    || !matches!(tokens.get(index + 1), Some(Tok::Equal))
                {
                    return false;
                }
                let Some(constructor) = Chain::at(tokens, index + 2) else {
                    return false;
                };
                self.mock_constructor(&constructor)
                    && matches!(tokens.get(constructor.end + 1), Some(Tok::Lpar))
                    && matches!(tokens.get(constructor.end + 2), Some(Tok::Rpar))
                    && matches!(
                        tokens.get(constructor.end + 3),
                        Some(Tok::Newline | Tok::Semi)
                    )
            })
    }

    fn mock_inspection(&self, tokens: &[Tok], chain: &Chain<'_>) -> bool {
        let [receiver, verb] = chain.segments.as_slice() else {
            return false;
        };
        matches!(
            *verb,
            "assert_called"
                | "assert_called_once"
                | "assert_called_with"
                | "assert_called_once_with"
                | "assert_any_call"
                | "assert_has_calls"
                | "assert_not_called"
                | "assert_awaited"
                | "assert_awaited_once"
                | "assert_awaited_with"
                | "assert_awaited_once_with"
                | "assert_any_await"
                | "assert_has_awaits"
                | "assert_not_awaited"
                | "reset_mock"
        ) && self.mock_binding(tokens, receiver)
    }

    fn constructor_assignment(&self, tokens: &[Tok], start: usize) -> bool {
        let Some(receiver_index) = start.checked_sub(2) else {
            return false;
        };
        is_statement_start(tokens, receiver_index)
            && matches!(tokens.get(start - 1), Some(Tok::Equal))
            && matches!(&tokens[receiver_index], Tok::Name { name }
                if self.mock_binding(tokens, name))
    }

    fn injection_site(
        &self,
        function: &str,
        args: &ast::Arguments,
        decorators: &[Expr],
        name: &str,
    ) -> bool {
        let fixture = decorators.len() == 1 && self.imported("pytest", "pytest", None)
            && decorators.iter().all(|decorator| {
                let target = match decorator { Expr::Call(call) => &*call.func, other => other };
                matches!(target, Expr::Attribute(attribute) if attribute.attr.as_str() == "fixture"
                    && matches!(&*attribute.value, Expr::Name(root) if root.id.as_str() == "pytest"))
            });
        (fixture || function.starts_with("test_") && decorators.is_empty())
            && args
                .args
                .iter()
                .chain(&args.kwonlyargs)
                .any(|arg| arg.def.arg.as_str() == name && arg.default.is_none())
    }

    fn injection_sites<'a>(&self, body: &'a [Stmt], name: &str, sites: &mut Vec<&'a str>) {
        for statement in body {
            match statement {
                Stmt::FunctionDef(function)
                    if self.injection_site(
                        &function.name,
                        &function.args,
                        &function.decorator_list,
                        name,
                    ) =>
                {
                    sites.push(&function.name)
                }
                Stmt::AsyncFunctionDef(function)
                    if self.injection_site(
                        &function.name,
                        &function.args,
                        &function.decorator_list,
                        name,
                    ) =>
                {
                    sites.push(&function.name)
                }
                Stmt::ClassDef(class) => self.injection_sites(&class.body, name, sites),
                _ => {}
            }
        }
    }

    fn fixture_parameter(&self, tokens: &[Tok], regions: &Regions, name: &str) -> bool {
        let mut sites = Vec::new();
        self.injection_sites(self.module, name, &mut sites);
        !sites.is_empty()
            && super::count_bindings(self.module, name) == sites.len()
            && !super::binds_by_walrus(self.source, name)
            // An explicit call or alias can supply an arbitrary receiver instead
            // of pytest's fixture. Fixture dependencies may name another parameter.
            && !tokens.iter().enumerate().any(|(index, token)| {
                matches!(token, Tok::Name { name } if sites.contains(&name.as_str()))
                    && !regions.declarations[index] && !regions.parameters[index]
            })
    }
}

/// `import patch as p` / `from unittest import mock as m` hides a patch-like
/// name behind an alias this reading would not recognize later.
fn import_alias_hides_marker(tokens: &[Tok], regions: &Regions, index: usize) -> bool {
    matches!(tokens[index], Tok::As)
        && regions.imports[index]
        && index > 0
        && matches!(&tokens[index - 1], Tok::Name { name } if has_marker(name))
}

/// A dotted name `a.b.c`, with the index of its last token.
struct Chain<'a> {
    segments: Vec<&'a str>,
    end: usize,
}

impl<'a> Chain<'a> {
    /// The dotted name starting at `index`, when a name starts there. A name
    /// after a dot starts a chain only when the dot follows a non-name (`f().mock`).
    fn at(tokens: &'a [Tok], index: usize) -> Option<Self> {
        let Tok::Name { name } = tokens.get(index)? else {
            return None;
        };
        if index >= 1 && matches!(tokens[index - 1], Tok::Dot) {
            let after_name = index >= 2 && matches!(tokens[index - 2], Tok::Name { .. });
            if after_name {
                return None;
            }
        }
        let mut segments = vec![name.as_str()];
        let mut end = index;
        while matches!(tokens.get(end + 1), Some(Tok::Dot))
            && let Some(Tok::Name { name }) = tokens.get(end + 2)
        {
            segments.push(name.as_str());
            end += 2;
        }
        Some(Self { segments, end })
    }
}

fn chain_may_patch(
    tokens: &[Tok],
    regions: &Regions,
    bindings: &Bindings<'_>,
    start: usize,
    chain: &Chain<'_>,
    names: &[&str],
) -> bool {
    if regions.declarations[start] || regions.imports[start] {
        return false;
    }
    let verb = chain.segments.last().copied().unwrap_or("");
    if matches!(tokens.get(chain.end + 1), Some(Tok::Lpar)) {
        let allowed = (chain.segments.len() == 2
            && chain.segments[0] == "monkeypatch"
            && ENVIRONMENT_VERBS.contains(&verb)
            && bindings.fixture_parameter(tokens, regions, "monkeypatch")
            && inert_call_arguments(tokens, chain.end + 1))
            || (bindings.mock_constructor(chain)
                && matches!(tokens.get(chain.end + 2), Some(Tok::Rpar))
                && bindings.constructor_assignment(tokens, start))
            || (bindings.mock_inspection(tokens, chain)
                && discarded_call(tokens, start, chain.end + 1)
                && inert_call_arguments(tokens, chain.end + 1));
        return !allowed || call_args_hide_a_target(tokens, chain.end + 1, names);
    }
    // A type annotation may be a patch factory, so only recognized fixture
    // types qualify. A dotted chain is never a formal parameter name.
    let mock_assignment = chain.segments.len() == 1
        && is_statement_start(tokens, start)
        && matches!(tokens.get(chain.end + 1), Some(Tok::Equal))
        && bindings.mock_binding(tokens, chain.segments[0]);
    !(mock_assignment
        || regions.parameters[start] && chain.segments.len() == 1
        || regions.annotations[start] && bindings.fixture_annotation(chain))
}

/// A mock call can return another configurable mock. Do not let that result
/// escape into an assignment, argument, return, or chained expression.
fn discarded_call(tokens: &[Tok], start: usize, open: usize) -> bool {
    is_statement_start(tokens, start)
        && matching_close(tokens, open).is_some_and(|close| {
            matches!(tokens.get(close + 1), None | Some(Tok::Newline | Tok::Semi))
        })
}

/// Comprehension targets have their own scope and are not statement bindings.
/// Refuse any for-target with this name rather than borrow a different scope's
/// import or constructor assignment. The input has already parsed successfully.
fn for_target_binds(tokens: &[Tok], name: &str) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        matches!(token, Tok::For)
            && tokens[index + 1..]
                .iter()
                .take_while(|token| !matches!(token, Tok::In))
                .any(|token| matches!(token, Tok::Name { name: bound } if bound == name))
    })
}

/// Real environment and mock APIs can invoke __fspath__, __str__ or __eq__.
/// Only primitive literal arguments avoid user-defined protocol execution.
fn inert_call_arguments(tokens: &[Tok], open: usize) -> bool {
    let Some(close) = matching_close(tokens, open) else {
        return false;
    };
    tokens[open + 1..close]
        .iter()
        .enumerate()
        .all(|(offset, token)| match token {
            Tok::String { kind, .. } => !kind.is_any_fstring(),
            Tok::Int { .. }
            | Tok::Float { .. }
            | Tok::True
            | Tok::False
            | Tok::None
            | Tok::Comma
            | Tok::Equal => true,
            Tok::Name { .. } => matches!(tokens.get(open + offset + 2), Some(Tok::Equal)),
            _ => false,
        })
}

fn call_args_hide_a_target(tokens: &[Tok], open: usize, names: &[&str]) -> bool {
    matching_close(tokens, open)
        .is_none_or(|close| args_name_a_target_or_hide_one(&tokens[open + 1..close], names))
}

/// Any identifier or string in the arguments that names a target, or a form
/// (`*args`, f-string, adjacent literals, `autospec`) whose target is unreadable.
fn args_name_a_target_or_hide_one(args: &[Tok], names: &[&str]) -> bool {
    let mut previous_was_string = false;
    for token in args {
        let is_string = matches!(token, Tok::String { .. });
        let hides = match token {
            Tok::Name { name } => {
                name.contains("autospec") || names.iter().any(|callee| name == callee)
            }
            Tok::String { value, kind, .. } => {
                kind.is_any_fstring()
                    // An escape or line continuation can spell the name unreadably.
                    || value.contains('\\')
                    || previous_was_string
                    || names
                        .iter()
                        .any(|callee| mentions_identifier(value, callee))
            }
            Tok::Star | Tok::DoubleStar => true,
            _ => false,
        };
        if hides {
            return true;
        }
        previous_was_string = is_string;
    }
    false
}

/// The index of the bracket that closes the one at `open`.
fn matching_close(tokens: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, token) in tokens.get(open..)?.iter().enumerate() {
        match token {
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}
