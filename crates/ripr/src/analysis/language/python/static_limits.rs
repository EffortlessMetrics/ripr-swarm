use super::PythonFramework;
use super::admission::PythonAssertionAdmission;
use super::approx_band::{PythonBandReading, parse_expression, python_band_reading};
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::caplog_text::{PythonLogEvidence, substitute_caplog_aliases};
use super::effect_alignment::ChangedEffect;
use super::forwarding_decorator::is_forwarding_decorator;
use super::literal_patch::{literal_patch_calls, without_calls};
use super::related_tests::{
    PythonRelatedCandidate, PythonRelationKind, body_calls_owner, dunder_method_class,
    import_source_module_matches_owner, is_python_identifier_char,
    line_prefix_looks_like_comment_or_string, test_imports_owner_module,
    test_may_reach_owner_class,
};
use super::sink_alignment::CreditedAssertion;
use super::source_utils::SourceText;
use super::{
    PythonImport, PythonOracleShape, PythonOwner, PythonTest, expr_full_name,
    split_python_assignment,
};
use crate::domain::{OracleStrength, StaticLimitKind};
use rustpython_parser::ast::{self, Expr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonStaticLimit {
    pub(super) kind: StaticLimitKind,
    pub(super) evidence: String,
    pub(super) missing: String,
}

/// The static limit for a changed line without its old text. Callers that
/// have the finding's assertion focus use [`static_limit_for_focused_change`]
/// so a changed dict key is localized the same way the rows are.
#[cfg(test)]
pub(super) fn static_limit_for_change(
    line_text: &str,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<PythonStaticLimit> {
    let focus = PythonAssertionFocus::for_change(
        super::probe_shape::classify_probe_shape(line_text).0,
        line_text,
        None,
    );
    static_limit_for_focused_change(line_text, None, None, &focus, owner, "", related_candidates)
}

/// A test that rebinds the owner's module object (`route.response = fake`,
/// `importlib.reload`, `setattr`, `__setattr__`, `delattr`) reads a different
/// object than the owner mutates. Withholding credit alone leaves a weak
/// finding whose repair card asserts on the stale import, so the finding is
/// withheld as a runtime-substitution limit like `patch` (#10289).
///
/// `credited` is the test the finding's observation credit comes from. A
/// credit from a test that does not rebind the object is an independent live
/// read, so a sibling's rebinding does not take it away (#10354); with no
/// credit, or credit from a rebinding test, the limit holds (fail closed).
pub(super) fn module_object_rebind_limit(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    credited: Option<&CreditedAssertion>,
) -> Option<PythonStaticLimit> {
    let ChangedEffect::ModuleField { object, attr } =
        ChangedEffect::of(line_text, old_line_text, owner, false)?
    else {
        return None;
    };
    let rebinds = |test: &PythonTest| test.constant_rebinding.assigns_attribute(&object);
    // Only a credit that reads the changed field through the object's own
    // import is a live read: a return or other assertion that merely names
    // the owner does not discriminate the write (#10354).
    let live_credit = credited.is_some_and(|credited| {
        related_candidates.iter().any(|candidate| {
            let test = candidate.test;
            test.name == credited.test_name
                && test.file == credited.test_file
                && !rebinds(test)
                && test.imports.iter().any(|import| {
                    import.imported == object
                        && import_source_module_matches_owner(import, owner, &test.file)
                        && credited.text.contains(&format!("{}.{attr}", import.alias))
                })
        })
    });
    if live_credit {
        return None;
    }
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            let test = candidate.test;
            // Only a test that imports the object can read a stale one.
            rebinds(test)
                && test.imports.iter().any(|import| {
                    import.imported == object
                        && import_source_module_matches_owner(import, owner, &test.file)
                })
        })
        .then(|| PythonStaticLimit {
            kind: StaticLimitKind::MockedModule,
            evidence: format!(
                "static_limit mocked_module: related test rebinds the module namespace that holds `{object}`"
            ),
            missing: format!(
                "Static limit `mocked_module`: a related Python test reloads the module or replaces its names (`{object}`) by assignment, `setattr`, `__setattr__` or `delattr`; the preview adapter does not resolve which object an imported name reads afterwards."
            ),
        })
}

/// `focus` is the one the finding's rows are selected with (#5572): only the
/// assertion a row selects can suppress a test-side limit, so a strong
/// sibling-field or other-family assertion never hides one.
pub(super) fn static_limit_for_focused_change(
    line_text: &str,
    old_line_text: Option<&str>,
    log_evidence: Option<&PythonLogEvidence>,
    focus: &PythonAssertionFocus,
    owner: &PythonOwner,
    owner_source: &str,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<PythonStaticLimit> {
    let trimmed = line_text.trim();
    if contains_dynamic_dispatch(trimmed) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DynamicDispatch,
            evidence: "static_limit dynamic_dispatch: changed line uses dynamic call dispatch"
                .to_string(),
            missing: "Static limit `dynamic_dispatch`: the Python preview adapter saw a dynamic call shape such as `getattr(...)` or `registry[key](...)`; syntax alone cannot resolve the called behavior.".to_string(),
        });
    }
    if contains_dynamic_import(trimmed) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::MissingImportGraph,
            evidence: "static_limit missing_import_graph: changed line uses dynamic import syntax"
                .to_string(),
            missing: "Static limit `missing_import_graph`: the changed line uses dynamic import syntax such as `importlib.import_module(...)` or `__import__(...)`; the Python preview adapter does not build an import graph or resolve imported implementation semantics.".to_string(),
        });
    }
    if contains_metaprogramming(trimmed) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::Metaprogramming,
            evidence: "static_limit metaprogramming: changed line uses metaprogramming syntax"
                .to_string(),
            missing: "Static limit `metaprogramming`: the Python preview adapter saw metaprogramming syntax and does not infer runtime-created behavior.".to_string(),
        });
    }
    if let Some(decorator) = owner.dynamic_route_decorators.first() {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DecoratorIndirection,
            evidence: format!(
                "static_limit decorator_indirection: dynamic_route_registration `{decorator}`"
            ),
            missing: format!(
                "Static limit `dynamic_route_registration`: owner `{}` uses dynamic route registration `{decorator}`; syntax-first preview evidence cannot safely match client calls to a concrete route path.",
                owner.qualified_name
            ),
        });
    }
    if let Some(decorator) = owner.decorators.iter().find(|decorator| {
        !is_transparent_owner_decorator_for_owner(decorator, owner)
            && !is_forwarding_decorator(decorator, owner_source)
    }) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DecoratorIndirection,
            evidence: format!("static_limit decorator_indirection: `{decorator}`"),
            missing: format!(
                "Static limit `decorator_indirection`: owner `{}` is decorated with `{decorator}`; syntax-first preview evidence does not resolve decorator-modified call behavior.",
                owner.qualified_name
            ),
        });
    }
    // A route decorator edit changes where the handler is registered, which
    // only a client call to a route observes; a test that calls the function
    // directly never sees it (#9756).
    if is_route_decorator_edit(trimmed)
        && !related_candidates.iter().any(|candidate| {
            matches!(
                candidate.relation,
                PythonRelationKind::ApiClientRouteCall
                    | PythonRelationKind::ApiClientRouteUnidentifiedApp
            )
        })
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DecoratorIndirection,
            evidence: "static_limit decorator_indirection: route registration edit".to_string(),
            missing: format!(
                "Static limit `decorator_indirection`: the changed line edits the route registration of `{}`; no related test calls the route through an API client, so a direct call to the handler cannot observe the edit.",
                owner.qualified_name
            ),
        });
    }
    // A request client whose application the adapter cannot tie to the owner's
    // route registration reaches the route without evidence that the owner's
    // application serves it: the test may discriminate the change or may
    // exercise another application, so neither a gap nor a credit is claimed
    // (#10110).
    if !related_candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle())
        && let Some(candidate) = related_candidates.iter().find(|candidate| {
            candidate.relation == PythonRelationKind::ApiClientRouteUnidentifiedApp
        })
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::MissingImportGraph,
            evidence: format!(
                "static_limit missing_import_graph: the request client in `{}` is not tied to the application `{}` is registered on",
                candidate.test.qualified_name, owner.qualified_name
            ),
            missing: format!(
                "Static limit `missing_import_graph`: related test `{}` calls the route of `{}` through a request client whose application the Python preview adapter does not resolve (a fixture it cannot read, a factory-built app, an app defined in the test or a router included elsewhere); it does not build an import graph through them. Build the client over the application the route is registered on, or inspect which application the test exercises.",
                candidate.test.qualified_name, owner.qualified_name
            ),
        });
    }
    // An independent active relation can still supply its own oracle. The
    // shared matcher withholds assertions and boundary inputs from controlled
    // tests, so those tests cannot lend credit to a weaker active relation.
    if !related_candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle())
        && let Some((test, decorator)) = related_candidates.iter().find_map(|candidate| {
            super::test_activation::activation_control(candidate.test)
                .map(|decorator| (candidate.test, decorator))
        })
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DecoratorIndirection,
            evidence: format!(
                "static_limit decorator_indirection: test activation for `{}` is controlled by `{decorator}`",
                test.qualified_name
            ),
            missing: format!(
                "Static limit `decorator_indirection`: related test `{}` uses `{decorator}`; static evidence does not establish that its body runs and an assertion failure fails verification. Add an independent active test or inspect the activation condition.",
                test.qualified_name
            ),
        });
    }
    // A mocked test keeps the limit unless its substitutions are literal
    // zero-argument stand-ins the edit cannot hide behind (`literal_patch`).
    let mut substituted_calls: Vec<String> = Vec::new();
    let mut mocked = false;
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .filter(|candidate| test_has_mocked_module(candidate.test))
    {
        match literal_patch_calls(
            candidate.test,
            owner,
            owner_source,
            old_line_text,
            line_text,
        ) {
            Some(calls) => substituted_calls.extend(calls),
            None => mocked = true,
        }
    }
    if mocked {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::MockedModule,
            evidence: "static_limit mocked_module: related test uses patch/mock/monkeypatch module syntax"
                .to_string(),
            missing: "Static limit `mocked_module`: a related Python test uses patch/mock/monkeypatch module syntax; the preview adapter does not resolve runtime substitution semantics.".to_string(),
        });
    }
    if related_candidates_have_property_based_test_limit(related_candidates, focus) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::PropertyBasedTest,
            evidence: "static_limit property_based_test: related test uses generated inputs"
                .to_string(),
            missing: "Static limit `property_based_test`: a related Python test uses property-based generated inputs such as `@given(...)`; syntax-first preview evidence cannot prove whether the generated cases include the changed discriminator.".to_string(),
        });
    }
    if related_candidates_have_unresolved_pytest_fixture_limit(owner, related_candidates) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::UnresolvedPytestFixture,
            evidence:
                "static_limit unresolved_pytest_fixture: related test uses fixture-sourced values"
                    .to_string(),
            missing: "Static limit `unresolved_pytest_fixture`: a related pytest test depends on fixture-sourced values; syntax-first preview evidence cannot prove whether the fixture supplies the changed discriminator or expected value.".to_string(),
        });
    }
    if trimmed.chars().any(|ch| ch.is_ascii_digit())
        && matches!(
            python_band_reading(old_line_text, trimmed, owner, related_candidates, focus),
            PythonBandReading::Unread
        )
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::UnsupportedSyntax,
            evidence: "static_limit unsupported_syntax: related test compares through an `approx` tolerance band"
                .to_string(),
            missing: "Static limit `unsupported_syntax`: a related Python test compares the result with `approx(..., rel=..., abs=...)`; the preview adapter does not evaluate the tolerance band, so it cannot tell whether a numeric edit stays inside it. Verify with a runtime test or tighten the tolerance.".to_string(),
        });
    }
    if log_evidence.is_none()
        && changed_logging_call_literal(trimmed).is_some_and(|literal| {
            related_candidates_have_log_membership_limit(related_candidates, focus, literal)
        })
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::UnsupportedSyntax,
            evidence: "static_limit unsupported_syntax: related test reads the changed log message through `caplog` membership"
                .to_string(),
            missing: "Static limit `unsupported_syntax`: a related Python test checks `<text> in caplog.text` against the changed logging call; the preview adapter does not evaluate `%`-formatted message arguments or the `caplog` level filter, so it cannot tell whether the edit changes the logged text the test reads.".to_string(),
        });
    }
    if related_candidates_have_opaque_custom_assertion_limit(owner, related_candidates, focus) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::OpaqueCustomAssertionHelper,
            evidence: "static_limit opaque_custom_assertion_helper: related test uses an opaque custom assertion helper"
                .to_string(),
            missing: "Static limit `opaque_custom_assertion_helper`: a related Python test uses a custom assertion helper such as `assert_*(...)`; the preview adapter cannot inspect the helper body or determine whether it already observes the changed discriminator.".to_string(),
        });
    }
    if line_uses_imported_symbol(&without_calls(trimmed, &substituted_calls), &owner.imports) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::MissingImportGraph,
            evidence: "static_limit missing_import_graph: changed line calls an imported symbol"
                .to_string(),
            missing: "Static limit `missing_import_graph`: the changed line calls an imported symbol; the Python preview adapter does not build an import graph or resolve imported implementation semantics.".to_string(),
        });
    }
    if contains_lambda_keyword(trimmed) {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::UnsupportedSyntax,
            evidence: "static_limit unsupported_syntax: changed line uses lambda syntax"
                .to_string(),
            missing: "Static limit `unsupported_syntax`: the changed line uses a Python syntax shape this preview adapter does not model precisely yet.".to_string(),
        });
    }
    None
}

