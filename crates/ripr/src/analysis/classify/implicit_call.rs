//! RIPR-SPEC-0240: a method the language calls implicitly is not a gap.
//!
//! A trait-impl method often runs without its name appearing in a test:
//! `Money(2) + Money(3)` calls `Add::add`, the end of a block calls
//! `Drop::drop`, `Countdown(3).collect()` calls `Iterator::next`, and
//! `shapes[0].area()` on a `Vec<Box<dyn Shape>>` calls whichever `area` the
//! boxed value carries. The test pins an exact value, but name-based call
//! matching sees only tests that share the file or a name token with the
//! owner, so the finding reads `weakly_exposed`: a certain "no test calls this
//! and the oracle is weak" claim about code the tests do run.
//!
//! ripr cannot tell whether the operator, drop glue, iterator adapter or
//! virtual call lands on the changed method, so it withholds the gap under
//! `rust_implicit_call_unresolved` and credits nothing. The rule needs two
//! established links, both read from syntax:
//!
//! - the owner is a method of a trait impl whose trait the language calls
//!   from syntax (an operator trait, `Drop`, an iterator trait, `Deref`,
//!   `Index`, `PartialOrd`), or of any other trait that no gated std rule
//!   owns, and
//! - a non-ignored indexed test with a strong assertion names the impl's self type and, for a trait without
//!   syntax of its own, calls a method of the owner's name on some receiver.
//!
//! Like every RIPR-SPEC-0240 rule it only withholds, so the actionability
//! flip stays fail-closed. A trait whose methods run only through syntax a
//! test must spell (`Display`, `Debug`, `PartialEq`, `Hash`, `Clone`,
//! `Default`, `FromStr`, serde) keeps its own rules.

use super::gap_admission::{is_ignored, withhold_gap};
use super::related_tests::{
    expr_ending_at_has_type, impl_self_type_name, inherent_receiver_call_resolves,
    let_binding_certain_type_head, let_binding_mentions_type, owner_trait_name, stringified_ranges,
};
use super::reveal::contains_as_whole_word;
use crate::analysis::facts::TestFact;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::*;
use ra_ap_syntax::{
    AstNode, ast,
    ast::{HasAttrs, HasGenericParams, HasModuleItem, HasName},
};

const LIMIT: StaticLimitKind = StaticLimitKind::RustImplicitCallUnresolved;
const FIRST_UNRESOLVED_EDGE: &str = "whether the operator, drop glue, iterator or virtual call a test makes lands on the changed method";
const JUDGED_TEST_LIMIT: usize = 3;

/// Traits whose methods the language calls from syntax a test need not spell
/// as a method call: `a + b`, `-a`, `a[i]`, `*a`, `a < b`, a `for` loop, scope
/// exit.
const IMPLICIT_SYNTAX_TRAITS: [&str; 32] = [
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Rem",
    "Neg",
    "Not",
    "BitAnd",
    "BitOr",
    "BitXor",
    "Shl",
    "Shr",
    "AddAssign",
    "SubAssign",
    "MulAssign",
    "DivAssign",
    "RemAssign",
    "BitAndAssign",
    "BitOrAssign",
    "BitXorAssign",
    "ShlAssign",
    "ShrAssign",
    "Drop",
    "Iterator",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Deref",
    "DerefMut",
    "Index",
    "IndexMut",
    "PartialOrd",
];

/// Traits a test uses only through syntax or a method it must spell, which
/// the gated std rules and `Display`/`Debug` dispatch already judge.
const OWNED_ELSEWHERE_TRAITS: [&str; 14] = [
    "Display",
    "Debug",
    "PartialEq",
    "Eq",
    "Ord",
    "Hash",
    "Clone",
    "Default",
    "FromStr",
    "Serialize",
    "Deserialize",
    "Visitor",
    "Arbitrary",
    "Error",
];

/// Withhold a `weakly_exposed` or `reachable_unrevealed` gap whose owner a
/// test may run through an implicit call. Returns `true` when the finding was
/// rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_implicit_call_gap(
    finding: &mut Finding,
    index: &RustIndex,
    pre_cap_syntax_only: Option<bool>,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let Some(kind) = ImplicitCall::of_owner(owner) else {
        return false;
    };
    // A test seen calling the owner already decided reach, so this rule is
    // only for the "no test is seen calling" reading. The one exception is a
    // reach that syntax alone established (operator crediting, #9567) and
    // whose observation no assertion confirmed: relating the test says the
    // owner runs, not that an assertion would notice it change, so the gap
    // stays withheld instead of becoming a "add a test" signal (#9567). A
    // weakness with another cause (the test never drives the changed branch)
    // is a real gap and keeps its class.
    if finding.ripr.reach.state != StageState::Weak
        && !(matches!(kind, ImplicitCall::Syntax(_))
            && only_discrimination_unconfirmed(finding)
            && pre_cap_syntax_only
                .unwrap_or_else(|| reached_only_through_syntax(finding, owner, index, None)))
    {
        return false;
    }
    let Some(self_type) = impl_self_type_name(&owner.id.0) else {
        return false;
    };
    // A same-named trait or a second impl of it for this type (a test-local
    // shadow) makes `Type.method()` resolve to the shadow at runtime, so the
    // finding is a real gap, not an implicit call.
    if trait_is_shadowed(owner, &self_type, index, kind) {
        return false;
    }
    let judged: Vec<String> = index
        .tests()
        .iter()
        .filter(|test| !is_ignored(test) && test_makes_implicit_call(test, owner, &self_type, kind))
        .take(JUDGED_TEST_LIMIT)
        .map(|test| {
            format!(
                "`{}` ({}:{})",
                test.name,
                test.file.display().to_string().replace('\\', "/"),
                test.start_line
            )
        })
        .collect();
    if judged.is_empty() {
        return false;
    }
    let next_step = format!(
        "{} names `{self_type}` and {}, and ripr does not follow that call to `{}`, so it does not report a gap. Check that an assertion in that test would fail if `{}` returned the wrong value; add a test only if none does.",
        judged.join(", "),
        kind.advice(&owner.name),
        owner.name,
        owner.name,
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &[]);
    true
}

/// Withhold an `exposed` verdict on a standard `Drop::drop`. A destructor runs
/// at scope end and cannot be named, so a test never calls it; every route to
/// `exposed` (a reach the scope-end relation set, a name match on the free
/// `drop`) credits an assertion no authority ties to that scope end, whichever
/// probe family the changed line is (#10553). The one exception is the
/// scope-end carrier (#10814): when every strong contributing test ends the
/// guard's scope over a handle its assertion reads, the verdict stands.
/// Returns `true` when rewritten.
pub(in crate::analysis) fn withhold_destructor_credit(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.class != ExposureClass::Exposed || finding.static_limit_kind.is_some() {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    if !super::is_drop_destructor(owner, index) {
        return false;
    }
    // #10814: an assertion the scope-end carrier ties to this destructor's
    // write stands; every other route to `exposed` is still withheld.
    if super::destructor_credit_established(finding, owner, index) {
        return false;
    }
    let next_step = format!(
        "`{}` runs at the end of a scope no test can name, and no assertion is tied to that scope end, so ripr does not report it exposed. Check that an assertion would fail if the destructor did the wrong thing.",
        owner.name,
    );
    withhold_gap(
        finding,
        LIMIT,
        "whether an assertion observes the effect of the scope end that runs the destructor",
        next_step,
        &[],
    );
    true
}

/// Whether the earlier stages are established and only the discriminator is
/// unconfirmed, the way `pinned_shape` reads it: reach is already decided by
/// the caller, infection is read, and propagation is read or weak. A finding
/// whose infection or propagation is not read has a different missing leg and
/// keeps its own gap. Stage states, not summary text, carry this: the summary
/// differs between "an assertion matched but cannot confirm" and "no assertion
/// matched", and both leave the credit unconfirmed.
fn only_discrimination_unconfirmed(finding: &Finding) -> bool {
    let ripr = &finding.ripr;
    ripr.infect.state == StageState::Yes
        && matches!(ripr.propagate.state, StageState::Yes | StageState::Weak)
}

/// Compute the syntax-only decision while the admitted pre-cap rows are in scope.
/// Retaining this compact result avoids carrying an unbounded row set for every
/// probe while diff classification waits for its serial limit pass.
pub(in crate::analysis) fn pre_cap_syntax_only(
    finding: &Finding,
    index: &RustIndex,
    direct_rows: &[crate::domain::RelatedTest],
) -> Option<bool> {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return None;
    }
    let owner_id = finding.probe.owner.as_ref()?;
    let line = finding.probe.location.line;
    let owner = index.functions().iter().find(|function| {
        function.id == *owner_id && function.start_line <= line && line <= function.end_line
    })?;
    if !matches!(ImplicitCall::of_owner(owner), Some(ImplicitCall::Syntax(_))) {
        return None;
    }
    Some(reached_only_through_syntax(
        finding,
        owner,
        index,
        Some(direct_rows),
    ))
}

