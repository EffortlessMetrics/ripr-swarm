//! RIPR-SPEC-0240: an owner a test reaches without a call ripr can place is
//! not a gap.
//!
//! Four source forms leave a test whose oracle is real and whose reading is
//! `weakly_exposed`, a certain "assertion too weak" claim, because ripr sees no
//! `owner(..)` call it can pair with the assertion:
//!
//! - **function value**: the test names the owner without calling it
//!   (`.map(owner)`, an untyped `let f = owner; f(2)`), so the call
//!   happens inside machinery ripr does not follow; a typed
//!   `let f: fn(..) -> .. = owner;` binding is left to the typed
//!   function-pointer reading (RIPR-SPEC-0115, #9478);
//! - **generated owner**: the changed line is an item-position macro
//!   invocation (`def_add!(add_five, 2 + 3);`) that defines the owner, and the
//!   expansion is not source ripr reads;
//! - **executor-wrapped call**: the owner call is the only argument of
//!   `block_on(..)`, a local helper polling a `Future` or a known executor,
//!   and the assertion reads the helper's result;
//! - **blanket impl hop** (#6732): the owner is a trait-impl method and the test
//!   calls it only through a blanket `impl<T: Trait> Other for T` entry whose
//!   body calls the owner on `self`. Reach is established, but the owner pin
//!   does not read a trait-impl owner behind such an entry, so whether the
//!   assertion's value comes from the owner's changed behavior is not.
//!
//! None of these is credited; the gap is withheld under
//! `rust_indirect_owner_call_unresolved`. Like every RIPR-SPEC-0240 rule this
//! only withholds, so the actionability flip stays fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionContainer, FunctionFact, TestFact};
use crate::analysis::rust_index::{RustIndex, is_test_file};
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustIndirectOwnerCallUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the assertion observes the owner reached without a call ripr can place";

/// Callee names read as running a future to completion even when their
/// definition is not in the workspace (`futures::executor::block_on`,
/// `pollster::block_on`, a runtime's `rt.block_on(..)`).
const KNOWN_EXECUTORS: [&str; 1] = ["block_on"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    FunctionValue,
    Generated,
    ExecutorWrapped,
    BlanketImpl,
}

impl Form {
    fn advice(self) -> &'static str {
        match self {
            Form::FunctionValue => {
                "passes the owner as a function value, and ripr does not follow the indirect call to the assertion"
            }
            Form::Generated => {
                "reaches an owner a macro generates, and ripr does not expand the macro"
            }
            Form::BlanketImpl => {
                "reaches the owner only through a blanket trait impl method that calls it on `self`, and ripr does not pair the assertion on that method's result with the owner's value"
            }
            Form::ExecutorWrapped => {
                "calls the owner inside an executor helper such as `block_on`, and ripr does not pair the assertion on the helper's result with the owner's call"
            }
        }
    }
}