/// A `lambda` expression: the keyword at an identifier boundary and followed
/// by a space, `:` or `(`. Text in a string is read as text only when the line
/// is a plain one-line string literal return (`return "lambda x"`): no braces
/// (an f-string expression or a dict could hold code), no triple quotes (the
/// line may close a string opened above) and no other quote state to track.
fn contains_lambda_keyword(text: &str) -> bool {
    let plain_line =
        !text.contains(['{', '\\']) && !text.contains("\"\"\"") && !text.contains("'''");
    text.match_indices("lambda").any(|(idx, _)| {
        let before = text[..idx].chars().next_back();
        let after = text[idx + "lambda".len()..].chars().next();
        before.is_none_or(|ch| !is_python_identifier_char(ch))
            && after.is_some_and(|ch| ch == ' ' || ch == ':' || ch == '(')
            && !(plain_line && python_prefix_hides_code(line_prefix_before(text, idx)))
    })
}

/// A dunder method owner (`Cache.__setitem__`) with no related test, in a
/// workspace whose tests import the owner class or module. Python calls these
/// methods through syntax (`cache[key] = value`, `Cache(...)`) on instances the
/// adapter cannot always bind to the class — a unittest mixin's `self.Cache`
/// attribute or a test-local subclass — so "no static test path" is not a safe
/// claim. Returns `None` when a test relates, or when no test imports the
/// owner's class or module and none names the class (such a class keeps
/// `no_static_path`).
pub(super) fn implicit_dunder_dispatch_limit(
    owner: &PythonOwner,
    all_tests: &[PythonTest],
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<PythonStaticLimit> {
    if !related_candidates.is_empty() {
        return None;
    }
    let class = dunder_method_class(owner)?;
    all_tests
        .iter()
        .any(|test| test_may_reach_owner_class(test, owner, class))
        .then(|| PythonStaticLimit {
            kind: StaticLimitKind::DynamicDispatch,
            evidence: format!(
                "static_limit dynamic_dispatch: dunder method `{}` is invoked implicitly by Python syntax",
                owner.qualified_name
            ),
            missing: format!(
                "Static limit `dynamic_dispatch`: `{}` is a dunder method Python invokes through syntax on an instance of `{class}`; tests import `{class}`, its module or its package, or name `{class}`, but the preview adapter cannot bind the instance a test builds to the class, so it does not claim that no test reaches the owner.",
                owner.qualified_name
            ),
        })
}

pub(super) fn contains_dynamic_dispatch(text: &str) -> bool {
    text.contains("getattr(") || (text.contains('[') && text.contains("]("))
}

pub(super) fn contains_dynamic_import(text: &str) -> bool {
    contains_python_call_shape(text, "importlib.import_module")
        || contains_python_call_shape(text, "__import__")
}

pub(super) fn contains_python_call_shape(text: &str, callee: &str) -> bool {
    text.match_indices(callee).any(|(idx, _)| {
        python_callee_start_has_boundary(text, idx)
            && text[idx + callee.len()..].trim_start().starts_with('(')
            && !python_prefix_hides_code(line_prefix_before(text, idx))
    })
}

pub(super) fn python_callee_start_has_boundary(text: &str, idx: usize) -> bool {
    text[..idx]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_python_identifier_char(ch) && ch != '.')
}

pub(super) fn line_prefix_before(text: &str, idx: usize) -> &str {
    text[..idx]
        .rsplit_once('\n')
        .map_or(&text[..idx], |(_, line)| line)
}

/// Whether the end of `prefix` (the text of a line before a position) sits in
/// a comment or an open string. A tripled quote opens a string that only the
/// same tripled delimiter closes, so `'''it's'''` is one balanced literal and
/// code after it is visible (#9992); a triple string left open at the end of
/// the prefix hides what follows. F-string fields are not read here.
pub(super) fn python_prefix_hides_code(prefix: &str) -> bool {
    let chars: Vec<char> = prefix.chars().collect();
    // The active delimiter and its width (1 or 3).
    let mut open: Option<(char, usize)> = None;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if let Some((quote, width)) = open {
            let closes = ch == quote
                && chars.len() - index >= width
                && chars[index..index + width].iter().all(|c| *c == quote);
            if ch == '\\' {
                index += 2;
            } else if closes {
                open = None;
                index += width;
            } else {
                index += 1;
            }
            continue;
        }
        if ch == '#' {
            return true;
        }
        if ch == '\'' || ch == '"' {
            let triple = chars.get(index + 1) == Some(&ch) && chars.get(index + 2) == Some(&ch);
            let width = if triple { 3 } else { 1 };
            open = Some((ch, width));
            index += width;
            continue;
        }
        index += 1;
    }
    open.is_some()
}