/// Whether every test related to the owner by a direct call reaches it only
/// through syntax: none of them calls `.name(` on the owner's method, so the relation
/// came from an operator, loop or scope exit the language runs. A finding with
/// no such test, or with one that names the method, is judged by its own
/// evidence.
fn reached_only_through_syntax(
    finding: &Finding,
    owner: &FunctionSummary,
    index: &RustIndex,
    direct_rows: Option<&[crate::domain::RelatedTest]>,
) -> bool {
    let mut direct = direct_rows
        .unwrap_or(&finding.related_tests)
        .iter()
        .filter(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .peekable();
    if direct.peek().is_none() {
        return false;
    }
    let path_owner = type_path_owner(owner, index);
    let capped = finding
        .related_tests_matched_total
        .is_some_and(|total| finding.related_tests.len() < total);
    // Fallback callers without pre-cap evidence must still fail closed.
    if direct_rows.is_none() && capped {
        return true;
    }
    // Large capped relation sets must not scan the full test index once per
    // row. Preserve `find`'s first-match behavior for duplicate identities.
    // Small sets retain the cheap linear path without building a map.
    let lookup = (capped && direct_rows.is_some_and(|rows| rows.len() > 8)).then(|| {
        let mut lookup = std::collections::HashMap::new();
        for test in index.tests().iter() {
            lookup
                .entry((test.name.as_str(), test.file.as_path(), test.start_line))
                .or_insert(test);
        }
        lookup
    });
    direct.all(|related| {
        let test = if let Some(lookup) = &lookup {
            lookup
                .get(&(related.name.as_str(), related.file.as_path(), related.line))
                .copied()
        } else {
            index.tests().iter().find(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
            })
        };
        test.is_none_or(|test| {
            if is_ignored(test) {
                return true;
            }
            let code = crate::analysis::extract::mask_comments_and_strings(&test.body);
            let scope_is_clear =
                |path: &TypePathOwner| type_path_scope_is_clear(test, owner, index, path);
            // Previously a capped finding was withheld outright. Do not
            // lift that safety on a same-named call of another receiver.
            // Preserve the existing uncapped reading.
            let names_method = calls_method_named(&code, &owner.name)
                && (!capped
                    || path_owner.as_ref().is_some_and(|path| {
                        path.sole_method
                            && path.trait_unique
                            && scope_is_clear(path)
                            && inherent_receiver_call_resolves(
                                test,
                                &owner.name,
                                &path.self_type,
                                Some(index),
                                Some(owner),
                            )
                    }));
            !names_method
                && !path_owner.as_ref().is_some_and(|path| {
                    scope_is_clear(path) && calls_by_type_path(&code, &owner.name, path)
                })
        })
    })
}

/// Check only the scopes enclosing this test, not sibling modules. An
/// unresolved import is not proof that a bare type or trait names the owner.
fn type_path_scope_is_clear(
    test: &TestFact,
    owner: &FunctionSummary,
    index: &RustIndex,
    path: &TypePathOwner,
) -> bool {
    if test.file != owner.file {
        return false;
    }
    let Some(snapshot) = index.source_syntax.get(index, &test.file) else {
        return false;
    };
    let (Some(scopes), Some(tree)) = (snapshot.lexical_items(), snapshot.tree()) else {
        return false;
    };
    let mut tests = scopes.functions.iter().filter(|f| {
        f.name == test.name && f.start_line <= test.end_line && test.start_line <= f.end_line
    });
    let Some(test_scope) = tests.next() else {
        return false;
    };
    if tests.next().is_some() {
        return false;
    }
    let mut owners = scopes.functions.iter().filter(|f| {
        f.name == owner.name && f.start_line <= owner.start_line && owner.start_line <= f.end_line
    });
    let Some(owner_scope) = owners.next() else {
        return false;
    };
    if owners.next().is_some() {
        return false;
    }
    let owner_module = owner_scope.modules.first().copied();
    if owner_module.is_some_and(|module| !test_scope.modules.contains(&module)) {
        return false;
    }
    let root = tree.syntax();
    let plain_self_type = impl_self_type_is_unqualified(&owner.id.0);
    let clear = |item: ast::Item, module, parent| {
        let owner_path = |value: &str| {
            value.strip_prefix("super::") == Some(path.self_type.as_str()) && parent == owner_module
        };
        if let ast::Item::Use(ref item) = item {
            return super::reveal::flattened_use_paths(&item.syntax().text().to_string())
                .iter()
                .all(|import| {
                    // In `use a::T::{self}`, the leaf binds `T`, not `self`.
                    // Retain the flattened path's existing contract for its
                    // other consumers; normalize only this binding check.
                    let target = import.path.strip_suffix("::self").unwrap_or(&import.path);
                    let bound = import
                        .alias
                        .as_deref()
                        .unwrap_or_else(|| target.rsplit("::").next().unwrap_or(""));
                    let bound = bound.strip_prefix("r#").unwrap_or(bound);
                    if bound == "*" {
                        return matches!(
                            import.path.as_str(),
                            "super::*" | "self::*" | "std::ops::*" | "core::ops::*"
                        );
                    }
                    if bound == path.self_type {
                        return owner_path(target);
                    }
                    if bound == path.trait_name {
                        return target
                            .strip_prefix("std::ops::")
                            .or_else(|| target.strip_prefix("core::ops::"))
                            == Some(path.trait_name.as_str());
                    }
                    true
                });
        }
        if let ast::Item::Module(ref module) = item {
            return !module.name().is_some_and(|name| {
                let name = name.text().to_string();
                name.strip_prefix("r#").unwrap_or(&name) == path.self_type
            });
        }
        let text =
            crate::analysis::extract::mask_comments_and_strings(&item.syntax().text().to_string());
        if matches!(item, ast::Item::Fn(_) | ast::Item::Impl(_)) {
            return true;
        }
        if matches!(item, ast::Item::MacroRules(_) | ast::Item::MacroDef(_)) {
            return !contains_as_whole_word(&text, &path.self_type)
                && !contains_as_whole_word(&text, &path.trait_name);
        }
        if matches!(item, ast::Item::MacroCall(_)) {
            // A macro invocation is unresolved: its expansion can introduce
            // any binding without naming it at the invocation site, so any
            // invocation in an enclosing scope refuses the lift.
            return false;
        }
        let rebinds =
            rebinds_type_name(&text, &path.self_type) || rebinds_type_name(&text, &path.trait_name);
        if module == owner_module {
            // The owner's own module defines the impl type, so a `struct`,
            // `enum` or `union` of that name is the genuine definition, but
            // only when the impl header spells its self type unqualified. A
            // qualified header (`impl Add for other::Money`) leaves a same-
            // module `type Money = i64` or `struct Money` free to shadow it.
            // Any other rebinding of the type or trait name is refused.
            return !rebinds
                || (plain_self_type
                    && matches!(
                        item,
                        ast::Item::Struct(_) | ast::Item::Enum(_) | ast::Item::Union(_)
                    )
                    && !rebinds_type_name(&text, &path.trait_name));
        }
        !rebinds
    };
    if !tree.items().all(|item| clear(item, None, None)) {
        return false;
    }
    let mut enclosing: Vec<_> = scopes
        .modules
        .iter()
        .filter(|m| test_scope.modules.contains(&m.range))
        .collect();
    enclosing.sort_by_key(|m| m.range.start());
    let mut parent = None;
    for module in enclosing {
        let node = module.item.to_node(root);
        if !node.item_list().is_some_and(|items| {
            items
                .items()
                .all(|item| clear(item, Some(module.range), parent))
        }) {
            return false;
        }
        parent = Some(module.range);
    }
    true
}