/// Withhold a gap whose reaching tests use one of the forms above. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_indirect_owner_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let changed_line = finding.probe.location.line;
    let owner = finding.probe.owner.as_ref().and_then(|owner_id| {
        index.functions().iter().find(|function| {
            function.id == *owner_id
                && function.start_line <= changed_line
                && changed_line <= function.end_line
        })
    });
    let judged: Vec<(String, Form)> = match owner {
        Some(owner) => judged_for_owner(finding, owner, index),
        None => generated_owner(finding, index).into_iter().collect(),
    };
    if judged.is_empty() {
        return false;
    }
    let labels: Vec<String> = judged.iter().map(|(label, _)| label.clone()).collect();
    let mut forms = Vec::<Form>::new();
    for (_, form) in &judged {
        if !forms.contains(form) {
            forms.push(*form);
        }
    }
    let next_step = format!(
        "{} {}, so ripr does not report a gap. Check that the assertion's expected value differs between the old and the new behavior; add a test only if it does not.",
        labels.join(", "),
        forms
            .iter()
            .map(|form| form.advice())
            .collect::<Vec<_>>()
            .join("; and ")
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

/// Repo-scope twin of [`withhold_indirect_owner_gap`]'s owner-side reading
/// (#10701): whether any of the named related tests reaches the owner that
/// encloses `line` of `owner_file` only as a function value or inside an
/// executor helper. `check` withholds such a gap under
/// `rust_indirect_owner_call_unresolved`, so repair must not start one either.
/// Related tests are `(name, file, line)` identities.
pub(in crate::analysis) fn related_tests_reach_owner_indirectly(
    owner_file: &std::path::Path,
    line: usize,
    owner_name: &str,
    related: &[(&str, &std::path::Path, usize)],
    index: &RustIndex,
) -> bool {
    let owner = index
        .functions()
        .iter()
        .filter(|function| {
            function.file == owner_file
                && function.start_line <= line
                && line <= function.end_line
                && function.name == owner_name
        })
        .min_by_key(|function| function.end_line - function.start_line);
    let Some(owner) = owner else {
        return false;
    };
    let is_async = super::owner_pin::declares_async(owner, index);
    index.tests().iter().any(|test| {
        let is_related = related.iter().any(|(name, file, test_line)| {
            *name == test.name && *file == test.file && *test_line == test.start_line
        });
        if !is_related || is_ignored(test) {
            return false;
        }
        let code = mask_comments_and_strings(&test.body);
        names_owner_as_value(&code, owner, test, index)
            || (is_async && executor_wraps_owner_call(&code, &owner.name, test, index))
            || reaches_owner_only_through_blanket_impl(&code, owner, test, index)
    })
}

fn label(test: &TestFact) -> String {
    format!(
        "`{}` ({}:{})",
        test.name,
        test.file.display().to_string().replace('\\', "/"),
        test.start_line
    )
}

fn judged_for_owner(
    finding: &Finding,
    owner: &FunctionFact,
    index: &RustIndex,
) -> Vec<(String, Form)> {
    let mut judged = Vec::new();
    let is_related = |test: &TestFact, direct_only: bool| {
        finding.related_tests.iter().any(|related| {
            related.name == test.name
                && related.file == test.file
                && related.line == test.start_line
                && (!direct_only
                    || related.relation_reason == Some(RelationReason::DirectOwnerCall))
        })
    };
    // When some related test already calls the owner, reach is not in doubt:
    // a value mention only counts in a test the finding itself lists, so an
    // unrelated test that binds a same-named local cannot hide the gap of the
    // test that does call it. With proximity-only reach (the usual shape of
    // `.map(owner)`), every test that can see the owner is a candidate, since
    // the related list is capped.
    let called = finding.related_tests.iter().any(|related| {
        matches!(
            related.relation_reason,
            Some(RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall)
        )
    });
    for test in index.tests() {
        if is_ignored(test) || test.assertions.is_empty() {
            continue;
        }
        // The function-value and executor forms below need a masked mention of
        // the owner's name, and masking only blanks bytes, so a body whose raw
        // text never spells the name cannot qualify for them. This pass runs
        // over every test for each withheld finding; skipping the mask there
        // is #13754's saving. Only the blanket hop (#6732) is defined by the
        // test NOT spelling the name, so it is judged here without the mask.
        if !test.body.contains(&*owner.name) {
            if is_related(test, false)
                && super::transitive_reach::test_reaches_trait_owner_through_blanket_impl(
                    test, owner, index,
                )
            {
                judged.push((label(test), Form::BlanketImpl));
            }
            continue;
        }
        let code = mask_comments_and_strings(&test.body);
        // A test that names the owner as a value is a candidate only where it
        // can see the owner: its own file, or an integration test.
        let sees_owner = test.file == owner.file || is_test_file(&test.file);
        if sees_owner
            && (!called || is_related(test, false))
            && names_owner_as_value(&code, owner, test, index)
        {
            judged.push((label(test), Form::FunctionValue));
            continue;
        }
        let related_direct = is_related(test, true);
        // A sync owner's bare call is its value, not a future an executor
        // drives: `block_on(owner(..))` on one stays a plain gap (#10143).
        if related_direct
            && super::owner_pin::declares_async(owner, index)
            && executor_wraps_owner_call(&code, &owner.name, test, index)
        {
            judged.push((label(test), Form::ExecutorWrapped));
            continue;
        }
        // A related test that only reaches the owner through the blanket hop
        // is judged even beside one that calls the owner by name: that direct
        // call can be weak while the blanket test discriminates, so the
        // direct assertion cannot establish a gap for the combined test set.
        if is_related(test, false)
            && reaches_owner_only_through_blanket_impl(&code, owner, test, index)
        {
            judged.push((label(test), Form::BlanketImpl));
        }
    }
    judged
}

/// #6732: the test reaches the trait-impl `owner` through a blanket impl entry
/// and never spells the owner's name, so no `owner(..)` call exists for the
/// owner pin to pair with the assertion. `code` is the masked test body.
fn reaches_owner_only_through_blanket_impl(
    code: &str,
    owner: &FunctionFact,
    test: &TestFact,
    index: &RustIndex,
) -> bool {
    identifier_offsets(code, &owner.name).next().is_none()
        && super::transitive_reach::test_reaches_trait_owner_through_blanket_impl(
            test, owner, index,
        )
}

/// The changed line is an item-position macro invocation: no function in its
/// file holds the line, and the changed expression is `name!(..)`. The tests
/// named are the related ones that call a name the invocation spells.
fn generated_owner(finding: &Finding, index: &RustIndex) -> Option<(String, Form)> {
    let line = finding.probe.location.line;
    let file = &finding.probe.location.file;
    let inside_function = index.functions().iter().any(|function| {
        function.file == *file && function.start_line <= line && line <= function.end_line
    });
    if inside_function {
        return None;
    }
    let expression = mask_comments_and_strings(finding.probe.expression.trim());
    let expression = expression.trim().trim_end_matches(';').trim_end();
    let name_end = expression.find('!')?;
    let name = &expression[..name_end];
    if !is_path(name) {
        return None;
    }
    let rest = expression[name_end + 1..].trim_start();
    if !rest.starts_with(['(', '[', '{']) {
        return None;
    }
    let related = finding
        .related_tests
        .iter()
        .find(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .or_else(|| finding.related_tests.first())?;
    let test = index.tests().iter().find(|test| {
        test.name == related.name && test.file == related.file && test.start_line == related.line
    })?;
    Some((label(test), Form::Generated))
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// Whether the owner-name mentions inside `oracle_text`, as it appears in the
/// test body `source`, are item shadows or declarations rather than the owner
/// passed as a value — the bare-name fold's per-assertion premise
/// (RIPR-SPEC-0240, #10162 review). The shadow reading is the same scoped
/// item reading the mention gate uses: a same-name `fn` binds another item
/// only inside its own block, and an explicit import binds another definition
/// only where the shared call-identity resolver reads it as one.
/// `Some(true)` refuses the fold for this assertion, `Some(false)` admits it,
/// and `None` — the oracle text is not a body substring, the body is
/// unreadable, or the assertion carries no owner mention — keeps the
/// caller's prior answer.
pub(in crate::analysis) fn oracle_owner_call_is_shadowed(
    source: &str,
    oracle_text: &str,
    owner: &FunctionFact,
    test: &TestFact,
    index: &RustIndex,
) -> Option<bool> {
    use super::call_identity::{CallTarget, shared_owner_call_identity};
    let name = owner.name.as_str();
    let anchor = oracle_text.trim();
    if anchor.is_empty() {
        return None;
    }
    // An assertion that never spells the owner has no mention to read: every
    // remaining path (an unreadable body, a duplicated or unlocated statement, a
    // located statement with no mention) answers `None`. The statement or its
    // first line is a part of the anchor, so a whole-identifier mention there is
    // one here. Checking the short anchor first skips the body-sized work below
    // for the many oracles of a naming test that do not name the owner (#13577).
    identifier_offsets(anchor, name).next()?;
    // The assertion fact's text is a trimmed statement of the body; locate it
    // (or its first line) to read the mentions in their real lexical scope.
    // The fact carries the statement's text, not its position: identical text in
    // two lexical scopes cannot be told apart, so neither scope is established.
    // One pass finds the first match and whether a second exists.
    let mut matches = source.match_indices(anchor);
    let first_match = matches.next();
    if matches.next().is_some() {
        return None;
    }
    let (at, len, partial) = match first_match.map(|(at, _)| at) {
        Some(at) => (at, anchor.len(), false),
        None => {
            let first = anchor.split('\n').next()?.trim();
            if first.is_empty() {
                return None;
            }
            (source.find(first)?, first.len(), true)
        }
    };
    let span = source.get(at..at.checked_add(len)?)?;
    let identity = shared_owner_call_identity(index, owner);
    let import_is_other = |path: &[String]| {
        let Some((leaf, qualifier)) = path.split_last() else {
            return false;
        };
        leaf == name
            && identity.as_ref().is_some_and(|identity| {
                identity.resolve_call(test, index, qualifier) == CallTarget::Other
            })
    };
    // Only an `fn name` item or a `use` item can declare or shadow the owner
    // here. A body with neither has no declaration or shadow to read, so the
    // per-assertion tree walk is skipped (it ran once per oracle of every
    // naming test and dominated the large-workspace cold pilot). The parse
    // still decides readability.
    let items = match owner_item_scan(source, name) {
        OwnerItemScan::NoItemSyntax { unreadable } => ItemBindings {
            unreadable,
            ..ItemBindings::default()
        },
        OwnerItemScan::ReadTree => {
            // The walk depends on the body, the owner and the owner's identity
            // against this test, not on the assertion: share it (#13868).
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut hasher);
            let key = (
                owner.id.0.clone(),
                test.file.clone(),
                test.start_line,
                test.name.clone(),
                hasher.finish(),
            );
            let shared = index.call_identity_memos.item_bindings(key, || {
                #[cfg(test)]
                ITEM_BINDING_READS.with(|reads| reads.set(reads.get() + 1));
                ItemBindings::read(source, name, import_is_other)
            });
            return finish_shadow_scan(&shared, source, name, (at, span, partial));
        }
    };
    finish_shadow_scan(&items, source, name, (at, span, partial))
}

fn finish_shadow_scan(
    items: &ItemBindings,
    source: &str,
    name: &str,
    (at, span, partial): (usize, &str, bool),
) -> Option<bool> {
    if items.unreadable {
        return None;
    }
    let mut mentions = 0usize;
    for offset in identifier_offsets(span, name) {
        // A spelling inside a string literal (an assertion message) is text, not a
        // reference; the scan cannot say what the executable call resolves to.
        if inside_string_literal(span, offset) {
            return None;
        }
        let at = at + offset;
        let before = source[..at].trim_end();
        mentions += 1;
        let shadowed =
            items.declaration_at(at) || (!before.ends_with("::") && items.shadows_at(at));
        if !shadowed {
            // A mention that is neither an established declaration nor an established
            // shadow grounds the fold's owner-call premise.
            return Some(false);
        }
    }
    // Only the first line of the statement was located, so the later lines were
    // not read: an all-shadowed first line cannot establish the whole statement.
    if mentions == 0 || partial {
        return None;
    }
    Some(true)
}

/// Whether `offset` falls inside a `"` string literal of `code`. Raw strings and
/// character literals can misread; a misread only withholds a conclusion.
fn inside_string_literal(code: &str, offset: usize) -> bool {
    let (mut inside, mut escaped) = (false, false);
    for (at, ch) in code.char_indices() {
        if at >= offset {
            break;
        }
        if inside {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                inside = false;
            }
        } else if ch == '"' {
            inside = true;
        }
    }
    inside
}

fn is_path(text: &str) -> bool {
    !text.is_empty() && text.split("::").all(is_ident)
}

/// #14089: Unicode `XID_Continue`, the set Rust identifiers use. A mark such
/// as the virama U+094D continues an identifier without being alphanumeric,
/// so an alphanumeric test read `क्Owner` as `Owner`; a format char such as
/// U+200E is not an identifier char.
fn is_ident_char(ch: char) -> bool {
    ch == '_' || unicode_ident::is_xid_continue(ch)
}

/// Offsets at which `name` appears as a whole identifier in `code`.
fn identifier_offsets<'a>(code: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    code.match_indices(name)
        .map(|(at, _)| at)
        .filter(move |at| {
            code[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch))
                && code[at + name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !is_ident_char(ch))
        })
}