pub(super) fn contains_metaprogramming(text: &str) -> bool {
    text.contains("__getattr__")
        || contains_call_at_name_boundary(text, "type(")
        || contains_call_at_name_boundary(text, "setattr(")
        || contains_metaclass_declaration(text)
}

/// `call` (`type(`) where the name starts at an identifier boundary:
/// `type(x)` and `builtins.type(x)`, not `content_type(x)`. Text in a string
/// still counts: a boundary is the only thing read, so an f-string expression
/// or a multi-line string can never hide a real call.
fn contains_call_at_name_boundary(text: &str, call: &str) -> bool {
    text.match_indices(call).any(|(idx, _)| {
        !text[..idx]
            .chars()
            .next_back()
            .is_some_and(is_python_identifier_char)
    })
}

fn contains_metaclass_declaration(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("class ") {
        return false;
    }
    contains_python_keyword_assignment_shape(text, "metaclass")
}

fn contains_python_keyword_assignment_shape(text: &str, keyword: &str) -> bool {
    text.match_indices(keyword).any(|(idx, _)| {
        python_callee_start_has_boundary(text, idx)
            && text[idx + keyword.len()..].trim_start().starts_with('=')
            && !python_prefix_hides_code(line_prefix_before(text, idx))
    })
}

pub(super) fn is_transparent_owner_decorator(decorator: &str) -> bool {
    decorator == "staticmethod"
        || decorator == "classmethod"
        || decorator == "async_def"
        || is_static_route_decorator(decorator)
        || is_static_cli_decorator(decorator)
}

pub(super) fn is_transparent_owner_decorator_for_owner(
    decorator: &str,
    owner: &PythonOwner,
) -> bool {
    is_transparent_owner_decorator(decorator)
        || is_static_cli_decorator_with_import_context(
            decorator,
            &owner.imports,
            &owner.cli_receiver_names,
        )
        || is_imported_functools_memoization_decorator(decorator, &owner.imports)
        || (is_imported_typing_marker_decorator(decorator, &owner.imports)
            && !owner
                .unplain_decorators
                .iter()
                .any(|name| name == decorator))
}

const TYPING_MODULES: [&str; 2] = ["typing", "typing_extensions"];

/// Typing markers return the decorated function unchanged (they only set an
/// attribute or do nothing), so test-to-owner call matching is unaffected.
/// `typing.overload`, `abc.abstractmethod`, `contextlib.contextmanager` and
/// `property` are deliberately excluded: they replace, block or re-shape the
/// call.
fn is_typing_marker_name(module: &str, name: &str) -> bool {
    match name {
        "final" | "override" => TYPING_MODULES.contains(&module),
        "no_type_check" => module == "typing",
        _ => false,
    }
}

fn import_binds_typing_module(import: &PythonImport) -> bool {
    TYPING_MODULES.contains(&import.imported.as_str()) && import.source_module.is_empty()
}

/// `@final` / `@override` / `@no_type_check` (and `typing.` / `typing_extensions.`
/// qualified spellings) only when the name resolves to the typing module. A
/// receiver or bare name rebound from another module, or bound by a competing
/// import, stays limited, mirroring the functools memoization rule.
fn is_imported_typing_marker_decorator(decorator: &str, imports: &[PythonImport]) -> bool {
    if let Some((receiver, method)) = decorator.rsplit_once('.') {
        let rebound = imports
            .iter()
            .any(|import| import.alias == receiver && !import_binds_typing_module(import));
        if rebound {
            return false;
        }
        if TYPING_MODULES.contains(&receiver) {
            return is_typing_marker_name(receiver, method);
        }
        return imports.iter().any(|import| {
            import_binds_typing_module(import)
                && import.alias == receiver
                && is_typing_marker_name(import.imported.as_str(), method)
        });
    }
    let from_typing = imports.iter().any(|import| {
        import.alias == decorator
            && is_typing_marker_name(import.source_module.as_str(), import.imported.as_str())
    });
    let competing_alias = imports.iter().any(|import| {
        import.alias == decorator
            && !is_typing_marker_name(import.source_module.as_str(), import.imported.as_str())
    });
    from_typing && !competing_alias
}

fn is_functools_memoization_name(name: &str) -> bool {
    matches!(name, "lru_cache" | "cache" | "cached_property")
}

fn import_binds_stdlib_functools_module(import: &PythonImport) -> bool {
    import.imported == "functools" && import.source_module.is_empty()
}

/// `import wrappers as functools` / `from helpers import functools` rebound
/// the name; a plain `import functools` did not.
fn receiver_is_rebound_from_non_stdlib_functools(imports: &[PythonImport], receiver: &str) -> bool {
    imports
        .iter()
        .any(|import| import.alias == receiver && !import_binds_stdlib_functools_module(import))
}

/// Bare `@lru_cache` / `@cache` / `@cached_property` only when the name is
/// bound from `functools` and not also bound from another module. A
/// same-named local decorator stays limited. `@functools.lru_cache` is the
/// stdlib qualified spelling unless `functools` is rebound (`import wrappers
/// as functools` or `from helpers import functools`). `import functools as
/// ft` then `@ft.lru_cache` is the same binding unless `ft` is also rebound.
/// Call arguments are already dropped by `expr_full_name`.
fn is_imported_functools_memoization_decorator(decorator: &str, imports: &[PythonImport]) -> bool {
    if let Some((receiver, method)) = decorator.rsplit_once('.') {
        if !is_functools_memoization_name(method) {
            return false;
        }
        if receiver_is_rebound_from_non_stdlib_functools(imports, receiver) {
            return false;
        }
        if receiver == "functools" {
            return true;
        }
        return imports.iter().any(|import| {
            import_binds_stdlib_functools_module(import) && import.alias == receiver
        });
    }
    let from_functools = imports.iter().any(|import| {
        import.source_module == "functools"
            && is_functools_memoization_name(import.imported.as_str())
            && import.alias == decorator
    });
    let competing_alias = imports
        .iter()
        .any(|import| import.alias == decorator && import.source_module != "functools");
    from_functools && !competing_alias
}

/// A changed line that is itself a static route decorator (`@app.get("/a")`).
fn is_route_decorator_edit(trimmed: &str) -> bool {
    trimmed
        .strip_prefix('@')
        .and_then(|rest| rest.split('(').next())
        .is_some_and(is_static_route_decorator)
}

pub(super) fn is_static_route_decorator(decorator: &str) -> bool {
    let Some((receiver, method)) = decorator.rsplit_once('.') else {
        return false;
    };
    if !matches!(
        method,
        "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "options"
            | "head"
            | "route"
            | "api_route"
            | "websocket"
    ) {
        return false;
    }

    let Some(receiver_name) = receiver.rsplit('.').next() else {
        return false;
    };
    matches!(
        receiver_name,
        "app" | "api" | "router" | "routes" | "bp" | "blueprint"
    ) || receiver_name.ends_with("_app")
        || receiver_name.ends_with("_api")
        || receiver_name.ends_with("_router")
        || receiver_name.ends_with("_routes")
        || receiver_name.ends_with("_bp")
}

fn is_static_cli_decorator(decorator: &str) -> bool {
    matches!(
        decorator,
        "click.command"
            | "click.group"
            | "click.option"
            | "click.argument"
            | "typer.command"
            | "typer.callback"
    )
}

fn is_static_cli_decorator_with_import_context(
    decorator: &str,
    imports: &[PythonImport],
    cli_receiver_names: &[String],
) -> bool {
    if is_static_cli_decorator(decorator) {
        return true;
    }
    let Some((receiver, method)) = decorator.rsplit_once('.') else {
        return false;
    };
    if !matches!(method, "command" | "callback") || !imports.iter().any(has_typer_import) {
        return false;
    }
    let receiver_name = receiver.rsplit('.').next().unwrap_or(receiver);
    cli_receiver_names
        .iter()
        .any(|candidate| candidate == receiver_name)
}

fn has_typer_import(import: &PythonImport) -> bool {
    import.imported == "typer" && import.alias == "typer"
}

pub(super) fn collect_static_cli_receiver_names(
    source: &SourceText<'_>,
    imports: &[PythonImport],
) -> Vec<String> {
    if !imports.iter().any(has_typer_import) {
        return Vec::new();
    }
    // One scan per file, not per owner: every owner of a Typer module asks.
    source.cli_receiver_names(scan_cli_receiver_names).to_vec()
}

fn scan_cli_receiver_names(source: &str) -> Vec<String> {
    let mut receivers: Vec<String> = source
        .lines()
        .filter_map(|line| {
            let text = line.trim();
            let (lhs, rhs) = split_python_assignment(text)?;
            if !is_simple_python_identifier(lhs) || !contains_python_call_shape(rhs, "typer.Typer")
            {
                return None;
            }
            Some(lhs.to_string())
        })
        .collect();
    receivers.sort();
    receivers.dedup();
    receivers
}