/// Whether the impl header of an owner id spells its self type as a bare name
/// (`impl Add for Money`, `impl Add for &Money`), not a path
/// (`impl Add for other::Money`) or anything else the id cannot show plainly.
fn impl_self_type_is_unqualified(owner_id: &str) -> bool {
    let Some(impl_rest) = owner_id.split("::impl ").nth(1) else {
        return false;
    };
    let Some((impl_body, _)) = impl_rest.rsplit_once("::") else {
        return false;
    };
    let self_ty = impl_body
        .rsplit_once(" for ")
        .map_or(impl_body, |(_, ty)| ty);
    let bare = self_ty.trim_start_matches('&').trim();
    !bare.is_empty() && bare.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// How the language reaches the owner without a call that names it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ImplicitCall<'a> {
    /// An operator, scope exit, loop or index: the syntax is the call.
    Syntax(&'a str),
    /// A user or library trait method called on a receiver whose concrete
    /// type ripr does not infer (`dyn Trait`, a generic, a second impl).
    Dispatch,
}

impl<'a> ImplicitCall<'a> {
    fn of_owner(owner: &'a FunctionSummary) -> Option<Self> {
        let trait_name = owner_trait_name(&owner.id.0)?;
        let trait_name = trait_name.rsplit("::").next().unwrap_or(&trait_name);
        if let Some(known) = IMPLICIT_SYNTAX_TRAITS
            .iter()
            .find(|known| **known == trait_name)
        {
            Some(Self::Syntax(known))
        } else if OWNED_ELSEWHERE_TRAITS.contains(&trait_name) {
            None
        } else if owner.item.has_self_param {
            Some(Self::Dispatch)
        } else {
            // An associated function is called by path (`Type::create()`), which
            // existing reach matching resolves; a `.create()` receiver call is
            // not evidence of it.
            None
        }
    }

    fn advice(&self, method: &str) -> String {
        match self {
            Self::Syntax(_) => format!(
                "uses it, and the language may call `{method}` from an operator, a scope exit, a loop or an index"
            ),
            Self::Dispatch => format!(
                "calls a method named `{method}` on a receiver whose type ripr does not infer"
            ),
        }
    }
}

fn test_makes_implicit_call(
    test: &TestFact,
    owner: &FunctionSummary,
    self_type: &str,
    kind: ImplicitCall<'_>,
) -> bool {
    // A test with no exact assertion is a real gap whatever runs the owner.
    if !test
        .assertions
        .iter()
        .any(|assertion| assertion.strength == OracleStrength::Strong)
    {
        return false;
    }
    let code = crate::analysis::extract::mask_comments_and_strings(&test.body);
    if !contains_as_whole_word(&code, self_type) {
        return false;
    }
    match kind {
        ImplicitCall::Syntax(trait_name) => {
            uses_syntax_of(trait_name, &code) && binds_impl_value(&code, trait_name, self_type)
        }
        ImplicitCall::Dispatch => {
            calls_method_named(&code, &owner.name)
                && dispatch_binds_impl_value(&code, &owner.name, self_type)
        }
    }
}

/// Whether the owner's trait is not the one the implicit call reaches. A
/// dispatch trait is shadowed when it is declared more than once in the
/// workspace or another impl of it for the same self type defines the same
/// method. A syntax trait names a std operator or glue trait, so any workspace
/// declaration of that name is a different trait the syntax cannot call.
fn trait_is_shadowed(
    owner: &FunctionSummary,
    self_type: &str,
    index: &RustIndex,
    kind: ImplicitCall<'_>,
) -> bool {
    let Some(trait_name) = owner_trait_name(&owner.id.0) else {
        return true;
    };
    let same_impl = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == owner.name
                && owner_trait_name(&function.id.0).as_deref() == Some(trait_name.as_str())
                && impl_self_type_name(&function.id.0).as_deref() == Some(self_type)
        })
        .count();
    if kind == ImplicitCall::Dispatch && same_impl != 1 {
        return true;
    }
    let short_name = trait_name.rsplit("::").next().unwrap_or(&trait_name);
    let declarations: usize = index
        .files()
        .values()
        .map(|file| {
            let masked = crate::analysis::extract::mask_comments_and_strings(&file.source);
            let tokens: Vec<&str> = masked
                .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .collect();
            tokens
                .windows(2)
                .filter(|pair| pair[0] == "trait" && pair[1] == short_name)
                .count()
        })
        .sum();
    match kind {
        ImplicitCall::Syntax(_) => declarations > 0,
        ImplicitCall::Dispatch => declarations > 1,
    }
}

/// Whether `code` (comments and strings masked) spells the syntax that calls
/// a method of `trait_name`. Drop glue, iteration and auto-deref have no
/// token to look for, so those traits pass on the type name alone.
fn uses_syntax_of(trait_name: &str, code: &str) -> bool {
    let has = |needles: &[&str]| needles.iter().any(|needle| code.contains(needle));
    match trait_name {
        "Add" => binary(code, '+'),
        "AddAssign" => has(&["+="]),
        "Sub" => minus(code),
        "SubAssign" => has(&["-="]),
        "Neg" => minus(code),
        "Mul" => binary(code, '*'),
        "MulAssign" => has(&["*="]),
        "Div" => binary(code, '/'),
        "DivAssign" => has(&["/="]),
        "Rem" => binary(code, '%'),
        "RemAssign" => has(&["%="]),
        "BitAnd" => binary(code, '&'),
        "BitAndAssign" => has(&["&="]),
        "BitOr" => binary(code, '|'),
        "BitOrAssign" => has(&["|="]),
        "BitXor" => binary(code, '^'),
        "BitXorAssign" => has(&["^="]),
        "Shl" => has(&["<<"]),
        "ShlAssign" => has(&["<<="]),
        "Shr" => has(&[">>"]),
        "ShrAssign" => has(&[">>="]),
        "Not" => not_operator(code),
        "Index" | "IndexMut" => indexes(code),
        "Drop" => drops_at_scope_end(code),
        "PartialOrd" => has(&["<", ">", "partial_cmp", "max", "min", "sort", "clamp"]),
        _ => true,
    }
}

/// `op` between two operands: the nearest non-space character before it ends
/// an operand (an identifier, literal, `)` or `]`), and it is neither doubled
/// (`&&`, `||`) nor an assignment (`+=`). A reference (`&x`), a closure
/// (`|x|`) or a dereference (`*r`) has no operand before it.
fn binary(code: &str, op: char) -> bool {
    let stripped;
    let code = if op == '|' {
        stripped = without_closure_params(code);
        stripped.as_str()
    } else {
        code
    };
    code.match_indices(op).any(|(at, _)| {
        let before = code[..at].trim_end().chars().next_back();
        let after = code[at + 1..].chars().next();
        before.is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']'))
            && after != Some(op)
            && after != Some('=')
            && !code[..at].ends_with(op)
    })
}