/// The test's code names `name` as a value: not called, not a macro, not a
/// method, field label or definition, the test binds no local of that name,
/// and a qualifying path (`other::inc`) can still name the owner.
pub(in crate::analysis::classify) fn names_owner_as_value(
    code: &str,
    owner: &FunctionFact,
    test: &TestFact,
    index: &RustIndex,
) -> bool {
    use super::call_identity::{CallTarget, shared_owner_call_identity};

    let identity = shared_owner_call_identity(index, owner);
    names_value(
        &test.body,
        code,
        &owner.name,
        &owner_path_names(owner),
        |path| {
            let Some((leaf, qualifier)) = path.split_last() else {
                return false;
            };
            leaf == &owner.name
                && identity.as_ref().is_some_and(|identity| {
                    identity.resolve_call(test, index, qualifier) == CallTarget::Other
                })
        },
    )
}

fn names_value(
    source: &str,
    code: &str,
    name: &str,
    owner_names: &[String],
    import_is_other: impl Fn(&[String]) -> bool,
) -> bool {
    // The answer is an `any` over the masked mentions of `name`, so a body
    // with none is `false` whatever its item bindings say. The judged-owner
    // pass asks this of every test in the index for each withheld probe, and
    // almost no test mentions a given owner; reading the bindings first parsed
    // each body (through a bounded memo that the test count overflows) only to
    // throw the tree away (#12947). Both checks below are pure, so testing for
    // a mention first changes no answer.
    let mut mentions = identifier_offsets(code, name).peekable();
    if mentions.peek().is_none() || is_bound_locally(code, name) {
        return false;
    }
    let items = ItemBindings::read(source, name, import_is_other);
    mentions.any(|at| mention_is_value(code, name, owner_names, &items, at))
}

/// Whether the mention of `name` at byte `at` of `code` names it as a value,
/// given the body's item bindings.
fn mention_is_value(
    code: &str,
    name: &str,
    owner_names: &[String],
    items: &ItemBindings,
    at: usize,
) -> bool {
    let reference_at = code[..at].strip_suffix("r#").map_or(at, str::len);
    let before = code[..reference_at].trim_end();
    if items.declaration_at(at)
        || items.inert_macro_at(at)
        || (!before.ends_with("::") && items.shadows_at(at))
        || (items.unreadable && use_line_mention(code, at))
    {
        return false;
    }
    let after = code[at + name.len()..].trim_start();
    let defined = before.ends_with("fn") && ends_at_boundary(&before[..before.len() - 2]);
    let method = before.ends_with('.');
    // #14941: whitespace may separate `::` from `<` (`thr :: <u8>(50)`); the
    // code is comment-masked, so a comment between reads as whitespace too.
    // A turbofish is a call only when its argument list opens one: a bare
    // `thr::<u8>` (`let g = thr:: <u8>;`, `.map(thr::<u8>)`) is a value.
    let turbofish = after
        .strip_prefix("::")
        .and_then(|rest| rest.trim_start().strip_prefix('<'))
        .and_then(after_generic_arguments)
        .is_some_and(|rest| rest.trim_start().starts_with('('));
    let called = after.starts_with(['(', '!']) || turbofish;
    // #10604: a mention that continues a path (`thr::internal::thr(50)`, where
    // the crate or module is spelled like the owner) is a qualifier segment,
    // not the owner passed as a value: a value use is never followed by `::`
    // and another segment. `::<` stays the turbofish call read above.
    let qualifier = after
        .strip_prefix("::")
        .is_some_and(|rest| rest.trim_start().starts_with(is_ident_char));
    let field_label = after.starts_with(':') && !after.starts_with("::");
    !(defined
        || method
        || called
        || qualifier
        || field_label
        || typed_fn_pointer_initializer(code, reference_at)
        || !qualifier_may_name_owner(before, name, owner_names))
}

/// #14941: the text after the `>` that closes a generic argument list whose
/// opening `<` was already consumed, or None when the list does not close.
/// A `->` inside a function type (`fn() -> u8`) does not close it, and an
/// array type or const block (`[u8; 4]`, `{ N + 1 }`) is skipped whole.
fn after_generic_arguments(text: &str) -> Option<&str> {
    let mut depth = 1usize;
    let mut nest = 0usize;
    let mut previous = '<';
    for (offset, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' => nest += 1,
            ')' | ']' | '}' => nest = nest.checked_sub(1)?,
            _ if nest > 0 => {}
            '<' => depth += 1,
            '>' if previous != '-' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(offset + 1..);
                }
            }
            ';' => return None,
            _ => {}
        }
        previous = character;
    }
    None
}

/// Path segments that identify where the owner lives: `self`, `super` and
/// `crate`, the components of its file path (module and crate directory names,
/// `-` read as `_`), and the self type and trait of an impl it sits in.
fn owner_path_names(owner: &FunctionFact) -> Vec<String> {
    let mut names: Vec<String> = ["self", "super", "crate", "Self"]
        .into_iter()
        .map(String::from)
        .collect();
    for component in owner.file.components() {
        if let Some(text) = component.as_os_str().to_str() {
            let stem = text.strip_suffix(".rs").unwrap_or(text);
            names.push(stem.replace('-', "_"));
        }
    }
    let mut add_path = |path: &str| {
        names.extend(
            path.split(|ch: char| !is_ident_char(ch))
                .filter(|segment| !segment.is_empty())
                .map(String::from),
        );
    };
    match &owner.item.container {
        FunctionContainer::Inherent { self_ty } => add_path(self_ty),
        FunctionContainer::TraitImpl {
            trait_path,
            self_ty,
            ..
        } => {
            add_path(trait_path);
            add_path(self_ty);
        }
        _ => {}
    }
    names
}