pub(super) fn is_simple_python_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `patch.object` and `patch.multiple` substitute attributes the same way as
/// `patch` and `monkeypatch.setattr`, so they are the same runtime
/// substitution limit (#4565).
pub(super) fn test_has_mocked_module(test: &PythonTest) -> bool {
    test.decorators.iter().any(|decorator| {
        ["patch", "patch.object", "patch.multiple"]
            .iter()
            .any(|form| decorator == form || decorator.ends_with(&format!(".{form}")))
    }) || [
        "patch(",
        "patch.object(",
        "patch.multiple(",
        "monkeypatch.setattr(",
        "monkeypatch.setitem(",
        "monkeypatch.delattr(",
    ]
    .iter()
    .any(|call| {
        if call.starts_with("patch") {
            body_calls_patch(&test.body_text, call, &test.imports)
        } else {
            body_calls_at_name_boundary(&test.body_text, call)
        }
    })
}

/// Whether `body` contains `call` where the name starts at an identifier
/// boundary: `mock.patch(` and `patch.object(`, not `dispatch(` or
/// `dispatch.object(`.
fn body_calls_at_name_boundary(body: &str, call: &str) -> bool {
    body.match_indices(call).any(|(idx, _)| {
        !body[..idx]
            .chars()
            .next_back()
            .is_some_and(is_python_identifier_char)
    })
}

/// Receivers whose `.patch(...)` is an HTTP request, not `mock.patch`.
const HTTP_CLIENT_RECEIVERS: &[&str] = &[
    "client",
    "test_client",
    "api_client",
    "async_client",
    "session",
    "requests",
    "httpx",
];

/// `body_calls_at_name_boundary` for a `patch` form, except an HTTP client's
/// `client.patch("/x")` (RIPR-SPEC-0238 rule 8). The exemption is fail-closed:
/// it applies only to a plain `<http client>.patch(` receiver, and only when
/// neither the test body (which holds its signature) nor its imports mention
/// a mock at all, so a client that is itself a mock keeps the limit.
fn body_calls_patch(body: &str, call: &str, imports: &[PythonImport]) -> bool {
    let mentions_mock = body.to_ascii_lowercase().contains("mock")
        || imports.iter().any(|import| {
            let text = format!(
                "{} {} {}",
                import.imported, import.alias, import.source_module
            );
            text.to_ascii_lowercase().contains("mock")
        });
    body.match_indices(call).any(|(idx, _)| {
        let before = &body[..idx];
        if before
            .chars()
            .next_back()
            .is_some_and(is_python_identifier_char)
        {
            return false;
        }
        let http_request = before.ends_with('.') && !mentions_mock && http_client_receiver(before);
        !http_request
    })
}

/// Whether the text before `.patch(` ends in `<http client>.`, with the client
/// name starting at an identifier boundary (`self.client.` and `client.` both,
/// `other_client.` not).
fn http_client_receiver(before: &str) -> bool {
    let receiver = before.trim_end_matches('.');
    HTTP_CLIENT_RECEIVERS.iter().any(|name| {
        receiver.strip_suffix(name).is_some_and(|rest| {
            rest.chars()
                .next_back()
                .is_none_or(|ch| !is_python_identifier_char(ch))
        })
    })
}

fn related_candidates_have_property_based_test_limit(
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> bool {
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            test_uses_property_based_inputs(candidate.test)
                && property_inputs_hide_the_discriminator(candidate.test, focus)
        })
}

/// Whether the assertion the rows select for this test is a strong oracle.
/// A strong sibling-field or other-family assertion the selector passes over
/// is not a known oracle for this change, so it never suppresses a
/// test-side limit (#5572).
fn selected_assertion_is_strong(test: &PythonTest, focus: &PythonAssertionFocus) -> bool {
    select_relevant_assertion(&test.assertions, Some(focus))
        .assertion()
        .is_some_and(|assertion| assertion.oracle_strength.rank() >= OracleStrength::Strong.rank())
}

/// Whether a property-based test's generated inputs leave static evidence
/// unable to tell if the changed discriminator is reached (#6601): it has an
/// assertion but the selected one is not strong, or a strong one whose inputs
/// come only from strategies the test body reads, with no literal `@example`
/// row. A test with no assertion has no oracle to read, so it reads
/// reach-only like any other test; `@example` rows bind owner inputs like
/// `parametrize` cases.
fn property_inputs_hide_the_discriminator(test: &PythonTest, focus: &PythonAssertionFocus) -> bool {
    if test.assertions.is_empty() {
        return false;
    }
    !selected_assertion_is_strong(test, focus)
        || (!has_example_rows(test)
            && test
                .generated_inputs
                .iter()
                .any(|input| body_uses_identifier(&test.body_text, input)))
}

/// Whether a readable Hypothesis `@example(...)` row binds a generated input.
/// The decorator alone is not enough: an `@example(*args)` row is not read,
/// so it binds nothing ripr can see. A `parametrize` mark binds other
/// parameters, never the generated ones.
fn has_example_rows(test: &PythonTest) -> bool {
    test.parametrize.as_ref().is_some_and(|cases| {
        cases
            .argnames()
            .iter()
            .any(|name| test.generated_inputs.contains(name))
    })
}

fn test_uses_property_based_inputs(test: &PythonTest) -> bool {
    test.decorators.iter().any(|decorator| {
        decorator == "given"
            || decorator.ends_with(".given")
            || decorator == "hypothesis.given"
            || decorator == "example"
            || decorator.ends_with(".example")
    })
}

fn related_candidates_have_unresolved_pytest_fixture_limit(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> bool {
    let mut has_unresolved_fixture_relation = false;
    let mut has_concrete_oracle_relation = false;

    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if test_has_unresolved_pytest_fixture_inputs(candidate.test, owner) {
            has_unresolved_fixture_relation = true;
        } else {
            has_concrete_oracle_relation = true;
        }
    }

    has_unresolved_fixture_relation && !has_concrete_oracle_relation
}

fn test_has_unresolved_pytest_fixture_inputs(test: &PythonTest, owner: &PythonOwner) -> bool {
    test.framework == PythonFramework::Pytest
        && !test.parametrized
        && body_calls_owner(&test.body_text, owner)
        && test
            .fixtures
            .iter()
            .filter(|fixture| !is_known_auxiliary_pytest_fixture(fixture))
            .any(|fixture| body_uses_identifier(&test.body_text, fixture))
}

fn is_known_auxiliary_pytest_fixture(fixture: &str) -> bool {
    matches!(
        fixture,
        "capfd"
            | "capfdbinary"
            | "caplog"
            | "capsys"
            | "capsysbinary"
            | "client"
            | "monkeypatch"
            | "mocker"
            | "record_property"
            | "record_testsuite_property"
            | "recwarn"
            | "test_client"
            | "tmp_path"
            | "tmp_path_factory"
            | "tmpdir"
            | "tmpdir_factory"
    )
}

fn body_uses_identifier(body_text: &str, identifier: &str) -> bool {
    let mut line_start = 0;
    for line in body_text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('@')
            && !trimmed.starts_with("def ")
            && !trimmed.starts_with("async def ")
            && line.match_indices(identifier).any(|(idx, _)| {
                let body_idx = line_start + idx;
                has_identifier_boundary(body_text, body_idx, identifier.len())
                    && !line_prefix_looks_like_comment_or_string(body_text, body_idx)
            })
        {
            return true;
        }
        line_start += line.len();
    }
    false
}

pub(super) fn has_identifier_boundary(body_text: &str, idx: usize, len: usize) -> bool {
    let before = body_text[..idx].chars().next_back();
    let after = body_text[idx + len..].chars().next();
    before.is_none_or(|ch| !is_python_identifier_char(ch))
        && after.is_none_or(|ch| !is_python_identifier_char(ch))
}

pub(super) fn approx_call_has_band(assertion: &str) -> bool {
    [false, true]
        .into_iter()
        .flat_map(|attribute| {
            super::no_behavior::call_arglists_with_offsets(assertion, "approx", attribute)
        })
        .any(|(_, arglist)| {
            super::no_behavior::split_top_level_args(arglist)
                .into_iter()
                .map(str::trim)
                .enumerate()
                .any(|(index, argument)| {
                    // A spread may supply the expected value and the band, so
                    // it counts in any position, the first included.
                    if argument.starts_with('*') {
                        return true;
                    }
                    if index == 0 {
                        return false;
                    }
                    if let Some((name, value)) = argument.split_once('=') {
                        matches!(name.trim(), "rel" | "abs") && value.trim() != "None"
                    } else {
                        argument != "None"
                    }
                })
        })
}