/// `code` with each closure parameter list (`|x, y|`) blanked, same length: a `|` that
/// opens a list follows `(`, `,`, `=`, `{`, `;`, `move` or the start.
fn without_closure_params(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut chars = code.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        let head = code[..at].trim_end();
        let opens = ch == '|'
            && (head.is_empty()
                || head.ends_with(['(', ',', '=', '{', ';'])
                || head.ends_with("move"));
        if opens {
            // Blank every byte, so offsets into the result are offsets into
            // `code` (the operator sites are typed against the original).
            out.push(' ');
            for (_, inner) in chars.by_ref() {
                out.extend(std::iter::repeat_n(' ', inner.len_utf8()));
                if inner == '|' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// A `[` that indexes an operand (`table[2]`), not an array or `vec![..]`.
fn indexes(code: &str) -> bool {
    code.match_indices('[').any(|(at, _)| {
        code[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']'))
    })
}

/// The test drops a value on purpose (`drop(x)`) or holds it in an inner
/// block that ends before the assertion; the test fn's own braces do not
/// count.
fn drops_at_scope_end(code: &str) -> bool {
    code.contains("drop(") || code.matches('{').count() >= 2
}

/// A `-` that is not part of `->` or `-=`.
fn minus(code: &str) -> bool {
    code.match_indices('-')
        .any(|(at, _)| !matches!(code[at + 1..].chars().next(), Some('>' | '=')))
}

/// A `!` that is neither a macro bang (`assert!`) nor part of `!=`.
fn not_operator(code: &str) -> bool {
    code.match_indices('!').any(|(at, _)| {
        let after = code[at + 1..].chars().next();
        let before = code[..at].chars().next_back();
        after != Some('=') && !before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
    })
}

/// Which operand of an operator site is the `Self` of the trait call: a
/// binary or assigning operator calls the trait on its left operand, a unary
/// operator on the one after it.
#[derive(Clone, Copy)]
enum SelfOperand {
    Left,
    Right,
}

/// One place in a test where an operator trait's syntax is spelled.
pub(in crate::analysis) struct OperatorSite {
    pub(in crate::analysis) at: usize,
    pub(in crate::analysis) len: usize,
    operand: SelfOperand,
}

/// What ripr can say about the `Self` operand of an operator site.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operand {
    /// A value of the impl's type: a constructor, a unit value or a binding.
    Bound,
    /// Positively not the impl's type: a numeric or boolean literal, or a
    /// binding whose `let` names another type.
    Other,
    /// A helper call, constant, field, tuple pattern or anything else ripr
    /// does not type.
    Unknown,
}

/// Whether the test's use of the trait's syntax may reach the impl's method.
/// The pass withholds a gap, so it asks the fail-closed question: is the use
/// provably unrelated? An operator applied to integers beside an unused value
/// of the type, a binding of another type, or a result thrown away does not
/// run the impl; an operand ripr cannot type (a helper's return value, an
/// associated constant, a destructured tuple) might, and keeps the withholding.
fn binds_impl_value(code: &str, trait_name: &str, self_type: &str) -> bool {
    let Some(sites) = operator_sites(trait_name, code) else {
        // Drop glue, iteration and auto-deref have no operator to inspect.
        return match trait_name {
            "Drop" => drops_at_scope_end(code),
            "Iterator" | "IntoIterator" | "DoubleEndedIterator" | "ExactSizeIterator" => {
                iterates(code)
            }
            _ => true,
        };
    };
    // Ordering is also reached through methods that spell no operator.
    if trait_name == "PartialOrd"
        && ["partial_cmp", "max", "min", "sort", "clamp"]
            .iter()
            .any(|name| calls_method_named(code, name))
    {
        return true;
    }
    sites.iter().any(|site| {
        let operand = match site.operand {
            SelfOperand::Left => operand_before(code, site.at, self_type),
            SelfOperand::Right => operand_after(code, site.at + site.len, self_type),
        };
        // A compound assignment or `IndexMut` write changes its operand, so
        // its unit result being thrown away does not make it unrelated.
        let mutates = trait_name.ends_with("Assign") || trait_name == "IndexMut";
        operand != Operand::Other && (mutates || result_is_used(code, site.at))
    })
}

/// The operator sites of `trait_name`'s syntax, or `None` for a trait with no
/// operator to find.
fn operator_sites(trait_name: &str, code: &str) -> Option<Vec<OperatorSite>> {
    let binary = |ops: &[&str]| -> Vec<OperatorSite> {
        ops.iter()
            .flat_map(|op| operator_positions(code, op, true))
            .collect()
    };
    Some(match trait_name {
        "Add" => binary(&["+"]),
        "Sub" => binary(&["-"]),
        "Mul" => binary(&["*"]),
        "Div" => binary(&["/"]),
        "Rem" => binary(&["%"]),
        "BitAnd" => binary(&["&"]),
        "BitOr" => binary(&["|"]),
        "BitXor" => binary(&["^"]),
        "Shl" => binary(&["<<"]),
        "Shr" => binary(&[">>"]),
        "AddAssign" => binary(&["+="]),
        "SubAssign" => binary(&["-="]),
        "MulAssign" => binary(&["*="]),
        "DivAssign" => binary(&["/="]),
        "RemAssign" => binary(&["%="]),
        "BitAndAssign" => binary(&["&="]),
        "BitOrAssign" => binary(&["|="]),
        "BitXorAssign" => binary(&["^="]),
        "ShlAssign" => binary(&["<<="]),
        "ShrAssign" => binary(&[">>="]),
        "Neg" => operator_positions(code, "-", false),
        "Not" => operator_positions(code, "!", false),
        "Index" | "IndexMut" => code
            .match_indices('[')
            .filter(|(at, _)| operand_char_before(code, *at))
            .map(|(at, _)| OperatorSite {
                at,
                len: 1,
                operand: SelfOperand::Left,
            })
            .collect(),
        "PartialOrd" => binary(&["<", ">"]),
        _ => return None,
    })
}

/// Occurrences of `op` as a binary (`binary`) or unary operator. A binary
/// operator has an operand before it and is not part of a longer operator
/// (`->`, `=>`, `&&`, `||`; `<=` is still ordering) or a closure parameter
/// list; a unary one has none.
pub(in crate::analysis) fn operator_positions(
    code: &str,
    op: &str,
    binary: bool,
) -> Vec<OperatorSite> {
    let stripped;
    let code = if op == "|" {
        stripped = without_closure_params(code);
        stripped.as_str()
    } else {
        code
    };
    let bytes = code.as_bytes();
    let first = op.as_bytes()[0];
    code.match_indices(op)
        .filter(|(at, _)| {
            let after = bytes.get(at + op.len()).copied();
            let before = at.checked_sub(1).map(|i| bytes[i]);
            let continues = match after {
                Some(b'=') => !matches!(op, "<" | ">") || op.len() > 1,
                Some(next) => {
                    next == first && op.len() == 1 && !matches!(op, "!")
                        || (op == "-" && next == b'>')
                        || (matches!(op, "<" | ">") && matches!(next, b'<' | b'>'))
                }
                None => false,
            };
            let doubled = before == Some(first) && op.len() == 1 && !matches!(op, "!");
            let arrow = matches!(op, ">") && before == Some(b'-')
                || matches!(op, ">" | "<") && before == Some(b'=')
                || matches!(op, "<") && before == Some(b':');
            // `!=` is not `Not`, and a macro bang (`assert!`) has an
            // identifier before it.
            let not_macro = op != "!" || !operand_char_before(code, *at) && after != Some(b'=');
            !continues
                && !doubled
                && !arrow
                && not_macro
                && (operand_char_before(code, *at) == binary || op == "!")
        })
        .map(|(at, _)| OperatorSite {
            at,
            len: op.len(),
            operand: if binary {
                SelfOperand::Left
            } else {
                SelfOperand::Right
            },
        })
        .collect()
}

/// Whether the nearest non-space character before `at` ends an operand.
fn operand_char_before(code: &str, at: usize) -> bool {
    code[..at]
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']' | '}'))
}

/// The `Self` operand ending just before byte `end`.
fn operand_before(code: &str, end: usize, ty: &str) -> Operand {
    if expr_ending_at_has_type(code, end, ty) {
        return Operand::Bound;
    }
    let head = code[..end].trim_end();
    let token_start = head
        .rfind(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '.')))
        .map_or(0, |i| {
            i + head[i..].chars().next().map_or(1, char::len_utf8)
        });
    // A higher-precedence operator before the token (`Money(2) * 3 + x`)
    // makes the real `Self` operand the whole product, which is not typed here.
    let alone = stands_alone(&head[..token_start]);
    if is_literal_token(&head[token_start..]) {
        // A literal is another type only when `Self` is not a primitive
        // (`impl Mul<Money> for f64` is called with `2.0 * Money(1)`).
        return if alone && !is_primitive_head(ty) {
            Operand::Other
        } else {
            Operand::Unknown
        };
    }
    let lets = code[..end].rfind(';').map_or("", |at| &code[..=at]);
    match named_binding_operand(ident_before(code, end), lets, &code[..end], ty) {
        Operand::Other if !alone => Operand::Unknown,
        operand => operand,
    }
}

/// Whether the text before an operand ends where an expression can begin, so
/// no operator of tighter precedence takes the operand first.
fn stands_alone(before: &str) -> bool {
    let before = before.trim_end();
    match before.bytes().last() {
        None | Some(b'(' | b',' | b'{' | b';' | b'[') => true,
        Some(b'=') => {
            before.ends_with("=>")
                || !before[..before.len() - 1].ends_with(|ch: char| {
                    matches!(
                        ch,
                        '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^'
                    )
                })
        }
        Some(_) => false,
    }
}

/// The unary `Self` operand starting at byte `start`: a constructor or unit
/// value whose path head is `ty`, or one identifier bound to `ty`. A method
/// chain or field access after it may change the type, so it is unknown.
fn operand_after(code: &str, start: usize, ty: &str) -> Operand {
    let rest = code[start..].trim_start();
    let rest = rest.strip_prefix('&').map_or(rest, str::trim_start);
    let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
    let path_len = rest
        .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | ':' | '.')))
        .unwrap_or(rest.len());
    let path = &rest[..path_len];
    if is_literal_token(path) {
        return if is_primitive_head(ty) {
            Operand::Unknown
        } else {
            Operand::Other
        };
    }
    if path.is_empty() || path.contains('.') || rest[path_len..].trim_start().starts_with('.') {
        return Operand::Unknown;
    }
    // `-Money(1).0` negates the field, not the `Money`: a postfix `.`, `?` or
    // `[` after the constructor's arguments binds tighter than the operator.
    // The same holds for `-t[0]` and `!t?` on a bare name.
    if postfix_follows_group(&rest[path_len..])
        || rest[path_len..].trim_start().starts_with(['[', '?'])
    {
        return Operand::Unknown;
    }
    if path.split("::").next() == Some(ty) {
        return Operand::Bound;
    }
    let lets = code[..start].rfind(';').map_or("", |at| &code[..=at]);
    named_binding_operand(
        (!path.contains("::")).then_some(path),
        lets,
        &code[..start],
        ty,
    )
}