/// Whether the path written before the owner's name (`other::` in
/// `other::inc`) can still be the owner's: no qualifier, or one whose first or
/// last segment is among the names that locate the owner, or is spelled like
/// the owner itself. A qualifier that names none of these is a different
/// entity with the same leaf name.
///
/// The owner's own spelling counts because a module or crate-root re-export can
/// carry it: `thr::thr` is the re-exported `thr` taken as a function value, and
/// reading its qualifier as a stranger credited a gap the indirect-owner limit
/// is there to withhold (review round 1, Devin).
fn qualifier_may_name_owner(before: &str, name: &str, owner_names: &[String]) -> bool {
    let Some(path) = before.strip_suffix("::") else {
        return true;
    };
    let start = path
        .char_indices()
        .rev()
        .find(|&(_, ch)| !(is_ident_char(ch) || ch == ':'))
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let segments: Vec<&str> = path[start..]
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect();
    let known = |segment: &&str| *segment == name || owner_names.iter().any(|name| name == segment);
    match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => known(first) || known(last),
        // A leading `::` or a generic qualifier (`<T as Trait>::inc`) is not read.
        _ => false,
    }
}

/// The mention is the initializer of a `let f: fn(..) -> .. = owner;` binding.
/// The typed function-pointer reading follows that binding to its calls and
/// either credits or keeps the gap on the evidence (RIPR-SPEC-0115, #9478), so
/// this limit leaves the shape to it.
fn typed_fn_pointer_initializer(code: &str, at: usize) -> bool {
    let mut before = code[..at].trim_end();
    // Drop a path prefix such as `crate::` or `self::tests::`.
    while let Some(rest) = before.strip_suffix("::") {
        let rest = rest.trim_end();
        let start = rest
            .char_indices()
            .rev()
            .find(|&(_, ch)| !is_ident_char(ch))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        before = rest[..start].trim_end();
    }
    let Some(before) = before.strip_suffix('=') else {
        return false;
    };
    if before.ends_with(['=', '!', '<', '>']) {
        return false;
    }
    let Some(let_at) = identifier_offsets(before, "let").last() else {
        return false;
    };
    let binding = &before[let_at + 3..];
    binding
        .find(':')
        .is_some_and(|colon| binding[colon + 1..].trim_start().starts_with("fn("))
}

fn ends_at_boundary(text: &str) -> bool {
    text.chars().next_back().is_none_or(|ch| !is_ident_char(ch))
}

/// Whether the mention at `at` sits on a line that begins with a `use` item.
/// A parse-refused body cannot prove a declaration range, so this lexical
/// exclusion keeps a `use` line from counting as the owner passed as a value;
/// the parse-backed declaration ranges remain the primary path.
fn use_line_mention(code: &str, at: usize) -> bool {
    let start = code[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let line = code[start..at].trim_start();
    line.starts_with("use ") || line.starts_with("pub use ")
}

/// Whether `name` is bound as a local anywhere in the test: a typed or
/// untyped pattern after `let` (`let name`, `let (name, n)`, `if let
/// Some(name) =`), after `for` (`for (name, _) in`), a closure parameter
/// list (`|name|`), a `match` arm pattern (`Some(name) =>`) or a typed
/// parameter. A later mention is then the local, not the owner.
fn is_bound_locally(code: &str, name: &str) -> bool {
    pattern_spans(code)
        .into_iter()
        .any(|(start, end)| identifier_offsets(&code[start..end], name).next().is_some())
        || identifier_offsets(code, name).any(|at| {
            let after = code[at + name.len()..].trim_start();
            after.starts_with(':')
                && !after.starts_with("::")
                && code[..at].trim_end().ends_with(['(', ',', '|'])
        })
}

/// What the item-syntax prefilter says about one test body and owner name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerItemScan {
    /// No `use` or same-name `fn` item: nothing to read, and whether the shared
    /// parser accepts the body.
    NoItemSyntax { unreadable: bool },
    /// Item syntax is present (or in doubt): read the tree.
    ReadTree,
}