/// The first string literal of a changed logging call (`log.warning("..")`).
fn changed_logging_call_literal(line: &str) -> Option<&str> {
    const LEVELS: [&str; 7] = [
        "debug",
        "info",
        "warning",
        "warn",
        "error",
        "critical",
        "exception",
    ];
    let call_start = line.find('(')?;
    let callee = &line[..call_start];
    let (_, level) = callee.rsplit_once('.')?;
    if !LEVELS.contains(&level) {
        return None;
    }
    let rest = &line[call_start + 1..];
    let quote_at = rest.find(['"', '\''])?;
    let quote = rest[quote_at..].chars().next()?;
    let body = &rest[quote_at + 1..];
    Some(&body[..body.find(quote)?])
}

/// Whether a related test's weak `<literal> in caplog.text` assertion names a word of the changed log message. A membership check
/// against a `%`-formatted message is a runtime comparison this adapter does
/// not evaluate, so the verdict abstains instead of reading a weak gap; an
/// assertion about unrelated text keeps its gap.
fn related_candidates_have_log_membership_limit(
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
    changed_message: &str,
) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| word.chars().count() >= 3)
            .map(str::to_lowercase)
            .collect()
    };
    let changed = words(changed_message);
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            select_relevant_assertion(&candidate.test.assertions, Some(focus))
                .assertion()
                .is_some_and(|assertion| {
                    // A local alias of a captured-log read (`msgs = caplog.messages`)
                    // is read as the capture itself: whether it is a faithful
                    // snapshot is the evaluator's question, not a reason to
                    // report a gap (#10151).
                    let gate_text = substitute_caplog_aliases(candidate.test, &assertion.gate_text);
                    // Only the checked literal (left of the membership operator)
                    // may overlap; `assert` and `caplog.text` are scaffolding.
                    caplog_membership_literals(&gate_text)
                        .iter()
                        .any(|checked| {
                            words(&checked.literal)
                                .iter()
                                .any(|word| changed.contains(word))
                        })
                        // Indexed and list forms over `caplog.messages` or
                        // `caplog.records` (`caplog.records[0].message == ...`,
                        // `caplog.messages == [...]`) are not evaluated: any
                        // shared word abstains.
                        || (gate_text.contains("caplog.messages")
                            || gate_text.contains("caplog.records"))
                            && words(&gate_text)
                                .iter()
                                .any(|word| changed.contains(word))
                })
        })
}

/// How an assertion compares its literal with what the logging call emitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaplogRead {
    /// `"<text>" in caplog.text`: a substring of the formatted record text.
    Text,
    /// `"<text>" in caplog.messages` or `any(r.message == "<text>" ...)`: one
    /// whole formatted message.
    MessageEquals,
    /// `any("<text>" in r.message for r in caplog.records)`: a substring of one
    /// formatted message.
    MessageContains,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CaplogMembership {
    pub(super) literal: String,
    pub(super) read: CaplogRead,
    /// `"<text>" not in ...` or `not any(...)`: the assertion holds while no
    /// captured text matches the literal.
    pub(super) negated: bool,
}

/// The string literals an assertion checks against the captured log records
/// (`caplog.text`, `caplog.messages`, or `any(...)` over `caplog.records`),
/// read from the parsed expression so spacing, parentheses, line breaks and
/// implicit literal concatenation do not change what is recognized.
pub(super) fn caplog_membership_literals(gate_text: &str) -> Vec<CaplogMembership> {
    caplog_literals(gate_text, false)
}

/// The literals that must all hold for the assertion to pass: a single
/// membership or an `and` chain of them. A literal under `or` is not one of
/// them (`assert A in caplog.text or B in caplog.text` still passes when A
/// alone disappears), so crediting an edit never reads through an `or`.
pub(super) fn caplog_required_literals(gate_text: &str) -> Vec<CaplogMembership> {
    caplog_literals(gate_text, true)
}

fn caplog_literals(gate_text: &str, conjunctive_only: bool) -> Vec<CaplogMembership> {
    let expression = gate_text.trim_start();
    let expression = expression.strip_prefix("assert").unwrap_or(expression);
    let mut literals = Vec::new();
    if let Some(value) = parse_expression(expression) {
        collect_caplog_membership_literals(&value, conjunctive_only, &mut literals);
    }
    literals
}

fn string_constant(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Str(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `<name>.message` or `<name>.getMessage()` for the loop variable `name`.
fn is_record_message(expr: &Expr, name: &str) -> bool {
    let is_name = |value: &Expr| matches!(value, Expr::Name(n) if n.id.as_str() == name);
    match expr {
        Expr::Attribute(attribute) => {
            attribute.attr.as_str() == "message" && is_name(&attribute.value)
        }
        Expr::Call(call) => {
            call.args.is_empty()
                && call.keywords.is_empty()
                && matches!(
                    call.func.as_ref(),
                    Expr::Attribute(attribute)
                        if attribute.attr.as_str() == "getMessage" && is_name(&attribute.value)
                )
        }
        _ => false,
    }
}

/// `[r.message for r in caplog.records]` (or `r.getMessage()`): the formatted
/// message of every captured record.
fn is_record_messages_list(expr: &Expr) -> bool {
    let Expr::ListComp(list) = expr else {
        return false;
    };
    let [comprehension] = list.generators.as_slice() else {
        return false;
    };
    let Expr::Name(target) = &comprehension.target else {
        return false;
    };
    !comprehension.is_async
        && comprehension.ifs.is_empty()
        && expr_full_name(&comprehension.iter).as_deref() == Some("caplog.records")
        && is_record_message(&list.elt, target.id.as_str())
}

/// `any("<text>" in r.message for r in caplog.records)` and the `==` forms.
fn caplog_records_any(call: &ast::ExprCall) -> Option<CaplogMembership> {
    if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "any")
        || call.args.len() != 1
        || !call.keywords.is_empty()
    {
        return None;
    }
    let Expr::GeneratorExp(generator) = &call.args[0] else {
        return None;
    };
    let [comprehension] = generator.generators.as_slice() else {
        return None;
    };
    if comprehension.is_async
        || !comprehension.ifs.is_empty()
        || expr_full_name(&comprehension.iter).as_deref() != Some("caplog.records")
    {
        return None;
    }
    let Expr::Name(target) = &comprehension.target else {
        return None;
    };
    let name = target.id.as_str();
    let Expr::Compare(compare) = generator.elt.as_ref() else {
        return None;
    };
    let [op] = compare.ops.as_slice() else {
        return None;
    };
    let [other] = compare.comparators.as_slice() else {
        return None;
    };
    match op {
        ast::CmpOp::In if is_record_message(other, name) => Some(CaplogMembership {
            literal: string_constant(&compare.left)?,
            read: CaplogRead::MessageContains,
            negated: false,
        }),
        ast::CmpOp::Eq if is_record_message(&compare.left, name) => Some(CaplogMembership {
            literal: string_constant(other)?,
            read: CaplogRead::MessageEquals,
            negated: false,
        }),
        ast::CmpOp::Eq if is_record_message(other, name) => Some(CaplogMembership {
            literal: string_constant(&compare.left)?,
            read: CaplogRead::MessageEquals,
            negated: false,
        }),
        _ => None,
    }
}

fn collect_caplog_membership_literals(
    expr: &Expr,
    conjunctive_only: bool,
    out: &mut Vec<CaplogMembership>,
) {
    match expr {
        Expr::BoolOp(bool_op) => {
            if conjunctive_only && bool_op.op != ast::BoolOp::And {
                return;
            }
            for value in &bool_op.values {
                collect_caplog_membership_literals(value, conjunctive_only, out);
            }
        }
        Expr::Call(call) => out.extend(caplog_records_any(call)),
        // `assert not any(...)`: the same read, asserted absent.
        Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => {
            if let Expr::Call(call) = unary.operand.as_ref() {
                out.extend(caplog_records_any(call).map(|membership| CaplogMembership {
                    negated: true,
                    ..membership
                }));
            }
        }
        Expr::Compare(compare)
            if compare.ops.len() == 1
                && matches!(compare.ops[0], ast::CmpOp::In | ast::CmpOp::NotIn) =>
        {
            let comparator = compare.comparators.first();
            let read = match comparator.and_then(expr_full_name).as_deref() {
                Some("caplog.text") => CaplogRead::Text,
                Some("caplog.messages") => CaplogRead::MessageEquals,
                _ if comparator.is_some_and(is_record_messages_list) => CaplogRead::MessageEquals,
                _ => return,
            };
            if let Some(literal) = string_constant(&compare.left) {
                out.push(CaplogMembership {
                    literal,
                    read,
                    negated: matches!(compare.ops[0], ast::CmpOp::NotIn),
                });
            }
        }
        _ => {}
    }
}

fn related_candidates_have_opaque_custom_assertion_limit(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> bool {
    let mut has_opaque_helper = false;
    let mut has_known_strong_oracle = false;

    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if candidate
            .test
            .assertions
            .iter()
            .any(|assertion| assertion.oracle_shape == PythonOracleShape::UnknownCustomHelper)
            // No assertion was read, but the admission scan found assertion-like
            // code it cannot resolve and the owner is called inside a `lambda`
            // handed to another call (`logged(lambda: must_equal(f(3), 9))`):
            // the callback may well assert, so a gap that asks for an assertion
            // is not honest. A bare call with no assertion keeps its gap.
            || (candidate.test.assertions.is_empty()
                && candidate.test.assertion_admission == PythonAssertionAdmission::Unresolved
                && owner_called_inside_lambda(&candidate.test.body_text, owner))
        {
            has_opaque_helper = true;
        }
        if selected_assertion_is_strong(candidate.test, focus) {
            has_known_strong_oracle = true;
        }
    }

    has_opaque_helper && !has_known_strong_oracle
}

/// Whether a line of the test body calls the owner inside a `lambda` body.
/// Same-line only: a lambda body spanning lines is not read. A `key=lambda`
/// sort key is inert, a lambda after an odd number of quotes sits in a string,
/// and the body ends at the first `;`.
fn owner_called_inside_lambda(body_text: &str, owner: &PythonOwner) -> bool {
    body_text.lines().any(|line| {
        line.match_indices("lambda").any(|(index, _)| {
            let before = &line[..index];
            let rest = &line[index + "lambda".len()..];
            let quotes = before.chars().filter(|ch| matches!(ch, '"' | '\'')).count();
            before
                .chars()
                .next_back()
                .is_none_or(|ch| !is_python_identifier_char(ch))
                && rest
                    .chars()
                    .next()
                    .is_none_or(|ch| !is_python_identifier_char(ch))
                && quotes % 2 == 0
                && !before.trim_end().ends_with("key=")
                && rest.split_once(':').is_some_and(|(_, body)| {
                    body_calls_owner(body.split(';').next().unwrap_or(body), owner)
                })
        })
    })
}

pub(super) fn line_uses_imported_symbol(text: &str, imports: &[PythonImport]) -> bool {
    imports.iter().any(|import| {
        !is_known_mock_constructor_import(import)
            && !line_uses_known_static_cli_symbol(text, import)
            && (text.contains(&format!("{}(", import.alias))
                || text.contains(&format!("{}.", import.alias)))
    })
}

pub(super) fn is_known_mock_constructor_import(import: &PythonImport) -> bool {
    matches!(import.imported.as_str(), "Mock" | "MagicMock")
        || matches!(import.alias.as_str(), "Mock" | "MagicMock")
}

fn line_uses_known_static_cli_symbol(text: &str, import: &PythonImport) -> bool {
    let alias = import.alias.as_str();
    match import.imported.as_str() {
        "click" if alias == "click" => contains_python_call_shape(text, "click.echo"),
        "typer" if alias == "typer" => contains_python_call_shape(text, "typer.echo"),
        "sys" if alias == "sys" => {
            contains_python_call_shape(text, "sys.exit")
                || contains_python_call_shape(text, "sys.stdout.write")
                || contains_python_call_shape(text, "sys.stderr.write")
                || prints_to_standard_stream(text)
        }
        _ => false,
    }
}

/// `print(..., file=sys.stderr)` names a standard stream, not an imported
/// implementation: a test reads it back through `capsys` without resolving `sys`.
///
/// The whole changed line must be that one `print` call, with `sys.stderr` or
/// `sys.stdout` as its `file=` argument and no other use of `sys` anywhere on
/// the line (`print(sys.argv[0], file=sys.stderr)` still reads an unresolved
/// attribute, and a conditional or second call may print elsewhere).
fn prints_to_standard_stream(text: &str) -> bool {
    // Offset of the parsed text inside `parse_expression`'s wrapper source.
    const WRAPPER_PREFIX: usize = "_ = (\n".len();
    let trimmed = text.trim();
    let Some(Expr::Call(call)) = parse_expression(trimmed) else {
        return false;
    };
    if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "print") {
        return false;
    }
    let file_keywords: Vec<_> = call
        .keywords
        .iter()
        .filter(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|arg| arg.as_str() == "file")
        })
        .collect();
    let [file_keyword] = file_keywords.as_slice() else {
        return false;
    };
    if !matches!(
        expr_full_name(&file_keyword.value).as_deref(),
        Some("sys.stderr" | "sys.stdout")
    ) {
        return false;
    }
    let start = usize::try_from(u32::from(file_keyword.range.start())).ok();
    let end = usize::try_from(u32::from(file_keyword.range.end())).ok();
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let (Some(start), Some(end)) = (
        start.checked_sub(WRAPPER_PREFIX),
        end.checked_sub(WRAPPER_PREFIX),
    ) else {
        return false;
    };
    let (Some(before), Some(after)) = (trimmed.get(..start), trimmed.get(end..)) else {
        return false;
    };
    !contains_identifier(before, "sys") && !contains_identifier(after, "sys")
}