/// Whether `rest` opens a `(..)` or `{..}` group that is followed by a
/// postfix `.`, `?` or `[`.
fn postfix_follows_group(rest: &str) -> bool {
    let rest = rest.trim_start();
    let (open, close) = match rest.bytes().next() {
        Some(b'(') => (b'(', b')'),
        Some(b'{') => (b'{', b'}'),
        _ => return false,
    };
    let mut depth = 0usize;
    for (i, byte) in rest.bytes().enumerate() {
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return matches!(
                    rest[i + 1..].trim_start().bytes().next(),
                    Some(b'.' | b'?' | b'[')
                );
            }
        }
    }
    false
}

/// An identifier operand typed by its `let`: the impl's type, another type
/// (an uppercase head, so a helper `money(2)` is not mistaken for one), or
/// not typed at all.
fn named_binding_operand(name: Option<&str>, lets: &str, scope: &str, ty: &str) -> Operand {
    let Some(name) = name else {
        return Operand::Unknown;
    };
    if let_binding_mentions_type(lets, name, ty) {
        return Operand::Bound;
    }
    match let_binding_certain_type_head(lets, name) {
        Some(head)
            if head.starts_with(|ch: char| ch.is_uppercase())
                && head != ty
                && !name_is_rebound(scope, name) =>
        {
            Operand::Other
        }
        _ => Operand::Unknown,
    }
}

/// Whether `name` may denote a different value at the use than its last `let`
/// says: it is bound by more than one `let` (a shadow in a closed block, an
/// `if let`/`while let` pattern), by a `for` pattern, or by a closure
/// parameter or match arm. Only a name bound exactly once keeps its type.
fn name_is_rebound(lets: &str, name: &str) -> bool {
    let mut binders = 0usize;
    for (at, _) in lets.match_indices("let ") {
        let pattern = lets[at + 4..].split('=').next().unwrap_or("");
        binders += usize::from(contains_as_whole_word(pattern, name));
    }
    for (at, _) in lets.match_indices("for ") {
        let pattern = lets[at + 4..].split(" in ").next().unwrap_or("");
        binders += usize::from(contains_as_whole_word(pattern, name));
    }
    let closure_param = lets
        .split('|')
        .skip(1)
        .step_by(2)
        .any(|segment| contains_as_whole_word(segment, name));
    let parts: Vec<&str> = lets.split("=>").collect();
    let arm = parts
        .iter()
        .take(parts.len().saturating_sub(1))
        .any(|before| {
            // Any mention in the arm's pattern (`Foo { a: t } =>`, a guard,
            // an `@` binding) may bind the name; over-matching only withholds.
            let pattern = before.rsplit(';').next().unwrap_or(before);
            contains_as_whole_word(pattern, name)
        });
    binders != 1 || closure_param || arm
}

/// Whether `ty` names a primitive (`f64`, `u8`), which a literal can be.
fn is_primitive_head(ty: &str) -> bool {
    ty.starts_with(|ch: char| ch.is_lowercase())
}

/// A numeric literal (`1`, `2.5`, `1u8`, `0xff`) or a boolean.
fn is_literal_token(token: &str) -> bool {
    token.starts_with(|ch: char| ch.is_ascii_digit()) || matches!(token, "true" | "false")
}

/// Whether the statement holding byte `at` can hand its value on. `let _ = ..`
/// throws the value away, and so does a name that nothing reads again; any
/// other statement (an assertion, a reassignment, a destructuring `let`)
/// might feed an assertion.
pub(in crate::analysis) fn result_is_used(code: &str, at: usize) -> bool {
    let (start, end) = statement_bounds(code, at);
    let Some(pattern) = code[start..end].trim().strip_prefix("let ") else {
        return true;
    };
    let pattern = pattern.trim_start();
    let pattern = pattern
        .strip_prefix("mut ")
        .map_or(pattern, str::trim_start);
    let len = pattern
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(pattern.len());
    let name = &pattern[..len];
    // `let Money(v) = ..` and `let P { x } = ..` bind names after the head.
    let destructures = matches!(
        pattern[len..].trim_start().bytes().next(),
        Some(b'(' | b'{')
    ) || name.starts_with(|ch: char| ch.is_uppercase());
    match name {
        "" => true,
        _ if destructures => true,
        "_" => false,
        name => contains_as_whole_word(&code[end..], name),
    }
}

/// The byte range of the statement holding byte `at`, which must be on an
/// ASCII character. Braces nest, so a struct literal or closure body inside
/// the statement does not end it.
fn statement_bounds(code: &str, at: usize) -> (usize, usize) {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut start = 0;
    for i in (0..at).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' if depth > 0 => depth -= 1,
            b'{' => {
                start = i + 1;
                break;
            }
            b';' if depth == 0 => {
                start = i + 1;
                break;
            }
            _ => {}
        }
    }
    depth = 0;
    let mut end = code.len();
    for (i, byte) in bytes.iter().enumerate().skip(at) {
        match byte {
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'}' => {
                end = i;
                break;
            }
            b';' if depth == 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    (start, end)
}

/// The test may iterate a value: a `for` loop, any method call (a terminal
/// adapter such as `any`, `next_back` or `len` consumes the iterator without a
/// name worth listing), or a path call into an iterator trait. Only code with
/// none of these is provably not running the iterator.
fn iterates(code: &str) -> bool {
    code.contains("for ")
        || code.contains("from_iter")
        || [
            "Iterator::",
            "IntoIterator::",
            "DoubleEndedIterator::",
            "ExactSizeIterator::",
        ]
        .iter()
        .any(|path| code.contains(path))
        || calls_any_method(code)
}

/// Whether the code has a `.name(` or `.name::<..>(` method call.
fn calls_any_method(code: &str) -> bool {
    code.match_indices('.').any(|(dot, _)| {
        let rest = &code[dot + 1..];
        let len = rest
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        len > 0
            && !rest.starts_with(|ch: char| ch.is_ascii_digit())
            && rest[len..].trim_start().starts_with(['(', ':'])
    })
}

/// Whether the dispatched `.method(` may be called on a value of the impl's
/// type: a call whose receiver is a binding of another type is not, and a
/// call whose result is thrown away checks nothing.
fn dispatch_binds_impl_value(code: &str, method: &str, self_type: &str) -> bool {
    method_call_dots(code, method).into_iter().any(|dot| {
        let lets = code[..dot].rfind(';').map_or("", |at| &code[..=at]);
        let other_type = ident_before(code, dot).is_some_and(|receiver| {
            named_binding_operand(Some(receiver), lets, &code[..dot], self_type) == Operand::Other
        });
        !other_type && result_is_used(code, dot)
    })
}

/// The identifier ending just before byte `end`, if the expression there is a
/// plain name (not a field, call result or index).
fn ident_before(code: &str, end: usize) -> Option<&str> {
    let head = code[..end].trim_end();
    let start = head
        .char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    let name = &head[start..];
    let preceded_by_dot = head[..start].trim_end().ends_with('.');
    (!name.is_empty() && !preceded_by_dot).then_some(name)
}

/// The byte offset of the `.` of each `.name(` or `.name::<..>(` call.
fn method_call_dots(code: &str, name: &str) -> Vec<usize> {
    code.match_indices(name)
        .filter_map(|(at, _)| {
            let before = code[..at].trim_end();
            let rest = code[at + name.len()..].trim_start();
            let whole = at == 0
                || !code.as_bytes()[at - 1].is_ascii_alphanumeric()
                    && code.as_bytes()[at - 1] != b'_';
            (whole && before.ends_with('.') && (rest.starts_with('(') || rest.starts_with("::<")))
                .then(|| before.len() - 1)
        })
        .collect()
}

/// Whether `code` (comments and strings masked) calls `.name(` on some
/// receiver, optionally through a turbofish.
fn calls_method_named(code: &str, name: &str) -> bool {
    code.match_indices(name).any(|(at, _)| {
        let before = code[..at].trim_end();
        let rest = code[at + name.len()..].trim_start();
        before.ends_with('.') && (rest.starts_with('(') || rest.starts_with("::<"))
    })
}

/// What a type-path call must name to be a call of this owner.
struct TypePathOwner {
    self_type: String,
    trait_name: String,
    /// The impl type has no other method of this name (inherent or another
    /// trait's), so `Type::name(..)` cannot resolve elsewhere.
    sole_method: bool,
    /// No other impl of the same trait for this type defines the method
    /// (`Add<f64>` beside `Add<Money>`), so `<Type as Trait>::name` names one.
    trait_unique: bool,
    /// The owner's trait generic arguments as the impl header spells them,
    /// whitespace removed (`Money` for `impl Add<Money> for Money`); `None`
    /// when the header names the trait bare.
    trait_args: Option<String>,
}

/// The generic arguments of the trait in an impl owner's symbol id, whitespace
/// removed (`impl Add<Money> for Money::add` → `Money`). `None` for a bare
/// trait, an inherent impl or a free function.
fn owner_trait_args(owner_id: &str) -> Option<String> {
    let impl_rest = owner_id.split("::impl ").nth(1)?;
    let impl_body = impl_rest.rsplit_once("::")?.0;
    let (trait_ty, _) = impl_body.rsplit_once(" for ")?;
    let (_, args) = trait_ty.split_once('<')?;
    let args = args.trim_end().strip_suffix('>')?;
    Some(args.chars().filter(|ch| !ch.is_whitespace()).collect())
}

fn type_path_owner(owner: &FunctionSummary, index: &RustIndex) -> Option<TypePathOwner> {
    let self_type = impl_self_type_name(&owner.id.0)?;
    let trait_name = owner_trait_name(&owner.id.0)?;
    let trait_name = trait_name
        .rsplit("::")
        .next()
        .unwrap_or(&trait_name)
        .to_string();
    let same_name = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == owner.name
                && impl_self_type_name(&function.id.0).as_deref() == Some(self_type.as_str())
        })
        .count();
    let same_trait = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == owner.name
                && impl_self_type_name(&function.id.0).as_deref() == Some(self_type.as_str())
                && owner_trait_name(&function.id.0)
                    .is_some_and(|other| other.rsplit("::").next() == Some(trait_name.as_str()))
        })
        .count();
    Some(TypePathOwner {
        self_type,
        trait_name,
        sole_method: same_name == 1 && !unindexed_impl_may_define(owner, index),
        trait_unique: same_trait == 1,
        trait_args: owner_trait_args(&owner.id.0),
    })
}