#[cfg(test)]
thread_local! {
    /// Times [`body_has_no_owner_item_syntax`] actually ran on this thread.
    static OWNER_ITEM_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Times the shadow check asked [`owner_item_scan`] on this thread.
    static OWNER_ITEM_ASKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Times [`ItemBindings::read`] itself ran on this thread, through any
    /// caller (#12947 pins that `names_value` skips it for unmentioned bodies).
    static BINDINGS_READ_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Times the item-binding tree walk actually ran on this thread (#13868).
    static ITEM_BINDING_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Times the item-binding tree walk has run on this thread (tests pin that
/// the per-(seam, test, assertion) asks share one walk per body).
#[cfg(test)]
pub(in crate::analysis) fn item_binding_read_count() -> usize {
    ITEM_BINDING_READS.with(std::cell::Cell::get)
}

/// Times the item-syntax prefilter has actually run on this thread (tests pin
/// how many of the shadow check's asks the run memo absorbs).
#[cfg(test)]
pub(in crate::analysis) fn owner_item_scan_count() -> usize {
    OWNER_ITEM_SCANS.with(std::cell::Cell::get)
}

/// Times the shadow check asked for the prefilter on this thread, memo or not.
#[cfg(test)]
pub(in crate::analysis) fn owner_item_ask_count() -> usize {
    OWNER_ITEM_ASKS.with(std::cell::Cell::get)
}

/// [`body_has_no_owner_item_syntax`] and the readability parse it feeds, as one
/// pure function of `(source, name)`. The shadow check asks it once per (seam,
/// naming test, oracle) triple and the body does not change between those
/// asks, so it is computed once per distinct pair per run scope (#13577) and
/// the per-call work is a single digest lookup. Outside a scope it recomputes.
fn owner_item_scan(source: &str, name: &str) -> OwnerItemScan {
    use crate::analysis::syntax::run_memo::{Product, memoize_with};
    #[cfg(test)]
    OWNER_ITEM_ASKS.with(|n| n.set(n.get() + 1));
    memoize_with(Product::OwnerItemScan, 0, name, source, || {
        if body_has_no_owner_item_syntax(source, name) {
            OwnerItemScan::NoItemSyntax {
                unreadable: crate::analysis::syntax::parse_clean_source_file_memoized(source)
                    .is_none(),
            }
        } else {
            OwnerItemScan::ReadTree
        }
    })
}

/// Whether `source` holds no syntax that [`ItemBindings::read`] turns into a
/// declaration or a shadow of `name`: no `use` item and no `fn` item (or raw
/// identifier) spelled with `name`. Conservative: any doubt reads the tree.
fn body_has_no_owner_item_syntax(source: &str, name: &str) -> bool {
    #[cfg(test)]
    OWNER_ITEM_SCANS.with(|n| n.set(n.get() + 1));
    if identifier_offsets(source, "use").next().is_some() {
        return false;
    }
    // Only a plain identifier may follow `fn` for the textual check below to
    // see the name. A comment (`fn /* c */ inc`, `fn // c` newline `inc`,
    // `fn/**/inc`), or whitespace the lexer accepts but `trim_start` does not
    // (U+200E), hides it, so any other next character reads the tree.
    if identifier_offsets(source, "fn").any(|at| {
        !source[at + "fn".len()..]
            .trim_start()
            .starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
    }) {
        return false;
    }
    !identifier_offsets(source, name).any(|at| {
        let before = source[..at].trim_end();
        before.ends_with("r#")
            || before
                .strip_suffix("fn")
                .is_some_and(|rest| rest.chars().next_back().is_none_or(|ch| !is_ident_char(ch)))
    })
}

/// Declaration ranges and hoisted item bindings in the test's real lexical
/// blocks. Macro arguments remain text: their function values are token trees,
/// not `PathExpr` nodes. An unreadable parse cannot prove a shadow.
#[derive(Default)]
pub(super) struct ItemBindings {
    declarations: Vec<ra_ap_syntax::TextRange>,
    scopes: Vec<(ra_ap_syntax::TextRange, bool)>,
    /// Bodies of `macro_rules!` definitions that are inert: the macro is not
    /// exported and its name occurs as no identifier token outside its own
    /// definition, so its token tree is never expanded into the test (#12231).
    /// A macro whose name appears anywhere else (an invocation, a forwarded
    /// `$m!()` argument, a comment-separated `name/**/!`), or that
    /// `#[macro_export]` may expose to another file, is absent here and its
    /// body text keeps counting.
    inert_macros: Vec<ra_ap_syntax::TextRange>,
    /// The shared parser refused the body, so no declaration range is known.
    /// Mentions on lexical `use` lines then keep the import exclusion instead
    /// of counting the import as the owner passed as a value (RIPR-SPEC-0240).
    unreadable: bool,
}

/// One `scopes` entry of [`ItemBindings`], in tree order, before the
/// caller's import resolution is applied.
#[derive(Clone)]
enum ScopeSyntax {
    /// A scope whose shadow flag the tree alone decides.
    Fixed(ra_ap_syntax::TextRange, bool),
    /// A block holding a `use` that binds the owner's name or holds a glob. It shadows only
    /// when `eligible` and the resolver reads every path as another
    /// definition (`None` is an opaque binding, which never does).
    Import {
        block: ra_ap_syntax::TextRange,
        eligible: bool,
        paths: Vec<Option<Vec<String>>>,
    },
}

/// The pure part of [`ItemBindings::read`]: everything the walk derives from
/// `(source, name)` alone, so it is computed once per distinct pair per run
/// scope. The shadow check reads the same test body once per (seam, naming
/// test, oracle), and each read walked the whole tree and re-collected the
/// body's non-whitespace text.
#[derive(Default)]
struct ItemSyntax {
    declarations: Vec<ra_ap_syntax::TextRange>,
    scopes: Vec<ScopeSyntax>,
    inert_macros: Vec<ra_ap_syntax::TextRange>,
    unreadable: bool,
}

impl ItemSyntax {
    fn memoized(source: &str, name: &str) -> std::rc::Rc<Self> {
        use crate::analysis::syntax::run_memo::{Product, memoize_with};
        memoize_with(Product::ItemSyntax, 0, name, source, || {
            std::rc::Rc::new(Self::scan(source, name))
        })
    }

    fn scan(source: &str, name: &str) -> Self {
        use crate::analysis::syntax::{
            UseBinding, parse_clean_source_file_memoized, use_item_bindings,
        };
        use ra_ap_syntax::{
            AstNode,
            ast::{self, HasAttrs, HasName},
        };

        #[cfg(test)]
        BINDINGS_READ_CALLS.with(|n| n.set(n.get() + 1));
        let mut result = Self::default();
        let Some(parse) = parse_clean_source_file_memoized(source) else {
            result.unreadable = true;
            return result;
        };
        // `include!` expands code this scan never sees, and a `#[path]` module
        // reads another file; either can invoke a local macro. Keep every
        // macro definition live when one is present rather than prove a
        // negative about unseen text (#12231 review).
        let expansion_unknown = identifier_offsets(source, "include").next().is_some()
            || source
                .split_whitespace()
                .collect::<String>()
                .contains("#[path=");
        for node in parse.tree().syntax().descendants() {
            // An item scope cannot inherit the surrounding function's local
            // shadow. Its imports also need their own module context; leave
            // that identity unresolved instead of borrowing the test's.
            if ast::ItemList::can_cast(node.kind()) {
                result
                    .scopes
                    .push(ScopeSyntax::Fixed(node.text_range(), false));
                continue;
            }
            if let Some(definition) = ast::MacroRules::cast(node.clone()) {
                let exported = definition
                    .attrs()
                    .any(|attr| attr.syntax().text().to_string().contains("macro_export"));
                let range = definition.syntax().text_range();
                if let Some(macro_name) = definition.name()
                    && !exported
                    && !expansion_unknown
                    && !macro_is_mentioned(
                        source,
                        macro_name.text().trim_start_matches("r#"),
                        range,
                    )
                {
                    result.inert_macros.push(range);
                }
                continue;
            }
            // Items in a nested module or impl belong to its ItemList, not the
            // surrounding test. Never climb across that boundary.
            let block = node
                .parent()
                .and_then(ast::StmtList::cast)
                .map(|block| block.syntax().text_range());
            if let Some(function) = ast::Fn::cast(node.clone()) {
                let Some(declared) = function.name() else {
                    continue;
                };
                if declared.text().trim_start_matches("r#") != name {
                    continue;
                }
                result.declarations.push(declared.syntax().text_range());
                if let Some(block) = block {
                    let shadow = function.attrs().next().is_none();
                    result.scopes.push(ScopeSyntax::Fixed(block, shadow));
                }
            } else if let Some(import) = ast::Use::cast(node.clone()) {
                result.declarations.push(node.text_range());
                let scope = use_item_bindings(&import);
                let matching: Vec<_> = scope
                    .imports
                    .iter()
                    .filter(|import| import.bound.trim_start_matches("r#") == name)
                    .collect();
                if matching.is_empty() && scope.globs.is_empty() {
                    continue;
                }
                let Some(block) = block else {
                    continue;
                };
                let eligible = import.attrs().next().is_none()
                    && !node
                        .ancestors()
                        .any(|ancestor| ast::ItemList::can_cast(ancestor.kind()))
                    && scope.globs.is_empty()
                    && !matching.is_empty();
                // Absence from a lexical owner-name list is not evidence of a
                // different definition: reexports and unreadable module
                // layouts retain the limit.
                let paths = matching
                    .iter()
                    .map(|import| match &import.binding {
                        UseBinding::Path(path) | UseBinding::Renamed(path) => Some(path.clone()),
                        UseBinding::Opaque => None,
                    })
                    .collect();
                result.scopes.push(ScopeSyntax::Import {
                    block,
                    eligible,
                    paths,
                });
            }
        }
        result
    }
}

impl ItemBindings {
    fn read(source: &str, name: &str, import_is_other: impl Fn(&[String]) -> bool) -> Self {
        let syntax = ItemSyntax::memoized(source, name);
        let scopes = syntax
            .scopes
            .iter()
            .map(|scope| match scope {
                ScopeSyntax::Fixed(range, shadow) => (*range, *shadow),
                ScopeSyntax::Import {
                    block,
                    eligible,
                    paths,
                } => (
                    *block,
                    *eligible
                        && paths
                            .iter()
                            .all(|path| path.as_deref().is_some_and(&import_is_other)),
                ),
            })
            .collect();
        Self {
            declarations: syntax.declarations.clone(),
            scopes,
            inert_macros: syntax.inert_macros.clone(),
            unreadable: syntax.unreadable,
        }
    }

    fn declaration_at(&self, at: usize) -> bool {
        self.declarations
            .iter()
            .any(|range| contains_offset(*range, at))
    }

    /// The offset sits in the body of a macro definition that is never
    /// expanded, so its raw token text is not a mention of the owner.
    fn inert_macro_at(&self, at: usize) -> bool {
        self.inert_macros
            .iter()
            .any(|range| contains_offset(*range, at))
    }

    fn shadows_at(&self, at: usize) -> bool {
        let Some((innermost, _)) = self
            .scopes
            .iter()
            .filter(|(range, _)| contains_offset(*range, at))
            .min_by_key(|(range, _)| range.len())
        else {
            return false;
        };
        // A nearer owner-preserving import overrides an outer shadow. An
        // opaque, conditional or competing binding cannot prove another owner.
        self.scopes
            .iter()
            .filter(|(range, _)| range == innermost)
            .all(|(_, shadow)| *shadow)
    }
}

/// Whether `source` mentions `name` as an identifier token anywhere outside
/// the definition at `definition`. Any spelling counts, including a forwarded
/// `forward!(name)`, `name/*note*/!()`, a comment, a string or another macro's
/// body: only a certain absence makes the definition inert (#12231).
fn macro_is_mentioned(source: &str, name: &str, definition: ra_ap_syntax::TextRange) -> bool {
    identifier_offsets(source, name).any(|at| !contains_offset(definition, at))
}

fn contains_offset(range: ra_ap_syntax::TextRange, at: usize) -> bool {
    usize::from(range.start()) <= at && at < usize::from(range.end())
}

/// Byte ranges of `code` that are binding patterns: from `let`/`for` to the
/// next `=` or `in`, closure parameter lists between `|` bars, and match-arm
/// patterns from the previous `{`, `,` or `;` to `=>`.
fn pattern_spans(code: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for keyword in ["let", "for"] {
        for at in identifier_offsets(code, keyword) {
            let from = at + keyword.len();
            let rest = &code[from..];
            let stop = if keyword == "let" {
                rest.find(['=', ';'])
            } else {
                rest.find(" in ").or_else(|| rest.find('{'))
            };
            if let Some(stop) = stop {
                spans.push((from, from + stop));
            }
        }
    }
    let bytes = code.as_bytes();
    let mut open = None;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'|'
            && bytes.get(at + 1) != Some(&b'|')
            && at.checked_sub(1).map(|p| bytes[p]) != Some(b'|')
        {
            match open.take() {
                Some(start) => spans.push((start, at)),
                None => open = Some(at + 1),
            }
        }
    }
    for (at, _) in code.match_indices("=>") {
        let start = code[..at]
            .rfind(['{', ',', ';'])
            .map_or(0, |found| found + 1);
        spans.push((start, at));
    }
    spans
}

/// The owner is called as the only argument of an executor: a local helper
/// whose signature mentions `Future`, or a callee named like a known
/// executor, written bare, as a path or as a method.
fn executor_wraps_owner_call(code: &str, owner: &str, test: &TestFact, index: &RustIndex) -> bool {
    identifier_offsets(code, owner).any(|at| {
        let after = &code[at + owner.len()..];
        if !after.trim_start().starts_with('(') {
            return false;
        }
        let blanks = after.len() - after.trim_start().len();
        let Some(call_end) = call_end(code, at + owner.len() + blanks) else {
            return false;
        };
        // The owner call is the whole argument list of the wrapper.
        if !code[call_end..].trim_start().starts_with(')') {
            return false;
        }
        let before = code[..at].trim_end();
        let Some(open) = before.strip_suffix('(') else {
            return false;
        };
        let open = open.trim_end();
        let callee_start = open
            .char_indices()
            .rev()
            .find(|&(_, ch)| !(is_ident_char(ch) || ch == ':'))
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        let callee = &open[callee_start..];
        let leaf = callee.rsplit("::").next().unwrap_or(callee);
        if !is_ident(leaf) || leaf == owner {
            return false;
        }
        let defined_locally = callee == leaf
            && !open[..callee_start].trim_end().ends_with('.')
            && index
                .functions()
                .iter()
                .any(|function| function.name == leaf && function.file == test.file);
        if defined_locally {
            // A helper of the test's own file counts only when it takes a
            // `Future`: a same-named synchronous identity wrapper is not an
            // executor, and a literal expectation through a helper is also
            // read by `framework_oracle` (#10143), which reaches the same
            // withhold. A non-literal expectation falls only to this rule.
            return index.functions().iter().any(|function| {
                function.name == leaf
                    && function.file == test.file
                    && signature(function).contains("Future")
            });
        }
        KNOWN_EXECUTORS.contains(&leaf)
    })
}

/// The text of a function up to its body's opening brace.
fn signature(function: &FunctionFact) -> String {
    let body: &str = &function.body;
    let masked = mask_comments_and_strings(body);
    masked
        .find('{')
        .map_or(masked.as_str(), |open| &masked[..open])
        .to_string()
}

/// Offset just past the `)` that closes the call whose `(` starts at `open`.
fn call_end(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn item_syntax_is_shared_per_run_while_each_caller_resolves_imports() {
        let source = "fn t() {\n    use other::inc;\n    let y = inc(1);\n    {\n        fn inc() {}\n        inc();\n    }\n}\n";
        let import_at = source.find("inc(1)").unwrap_or(0);
        let fn_scope_at = source.find("inc();").unwrap_or(0);
        // Fixture construction: both mentions exist and are distinct.
        assert!(import_at > 0 && fn_scope_at > import_at, "{source}");

        let scope = crate::analysis::syntax::run_memo::enter();
        let first = ItemSyntax::memoized(source, "inc");
        let second = ItemSyntax::memoized(source, "inc");
        assert!(
            std::rc::Rc::ptr_eq(&first, &second),
            "one walk per (source, name)"
        );
        assert!(!std::rc::Rc::ptr_eq(
            &first,
            &ItemSyntax::memoized(source, "dec")
        ));
        assert!(!first.unreadable);

        // The same memoized walk answers differently per resolver: the import
        // shadows only where the caller reads `other::inc` as another item.
        let other = ItemBindings::read(source, "inc", |_| true);
        let owner = ItemBindings::read(source, "inc", |_| false);
        assert!(other.shadows_at(import_at));
        assert!(!owner.shadows_at(import_at));
        // A same-name, attribute-free fn shadows in its own block either way.
        assert!(other.shadows_at(fn_scope_at));
        assert!(owner.shadows_at(fn_scope_at));
        // Declarations are tree facts and do not depend on the resolver.
        assert_eq!(other.declarations, owner.declarations);
        assert_eq!(other.declarations.len(), 2);

        // Outside a scope the walk still gives the same answer.
        drop(scope);
        let unscoped = ItemBindings::read(source, "inc", |_| true);
        assert!(unscoped.shadows_at(import_at));
        assert_eq!(unscoped.declarations, other.declarations);
    }

    #[test]
    fn owner_item_syntax_is_detected_conservatively() {
        assert!(body_has_no_owner_item_syntax(
            "let x = inc(1); assert_eq!(x, 2);",
            "inc"
        ));
        // A same-name fn item, a raw-identifier fn and any use item all read the tree.
        assert!(!body_has_no_owner_item_syntax("fn inc() {} inc();", "inc"));
        assert!(!body_has_no_owner_item_syntax(
            "fn r#inc() {} inc();",
            "inc"
        ));
        assert!(!body_has_no_owner_item_syntax(
            "use other::inc; inc();",
            "inc"
        ));
        // `fn` inside a longer identifier is not a declaration.
        assert!(body_has_no_owner_item_syntax(
            "let gfn = 1; gfn_inc(inc());",
            "inc"
        ));
        // A comment between `fn` and the name still declares the item.
        for spelled in [
            "fn /* c */ inc() {} inc();",
            "fn // c\n inc() {} inc();",
            "fn/**/inc() {} inc();",
            "fn /** d */ inc() {} inc();",
            "fn\u{200E}inc() {} inc();",
        ] {
            assert!(!body_has_no_owner_item_syntax(spelled, "inc"), "{spelled}");
        }
        // A different fn name does not shadow the owner.
        assert!(body_has_no_owner_item_syntax(
            "fn helper() {} inc();",
            "inc"
        ));
    }

    use super::*;

    fn owner_names() -> Vec<String> {
        ["self", "super", "crate", "Self", "src", "lib"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    fn value(code: &str, name: &str) -> bool {
        let source = format!("fn test_body() {{ {code} }}");
        names_value(
            &source,
            &mask_comments_and_strings(&source),
            name,
            &owner_names(),
            |_| false,
        )
    }

    fn value_raw(source: &str, name: &str) -> bool {
        names_value(
            source,
            &mask_comments_and_strings(source),
            name,
            &owner_names(),
            |_| false,
        )
    }

    /// A body the shared parser refuses cannot prove a declaration range, so
    /// a mention on a `use` line keeps the lexical exclusion instead of
    /// counting the import as the owner passed as a value, while a real value
    /// mention in the same refused body still counts (RIPR-SPEC-0240).
    #[test]
    fn a_use_line_keeps_its_exclusion_in_an_unparseable_body() {
        let broken = "fn test_body() {\n    use super::inc;\n    let x = ;\n}\n";
        assert!(
            crate::analysis::syntax::parse_clean_source_file(broken).is_none(),
            "premise: the body is parse-refused"
        );
        assert!(!value_raw(broken, "inc"));
        // The lexical fallback excludes only import lines: a real value
        // mention in the same refused body still counts.
        let broken_value =
            "fn test_body() {\n    let w: Vec<_> = rows.map(inc).collect();\n    let x = ;\n}\n";
        assert!(
            crate::analysis::syntax::parse_clean_source_file(broken_value).is_none(),
            "premise: the value body is parse-refused"
        );
        assert!(value_raw(broken_value, "inc"));
        // A readable body keeps the parse-backed path: the import does not
        // shadow, so the bare value mention beside it still counts.
        let readable = "fn test_body() {\n    use super::inc;\n    let w: Vec<_> = rows.map(inc).collect();\n}\n";
        assert!(value_raw(readable, "inc"));
    }

    /// The pre-#12947 order: read the item bindings, then scan the mentions.
    fn value_eager(source: &str, name: &str) -> bool {
        let code = mask_comments_and_strings(source);
        !is_bound_locally(&code, name) && {
            let items = ItemBindings::read(source, name, |_| false);
            identifier_offsets(&code, name)
                .any(|at| mention_is_value(&code, name, &owner_names(), &items, at))
        }
    }

    /// #12947: `names_value` tests for a masked mention before it reads the
    /// item bindings. Every body gets the answer the eager order gives, and a
    /// body with no mention of the name never parses.
    #[test]
    fn mention_first_order_matches_eager_bindings_and_skips_unmentioned_parses() {
        let bodies = [
            "fn t() { assert_eq!([1].map(inc).len(), 1); }",
            "fn t() { assert_eq!(inc(1), 2); }",
            "fn t() { let inc = 3; use_it(inc); }",
            "fn t() { fn inc(x: u8) -> u8 { x } assert_eq!([1].map(inc).len(), 1); }",
            "fn t() { use other::inc; f(inc); }",
            "fn t() { use super::inc; f(inc); }",
            "fn t() {\n    let w: Vec<_> = rows.map(inc).collect();\n    let x = ;\n}\n",
            "fn t() {\n    use super::inc;\n    let x = ;\n}\n",
            "fn t() { assert_eq!(dec(1), 0); /* inc */ let s = \"inc\"; }",
            "fn t() { assert_eq!(increment(1), 2); }",
            "fn t() { let x = ; }",
        ];
        let answers: Vec<bool> = bodies.iter().map(|body| value_eager(body, "inc")).collect();
        assert!(
            answers.contains(&true) && answers.contains(&false),
            "premise: the eager reading tells these bodies apart: {answers:?}"
        );
        for (body, eager) in bodies.iter().zip(&answers) {
            assert_eq!(value_raw(body, "inc"), *eager, "{body}");
            // The name is part of the answer, not only the body.
            assert_eq!(value_raw(body, "dec"), value_eager(body, "dec"), "{body}");
        }

        let reads = || BINDINGS_READ_CALLS.with(std::cell::Cell::get);
        for body in [
            "fn t() { assert_eq!(dec(1), 0); /* inc */ let s = \"inc\"; }",
            "fn t() { assert_eq!(increment(1), 2); }",
            "fn t() { let x = ; }",
        ] {
            let before = reads();
            assert!(!value_raw(body, "inc"), "{body}");
            assert_eq!(reads(), before, "no mention, no binding read: {body}");
            let before = reads();
            assert!(!value_eager(body, "inc"), "{body}");
            assert_eq!(
                reads(),
                before + 1,
                "premise: the eager order reads it: {body}"
            );
        }
        let before = reads();
        assert!(value_raw(bodies[0], "inc"));
        assert_eq!(reads(), before + 1, "a mention still reads the bindings");
    }

    #[test]
    fn a_path_value_is_named_and_a_call_is_not() {
        let code = "assert_eq!(v.into_iter().map(inc).collect::<Vec<_>>(), w);";
        assert!(value(code, "inc"));
        assert!(!value("assert_eq!(inc(1), 2);", "inc"));
        assert!(!value("assert_eq!(inc (1), 2);", "inc"));
        assert!(!value("inc!(1);", "inc"));
        assert!(!value("x.inc;", "inc"));
        assert!(!value("let inc = 3; use_it(inc);", "inc"));
        assert!(!value("use super::inc;\n", "inc"));
        assert!(!value("S { inc: 1 };", "inc"));
        assert!(!value("let increment = 3;", "inc"));
    }

    /// #10604: a path qualifier spelled like the owner is not the owner passed
    /// as a value. In `thr::internal::thr(50)` the first `thr` names the crate
    /// and the second is the call; only a mention that ends the path can be a
    /// value.
    #[test]
    fn a_path_qualifier_spelled_like_the_owner_is_not_a_value() {
        assert!(!value("assert!(!thr::internal::thr(50));", "thr"));
        assert!(!value("f(thr::internal::thr(500));", "thr"));
        // A single-segment qualifier is the same shape: a module or crate
        // spelled like the owner, then a path into it.
        assert!(!value("assert_eq!(thr::LIMIT, 1);", "thr"));
        assert!(!value("f(thr::helper());", "thr"));
        // Control 1: a genuine value keeps the limit.
        assert!(value("let g = thr; g(50);", "thr"));
        assert!(value(
            "assert_eq!(rows.into_iter().map(thr).count(), 1);",
            "thr"
        ));
        // Control 2: real calls and the turbofish are unchanged.
        assert!(!value("assert!(thr(50));", "thr"));
        assert!(!value("assert!(thr::<u8>(50));", "thr"));
    }

    /// #14941: a turbofish is `::` then `<`, and Rust allows whitespace (and
    /// comments, masked to whitespace) between them. Every spelling is the
    /// call, never the owner passed as a value.
    #[test]
    fn a_whitespace_spelled_turbofish_is_a_call_not_a_value() {
        assert!(!value("assert!(thr::<u8>(50));", "thr"));
        assert!(!value("assert!(thr ::<u8>(50));", "thr"));
        assert!(!value("assert!(thr:: <u8>(50));", "thr"));
        assert!(!value("assert!(thr :: <u8>(50));", "thr"));
        assert!(!value("assert!(thr :: \n <u8>(50));", "thr"));
        assert!(!value("assert!(thr::/* c */<u8>(50));", "thr"));
        assert!(!value("assert!(thr /* c */ :: <u8>(50));", "thr"));
        // Fail closed: a spaced `::` is a qualifier, and a lone `<` or `::`
        // not followed by `<` or a segment is still a value or comparison.
        assert!(!value("assert!(thr :: helper());", "thr"));
        assert!(value("let g = thr; g(50);", "thr"));
        assert!(value("let g = thr ;", "thr"));
        assert!(value("rows.map(thr).count();", "thr"));
        assert!(value("let b = thr < 5;", "thr"));
        assert!(value("let b = thr :: ;", "thr"));
        // Devin review: a turbofish that does not open a call is the owner
        // specialised as a value, whatever its spelling.
        assert!(value("let g = thr::<u8>; assert!(g(50));", "thr"));
        assert!(value("let g = thr:: <u8>; assert!(g(50));", "thr"));
        assert!(value("let g = thr::/* c */<u8>;", "thr"));
        assert!(value("rows.map(thr:: <u8>).count();", "thr"));
        assert!(value("rows.map(thr::<u8>).count();", "thr"));
        // A function type inside the arguments does not close the list early.
        assert!(!value("assert!(thr::<fn() -> u8>(f));", "thr"));
        assert!(value("let g = thr::<fn() -> u8>;", "thr"));
        assert!(!value("assert!(thr::<Vec<u8>>(v));", "thr"));
        // Graphite review: an array type or a const block inside the
        // arguments is skipped whole, so these stay calls.
        assert!(!value("assert!(thr::<[u8; 4]>(x));", "thr"));
        assert!(!value("assert!(thr::<{ N + 1 }>(x));", "thr"));
        assert!(!value("assert!(thr::<{ N > 1 }>(x));", "thr"));
        assert!(value("let g = thr::<[u8; 4]>;", "thr"));
    }

    /// Review round 1 (Devin): excluding the qualifier must not eat the value
    /// behind it. In `let f = thr::thr;` the first `thr` is a module or crate
    /// whose segment is spelled like the owner, and the terminal `thr` is the
    /// re-exported owner itself, so the pair is a function value and keeps the
    /// indirect-owner limit. Only the mention that ends the path can be the
    /// value, and a qualifier spelled like the owner can still name the owner.
    #[test]
    fn a_function_value_behind_a_same_name_qualifier_still_keeps_the_limit() {
        assert!(value("let f = thr::thr; assert!(!f(50));", "thr"));
        assert!(value("let f = thr::internal::thr; f(50);", "thr"));
        // A qualifier that names neither the owner nor the owner's own spelling
        // stays a different entity, as before.
        assert!(!value("let f = other::thr; f(50);", "thr"));
    }

    #[test]
    fn a_qualified_path_names_the_owner_only_when_it_locates_the_owner() {
        // A same-leaf function in another module is a different entity.
        assert!(!value("assert_eq!([1, 2].map(other::inc), w);", "inc"));
        assert!(!value("f(a::b::inc);", "inc"));
        assert!(!value("f(<T as Tr>::inc);", "inc"));
        assert!(!value("f(::inc);", "inc"));
        // The owner's own module, `super`, `self`, `crate` and the owner's file stem.
        assert!(value("assert_eq!([1, 2].map(super::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(self::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(crate::lib::inc), w);", "inc"));
        assert!(value("assert_eq!([1, 2].map(lib::inc), w);", "inc"));
        // One qualified other-module mention does not hide a bare value mention.
        assert!(value("f(other::inc); g(inc);", "inc"));
    }

    #[test]
    fn an_untyped_function_pointer_binding_is_a_value() {
        let code = "let f = call_through_pointer; assert_eq!(f(2), 6);";
        assert!(value(code, "call_through_pointer"));
    }

    #[test]
    fn a_typed_function_pointer_binding_is_left_to_the_typed_reading() {
        for code in [
            "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
            "let f: fn(u32) -> u32 = crate::call_through_pointer; assert_eq!(f(2), 6);",
        ] {
            assert!(!value(code, "call_through_pointer"), "{code}");
        }
    }

    #[test]
    fn a_local_with_the_owner_name_is_not_the_owner() {
        assert!(!value("let mut inc = 1; inc += 1; keep(inc);", "inc"));
        assert!(!value("let go = |inc: u32| inc + 1; go(1);", "inc"));
    }

    #[test]
    fn pattern_bindings_of_the_owner_name_are_locals() {
        for code in [
            "if let Some(total) = r { keep(total); }",
            "for (total, _) in rows { keep(total); }",
            "let (total, n) = pair; keep(total);",
            "let go = |total| total + 1; go(1);",
            "match r { Ok(total) => keep(total), Err(_) => {} }",
        ] {
            assert!(!value(code, "total"), "{code}");
        }
        assert!(value("rows.map(total).count();", "total"));
    }

    #[test]
    fn call_end_matches_nested_delimiters() {
        let code = "add(f(1, 2), [3]) + 1";
        assert_eq!(call_end(code, 3), Some(17));
        assert_eq!(call_end("add(1", 3), None);
    }

    #[test]
    fn item_macros_need_a_path_and_a_delimiter() {
        assert!(is_path("def_add"));
        assert!(is_path("crate::def_add"));
        assert!(!is_path("2 + 3"));
        assert!(!is_path(""));
    }

    /// #14089: a char-predicate scan found a multibyte char and the next
    /// slice started one byte later, inside it.
    #[test]
    fn a_qualifier_ending_in_a_multibyte_non_identifier_char_is_not_read() {
        // `क्` ends in U+094D, a virama: it continues an identifier but is not
        // alphanumeric, and is three bytes wide.
        assert!(!super::qualifier_may_name_owner(
            "क्::",
            "inc",
            &["Owner".to_string()]
        ));
        // A non-ASCII identifier char continues the qualifier, so `क्Owner`
        // is not read as `Owner`.
        assert!(!super::qualifier_may_name_owner(
            "क्Owner::",
            "Owner",
            &["Owner".to_string()]
        ));
        // `क्::` is a path prefix like `helpers::`.
        let code = "let f: fn(u32) -> u32 = क्::";
        assert!(super::typed_fn_pointer_initializer(code, code.len()));
        // The ASCII spelling still reads as before.
        let code = "let f: fn(u32) -> u32 = helpers::";
        assert!(super::typed_fn_pointer_initializer(code, code.len()));
        assert!(super::qualifier_may_name_owner(
            "Owner::",
            "inc",
            &["Owner".to_string()]
        ));
    }
}