fn contains_identifier(text: &str, identifier: &str) -> bool {
    text.match_indices(identifier)
        .any(|(index, _)| has_identifier_boundary(text, index, identifier.len()))
}

/// Heuristic-only relations that cannot honestly grade a gap (#9421, #9423).
///
/// - A dunder method owner whose related tests all relate only through
///   `dunder_protocol`: the test builds an instance of the owner class and
///   Python runs the dunder through operator or `with` syntax, but syntax alone
///   cannot bind the protocol use to that instance, so the oracle is unread.
/// - A module-level statement (`<module>` owner: a constant, a decorator
///   argument, a conditional definition) related only by name proximity while a
///   test imports the module: importing runs the top-level statements, which
///   this adapter does not model, so a filename-stem link must not read as a gap.
///
/// Both abstain (`static_unknown`) instead of reporting `weakly_exposed`. The
/// guard needs related tests and no oracle-eligible relation, so a direct call
/// keeps its ordinary verdict.
pub(super) fn heuristic_only_relation_limit(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<PythonStaticLimit> {
    if related_candidates.is_empty()
        || related_candidates
            .iter()
            .any(|candidate| candidate.relation.uses_oracle())
    {
        return None;
    }
    if let Some(class) = dunder_method_class(owner)
        && related_candidates
            .iter()
            .all(|candidate| candidate.relation == PythonRelationKind::DunderProtocol)
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::DynamicDispatch,
            evidence: format!(
                "static_limit dynamic_dispatch: dunder method `{}` is invoked implicitly by Python syntax on a constructed instance",
                owner.qualified_name
            ),
            missing: format!(
                "Static limit `dynamic_dispatch`: `{}` is a dunder method Python invokes through operator or `with` syntax; related tests build `{class}` instances, but the preview adapter does not bind the syntax to the instance or evaluate the assertion against the dunder's result, so it does not grade this edit as a gap.",
                owner.qualified_name
            ),
        });
    }
    if owner.qualified_name == "<module>"
        && related_candidates
            .iter()
            .any(|candidate| test_imports_owner_module(candidate.test, owner))
    {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::MissingImportGraph,
            evidence: "static_limit missing_import_graph: module-level statement run at import time"
                .to_string(),
            missing: "Static limit `missing_import_graph`: the changed line is a module-level statement (a constant, decorator argument or conditional definition) that runs when a test imports the module; related tests are linked only by a name heuristic and the preview adapter does not follow top-level values into their use sites, so it does not grade this edit as a gap.".to_string(),
        });
    }
    None
}