/// std derives and the methods they define. A derive absent from this table is
/// a derive ripr cannot read, so it may define any method.
const STD_DERIVE_METHODS: &[(&str, &[&str])] = &[
    ("Clone", &["clone", "clone_from"]),
    ("Copy", &[]),
    ("Debug", &["fmt"]),
    ("Default", &["default"]),
    ("Eq", &[]),
    ("Hash", &["hash", "hash_slice"]),
    ("Ord", &["cmp", "max", "min", "clamp"]),
    ("PartialEq", &["eq", "ne"]),
    ("PartialOrd", &["partial_cmp", "lt", "le", "gt", "ge"]),
];

/// What a type's derives may define.
#[derive(Default)]
struct DerivedMethods {
    /// A derive ripr cannot read (non-std, or behind `cfg_attr`): any method.
    unreadable: bool,
    /// Methods the std derives on the type define.
    methods: std::collections::BTreeSet<String>,
}

/// The impl sources the workspace index cannot list, read once per index
/// (owner independent) so each owner's question is a lookup, not a corpus walk.
/// Only item headers are visited; function bodies are skipped (#10625).
#[derive(Default)]
pub(super) struct HiddenImpls {
    /// A file the syntax snapshot cannot read: nothing can be ruled out.
    unreadable_source: bool,
    derives: std::collections::BTreeMap<String, DerivedMethods>,
    /// Method names defined by a blanket impl (`impl<T> Trait for T`).
    blanket_methods: std::collections::BTreeSet<String>,
    /// Words of every `macro_rules!`/`macro` definition that writes an `impl`.
    impl_macro_words: std::collections::BTreeSet<String>,
    /// Words of every item-level macro invocation's arguments.
    item_macro_words: std::collections::BTreeSet<String>,
}

impl HiddenImpls {
    pub(super) fn build(index: &RustIndex) -> Self {
        let mut table = Self::default();
        for (file, _) in index.files().iter() {
            let Some(tree) = index
                .source_syntax
                .get(index, file)
                .and_then(|snapshot| snapshot.tree())
            else {
                table.unreadable_source = true;
                continue;
            };
            table.read_items(tree.syntax());
        }
        table
    }

    fn read_items(&mut self, root: &ra_ap_syntax::SyntaxNode) {
        use ra_ap_syntax::{SyntaxKind, WalkEvent};
        let mut walk = root.preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            // Function bodies hold no item that expands at item level.
            if node.kind() == SyntaxKind::FN {
                walk.skip_subtree();
                continue;
            }
            let Some(item) = ast::Item::cast(node) else {
                continue;
            };
            match &item {
                ast::Item::Struct(it) => self.read_derives(it, it.name()),
                ast::Item::Enum(it) => self.read_derives(it, it.name()),
                ast::Item::Union(it) => self.read_derives(it, it.name()),
                ast::Item::MacroRules(_) | ast::Item::MacroDef(_) => {
                    let text = masked_text(item.syntax());
                    if contains_as_whole_word(&text, "impl") {
                        self.impl_macro_words
                            .extend(words_of(&text).into_iter().map(String::from));
                    }
                }
                ast::Item::MacroCall(call) => {
                    // Expression and statement macros (`assert_eq!`) are not items.
                    let at_item_level = item.syntax().parent().is_some_and(|parent| {
                        matches!(
                            parent.kind(),
                            SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST
                        )
                    });
                    if at_item_level && let Some(tokens) = call.token_tree() {
                        self.item_macro_words.extend(
                            words_of(&masked_text(tokens.syntax()))
                                .into_iter()
                                .map(String::from),
                        );
                    }
                }
                ast::Item::Impl(imp) => {
                    self.read_blanket_impl(imp);
                    walk.skip_subtree();
                }
                _ => {}
            }
        }
    }

    fn read_derives<T: HasAttrs>(&mut self, adt: &T, type_name: Option<ast::Name>) {
        let Some(type_name) = type_name else {
            return;
        };
        let entry = self
            .derives
            .entry(type_name.text().trim_start_matches("r#").to_string())
            .or_default();
        for attr in adt.attrs() {
            let text = attr.syntax().text().to_string();
            let idents: Vec<&str> = words_of(&text);
            match idents.split_first() {
                Some((&"derive", derived)) => {
                    for derived in derived {
                        match STD_DERIVE_METHODS
                            .iter()
                            .find(|(known, _)| known == derived)
                        {
                            Some((_, methods)) => {
                                entry.methods.extend(methods.iter().map(|m| m.to_string()));
                            }
                            None if *derived != "std" && *derived != "core" => {
                                entry.unreadable = true;
                            }
                            None => {}
                        }
                    }
                }
                // `cfg_attr(.., derive(..))` hides the derive list behind a cfg.
                Some((&"cfg_attr", rest)) if rest.contains(&"derive") => entry.unreadable = true,
                _ => {}
            }
        }
    }

    /// A trait impl whose self type is one of its own generic parameters
    /// (`impl<T: Into<i64>> Wide for T`, also through `&T`, `&'a mut T` or a
    /// raw pointer) records the methods it defines.
    fn read_blanket_impl(&mut self, imp: &ast::Impl) {
        if imp.trait_().is_none() {
            return;
        }
        let (Some(self_ty), Some(params)) = (imp.self_ty(), imp.generic_param_list()) else {
            return;
        };
        let Some(param) = referent_type_name(self_ty) else {
            return;
        };
        let generic = params.generic_params().any(|generic| {
            matches!(&generic, ast::GenericParam::TypeParam(p)
                if p.name().is_some_and(|n| n.text() == param))
        });
        if !generic {
            return;
        }
        if let Some(items) = imp.assoc_item_list() {
            for item in items.assoc_items() {
                if let ast::AssocItem::Fn(f) = item
                    && let Some(name) = f.name()
                {
                    self.blanket_methods.insert(name.text().to_string());
                }
            }
        }
    }

    fn may_define(&self, self_type: &str, name: &str) -> bool {
        self.unreadable_source
            || self.blanket_methods.contains(name)
            || self
                .derives
                .get(self_type)
                .is_some_and(|d| d.unreadable || d.methods.contains(name))
            || self.impl_macro_words.contains(self_type)
            || self.item_macro_words.contains(self_type)
    }
}

/// The single-segment path a type names after peeling references (with any
/// lifetime), raw pointers and parentheses: `&'a mut T` gives `T`.
fn referent_type_name(mut ty: ast::Type) -> Option<String> {
    loop {
        ty = match ty {
            ast::Type::RefType(it) => it.ty()?,
            ast::Type::PtrType(it) => it.ty()?,
            ast::Type::ParenType(it) => it.ty()?,
            ast::Type::PathType(it) => {
                let path = it.path()?;
                if path.qualifier().is_some() {
                    return None;
                }
                return Some(path.segment()?.name_ref()?.text().to_string());
            }
            _ => return None,
        };
    }
}

fn masked_text(node: &ra_ap_syntax::SyntaxNode) -> String {
    crate::analysis::extract::mask_comments_and_strings(&node.text().to_string())
}