/// An equality dunder (`__eq__`, `__ne__`) whose operator entries bind to the
/// owner but whose related tests credit no assertion (#9421).
///
/// An equality method's behavior turns on the operand's type as much as its
/// value: an `isinstance` guard, a `NotImplemented` fallback or an attribute
/// read on a foreign operand fails (or answers) by raising or by type, which a
/// weak `==` / `!=` read cannot tell from the edit. Grading that as a gap
/// would be a wrong actionable signal when the test does catch the edit
/// (`assert Money(1) != 1` against a removed type guard), so an uncredited
/// equality owner abstains. A credited assertion keeps its ordinary verdict.
pub(super) fn unpinned_equality_dunder_limit(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    credited: bool,
) -> Option<PythonStaticLimit> {
    if credited
        || !matches!(owner.name.as_str(), "__eq__" | "__ne__")
        || !related_candidates
            .iter()
            .any(|candidate| candidate.relation == PythonRelationKind::ProtocolEntryCall)
    {
        return None;
    }
    let class = dunder_method_class(owner)?;
    Some(PythonStaticLimit {
        kind: StaticLimitKind::DynamicDispatch,
        evidence: format!(
            "static_limit dynamic_dispatch: equality dunder `{}` is invoked through an operator and no assertion pins its result",
            owner.qualified_name
        ),
        missing: format!(
            "Static limit `dynamic_dispatch`: `{}` is an equality dunder Python invokes through `==` / `!=` on `{class}` instances; related tests compare instances, but a weak equality read does not establish whether an operand-type guard or fallback path is exercised, so it does not grade this edit as a gap.",
            owner.qualified_name
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::StaticLimitKind;
    use std::path::PathBuf;

    fn from_functools(name: &str) -> PythonImport {
        PythonImport {
            imported: name.to_string(),
            alias: name.to_string(),
            source_module: "functools".to_string(),
        }
    }

    fn import_functools() -> PythonImport {
        PythonImport {
            imported: "functools".to_string(),
            alias: "functools".to_string(),
            source_module: String::new(),
        }
    }

    fn owner_with(decorators: &[&str], imports: Vec<PythonImport>) -> PythonOwner {
        PythonOwner {
            name: "owner".to_string(),
            qualified_name: "owner".to_string(),
            file: PathBuf::from("src/owner.py"),
            start_line: 1,
            end_line: 4,
            owner_kind: None,
            decorators: decorators.iter().map(|name| (*name).to_string()).collect(),
            imports,
            cli_receiver_names: Vec::new(),
            route_paths: Vec::new(),
            route_methods: Vec::new(),
            route_receivers: Vec::new(),
            dynamic_route_decorators: Vec::new(),
            unplain_decorators: Vec::new(),
            decorator_lines: None,
            parameters: Vec::new(),
            reexport_modules: Vec::new(),
            star_exported: false,
            ambiguous_src_modules: Vec::new(),
            rival_definition_modules: Vec::new(),
            module_constants: Vec::new(),
            same_class_callees: Vec::new(),
            module_callees: Vec::new(),
            class_path: String::new(),
            term_facts: Default::default(),
            class_facts: Default::default(),
            foreign_writes: Default::default(),
            version_conditional: false,
        }
    }

    fn decorator_limit(owner: &PythonOwner) -> Option<StaticLimitKind> {
        static_limit_for_change("return amount + 2", owner, &[]).map(|limit| limit.kind)
    }

    #[test]
    fn functools_memoization_decorators_do_not_limit() {
        for name in ["lru_cache", "cache", "cached_property"] {
            let qualified = format!("functools.{name}");
            assert!(
                !is_transparent_owner_decorator(&qualified),
                "qualified `{qualified}` needs owner import context"
            );
            let owner = owner_with(&[name], vec![from_functools(name)]);
            assert!(
                is_transparent_owner_decorator_for_owner(name, &owner),
                "expected bare `{name}` imported from functools to be transparent"
            );
            assert_eq!(
                decorator_limit(&owner),
                None,
                "imported `{name}` must not emit decorator_indirection"
            );
            assert_eq!(
                decorator_limit(&owner_with(&[qualified.as_str()], vec![import_functools()])),
                None,
                "`{qualified}` with `import functools` must not emit decorator_indirection"
            );
            assert_eq!(
                decorator_limit(&owner_with(&[qualified.as_str()], Vec::new())),
                None,
                "`{qualified}` with no competing import is the stdlib spelling"
            );
        }
    }

    fn from_module(module: &str, name: &str, alias: &str) -> PythonImport {
        PythonImport {
            imported: name.to_string(),
            alias: alias.to_string(),
            source_module: module.to_string(),
        }
    }

    fn import_module(module: &str, alias: &str) -> PythonImport {
        PythonImport {
            imported: module.to_string(),
            alias: alias.to_string(),
            source_module: String::new(),
        }
    }

    #[test]
    fn typing_marker_decorators_do_not_limit() {
        for (module, name) in [
            ("typing", "final"),
            ("typing", "override"),
            ("typing", "no_type_check"),
            ("typing_extensions", "final"),
            ("typing_extensions", "override"),
        ] {
            let qualified = format!("{module}.{name}");
            assert!(!is_transparent_owner_decorator(name));
            let bare = owner_with(&[name], vec![from_module(module, name, name)]);
            assert_eq!(decorator_limit(&bare), None, "from {module} import {name}");
            let aliased = owner_with(&["mark"], vec![from_module(module, name, "mark")]);
            assert_eq!(decorator_limit(&aliased), None, "aliased {qualified}");
            let qualified_owner =
                owner_with(&[qualified.as_str()], vec![import_module(module, module)]);
            assert_eq!(decorator_limit(&qualified_owner), None, "{qualified}");
            let module_alias = owner_with(
                &[format!("t.{name}").as_str()],
                vec![import_module(module, "t")],
            );
            assert_eq!(decorator_limit(&module_alias), None, "t.{name}");
        }
    }

    fn parsed_limit(source: &str) -> Result<Option<StaticLimitKind>, String> {
        let owner = super::super::owners_tests::extract_owners(
            std::path::Path::new("src/rates.py"),
            source,
        )
        .into_iter()
        .find(|owner| owner.name == "rate")
        .ok_or_else(|| "owner `rate` was not extracted".to_string())?;
        Ok(decorator_limit(&owner))
    }

    #[test]
    fn parsed_typing_markers_limit_only_when_called_or_redefined() -> Result<(), String> {
        let limited = Some(StaticLimitKind::DecoratorIndirection);
        for plain in [
            "from typing import final\n\n@final\ndef rate(n):\n    return n + 1\n",
            "import typing\n\n@typing.final\ndef rate(n):\n    return n + 1\n",
            "import typing as t\n\n@t.final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\n@final\nclass rate:\n    pass\n",
            "from typing import (\n    final,\n    override,\n)\n\n@final\ndef rate(n):\n    return n + 1\n",
        ] {
            assert_eq!(parsed_limit(plain)?, None, "{plain}");
        }
        for wrapped in [
            // the decorator expression is a call, so its result wraps the owner
            "from typing import final\n\ndef wrapper(fn):\n    return lambda n: 0\n\n@final(wrapper)\ndef rate(n):\n    return n + 1\n",
            // the marker name is redefined after the import
            "from typing import final\n\ndef final(fn):\n    return lambda n: 0\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\nfinal = make_wrapper()\n\n@final\ndef rate(n):\n    return n + 1\n",
            // the module receiver is rebound after the import
            "import typing\n\ntyping = shim\n\n@typing.final\ndef rate(n):\n    return n + 1\n",
            // conditional, try/except, tuple-target and star-import rebinds
            "from typing import final\nimport os\n\nif os.environ.get(\"X\"):\n    def final(fn):\n        return lambda n: 0\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\ntry:\n    from helpers import final\nexcept ImportError:\n    pass\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\nfinal, other = make()\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\nfrom helpers import *\n\n@final\ndef rate(n):\n    return n + 1\n",
            // star imports and rebinds hidden behind other statements
            "from typing import final\nimport os; from helpers import *\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\ntry:\n    from helpers import *\nexcept ImportError:\n    pass\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final  # see (legacy\nfinal = wrapper\n# )\n\n@final\ndef rate(n):\n    return n + 1\n",
            "import typing as t\n\nt = shim\n\n@t.final\ndef rate(n):\n    return n + 1\n",
            "from typing import final as f\n\nf = wrapper\n\n@f\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\nfor final in wrappers:\n    pass\n\n@final\ndef rate(n):\n    return n + 1\n",
            "import typing\n\n@typing.final(wrapper)\ndef rate(n):\n    return n + 1\n",
            // a walrus rebind inside the marker's own decorator line
            "from typing import final\n\ndef w(f):\n    return lambda n: 0\n\n@final(final := w)\ndef other(n):\n    return n\n\n@final\ndef rate(n):\n    return n + 1\n",
            "import typing\n\nshim = None\n\n@typing.final(typing := shim)\ndef other(n):\n    return n\n\n@typing.final\ndef rate(n):\n    return n + 1\n",
            // parentheses inside string literals must not balance the call
            "from typing import final\n\n@final\n@wrap(\"(\")[0] or g(\")\")\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\n@final\n@wrap(\"(\") if c else g(\")\")\ndef rate(n):\n    return n + 1\n",
            // unreadable decorator shapes the head alone does not reveal
            "from typing import final\n\n@final\n@wrap(x)[0]\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\n@final\n@wrap(x) if c else y\ndef rate(n):\n    return n + 1\n",
            // a rebind on the line that closes a typing import block
            "from typing import (\n    final,\n); final = wrapper\n\n@final\ndef rate(n):\n    return n + 1\n",
            // a star import split by a backslash continuation
            "from typing import final\nfrom helpers import \\\n    *\n\n@final\ndef rate(n):\n    return n + 1\n",
            "from typing import final\nif c: from helpers import *\n\n@final\ndef rate(n):\n    return n + 1\n",
            // class-scope rebind of the marker
            "from typing import override\n\nclass A(B):\n    def override(fn):\n        return lambda *a: 0\n\n    @override\n    def rate(self, n):\n        return n + 1\n",
            // an unreadable decorator stacked with the marker
            "from typing import final\n\n@final\n@registry[\"x\"]\ndef rate(n):\n    return n + 1\n",
            "from typing import final\n\n@final\n@(lambda f: wrapper)\ndef rate(n):\n    return n + 1\n",
        ] {
            assert_eq!(parsed_limit(wrapped)?, limited, "{wrapped}");
        }
        Ok(())
    }

    #[test]
    fn typing_marker_rebound_or_local_stays_limited() {
        let limited = Some(StaticLimitKind::DecoratorIndirection);
        // local same-name decorator with no typing import
        for name in ["final", "override", "no_type_check"] {
            assert_eq!(decorator_limit(&owner_with(&[name], Vec::new())), limited);
        }
        // bare name also bound from another module
        let competing = owner_with(
            &["final"],
            vec![
                from_module("typing", "final", "final"),
                from_module("helpers", "final", "final"),
            ],
        );
        assert_eq!(decorator_limit(&competing), limited);
        // bare name only from another module
        let other = owner_with(
            &["override"],
            vec![from_module("helpers", "override", "override")],
        );
        assert_eq!(decorator_limit(&other), limited);
        // receiver rebound to a non-typing module
        let rebound = owner_with(&["typing.final"], vec![import_module("wrappers", "typing")]);
        assert_eq!(decorator_limit(&rebound), limited);
        let from_rebound = owner_with(
            &["typing.final"],
            vec![from_module("helpers", "typing", "typing")],
        );
        assert_eq!(decorator_limit(&from_rebound), limited);
        // alias bound to a non-typing module
        let alias = owner_with(&["t.final"], vec![import_module("wrappers", "t")]);
        assert_eq!(decorator_limit(&alias), limited);
        // typing_extensions has no no_type_check marker we trust
        let ext = owner_with(
            &["typing_extensions.no_type_check"],
            vec![import_module("typing_extensions", "typing_extensions")],
        );
        assert_eq!(decorator_limit(&ext), limited);
    }

    #[test]
    fn typing_overload_and_call_reshaping_decorators_stay_limited() {
        let limited = Some(StaticLimitKind::DecoratorIndirection);
        for (module, name) in [
            ("typing", "overload"),
            ("abc", "abstractmethod"),
            ("contextlib", "contextmanager"),
            ("typing", "final_not_real"),
        ] {
            let owner = owner_with(&[name], vec![from_module(module, name, name)]);
            assert_eq!(decorator_limit(&owner), limited, "{module}.{name}");
        }
        // a typing marker stacked with an unresolved decorator stays limited
        let mixed = owner_with(
            &["final", "retry"],
            vec![from_module("typing", "final", "final")],
        );
        assert_eq!(decorator_limit(&mixed), limited);
    }

    #[test]
    fn functools_imported_from_another_module_stays_limited() {
        let owner = owner_with(
            &["functools.lru_cache"],
            vec![PythonImport {
                imported: "functools".to_string(),
                alias: "functools".to_string(),
                source_module: "helpers".to_string(),
            }],
        );
        assert_eq!(
            decorator_limit(&owner),
            Some(StaticLimitKind::DecoratorIndirection)
        );
    }

    #[test]
    fn local_lru_cache_without_functools_import_stays_limited() {
        for name in ["lru_cache", "cache", "cached_property"] {
            let owner = owner_with(&[name], Vec::new());
            assert!(
                !is_transparent_owner_decorator(name),
                "bare `{name}` without import context must stay limited"
            );
            assert_eq!(
                decorator_limit(&owner),
                Some(StaticLimitKind::DecoratorIndirection),
                "local `{name}` must keep decorator_indirection"
            );
            let other_module = owner_with(
                &[name],
                vec![PythonImport {
                    imported: name.to_string(),
                    alias: name.to_string(),
                    source_module: "helpers".to_string(),
                }],
            );
            assert_eq!(
                decorator_limit(&other_module),
                Some(StaticLimitKind::DecoratorIndirection),
                "`{name}` imported from a non-functools module must stay limited"
            );
        }
    }

    #[test]
    fn retry_and_unrelated_decorators_stay_limited_with_functools_present() {
        let functools = vec![from_functools("lru_cache"), import_functools()];
        for decorator in ["retry", "retry.with_backoff", "cache.get", "post", "wraps"] {
            let owner = owner_with(&[decorator], functools.clone());
            assert!(
                !is_transparent_owner_decorator(decorator),
                "expected `{decorator}` to remain a static limit"
            );
            assert_eq!(
                decorator_limit(&owner),
                Some(StaticLimitKind::DecoratorIndirection),
                "`{decorator}` must keep decorator_indirection even when functools is imported"
            );
        }
    }

    #[test]
    fn memoization_plus_retry_still_limits() {
        let owner = owner_with(&["lru_cache", "retry"], vec![from_functools("lru_cache")]);
        assert_eq!(
            decorator_limit(&owner),
            Some(StaticLimitKind::DecoratorIndirection)
        );
    }

    #[test]
    fn aliased_functools_module_memoization_is_transparent() {
        let owner = owner_with(
            &["ft.lru_cache"],
            vec![PythonImport {
                imported: "functools".to_string(),
                alias: "ft".to_string(),
                source_module: String::new(),
            }],
        );
        assert_eq!(decorator_limit(&owner), None);
    }

    #[test]
    fn aliased_from_functools_import_is_transparent() {
        let owner = owner_with(
            &["memoize"],
            vec![PythonImport {
                imported: "lru_cache".to_string(),
                alias: "memoize".to_string(),
                source_module: "functools".to_string(),
            }],
        );
        assert_eq!(decorator_limit(&owner), None);
    }

    #[test]
    fn functools_aliased_from_another_module_stays_limited() {
        let owner = owner_with(
            &["functools.cache"],
            vec![PythonImport {
                imported: "wrappers".to_string(),
                alias: "functools".to_string(),
                source_module: String::new(),
            }],
        );
        assert_eq!(
            decorator_limit(&owner),
            Some(StaticLimitKind::DecoratorIndirection)
        );
    }

    #[test]
    fn competing_bare_cache_import_stays_limited() {
        let owner = owner_with(
            &["cache"],
            vec![
                PythonImport {
                    imported: "cache".to_string(),
                    alias: "cache".to_string(),
                    source_module: "wrappers".to_string(),
                },
                from_functools("cache"),
            ],
        );
        assert_eq!(
            decorator_limit(&owner),
            Some(StaticLimitKind::DecoratorIndirection)
        );
    }

    #[test]
    fn competing_module_alias_stays_limited() {
        let owner = owner_with(
            &["ft.lru_cache"],
            vec![
                PythonImport {
                    imported: "wrappers".to_string(),
                    alias: "ft".to_string(),
                    source_module: String::new(),
                },
                PythonImport {
                    imported: "functools".to_string(),
                    alias: "ft".to_string(),
                    source_module: String::new(),
                },
            ],
        );
        assert_eq!(
            decorator_limit(&owner),
            Some(StaticLimitKind::DecoratorIndirection)
        );
    }

    /// Token-shaped detection reads names, not substrings (RIPR-SPEC-0238
    /// rule 8): a call named `content_type`, the word `lambda` inside a string
    /// and an HTTP client's `.patch` are not metaprogramming, lambda syntax or a
    /// mocked module.
    #[test]
    fn substring_lookalikes_are_not_metaprogramming_lambda_or_mock_patch() {
        for code in ["    return content_type(x)", "    return my_setattr(x)"] {
            assert!(!contains_metaprogramming(code), "{code}");
        }
        // Fail closed where string hiding is unsafe: f-string expressions run.
        for code in [
            "    return f\"{type(x)}\"",
            "    return f\"{setattr(o, n, v)}\"",
            "    return type(x)",
            "    return builtins.type(x)",
            "    setattr(obj, name, value)",
            "    return cls.__getattr__(name)",
        ] {
            assert!(contains_metaprogramming(code), "{code}");
        }
        for code in [
            "    return \"lambda x\"",
            "    return 'a lambda (x)'",
            "    return lambdas(x)",
            "    return my_lambda + 1",
        ] {
            assert!(!contains_lambda_keyword(code), "{code}");
        }
        for code in [
            "    return lambda x: x",
            "    key = lambda: 0",
            "    return f\"{(lambda x: x)(1)}\"",
            "    \"\"\"; k = lambda x: x",
            "    return sorted(items, key=lambda(x))",
        ] {
            assert!(contains_lambda_keyword(code), "{code}");
        }
        let none: Vec<PythonImport> = Vec::new();
        for body in [
            "    client.patch(\"/x\")\n",
            "    self.client.patch(\"/x\")\n",
            "    response = requests.patch(url)\n",
            "    session.patch(url, json={})\n",
        ] {
            assert!(!body_calls_patch(body, "patch(", &none), "{body}");
        }
        let from_mock = vec![PythonImport {
            imported: "mock".to_string(),
            alias: "m".to_string(),
            source_module: "unittest".to_string(),
        }];
        for (body, imports) in [
            ("    patch(\"os.getcwd\")\n", &none),
            ("    mock.patch(\"os.getcwd\")\n", &none),
            ("    unittest.mock.patch(\"os.getcwd\")\n", &none),
            ("    mocker.patch(\"os.getcwd\")\n", &none),
            ("    m.patch(\"os.getcwd\")\n", &from_mock),
            ("    self.mocker.patch(\"os.getcwd\")\n", &none),
            ("    mock_client.patch(\"/x\")\n", &none),
            // An unrecognized receiver fails closed.
            ("    other_client.patch(\"/x\")\n", &none),
            (
                "    client = mock.Mock()\n    client.patch(\"/x\")\n",
                &none,
            ),
        ] {
            assert!(body_calls_patch(body, "patch(", imports), "{body}");
        }
        assert!(body_calls_patch(
            "    mocker.patch.object(a, \"b\")\n",
            "patch.object(",
            &none
        ));
    }
}