fn words_of(text: &str) -> Vec<&str> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether an impl the index does not list could define the owner's method for
/// its self type, so `Type::name(..)` is not known to resolve to the owner
/// (#10625). The index lists only written, indexed `impl` blocks. Three other
/// sources are invisible to it, and any one makes the answer yes (the
/// conservative reading: when unsure, never claim a sole method):
///
/// - a `#[derive(..)]` on the type naming a derive that defines the method, or
///   a derive ripr cannot read (any non-std derive, or one behind `cfg_attr`,
///   withholds the credit for every method name on that type);
/// - a blanket impl (`impl<T> Trait for T`) that defines the method;
/// - a macro: any `macro_rules!`/`macro` definition in the workspace that
///   mentions the type and writes an `impl`, or any item-level macro
///   invocation that names the type.
///
/// A file the syntax snapshot cannot read also answers yes. Not covered: a
/// statement-position macro that expands to a local impl inside a function
/// body, a macro from another crate that never names the type, a trait default
/// method reached through a blanket impl, and a `type Money = Other;` alias
/// (not followed).
fn unindexed_impl_may_define(owner: &FunctionSummary, index: &RustIndex) -> bool {
    let Some(self_type) = impl_self_type_name(&owner.id.0) else {
        return true;
    };
    index
        .call_identity_memos
        .hidden_impls(index)
        .may_define(&self_type, owner.name.as_str())
}

/// Whether the test body brings another item in under the impl type's name, by
/// a `use` or a local `type`, `struct`, `enum`, `trait` or `union`. A path
/// through that name may then name a different type (`use crate::other::Money;
/// Money::add(..)`), so no path call of it is attributed to this impl.
fn rebinds_type_name(code: &str, self_type: &str) -> bool {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let whole =
        |at: usize, len: usize| !code[..at].ends_with(word) && !code[at + len..].starts_with(word);
    let imports = code.match_indices("use").any(|(at, _)| {
        whole(at, 3)
            && code[at + 3..]
                .split(';')
                .next()
                .is_some_and(|statement| contains_as_whole_word(statement, self_type))
    });
    imports
        || ["type", "struct", "enum", "trait", "union"]
            .iter()
            .any(|keyword| {
                code.match_indices(keyword).any(|(at, _)| {
                    whole(at, keyword.len())
                        && code[at + keyword.len()..]
                            .trim_start()
                            .strip_prefix(self_type)
                            .is_some_and(|rest| !rest.starts_with(word))
                })
            })
}

/// Whether `code` (comments and strings masked) calls `name` through the
/// owner's own type: `<Type as Trait>::name(..)` with the owner's trait, or
/// `Type::name(..)` when no other method of that name exists on the type. The
/// qualifier must be exactly the impl type: another type's `add`, another
/// trait's, a longer identifier ending in the type, a longer method name, and
/// text inside `stringify!` (never evaluated) are not a call of this impl and
/// leave the test syntax-only (#10551).
fn calls_by_type_path(code: &str, name: &str, owner: &TypePathOwner) -> bool {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let self_type = owner.self_type.as_str();
    if rebinds_type_name(code, self_type) {
        return false;
    }
    // A trait the test imports from elsewhere under the owner's trait name is
    // another trait.
    let trait_rebound = rebinds_type_name(code, &owner.trait_name);
    let unevaluated = stringified_ranges(code);
    code.match_indices(name).any(|(at, _)| {
        if unevaluated.iter().any(|range| range.contains(&at)) {
            return false;
        }
        let rest = code[at + name.len()..].trim_start();
        if !(rest.starts_with('(') || rest.starts_with("::<")) {
            return false;
        }
        let Some(qualifier) = code[..at].trim_end().strip_suffix("::") else {
            return false;
        };
        let qualifier = qualifier.trim_end();
        // Comments are masked to blanks, so `crate::other:: Money` and
        // `crate::other::/* x */Money` are module paths too.
        let is_type = |text: &str| {
            text.strip_suffix(self_type)
                .is_some_and(|rest| !rest.ends_with(word) && !rest.trim_end().ends_with("::"))
        };
        (owner.sole_method && is_type(qualifier))
            || qualifier.strip_suffix('>').is_some_and(|inner| {
                // The `<` must open the qualified path itself, not a generic
                // argument list (`Wrapper<Money as X>::add` is not this call).
                // Match it by depth so the trait's own arguments
                // (`<Money as Add<Money>>`) do not end the scan early.
                let Some((head, inner)) = split_at_opening_angle(inner) else {
                    return false;
                };
                !head.ends_with(word)
                    && owner.trait_unique
                    && !trait_rebound
                    && inner.split_once(" as ").is_some_and(|(ty, trait_path)| {
                        // Bare, or a std/core operator path: a path through
                        // another module may be a different trait of the
                        // same name.
                        let trait_path = trait_path.trim();
                        let trait_path = trait_path
                            .strip_prefix("std::ops::")
                            .or_else(|| trait_path.strip_prefix("core::ops::"))
                            .unwrap_or(trait_path);
                        ty.trim() == self_type && trait_path_names_owner(trait_path, owner)
                    })
            })
    })
}

/// Split `text` (the qualifier without its final `>`) at the `<` that opens the
/// angle group the final `>` closes, balancing nested `<..>`.
fn split_at_opening_angle(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices().rev() {
        match ch {
            '>' => depth += 1,
            '<' if depth == 0 => return Some((&text[..at], &text[at + 1..])),
            '<' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Whether a qualified call's trait (`Add`, `Add<Money>`) is the owner's. Bare
/// names match as before. A call that spells generic arguments must spell the
/// same ones as the impl header; a bare header's omitted right-hand side is
/// the self type (`impl Add for Money` is `Add<Money>`).
fn trait_path_names_owner(trait_path: &str, owner: &TypePathOwner) -> bool {
    let Some((name, args)) = trait_path.split_once('<') else {
        return trait_path == owner.trait_name;
    };
    let Some(args) = args.trim_end().strip_suffix('>') else {
        return false;
    };
    let args: String = args.chars().filter(|ch| !ch.is_whitespace()).collect();
    name.trim() == owner.trait_name
        && !args.is_empty()
        && match owner.trait_args.as_deref() {
            Some(declared) => declared == args,
            // An omitted right-hand side reads as the self type. This holds
            // for every trait, not only operators; for a non-operator trait
            // with a different default it relies on `trait_unique` (checked by
            // the caller) and on the call compiling. `rebinds_type_name` is
            // not applied to the argument types.
            None => args == owner.self_type,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_syntax_is_read_per_trait() {
        assert!(uses_syntax_of("Add", "Money(2) + Money(3)"));
        assert!(!uses_syntax_of("Add", "Money(2) - Money(3)"));
        assert!(uses_syntax_of("Neg", "let _ = -Money(1);"));
        assert!(!uses_syntax_of("Neg", "let f = |m| -> u32 { 1 };"));
        assert!(!uses_syntax_of("Neg", "x -= 1;"));
        assert!(uses_syntax_of("Not", "!flag"));
        assert!(!uses_syntax_of("Not", "assert_eq!(a, b); assert!(a != b);"));
        assert!(uses_syntax_of("Index", "table[2]"));
        assert!(uses_syntax_of("Drop", "fn t() { { let _g = Guard; } }"));
        assert!(!uses_syntax_of(
            "Drop",
            "fn t() { assert_eq!(Guard.id(), 1); }"
        ));
        assert!(uses_syntax_of("BitAnd", "a & b"));
        assert!(!uses_syntax_of("BitAnd", "let r = &flags; a && b"));
        assert!(uses_syntax_of("BitOr", "a | b"));
        assert!(!uses_syntax_of("BitOr", "v.iter().map(|x| x)"));
        assert!(!uses_syntax_of("Index", "let v = vec![1, 2];"));
        assert!(uses_syntax_of("Index", "table[2]"));
        assert!(!uses_syntax_of("Mul", "*r"));
    }

    #[test]
    fn operator_positions_find_operators_not_lookalikes() {
        let binary = |code: &str, op: &str| operator_positions(code, op, true).len();
        let unary = |code: &str, op: &str| operator_positions(code, op, false).len();
        assert_eq!(binary("Money(2) + Money(3)", "+"), 1);
        assert_eq!(binary("x += 1", "+"), 0);
        assert_eq!(binary("a && b", "&"), 0);
        assert_eq!(binary("&x", "&"), 0);
        assert_eq!(binary("fn f() -> u32 { 1 }", "-"), 0);
        assert_eq!(binary("a - b", "-"), 1);
        assert_eq!(unary("let n = -Money(1);", "-"), 1);
        assert_eq!(unary("a != b", "!"), 0);
        assert_eq!(unary("assert!(x)", "!"), 0);
        assert_eq!(unary("let f = !flag;", "!"), 1);
        // A closure's pipes are not a bitwise or, and offsets survive blanking.
        let code = "let f = |x| x; a | b";
        let sites = operator_positions(code, "|", true);
        assert_eq!(sites.len(), 1);
        assert_eq!(&code[sites[0].at..sites[0].at + 1], "|");
    }

    #[test]
    fn a_receiver_is_a_plain_name_only() {
        assert_eq!(ident_before("tile.area()", 4), Some("tile"));
        assert_eq!(ident_before("self.tile.area()", 9), None);
        assert_eq!(ident_before("shapes[0].area()", 9), None);
        // A multi-byte character before the name is a boundary, not a panic.
        let code = "x→tile.area()";
        assert_eq!(
            ident_before(code, code.find('.').unwrap_or(0)),
            Some("tile")
        );
    }

    #[test]
    fn operands_are_bound_other_or_unknown() {
        let before = |code: &str| operand_before(code, code.find(" +").unwrap_or(0), "Money");
        assert!(before("Money(2) + 1") == Operand::Bound);
        // The unary operand of `-Money(1).0` is the field, not the `Money`.
        let after = |code: &str| operand_after(code, 1, "Money");
        assert!(after("-Money(1)") == Operand::Bound);
        assert!(after("-Money(1).0") == Operand::Unknown);
        assert!(after("-Money { v: 1 }.v") == Operand::Unknown);
        assert!(after("-Money(1)?") == Operand::Unknown);
        assert!(before("1 + 2") == Operand::Other);
        assert!(before("2.5 + 1.0") == Operand::Other);
        assert!(before("let t = Tile(1); t + t") == Operand::Other);
        assert!(before("let m = Money(1); m + m") == Operand::Bound);
        // A helper's return value, a constant, a field and a name with no
        // `let` are not typed, so none can rule the impl out.
        assert!(before("let m = money(1); m + m") == Operand::Unknown);
        assert!(before("Money::ZERO + 1") == Operand::Unknown);
        assert!(before("m.0 + 1") == Operand::Unknown);
        assert!(before("total + 1") == Operand::Unknown);
        // A constructor-named call says nothing about the type it returns.
        assert!(before("let v = Factory::new(); v + v") == Operand::Unknown);
        assert!(before("let v: Tile = make(); v + v") == Operand::Other);
        // A name bound twice may be the outer value at the use.
        assert!(before("let m = Money(1); { let m = Tile(1); } m + m") == Operand::Unknown);
        assert!(before("let t = Tile(1); for t in ts { t + t }") == Operand::Unknown);
        // A tighter operator before the token takes the operand first.
        assert!(before("let a = Tile(1); Money(2) * 3 + a") == Operand::Unknown);
        assert!(before("let t = Tile(1); x * t + 1") == Operand::Unknown);
        assert!(before("assert_eq!(1 + 2, 3)") == Operand::Other);
        // A postfix `[` or `?` on a bare name takes the operand first.
        assert!(after("-t[0]") == Operand::Unknown);
        assert!(after("!t?") == Operand::Unknown);
        // A literal can be the primitive `Self` of `impl Mul<Money> for f64`.
        assert!(operand_before("2.0 * x", 3, "f64") == Operand::Unknown);
        assert!(operand_after("-2.0", 1, "f64") == Operand::Unknown);
        // A match arm that destructures the name rebinds it.
        assert!(before("let t = Tile(1); match p { Foo { a: t } => t + t }") == Operand::Unknown);
        assert!(
            before("let t = Tile(1); match p { Some(t) if c => t + t, _ => 0 }")
                == Operand::Unknown
        );
    }

    #[test]
    fn iteration_is_any_method_call_or_loop() {
        assert!(iterates("for n in Countdown(3) {}"));
        assert!(iterates("Countdown(3).any(|n| n == 3)"));
        assert!(iterates("it.next_back()"));
        assert!(iterates("it.len()"));
        assert!(iterates("Vec::from_iter(c)"));
        assert!(iterates("Iterator::next(&mut c)"));
        assert!(!iterates("assert_eq!(Countdown(3).0, 3);"));
        assert!(!iterates("let v = 1.5; assert_eq!(v, 1.5);"));
    }

    #[test]
    fn assignment_operators_mutate_their_operand() {
        let code = "let mut acc = Money(0); let _ = acc += Money(1); assert_eq!(acc, Money(1));";
        assert!(binds_impl_value(code, "AddAssign", "Money"));
        let code = "let mut acc = Money(0); let _ = acc + Money(1); assert_eq!(1, 1);";
        assert!(!binds_impl_value(code, "Add", "Money"));
    }

    #[test]
    fn a_result_is_used_unless_discarded() {
        let used = |code: &str| result_is_used(code, code.find(" +").unwrap_or(0));
        assert!(used("assert_eq!(a + b, c);"));
        assert!(used("let t = a + b; assert_eq!(t, c);"));
        assert!(used("acc = acc + b; assert_eq!(acc, c);"));
        assert!(used("let (x, y) = (a + b, 1);"));
        assert!(used("let t = P { v: 1 } + P { v: 2 }; assert_eq!(t, c);"));
        assert!(used("let Money(v) = a + b; assert_eq!(v, 5);"));
        assert!(used("let P { x } = p + q; assert_eq!(x, 5);"));
        assert!(!used("let _ = a + b; assert_eq!(1, 1);"));
        assert!(!used("let _unused = a + b; assert_eq!(1, 1);"));
        assert!(!used("let _ = P { v: 1 } + P { v: 2 };"));
    }

    #[test]
    fn method_calls_need_a_dot_receiver_and_a_whole_name() {
        assert!(calls_method_named("shapes[0].area()", "area"));
        assert!(calls_method_named("s\n    .area ()", "area"));
        assert!(calls_method_named("s.area::<u32>()", "area"));
        assert!(!calls_method_named("area(&s)", "area"));
        assert!(!calls_method_named("s.areas()", "area"));
        assert!(!calls_method_named("s.total_area()", "area"));
        assert!(!calls_method_named("s.area", "area"));
    }

    #[test]
    fn type_path_calls_need_the_owner_type_and_a_whole_name() {
        let path_owner = |sole_method| TypePathOwner {
            self_type: "Money".into(),
            trait_name: "Add".into(),
            sole_method,
            trait_unique: true,
            trait_args: None,
        };
        assert!(calls_by_type_path(
            "Money::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(calls_by_type_path(
            "Money::add::<u8>(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(calls_by_type_path(
            "<Money as Add>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        // A module-qualified path may name another module's `Money`; stay
        // conservative and leave the test syntax-only.
        assert!(!calls_by_type_path(
            "crate::Money::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "Other::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "MyMoney::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "Money::address(a)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path("Money::add", "add", &path_owner(true)));
        assert!(!calls_by_type_path(
            "<Other as Add>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path("m.add(a)", "add", &path_owner(true)));
        // Another trait's method of the same name, an inherent method that
        // wins `Type::name`, and stringified tokens are not this call.
        assert!(!calls_by_type_path(
            "<Money as Audit>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "Money::add(a, b)",
            "add",
            &path_owner(false)
        ));
        assert!(calls_by_type_path(
            "<Money as std::ops::Add>::add(a, b)",
            "add",
            &path_owner(false)
        ));
        assert!(!calls_by_type_path(
            "stringify!(Money::add(1, 2))",
            "add",
            &path_owner(true)
        ));
        // Another module's trait of the same name, an import of that trait, and
        // a second impl of the same trait for the type are not this call.
        assert!(!calls_by_type_path(
            "<Money as other::Add>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "use other::Add; <Money as Add>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        let mut twice = path_owner(true);
        twice.trait_unique = false;
        assert!(!calls_by_type_path(
            "<Money as Add>::add(a, b)",
            "add",
            &twice
        ));
        assert!(calls_by_type_path(
            "<Money as core::ops::Add>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        // Spelled trait arguments must be the impl header's (#10625).
        assert!(calls_by_type_path(
            "<Money as Add<Money>>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "<Money as Add<i64>>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        assert!(!calls_by_type_path(
            "Wrapper<<Money as Add<Money>>::Output>::add(a, b)",
            "add",
            &path_owner(true)
        ));
        let mut declared = path_owner(true);
        declared.trait_args = Some("i64".into());
        assert!(calls_by_type_path(
            "<Money as core::ops::Add< i64 >>::add(a, 1)",
            "add",
            &declared
        ));
        assert!(!calls_by_type_path(
            "<Money as Add<Money>>::add(a, b)",
            "add",
            &declared
        ));
        assert!(calls_by_type_path(
            "let _ = stringify!(x); Money::add(1, 2)",
            "add",
            &path_owner(true)
        ));
    }
}
