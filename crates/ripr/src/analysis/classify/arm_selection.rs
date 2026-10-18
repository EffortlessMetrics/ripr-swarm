//! Which arm of a changed `match` a related test's owner call selects
//! (RIPR-SPEC-0229, #5432).
//!
//! A changed match arm is observed only by a test whose input reaches that
//! arm. The token rule in `reveal` confirms a qualified variant
//! (`Mode::Frozen`) wherever an assertion names it, but a glob-imported
//! variant (`LowerCase =>`), an `Option` arm (`None =>`) or an integer arm
//! (`2 =>`) has no qualified token, so the arm stays unconfirmed whichever
//! arm the test selects.
//!
//! This module reads the arm's pattern and the input a direct owner call
//! passes at the match scrutinee's position, and judges the pair:
//!
//! - `Selects`: the input's constructor or literal equals one of the arm's
//!   alternatives (or falls inside an integer range alternative), and that
//!   alternative binds its payload irrefutably; for an arm whose pattern
//!   the diff changed, the input instead moved between this arm and a
//!   later one whose results are provably unequal (literals, or variants
//!   of literals);
//! - `SelectsOther`: every alternative names a constructor, literal or
//!   integer range, and the input's differs from all of them;
//! - `Unknown`: anything else (a variable input, a wildcard or binding
//!   alternative, a guard, a const-looking name, mixed kinds).
//!
//! The scrutinee must be the owner's `self` receiver or one of its named
//! parameters, read from the enclosing `match <scrutinee> {` with no
//! intervening rebinding. Every other shape is `Unknown`, so the judgment
//! fails closed: it never names an arm that a computed value could select.

use super::super::rust_index::{FunctionSummary, TestSummary};
use super::activation::function_parameters;
use super::boundary_pairing::loan_call_literals;
use super::reveal::{
    UsePath, assertion_comparison_operands, find_fat_arrow, flattened_use_paths, lex_strings,
    matching_parenthesis, split_top_level_arguments, string_span_ranges,
};
use crate::analysis::extract::{mask_comments_and_strings, mask_with_char_literals};
use crate::analysis::syntax::HelperLoan;
use crate::domain::Probe;

/// Name an unsupported selector without extending `ArmSelector`'s proof rules.
/// Only an unambiguous direct arm/header of a statement-free tail match qualifies.
#[cfg(feature = "lang-rust")]
pub(in crate::analysis) fn unresolved_parameter_field_selector(
    probe: &Probe,
    owner: &FunctionSummary,
) -> Option<String> {
    use ra_ap_syntax::{
        AstNode,
        ast::{self, HasName},
    };
    if probe.owner.as_ref() != Some(&owner.id)
        || !probe.location.file.ends_with(&owner.file)
        || probe.family != crate::domain::ProbeFamily::MatchArm
    {
        return None;
    }
    let parse = crate::analysis::syntax::parse_clean_source_file(&owner.body)?;
    let tree = parse.tree();
    let mut items = tree.syntax().children().filter_map(ast::Item::cast);
    let Some(ast::Item::Fn(function)) = items.next() else {
        return None;
    };
    if items.next().is_some() || function.name()?.text() != owner.name {
        return None;
    }
    let body = function.body()?;
    if body.stmt_list()?.statements().next().is_some() {
        return None;
    }
    let ast::Expr::MatchExpr(matched) = body.tail_expr()? else {
        return None;
    };
    let ast::Expr::FieldExpr(field) = matched.expr()? else {
        return None;
    };
    let ast::Expr::PathExpr(base) = field.expr()? else {
        return None;
    };
    let base_text = base.syntax().text().to_string();
    let identifier = |text: &str| {
        let mut chars = text.chars();
        chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
            && chars.all(|c| c == '_' || c.is_alphanumeric())
    };
    let field_text = field.name_ref()?.text().to_string();
    if !identifier(&base_text) || !identifier(&field_text) {
        return None;
    }
    if !function.param_list()?.params().any(|param| {
        matches!(param.pat(), Some(ast::Pat::IdentPat(pat))
            if pat.name().is_some_and(|name| name.text() == base_text)
                && pat.pat().is_none())
    }) {
        return None;
    }
    let line_at = |offset: ra_ap_syntax::TextSize| {
        owner.start_line
            + owner.body[..usize::from(offset)]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
    };
    let header_line = line_at(matched.syntax().text_range().start());
    let arms = matched.match_arm_list()?.arms().collect::<Vec<_>>();
    if arms.is_empty()
        || arms.iter().any(|arm| {
            arm.guard().is_some()
                || arm.syntax().descendants().any(|node| {
                    ast::MatchExpr::can_cast(node.kind()) || ast::ClosureExpr::can_cast(node.kind())
                })
        })
    {
        return None;
    }
    let arm_lines = arms
        .iter()
        .map(|arm| line_at(arm.syntax().text_range().start()))
        .collect::<Vec<_>>();
    // Same-line arms/headers cannot identify a unique changed syntax subject.
    let hits = usize::from(header_line == probe.location.line)
        + arm_lines
            .iter()
            .filter(|line| **line == probe.location.line)
            .count();
    if hits != 1 {
        return None;
    }
    Some(format!("{base_text}.{field_text}"))
}

#[cfg(not(feature = "lang-rust"))]
pub(in crate::analysis) fn unresolved_parameter_field_selector(
    _probe: &Probe,
    _owner: &FunctionSummary,
) -> Option<String> {
    None
}

mod field_selector;
use field_selector::FieldSelector;

mod guard_entry;
pub(in crate::analysis) use guard_entry::{GuardEntry, named_integer_bound};

/// RIPR-SPEC-0229 decision 6: an integer scrutinee computed from its
/// parameter by one division or remainder by a positive integer literal
/// (`match status / 100`). An integer input is evaluated as Rust does, so
/// `301` reads as `3`; any other input is not provable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrutineeArithmetic {
    Div(i128),
    Rem(i128),
}

impl ScrutineeArithmetic {
    fn apply(self, value: i128) -> Option<i128> {
        match self {
            Self::Div(divisor) => value.checked_div(divisor),
            Self::Rem(divisor) => value.checked_rem(divisor),
        }
    }
}

/// Where the owner's `match` reads its scrutinee from.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScrutineeBinding {
    /// `match self { .. }` in a method: the call's receiver.
    Receiver,
    /// `match <param> { .. }`: the call's argument at this index (the
    /// `self` receiver, if any, is not counted).
    Parameter(usize),
}

/// The changed arm, the scrutinee binding, and the owner call shape that
/// supplies the scrutinee. Built once per probe.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct ArmSelector {
    alternatives: Vec<PatternHead>,
    /// The alternatives of each arm before the changed one, in source
    /// order. `None` for an arm that cannot be read (a guard): such an arm
    /// may take any input first.
    earlier: Vec<Option<Vec<PatternHead>>>,
    pattern_text: String,
    scrutinee: String,
    /// The scrutinee's type name (`Mode` for `mode: Mode`, the impl's self
    /// type for `self`), when the signature or impl names one. A qualified
    /// input must be qualified by it.
    scrutinee_type: Option<String>,
    /// The scrutinee type when it is written as a bare name (`k: Kind`, not
    /// `k: other::Kind`), so an enum the owner's file declares under that
    /// name is the scrutinee's type. `in_workspace` reads that enum's
    /// variants for uncased names (#7543).
    bare_scrutinee_type: Option<String>,
    /// Whether the owner's impl type is `bare_scrutinee_type`, so `Self::A`
    /// names the scrutinee's type. In `impl Router`, a match on `m: Method`
    /// may compare `Self::GET`, a `Router` constant, by value.
    self_is_scrutinee_type: bool,
    binding: ScrutineeBinding,
    /// A direct record literal supplies this field; no alias resolution.
    field_selector: Option<FieldSelector>,
    /// The arithmetic a computed integer scrutinee applies to the bound
    /// parameter (decision 6), or `None` for the parameter itself.
    arithmetic: Option<ScrutineeArithmetic>,
    owner: String,
    method: bool,
    /// Whether every call of the owner runs the enclosing `match`: it is
    /// the first thing the body does, after `let` statements with no
    /// control flow. A match nested in another arm, an `if`, a loop, or
    /// after a possible early `return`/`?` may be skipped, so an input
    /// that fits the arm does not show the arm ran.
    reached: bool,
    /// The arm's original alternatives when the diff changed its pattern
    /// (`Some(None)` when the original pattern cannot be read). An input
    /// inside both patterns runs the same arm either way, so a changed
    /// pattern earns credit only from an input that moved to an unequal
    /// result (`moved_to_unequal_result`), and the arm is named unselected
    /// only when neither pattern selects any observed input.
    changed_from: Option<Option<Vec<PatternHead>>>,
    /// The changed arm's result as it stands, when it is a literal or a
    /// constructor of literals (RIPR-SPEC-0229 "provably unequal").
    result: Option<ResultValue>,
    /// The arms after the changed one, in source order: alternatives
    /// (`None` for a guarded arm) and result. `None` when the arms after the
    /// changed one cannot be delimited.
    later: Option<Vec<LaterArm>>,
    /// Whether the diff's old text for this line is the same arm with only
    /// its pattern different: a head cut anchored on the unchanged body, or
    /// a whole old line whose body equals the current one. Moved-input
    /// credit reads the original `match` as the current one with this arm's
    /// pattern restored, so it needs the body to be the one that stood.
    original_same_body: bool,
    /// Whether the enclosing `match` is the owner body's tail expression,
    /// so the arm's result is the owner's return value. Moved-input credit
    /// compares arm results as return values; a `let t = match ..;` read
    /// later may map unequal results to one value.
    match_is_tail: bool,
    /// First path segments that name this workspace: `crate`, `self`,
    /// `super`, `Self`, the owner's impl type, and (once
    /// `with_workspace_packages` runs) each workspace package. A free
    /// owner called through any other path (`other_crate::reason(..)`) may
    /// be a same-named function outside the index, so it is not read as an
    /// owner call.
    local_roots: Vec<String>,
    owner_file: std::path::PathBuf,
    /// The flattened `use` paths of each related test's file, once
    /// `in_workspace` attaches them. A bare variant input (`LowerCase`)
    /// names the scrutinee's variant only when no import in the test's
    /// file binds that name from another enum.
    test_imports: Option<std::collections::BTreeMap<std::path::PathBuf, Vec<UsePath>>>,
    /// The variants of the scrutinee's enum, once `in_workspace` finds its
    /// declaration in the owner's file (`declared_enum_variants`) or in the
    /// one workspace file the owner's import of it names
    /// (`imported_enum_variants`, #7675). `None` leaves every uncased name
    /// unreadable.
    declared_variants: Option<std::collections::BTreeSet<String>>,
    /// Whether a test's uncased input (`Kind::A`) names the owner's
    /// `Kind`. `in_workspace` clears it when a related test file declares
    /// its own item of that name or glob-imports a module, since that
    /// `Kind::A` may be a test-side constant (#7620 review).
    uncased_inputs_trusted: bool,
    /// RIPR-SPEC-0229 (#6616): the value operand of each related test's admitted
    /// exact pattern pin, keyed by test file, test start line, test name,
    /// assertion line and assertion text.
    pattern_pins: std::collections::BTreeMap<PatternPinKey, String>,
    /// RIPR-SPEC-0229 decisions 5 and 7: the assertions whose owner call
    /// takes an input lent from outside it that selects this arm. Either
    /// the assertion is borrowed from a check helper (#6482) and an eager
    /// call to the helper fills the scrutinee parameter with a selecting
    /// literal, or the call is fed by a constant-row table (#5328) one of
    /// whose cells selects. Same key as `pattern_pins`.
    lent_selections: std::collections::BTreeSet<PatternPinKey>,
}

type PatternPinKey = (std::path::PathBuf, usize, String, usize, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum ArmSelection {
    Selects,
    SelectsOther,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LiteralValue {
    Int(i128),
    Str(String),
    Char(String),
    Bool(bool),
}

/// An arm result that is provably one value: a literal, or a variant path
/// or constructor whose payload is all literals (`None`, `Some(1)`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum ResultValue {
    Literal(LiteralValue),
    Variant {
        /// The path before the variant (`Kind` in `Kind::Alpha`), empty
        /// when bare.
        qualifier: String,
        name: String,
        payload: Vec<LiteralValue>,
    },
}

/// One arm after the changed one: its alternatives (`None` when guarded)
/// and its result, when that is provably one value.
#[derive(Clone, Debug)]
struct LaterArm {
    alternatives: Option<Vec<PatternHead>>,
    result: Option<ResultValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PatternHead {
    Literal(LiteralValue),
    /// An integer range with literal bounds, both inclusive (`a..b` is
    /// stored as `a..=b-1`); an open side is `None`.
    IntRange {
        start: Option<i128>,
        end: Option<i128>,
    },
    /// A tuple/struct/unit variant. `irrefutable` is true when the payload
    /// (if any) is made only of `_`, `..` and plain bindings, so the
    /// variant name alone decides selection.
    Variant {
        name: String,
        irrefutable: bool,
    },
    /// `_`: matches every input.
    CatchAll,
    /// A bare lowercase name (`other`, `ref v`): a binding that matches
    /// every input, unless the name resolves to a constant, which
    /// `in_workspace` checks against the owner's file.
    Binding(String),
    /// A qualified name with no lowercase letter (`Kind::A`, `Method::GET`),
    /// optionally with a payload. Rust allows such a variant, but the same
    /// path can name an associated constant, which a pattern compares by
    /// value. It reads as a `Variant` only once the owner's file declares
    /// it as a variant of the scrutinee's enum, and as `Opaque` otherwise
    /// (#7543).
    UncasedVariant {
        qualifier: String,
        name: String,
        irrefutable: bool,
    },
    Opaque,
}

/// One related test's owner-call inputs at the scrutinee position, judged
/// against the changed arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct ObservedArmInputs {
    /// The input text of every direct owner call, in source order.
    pub(in crate::analysis) inputs: Vec<String>,
    pub(in crate::analysis) selection: ArmSelection,
}

impl ArmSelector {
    /// The selector for a `MatchArm` probe whose owner reads the arm's
    /// scrutinee from `self` or a named parameter. `None` when the arm has
    /// a guard, the enclosing `match` is not found, or its scrutinee is
    /// computed or rebound.
    pub(in crate::analysis) fn establish(probe: &Probe, owner: &FunctionSummary) -> Option<Self> {
        if owner.name.is_empty() {
            return None;
        }
        let pattern_text = arm_pattern_text(&probe.expression)?;
        let alternatives = top_level_alternatives(&pattern_text)?
            .into_iter()
            .map(pattern_head)
            .collect::<Vec<_>>();
        if alternatives.is_empty() {
            return None;
        }
        // A removed-line probe names the arm as it was; the arm must still
        // stand on that line with the same pattern (only its body changed).
        if probe.after.is_none() {
            let current = owner
                .body
                .lines()
                .nth(probe.location.line.checked_sub(owner.start_line)?)?;
            if !arm_pattern_text(current)
                .is_some_and(|current| same_pattern(&current, &pattern_text))
            {
                return None;
            }
        }
        // The diff cuts `before` to the old head (`1..=8 =>`) only when the
        // unchanged text around the head, the arm body included, anchors
        // both ends (#7020), so a head cut is this arm with only its
        // pattern changed.
        // The cut needs unchanged text at the head's start, so an old head
        // sharing no leading text with the current pattern is not one; an
        // uncut old head with its body on the next removed line arrives
        // joined to that body and reads as a whole line.
        let before_head_cut = probe.before.as_deref().is_some_and(|before| {
            let before = before.trim();
            before.ends_with("=>")
                && before.matches("=>").count() == 1
                && before
                    .chars()
                    .zip(pattern_text.chars())
                    .next()
                    .is_some_and(|(old, new)| old == new)
        });
        let changed_from = match (&probe.before, &probe.after) {
            (Some(before), Some(_)) => match arm_pattern_text(before) {
                Some(original) if same_pattern(&original, &pattern_text) => None,
                // Reordered alternatives (`"CA" | "MX"` to `"MX" | "CA"`)
                // take exactly the same inputs, so the arm reads as one whose
                // pattern did not change. Alternatives compare as text, not
                // heads: `Some(1)` and `Some(2)` share a head (#5638 review).
                Some(original) if same_alternatives(&original, &pattern_text) => None,
                // The diff pairs an added line with the first adjacent
                // removed line that shares any token, and a qualified enum
                // name is shared by every arm in a multi-line hunk. The
                // pairing is trusted only when the two patterns share an
                // alternative, or the old text is a head cut anchored on
                // this line's unchanged body; otherwise the original is
                // unreadable.
                Some(original) => Some(
                    top_level_alternatives(&original)
                        .map(|original| original.into_iter().map(pattern_head).collect::<Vec<_>>())
                        .filter(|original| {
                            before_head_cut
                                || original.iter().any(|head| {
                                    *head != PatternHead::Opaque && alternatives.contains(head)
                                })
                        }),
                ),
                None => Some(None),
            },
            _ => None,
        };
        let method = owner.item.has_self_param || owner_has_self_parameter(owner);
        let mut local_roots = ["crate", "self", "super", "Self"]
            .map(str::to_string)
            .to_vec();
        let EnclosingMatch {
            scrutinee,
            earlier_patterns,
            reached,
            arms_from_changed,
            match_is_tail,
        } = enclosing_match(
            owner,
            probe.location.line,
            matches!(changed_from, Some(Some(_))),
        )?;
        let (scrutinee, arithmetic) = match integer_scrutinee(&scrutinee) {
            Some((name, arithmetic)) => (name.to_string(), Some(arithmetic)),
            None => (scrutinee, None),
        };
        // The first arm from the changed line on is the changed arm itself;
        // the rest are the arms an input that misses it falls through to.
        let (result, later) = match arms_from_changed.as_deref() {
            Some([(changed_pattern, changed_body), rest @ ..])
                if same_pattern(changed_pattern, &pattern_text) =>
            {
                (
                    result_value(changed_body),
                    Some(
                        rest.iter()
                            .map(|(pattern, body)| LaterArm {
                                alternatives: earlier_arm(pattern),
                                result: result_value(body),
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
            }
            _ => (None, None),
        };
        // Only a head cut says the old text is this arm with its body
        // unchanged and nothing else in the hunk: the diff keeps the whole
        // old line when another arm of the owner changed too, so a whole
        // old line, even with an equal body, never reads as one.
        let original_same_body = before_head_cut;
        let earlier = earlier_patterns
            .iter()
            .map(|pattern| earlier_arm(pattern))
            .collect::<Vec<_>>();
        let field = scrutinee
            .split_once('.')
            .map(|(parameter, field)| (parameter.to_string(), field.to_string()));
        if field.is_some()
            && unresolved_parameter_field_selector(probe, owner).as_deref() != Some(&scrutinee)
        {
            return None;
        }
        let mut field_selector = None;
        let (binding, scrutinee_type, bare_scrutinee_type) =
            if method && matches!(scrutinee.as_str(), "self" | "*self") {
                if receiver_may_change(owner) {
                    return None;
                }
                let self_type = match &owner.impl_context {
                    crate::analysis::facts::FunctionImplContext::Impl { self_type } => {
                        Some(self_type.clone())
                    }
                    _ => None,
                };
                let bare = self_type
                    .as_deref()
                    .map(|self_type| self_type.split('<').next().unwrap_or(self_type).trim())
                    .filter(|name| is_identifier(name))
                    .map(str::to_string);
                (ScrutineeBinding::Receiver, self_type, bare)
            } else {
                let parameter = field
                    .as_ref()
                    .map_or(scrutinee.as_str(), |(parameter, _)| parameter.as_str());
                let parameters = function_parameters(owner);
                let index = parameters
                    .iter()
                    .position(|candidate| parameter_name(candidate) == parameter)?;
                let declared = parameter_declaration(owner, parameter)?;
                if parameter_may_change(owner, parameter, declared.mutable) {
                    return None;
                }
                // Arithmetic is evaluated only on a primitive integer
                // parameter, whose `/` and `%` are the built-in ones.
                if arithmetic.is_some()
                    && (declared.qualified
                        || !declared
                            .type_name
                            .as_deref()
                            .is_some_and(|name| INTEGER_TYPES.contains(&name)))
                {
                    return None;
                }
                if let Some((_, field)) = &field {
                    if declared.mutable || declared.qualified || arithmetic.is_some() {
                        return None;
                    }
                    let mut selector =
                        FieldSelector::new(field, declared.type_name.as_deref()?, &owner.body)?;
                    if let Some(original) = probe.before.as_deref().and_then(arm_pattern_text) {
                        selector.require_original_pattern(&original);
                    }
                    field_selector = Some(selector);
                    (
                        ScrutineeBinding::Parameter(index),
                        Some("Option".to_string()),
                        None,
                    )
                } else {
                    let bare = declared.type_name.clone().filter(|_| !declared.qualified);
                    (ScrutineeBinding::Parameter(index), declared.type_name, bare)
                }
            };
        let self_is_scrutinee_type = match &owner.impl_context {
            crate::analysis::facts::FunctionImplContext::Impl { self_type } => {
                let impl_type = self_type.split('<').next().unwrap_or(self_type).trim();
                is_identifier(impl_type) && bare_scrutinee_type.as_deref() == Some(impl_type)
            }
            _ => false,
        };
        if let crate::analysis::facts::FunctionImplContext::Impl { self_type } = &owner.impl_context
            && let Some(type_name) = self_type
                .split('<')
                .next()
                .and_then(|path| path.rsplit("::").next())
                .map(str::trim)
                .filter(|name| !name.is_empty())
        {
            local_roots.push(type_name.to_string());
        }
        Some(Self {
            alternatives,
            earlier,
            pattern_text,
            scrutinee,
            scrutinee_type,
            bare_scrutinee_type,
            self_is_scrutinee_type,
            declared_variants: None,
            uncased_inputs_trusted: true,
            binding,
            field_selector,
            arithmetic,
            owner: owner.name.clone(),
            method,
            reached,
            changed_from,
            result,
            later,
            original_same_body,
            match_is_tail,
            local_roots,
            owner_file: owner.file.clone(),
            test_imports: None,
            pattern_pins: std::collections::BTreeMap::new(),
            lent_selections: std::collections::BTreeSet::new(),
        })
    }

    /// Reads the workspace around the owner and its related tests:
    /// - each workspace package, in manifest and crate-identifier form,
    ///   becomes a path root of a free owner call;
    /// - a binding pattern whose name the owner's file may resolve to a
    ///   constant (a `const`/`static` of that name, an import binding it,
    ///   or a glob import of a module) becomes unreadable, since Rust
    ///   compares such a pattern by value;
    /// - each related test file's imports are kept for bare variant inputs.
    pub(in crate::analysis) fn in_workspace<'a>(
        mut self,
        index: &crate::analysis::rust_index::RustIndex,
        test_files: impl IntoIterator<Item = &'a std::path::Path>,
    ) -> Self {
        self = self.with_workspace_packages(&index.package_names);
        let owner_source = index
            .files()
            .get(&self.owner_file)
            .map(|facts| facts.source.clone());
        if let Some(field) = &mut self.field_selector {
            let source = owner_source.as_deref().filter(|_| {
                index
                    .files()
                    .get(&self.owner_file)
                    .is_some_and(|facts| !facts.used_lexical_fallback)
            });
            // Rebinding into absent or fallback facts must clear prior admission.
            field.admit_source(source.unwrap_or_default());
        }
        self.demote_constant_bindings(owner_source.as_deref());
        let declared = self.bare_scrutinee_type.as_deref().and_then(|type_name| {
            owner_source
                .as_deref()
                .and_then(|source| declared_enum_variants(source, type_name, &self.owner))
                .or_else(|| imported_enum_variants(index, &self.owner_file, type_name, &self.owner))
        });
        self.resolve_uncased_variants(declared);
        let mut imports = std::collections::BTreeMap::new();
        for file in test_files {
            if file != self.owner_file
                && let Some(field) = &mut self.field_selector
            {
                // Imported record/type identity is outside this bounded slice.
                field.admit_source("");
            }
            if let Some(facts) = index.files().get(file) {
                if file != self.owner_file
                    && let Some(type_name) = &self.bare_scrutinee_type
                    && test_file_may_rebind(&facts.source, type_name, &self.local_roots)
                {
                    self.uncased_inputs_trusted = false;
                }
                imports
                    .entry(file.to_path_buf())
                    .or_insert_with(|| flattened_use_paths(&facts.source));
            }
        }
        self.test_imports = Some(imports);
        self
    }

    fn with_workspace_packages(
        mut self,
        package_names: &std::collections::BTreeSet<String>,
    ) -> Self {
        for name in package_names {
            self.local_roots.push(name.clone());
            self.local_roots.push(name.replace('-', "_"));
        }
        self
    }

    fn demote_constant_bindings(&mut self, owner_source: Option<&str>) {
        // An item macro (`def_const!(p);`) may declare a constant the
        // pattern name then resolves to (#7863), so a file with one reads
        // every bare binding as unreadable.
        let macro_may_declare = owner_source
            .is_some_and(|source| may_declare_items_in_owner_scope(source, &self.owner));
        let resolves = |name: &str| {
            macro_may_declare || owner_source.is_none_or(|source| may_name_constant(source, name))
        };
        let demote = |heads: &mut Vec<PatternHead>| {
            for head in heads.iter_mut() {
                if let PatternHead::Binding(name) = head
                    && resolves(name)
                {
                    *head = PatternHead::Opaque;
                }
            }
        };
        demote(&mut self.alternatives);
        self.earlier.iter_mut().flatten().for_each(demote);
        if let Some(Some(original)) = &mut self.changed_from {
            demote(original);
        }
        for arm in self.later.iter_mut().flatten() {
            if let Some(alternatives) = &mut arm.alternatives {
                demote(alternatives);
            }
        }
    }

    /// Reads each uncased name (`Kind::A`) as a variant or as unreadable,
    /// from the enum the owner's file declares (#7543).
    fn resolve_uncased_variants(&mut self, declared: Option<std::collections::BTreeSet<String>>) {
        self.declared_variants = declared;
        let resolve = |heads: &mut Vec<PatternHead>, selector: &Self| {
            for head in heads.iter_mut() {
                *head = selector.resolve_head(std::mem::replace(head, PatternHead::Opaque));
            }
        };
        let mut alternatives = std::mem::take(&mut self.alternatives);
        resolve(&mut alternatives, self);
        self.alternatives = alternatives;
        let mut earlier = std::mem::take(&mut self.earlier);
        earlier
            .iter_mut()
            .flatten()
            .for_each(|arm| resolve(arm, self));
        self.earlier = earlier;
        let mut changed_from = self.changed_from.take();
        if let Some(Some(original)) = &mut changed_from {
            resolve(original, self);
        }
        self.changed_from = changed_from;
        // Later arms decide where a moved input lands (#7670).
        let mut later = self.later.take();
        later
            .iter_mut()
            .flatten()
            .filter_map(|arm| arm.alternatives.as_mut())
            .for_each(|arm| resolve(arm, self));
        self.later = later;
    }

    /// An uncased pattern name becomes a `Variant` when its qualifier names
    /// the scrutinee's type (by name, or `Self` in that type's own impl) and
    /// the owner's file declares that enum with the name, and `Opaque`
    /// otherwise. Every other head passes through unchanged.
    fn resolve_head(&self, head: PatternHead) -> PatternHead {
        let PatternHead::UncasedVariant {
            qualifier,
            name,
            irrefutable,
        } = head
        else {
            return head;
        };
        let names_scrutinee_type = if qualifier == "Self" {
            self.self_is_scrutinee_type
        } else {
            self.bare_scrutinee_type.as_deref() == Some(qualifier.as_str())
        };
        if names_scrutinee_type
            && self
                .declared_variants
                .as_ref()
                .is_some_and(|variants| variants.contains(&name))
        {
            PatternHead::Variant { name, irrefutable }
        } else {
            PatternHead::Opaque
        }
    }

    /// Whether an input's leading name may resolve outside the scrutinee's
    /// type in this test's file. For a bare variant (`LowerCase`) or a
    /// type-qualified one (`RenameRule::LowerCase`), an import that binds
    /// that name is ambiguous when it renames an item to it, roots outside
    /// the workspace (`use other_crate::RenameRule;`), or, for a bare
    /// variant, comes through another type; so is a glob of another type's
    /// variants, or of the scrutinee type's name from outside the
    /// workspace. A file whose imports were not read is ambiguous.
    fn input_import_ambiguous(&self, test: &TestSummary, input: &str) -> bool {
        let Some(test_imports) = &self.test_imports else {
            return false;
        };
        let (PatternHead::Variant { name, .. } | PatternHead::UncasedVariant { name, .. }) =
            input_head(input)
        else {
            return false;
        };
        let lead = match input_qualifier(input) {
            Some(_) if input_path_root(input).is_some() => return false,
            Some(qualifier) => qualifier.to_string(),
            None => name,
        };
        let bare = input_qualifier(input).is_none();
        let Some(paths) = test_imports.get(&test.file) else {
            return true;
        };
        let scrutinee_type = self.scrutinee_type.as_deref();
        let local_root = |path: &str| {
            let root = path.split("::").next().unwrap_or("");
            Some(root) == scrutinee_type || self.local_roots.iter().any(|local| local == root)
        };
        paths.iter().any(|import| {
            let mut segments = import.path.rsplit("::");
            let last = segments.next().unwrap_or("");
            let parent = segments.next();
            match &import.alias {
                Some(alias) => *alias == lead,
                None if last == "*" => {
                    bare && parent.is_some_and(is_variant_name)
                        && (parent != scrutinee_type || !local_root(&import.path))
                }
                None if last != lead => false,
                None if !local_root(&import.path) => true,
                None => bare && (scrutinee_type.is_none() || parent != scrutinee_type),
            }
        })
    }

    /// Whether the test binds an owner call's result (`let expected =
    /// reason(None);`): that value may stand on the expected side of an
    /// assertion and move with the arm, so no assertion in the test
    /// confirms a selection.
    pub(in crate::analysis) fn binds_owner_result(&self, test: &TestSummary) -> bool {
        let masked = mask_comments_and_strings(&test.body);
        masked.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("let ")
                && line
                    .find('=')
                    .is_some_and(|eq| !whole_word_offsets(&line[eq..], &self.owner).is_empty())
        })
    }

    /// The scrutinee as the owner's `match` names it (`x`, `self`).
    pub(in crate::analysis) fn scrutinee(&self) -> &str {
        &self.scrutinee
    }

    /// The direct-record extension must use shared execution admission before
    /// pairing an assertion with a selected field input. Unsupported legacy
    /// field inputs retain their existing limitation evidence.
    pub(in crate::analysis) fn field_oracle_needs_admission(&self) -> bool {
        self.field_selector
            .as_ref()
            .is_some_and(FieldSelector::source_is_admitted)
    }

    /// The arm pattern as written, without the `=>`.
    pub(in crate::analysis) fn pattern_text(&self) -> &str {
        &self.pattern_text
    }

    /// Whether either compared operand of an `assert_eq!` is exactly a
    /// direct owner call whose scrutinee input selects this arm.
    /// Diagnostic arguments and every other assertion shape never select.
    /// An `assert_ne!` never does: `assert_ne!(reason(None), 2)` passes
    /// whether the arm yields 0 or 1, so selecting the arm shows nothing.
    pub(in crate::analysis) fn assertion_selects(&self, assertion_text: &str) -> bool {
        // Any `*assert_ne` (`debug_assert_ne!`, `prop_assert_ne!`) too.
        if mask_comments_and_strings(assertion_text).contains("assert_ne") {
            return false;
        }
        let Some(operands) = assertion_comparison_operands(assertion_text) else {
            return false;
        };
        // An expected side that names the owner (`assert_eq!(reason(None),
        // (reason(None)))`) moves with the arm, so the comparison may hold
        // whatever the arm yields.
        if operands.iter().all(|operand| {
            !whole_word_offsets(&mask_comments_and_strings(operand), &self.owner).is_empty()
        }) {
            return false;
        }
        operands.iter().any(|operand| self.operand_selects(operand))
    }

    /// Field bindings are interpreted at the compared call's position, never
    /// at a later write or an unrelated call elsewhere in the test.
    pub(in crate::analysis) fn assertion_selects_at(
        &self,
        test: &TestSummary,
        assertion: &crate::analysis::facts::OracleFact,
    ) -> bool {
        if self.field_selector.is_none() {
            return self.assertion_selects(&assertion.text);
        }
        if mask_comments_and_strings(&assertion.text).contains("assert_ne") {
            return false;
        }
        let Some(operands) = assertion_comparison_operands(&assertion.text) else {
            return false;
        };
        if operands.iter().all(|operand| {
            !whole_word_offsets(&mask_comments_and_strings(operand), &self.owner).is_empty()
        }) {
            return false;
        }
        operands.iter().any(|operand| {
            let operand = operand.trim();
            owner_calls_in(operand, &self.owner, self.method, &self.local_roots)
                .into_iter()
                .any(|call| {
                    call.span == (0, operand.len())
                        && self
                            .call_input_at(&call, test, assertion.line, operand)
                            .is_some_and(|input| self.judge(&input) == ArmSelection::Selects)
                })
        })
    }

    /// Whether `operand` is exactly a direct owner call whose scrutinee
    /// input selects this arm.
    fn operand_selects(&self, operand: &str) -> bool {
        let operand = operand.trim();
        owner_calls_in(operand, &self.owner, self.method, &self.local_roots)
            .into_iter()
            .any(|call| {
                call.span == (0, operand.len())
                    && self
                        .call_input(&call)
                        .is_some_and(|input| self.judge(input) == ArmSelection::Selects)
            })
    }

    /// RIPR-SPEC-0229 (#6616): keeps each related test's admitted exact pattern
    /// pins (`assert_matches!(owner("q"), Err(E::Unknown))` through a
    /// verified local macro), as `admitted` reads them.
    pub(in crate::analysis) fn with_pattern_pins<'a>(
        mut self,
        tests: impl IntoIterator<Item = &'a TestSummary>,
        admitted: impl Fn(&TestSummary, &crate::analysis::facts::OracleFact) -> Option<String>,
    ) -> Self {
        for test in tests {
            for assertion in &test.assertions {
                if let Some(value) = admitted(test, assertion) {
                    self.pattern_pins.insert(
                        (
                            test.file.clone(),
                            test.start_line,
                            test.name.clone(),
                            assertion.line,
                            assertion.text.clone(),
                        ),
                        value,
                    );
                }
            }
        }
        self
    }

    /// Whether `assertion` is an admitted exact pattern pin whose value
    /// operand is exactly a direct owner call selecting this arm. A pattern
    /// names no call, so its expected side cannot move with the arm.
    pub(in crate::analysis) fn pattern_pin_selects(
        &self,
        test: &TestSummary,
        assertion: &crate::analysis::facts::OracleFact,
    ) -> bool {
        // Most selectors carry no pins; skip building the owned key.
        if self.pattern_pins.is_empty() {
            return false;
        }
        self.pattern_pins
            .get(&(
                test.file.clone(),
                test.start_line,
                test.name.clone(),
                assertion.line,
                assertion.text.clone(),
            ))
            .is_some_and(|value| self.operand_selects(value))
    }

    /// RIPR-SPEC-0229 decision 5: keeps each related test's admitted
    /// assertion borrowed from a check helper (#6482) that selects this arm
    /// through the helper's parameters, as `loan` lends them.
    pub(in crate::analysis) fn with_helper_loans<'a>(
        mut self,
        tests: impl IntoIterator<Item = &'a TestSummary>,
        loan: impl Fn(&TestSummary, &crate::analysis::facts::OracleFact) -> Option<HelperLoan>,
    ) -> Self {
        for test in tests {
            for assertion in &test.assertions {
                if let Some(loan) = loan(test, assertion)
                    && self.loan_selects(test, &assertion.text, &loan)
                {
                    self.lent_selections.insert((
                        test.file.clone(),
                        test.start_line,
                        test.name.clone(),
                        assertion.line,
                        assertion.text.clone(),
                    ));
                }
            }
        }
        self
    }

    /// RIPR-SPEC-0229 decision 7: keeps each related test's `assert_eq!`
    /// whose compared operand is exactly a direct owner call fed by a
    /// column of a constant-row `for` table (#5328), when some row's cell
    /// selects this arm. The loop runs the assertion once per row, so that
    /// row's call is a selecting call with an exact oracle on its result.
    pub(in crate::analysis) fn with_table_rows<'a>(
        mut self,
        tests: impl IntoIterator<Item = &'a TestSummary>,
    ) -> Self {
        for test in tests {
            for assertion in &test.assertions {
                if self.table_row_selects(test, &assertion.text, assertion.line) {
                    self.lent_selections.insert((
                        test.file.clone(),
                        test.start_line,
                        test.name.clone(),
                        assertion.line,
                        assertion.text.clone(),
                    ));
                }
            }
        }
        self
    }

    fn table_row_selects(&self, test: &TestSummary, assertion_text: &str, line: usize) -> bool {
        let Some(operands) = self.exact_operands(assertion_text) else {
            return false;
        };
        operands.iter().any(|operand| {
            let operand = operand.trim();
            owner_calls_in(operand, &self.owner, self.method, &self.local_roots)
                .into_iter()
                .filter(|call| call.span == (0, operand.len()))
                .filter_map(|call| self.call_input(&call))
                .map(str::trim)
                .filter(|input| is_identifier(input) && runs_in_table_loop(test, line, input))
                .filter_map(|input| {
                    crate::analysis::syntax::constant_table_column(&test.body, input)
                })
                .any(|cells| {
                    cells.iter().any(|cell| {
                        !self.input_import_ambiguous(test, cell)
                            && self.judge(cell) == ArmSelection::Selects
                    })
                })
        })
    }

    /// The compared operands of an `assert_eq!` that may confirm a
    /// selection: never an `assert_ne!` form, and never one whose every
    /// operand names the owner, since that expected side moves with the arm.
    fn exact_operands<'t>(&self, assertion_text: &'t str) -> Option<[&'t str; 2]> {
        if mask_comments_and_strings(assertion_text).contains("assert_ne") {
            return None;
        }
        let operands = assertion_comparison_operands(assertion_text)?;
        if operands.iter().all(|operand| {
            !whole_word_offsets(&mask_comments_and_strings(operand), &self.owner).is_empty()
        }) {
            return None;
        }
        Some(operands)
    }

    /// Whether `assertion` selects this arm through inputs lent from
    /// outside the owner call: a check helper's literals
    /// (`with_helper_loans`) or a constant-row table's cells
    /// (`with_table_rows`).
    pub(in crate::analysis) fn lent_selects(
        &self,
        test: &TestSummary,
        assertion: &crate::analysis::facts::OracleFact,
    ) -> bool {
        !self.lent_selections.is_empty()
            && self.lent_selections.contains(&(
                test.file.clone(),
                test.start_line,
                test.name.clone(),
                assertion.line,
                assertion.text.clone(),
            ))
    }

    /// `fn check(suit: char, want: &str) { assert_eq!(suit_color(suit),
    /// want); }` called as `check('S', "black")` compares
    /// `suit_color('S')`: the helper's compared operand is exactly a direct
    /// owner call whose scrutinee argument is one of the helper's plain,
    /// never-rebound parameters, the expected side does not name the owner,
    /// and some eager call to the helper, on a line holding only calls of
    /// that same text, passes only
    /// scalar literals and fills that parameter with an input selecting
    /// this arm. That call runs the helper's assertion on its own owner
    /// call, so it is a selecting call with an exact oracle on its result.
    fn loan_selects(&self, test: &TestSummary, assertion_text: &str, loan: &HelperLoan) -> bool {
        if loan.parameters.is_empty() {
            return false;
        }
        let Some(operands) = self.exact_operands(assertion_text) else {
            return false;
        };
        let Some(position) = operands.iter().find_map(|operand| {
            let operand = operand.trim();
            owner_calls_in(operand, &self.owner, self.method, &self.local_roots)
                .into_iter()
                .find(|call| call.span == (0, operand.len()))
                .and_then(|call| self.call_input(&call))
                .and_then(|input| loan.parameters.iter().position(|name| name == input.trim()))
        }) else {
            return false;
        };
        loan.call_lines.iter().any(|line| {
            if crate::analysis::extract::line_at_call_budget(test.calls.iter(), *line) {
                return false;
            }
            let mut on_line = test.calls.iter().filter(|call| call.line == *line);
            let Some(helper_call) = on_line.next() else {
                return false;
            };
            if helper_call.name != loan.name
                || on_line.any(|call| call.name != loan.name || call.text != helper_call.text)
            {
                return false;
            }
            loan_call_literals(&helper_call.text, &loan.name).is_some_and(|literals| {
                literals.len() == loan.parameters.len()
                    && literals.get(position).is_some_and(|input| {
                        !self.input_import_ambiguous(test, input)
                            && self.judge(input) == ArmSelection::Selects
                    })
            })
        })
    }

    /// Whether the test names the owner at all. A test that reaches the
    /// owner only through other functions passes no input this selector
    /// reads, so selection has nothing to outrank there (#6297). An alias
    /// the test's file imports the owner under (`use m::flip as f;`) is a
    /// mention, and so is anything in a file whose imports were not read.
    pub(in crate::analysis) fn mentioned_by(&self, test: &TestSummary) -> bool {
        if !whole_word_offsets(&mask_comments_and_strings(&test.body), &self.owner).is_empty() {
            return true;
        }
        let Some(test_imports) = &self.test_imports else {
            return false;
        };
        test_imports.get(&test.file).is_none_or(|paths| {
            paths.iter().any(|import| {
                import.alias.is_some() && import.path.rsplit("::").next() == Some(&self.owner)
            })
        })
    }

    /// Every direct owner call in a test body, judged against this arm.
    /// `None` when the body names the owner anywhere other than a call
    /// this module can read (a function pointer, a multi-line call, a
    /// differently shaped call), since such a use may select any arm.
    pub(in crate::analysis) fn observed_inputs(
        &self,
        test: &TestSummary,
    ) -> Option<ObservedArmInputs> {
        let masked = mask_comments_and_strings(&test.body);
        let mentions = whole_word_offsets(&masked, &self.owner);
        if mentions.is_empty() {
            return None;
        }
        let mut inputs = Vec::new();
        let mut selection = ArmSelection::SelectsOther;
        for line_start in line_starts_of(&test.body, &mentions) {
            let line_end = test.body[line_start..]
                .find('\n')
                .map_or(test.body.len(), |offset| line_start + offset);
            let line = &test.body[line_start..line_end];
            let calls = owner_calls_in(line, &self.owner, self.method, &self.local_roots);
            let line_mentions = mentions
                .iter()
                .filter(|offset| (line_start..line_end).contains(*offset))
                .count();
            if calls.len() != line_mentions {
                return None;
            }
            for call in calls {
                let call_line = test.start_line
                    + test.body[..line_start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count();
                let input =
                    self.call_input_at(&call, test, call_line, &line[call.span.0..call.span.1])?;
                if self.input_import_ambiguous(test, &input) {
                    return None;
                }
                selection = combine(selection, self.judge(&input));
                inputs.push(input);
            }
        }
        Some(ObservedArmInputs { inputs, selection })
    }

    fn call_input<'a>(&self, call: &OwnerCall<'a>) -> Option<&'a str> {
        let input = match self.binding {
            ScrutineeBinding::Receiver => call.receiver,
            ScrutineeBinding::Parameter(index) => call.arguments.get(index).copied(),
        }?;
        match &self.field_selector {
            Some(field) => field.project(input),
            None => Some(input),
        }
    }

    fn call_input_at(
        &self,
        call: &OwnerCall<'_>,
        test: &TestSummary,
        call_line: usize,
        call_text: &str,
    ) -> Option<String> {
        match &self.field_selector {
            Some(field) => {
                let ScrutineeBinding::Parameter(index) = self.binding else {
                    return None;
                };
                field.project_at_call(call.arguments.get(index)?, test, call_line, call_text)
            }
            None => self.call_input(call).map(str::to_string),
        }
    }

    /// RIPR-SPEC-0229 first-match selection. When the diff changed the
    /// arm's pattern, "selects another arm under both patterns" stays
    /// `SelectsOther`, an input that moved between this arm and a later one
    /// with a provably unequal result is `Selects` (it discriminates the
    /// change), and everything else is `Unknown`.
    fn judge(&self, input: &str) -> ArmSelection {
        // `Priority::Low` is not `Level::Low`: a qualifier other than the
        // scrutinee's type (or `Self`) names another enum's variant.
        if let Some(qualifier) = input_qualifier(input)
            && qualifier != "Self"
            && self.scrutinee_type.as_deref() != Some(qualifier)
        {
            return ArmSelection::Unknown;
        }
        // `other_crate::RenameRule::LowerCase` names a same-named type
        // outside the workspace: a path above the type must start at a
        // workspace root, like a qualified free owner call.
        if input_path_root(input)
            .is_some_and(|root| !self.local_roots.iter().any(|local| local == root))
        {
            return ArmSelection::Unknown;
        }
        let input = match input_head(input) {
            PatternHead::UncasedVariant { .. } if !self.uncased_inputs_trusted => {
                PatternHead::Opaque
            }
            // A test's `Self` is the test-side impl's type, not the owner's,
            // so the owner's impl context cannot resolve it.
            PatternHead::UncasedVariant { qualifier, .. } if qualifier == "Self" => {
                PatternHead::Opaque
            }
            head => self.resolve_head(head),
        };
        let input = match (self.arithmetic, input) {
            (None, input) => input,
            (Some(arithmetic), PatternHead::Literal(LiteralValue::Int(value))) => arithmetic
                .apply(value)
                .map_or(PatternHead::Opaque, |value| {
                    PatternHead::Literal(LiteralValue::Int(value))
                }),
            (Some(_), _) => PatternHead::Opaque,
        };
        if input == PatternHead::Opaque {
            return ArmSelection::Unknown;
        }
        let current = self.first_match(&self.alternatives, &input);
        match &self.changed_from {
            None => current,
            Some(Some(original)) => {
                let before = self.first_match(original, &input);
                if current == ArmSelection::SelectsOther && before == ArmSelection::SelectsOther {
                    ArmSelection::SelectsOther
                } else if self.moved_to_unequal_result(current, before, &input) {
                    ArmSelection::Selects
                } else {
                    ArmSelection::Unknown
                }
            }
            Some(None) => ArmSelection::Unknown,
        }
    }

    /// RIPR-SPEC-0229 "Crediting the selected arm" for a changed pattern:
    /// the input runs this arm under exactly one of the two patterns, the
    /// first later arm that takes it under the other is provably found, and
    /// the two arms' results are provably unequal values. The original
    /// `match` is read as the current one with this arm's old pattern
    /// restored, so the credit says the test notices this line reverting;
    /// it needs the old text to be this arm with the same body.
    fn moved_to_unequal_result(
        &self,
        current: ArmSelection,
        before: ArmSelection,
        input: &PatternHead,
    ) -> bool {
        let moved = matches!(
            (current, before),
            (ArmSelection::Selects, ArmSelection::SelectsOther)
                | (ArmSelection::SelectsOther, ArmSelection::Selects)
        );
        if !moved || !self.original_same_body || !self.match_is_tail {
            return false;
        }
        let (Some(result), Some(later)) = (&self.result, self.later_result(input)) else {
            return false;
        };
        provably_unequal(result, &later)
    }

    /// The result of the first arm after the changed one that provably
    /// takes the input, when every arm between provably does not.
    fn later_result(&self, input: &PatternHead) -> Option<ResultValue> {
        for arm in self.later.as_ref()? {
            match judge_alternatives(arm.alternatives.as_ref()?, input) {
                ArmSelection::Selects => return arm.result.clone(),
                ArmSelection::SelectsOther => {}
                ArmSelection::Unknown => return None,
            }
        }
        None
    }

    /// Whether `alternatives`, standing at the changed arm's position, take
    /// the input. An earlier arm that provably matches runs first, whatever
    /// the arms between do. Otherwise the arm is selected only when its own
    /// alternatives match, every earlier arm provably does not, and the call
    /// always runs the enclosing `match`.
    fn first_match(&self, alternatives: &[PatternHead], input: &PatternHead) -> ArmSelection {
        if self
            .earlier
            .iter()
            .flatten()
            .any(|arm| judge_alternatives(arm, input) == ArmSelection::Selects)
        {
            return ArmSelection::SelectsOther;
        }
        match judge_alternatives(alternatives, input) {
            ArmSelection::Selects => {}
            other => return other,
        }
        for arm in &self.earlier {
            let Some(alternatives) = arm else {
                return ArmSelection::Unknown;
            };
            if judge_alternatives(alternatives, input) != ArmSelection::SelectsOther {
                return ArmSelection::Unknown;
            }
        }
        if self.reached {
            ArmSelection::Selects
        } else {
            ArmSelection::Unknown
        }
    }
}

fn judge_alternatives(alternatives: &[PatternHead], input: &PatternHead) -> ArmSelection {
    let mut selection = ArmSelection::SelectsOther;
    for alternative in alternatives {
        match judge_alternative(alternative, input) {
            ArmSelection::Selects => return ArmSelection::Selects,
            ArmSelection::Unknown => selection = ArmSelection::Unknown,
            ArmSelection::SelectsOther => {}
        }
    }
    selection
}

/// An earlier arm's alternatives, or `None` when it carries a guard.
fn earlier_arm(pattern: &str) -> Option<Vec<PatternHead>> {
    let masked = mask_comments_and_strings(pattern);
    if !whole_word_offsets(&masked, "if").is_empty() {
        return None;
    }
    Some(
        top_level_alternatives(pattern)?
            .into_iter()
            .map(pattern_head)
            .collect(),
    )
}

/// Across calls: any `Selects` wins, then any `Unknown`.
fn combine(left: ArmSelection, right: ArmSelection) -> ArmSelection {
    match (left, right) {
        (ArmSelection::Selects, _) | (_, ArmSelection::Selects) => ArmSelection::Selects,
        (ArmSelection::Unknown, _) | (_, ArmSelection::Unknown) => ArmSelection::Unknown,
        _ => ArmSelection::SelectsOther,
    }
}

fn judge_alternative(alternative: &PatternHead, input: &PatternHead) -> ArmSelection {
    match (alternative, input) {
        (PatternHead::Literal(pattern), PatternHead::Literal(value)) => {
            if std::mem::discriminant(pattern) != std::mem::discriminant(value) {
                ArmSelection::Unknown
            } else if pattern == value {
                ArmSelection::Selects
            } else {
                ArmSelection::SelectsOther
            }
        }
        (
            PatternHead::Variant { name, irrefutable },
            PatternHead::Variant {
                name: input_name, ..
            },
        ) => {
            if name != input_name {
                ArmSelection::SelectsOther
            } else if *irrefutable {
                ArmSelection::Selects
            } else {
                ArmSelection::Unknown
            }
        }
        (PatternHead::IntRange { start, end }, PatternHead::Literal(LiteralValue::Int(value))) => {
            if start.is_none_or(|start| *value >= start) && end.is_none_or(|end| *value <= end) {
                ArmSelection::Selects
            } else {
                ArmSelection::SelectsOther
            }
        }
        (
            PatternHead::CatchAll | PatternHead::Binding(_),
            PatternHead::Literal(_) | PatternHead::Variant { .. },
        ) => ArmSelection::Selects,
        _ => ArmSelection::Unknown,
    }
}

/// Two pattern texts that differ only in whitespace.
fn same_pattern(left: &str, right: &str) -> bool {
    left.split_whitespace().eq(right.split_whitespace())
}

/// The same top-level alternatives in any order, each compared ignoring
/// whitespace.
fn same_alternatives(left: &str, right: &str) -> bool {
    let normalized = |pattern: &str| {
        top_level_alternatives(pattern).map(|alternatives| {
            alternatives
                .into_iter()
                .map(|alternative| alternative.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<std::collections::BTreeSet<_>>()
        })
    };
    normalized(left).is_some_and(|left| normalized(right).is_some_and(|right| left == right))
        || same_literal_values(left, right)
}

/// RIPR-SPEC-0229 decision 6: alternatives that are all literals or
/// literal-bounded integer ranges, equal as sets of values in any spelling
/// (`3` and `0x3`), take exactly the same inputs.
fn same_literal_values(left: &str, right: &str) -> bool {
    let heads = |pattern: &str| {
        top_level_alternatives(pattern).and_then(|alternatives| {
            let heads = alternatives
                .into_iter()
                .map(pattern_head)
                .collect::<Vec<_>>();
            heads
                .iter()
                .all(|head| matches!(head, PatternHead::Literal(_) | PatternHead::IntRange { .. }))
                .then_some(heads)
        })
    };
    match (heads(left), heads(right)) {
        (Some(left), Some(right)) => {
            left.iter().all(|head| right.contains(head))
                && right.iter().all(|head| left.contains(head))
        }
        _ => false,
    }
}

/// The pattern before `=>`, without comments, trimmed, with no guard.
/// `None` without a fat arrow or with an `if` guard: a guard's truth is
/// never established here.
fn arm_pattern_text(expression: &str) -> Option<String> {
    let arrow = find_fat_arrow(expression)?;
    // Two arms on one line: which one changed is not known.
    if find_fat_arrow(&expression[arrow + 2..]).is_some() {
        return None;
    }
    let pattern = without_comments(&expression[..arrow])?;
    let pattern = pattern.trim();
    let masked = mask_comments_and_strings(pattern);
    if pattern.is_empty() || !whole_word_offsets(&masked, "if").is_empty() {
        return None;
    }
    Some(pattern.to_string())
}

/// `text` with every comment byte blanked and string and character
/// literals kept, so a `/* "x" */` beside a pattern never reads as part of
/// it while `'S' | 'C'` keeps its characters.
fn without_comments(text: &str) -> Option<String> {
    let (masked, chars) = mask_with_char_literals(text);
    let strings = string_span_ranges(text);
    let bytes = text
        .bytes()
        .zip(masked.bytes())
        .enumerate()
        .map(|(index, (original, masked))| {
            let in_string = strings
                .iter()
                .any(|(start, end)| index >= *start && index < *end)
                // A char span's end is its closing quote's offset, so it
                // is inclusive where a string span's end is exclusive.
                || chars
                    .iter()
                    .any(|(start, end)| index >= *start && index <= *end);
            if original != masked && !in_string {
                b' '
            } else {
                original
            }
        })
        .collect::<Vec<_>>();
    String::from_utf8(bytes).ok()
}

/// Split a pattern at top-level `|`, dropping a leading `|`.
fn top_level_alternatives(pattern: &str) -> Option<Vec<&str>> {
    let masked = mask_comments_and_strings(pattern);
    let mut alternatives = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 => {
                alternatives.push(pattern[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    if depth != 0 {
        return None;
    }
    alternatives.push(pattern[start..].trim());
    if alternatives.first().is_some_and(|first| first.is_empty()) {
        alternatives.remove(0);
    }
    if alternatives
        .iter()
        .any(|alternative| alternative.is_empty())
    {
        return None;
    }
    Some(alternatives)
}

fn pattern_head(alternative: &str) -> PatternHead {
    if let Some(literal) = literal_value(alternative) {
        return PatternHead::Literal(literal);
    }
    if let Some(range) = integer_range(alternative) {
        return range;
    }
    if let Some(binding) = catch_all_binding(alternative) {
        return if binding == "_" {
            PatternHead::CatchAll
        } else {
            PatternHead::Binding(binding.to_string())
        };
    }
    match variant_parts(alternative) {
        Some(VariantParts {
            qualifier,
            name,
            payload,
        }) => variant_head(qualifier, name, payload.is_none_or(payload_is_irrefutable)),
        None => PatternHead::Opaque,
    }
}

/// A CamelCase name is a variant by its case alone; an uncased one waits
/// for `ArmSelector::resolve_head`. "CamelCase" is `is_variant_name`.
fn variant_head(qualifier: Option<&str>, name: &str, irrefutable: bool) -> PatternHead {
    match qualifier {
        Some(qualifier) if !is_variant_name(name) => PatternHead::UncasedVariant {
            qualifier: qualifier.to_string(),
            name: name.to_string(),
            irrefutable,
        },
        _ => PatternHead::Variant {
            name: name.to_string(),
            irrefutable,
        },
    }
}

/// An integer range pattern with literal bounds (`1..=9`, `0..10`,
/// `..=9`, `10..`). A char range, a path bound (`MIN..=MAX`) or a bare
/// `..` is not one.
fn integer_range(alternative: &str) -> Option<PatternHead> {
    let text = alternative.trim();
    let (start, end, inclusive) = match text.split_once("..=") {
        Some((start, end)) => (start, end, true),
        None => {
            let (start, end) = text.split_once("..")?;
            (start, end, false)
        }
    };
    let (start, end) = (start.trim(), end.trim());
    let start = match start {
        "" => None,
        start => Some(integer_value(start)?),
    };
    let end = match end {
        "" if inclusive => return None,
        "" => None,
        end => {
            let end = integer_value(end)?;
            Some(if inclusive { end } else { end.checked_sub(1)? })
        }
    };
    (start.is_some() || end.is_some()).then_some(PatternHead::IntRange { start, end })
}

/// An arm result that is provably one value: a literal, or a variant path
/// or constructor whose every payload entry is a literal. Any other
/// expression (`1 + 1`, a call, a binding, a block) may compute any value.
fn result_value(body: &str) -> Option<ResultValue> {
    let body = arm_body_text(body);
    if let Some(literal) = literal_value(body) {
        return Some(ResultValue::Literal(literal));
    }
    let VariantParts { name, payload, .. } = variant_parts(body)?;
    // An uncased name (`Kind::A`) may be an associated constant, and two
    // constants may hold one value, so only a CamelCase name is a variant
    // here.
    if !is_variant_name(name) {
        return None;
    }
    let qualifier = input_qualifier(body).unwrap_or_default().to_string();
    let payload = match payload {
        None => Vec::new(),
        // A CamelCase path with arguments may be a function call (`Make(1)`)
        // whose value need not keep its arguments, so only the prelude's
        // `Some`, `Ok` and `Err` constructors read as payload variants.
        Some(_)
            if !(matches!(name, "Some" | "Ok" | "Err")
                && matches!(
                    qualifier.rsplit("::").next().unwrap_or_default(),
                    "" | "Option" | "Result"
                )) =>
        {
            return None;
        }
        Some(inner) => split_top_level_arguments(inner)?
            .into_iter()
            .map(literal_value)
            .collect::<Option<Vec<_>>>()?,
    };
    Some(ResultValue::Variant {
        qualifier,
        name: name.to_string(),
        payload,
    })
}

/// An arm body without surrounding whitespace or its trailing `,`.
fn arm_body_text(body: &str) -> &str {
    let body = body.trim();
    body.strip_suffix(',').unwrap_or(body).trim_end()
}

/// Whether two results are different values: literals of one kind with
/// different values, or variants under the same qualifier with different
/// names, or the same variant with a payload entry of one kind and a
/// different value. Differently qualified variants (`Kind::Alpha` and an
/// imported `Alpha`, or a renamed import) are never read as unequal.
fn provably_unequal(left: &ResultValue, right: &ResultValue) -> bool {
    let unequal_literals = |left: &LiteralValue, right: &LiteralValue| {
        std::mem::discriminant(left) == std::mem::discriminant(right) && left != right
    };
    match (left, right) {
        (ResultValue::Literal(left), ResultValue::Literal(right)) => unequal_literals(left, right),
        (
            ResultValue::Variant {
                qualifier: left_qualifier,
                name: left_name,
                payload: left_payload,
            },
            ResultValue::Variant {
                qualifier: right_qualifier,
                name: right_name,
                payload: right_payload,
            },
        ) => {
            if left_qualifier != right_qualifier {
                return false;
            }
            if left_name != right_name {
                return true;
            }
            left_payload.len() == right_payload.len()
                && left_payload.iter().zip(right_payload).all(|(left, right)| {
                    std::mem::discriminant(left) == std::mem::discriminant(right)
                })
                && left_payload
                    .iter()
                    .zip(right_payload)
                    .any(|(left, right)| unequal_literals(left, right))
        }
        _ => false,
    }
}

/// `_`, or a bare lowercase binding with an optional `ref`/`mut`, as its
/// name. A binding with an `@` subpattern, a path or a literal is not one.
fn catch_all_binding(alternative: &str) -> Option<&str> {
    let binding = alternative.trim();
    let binding = binding
        .strip_prefix("ref ")
        .or_else(|| binding.strip_prefix("mut "))
        .unwrap_or(binding)
        .trim();
    (binding == "_"
        || is_identifier(binding)
            && binding.starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
            && !matches!(binding, "true" | "false" | "self"))
    .then_some(binding)
}

/// Whether a lowercase pattern name may resolve to a constant in the
/// owner's file: a `const`/`static` item of that name, an import that
/// binds it, or a glob import of a module (`use consts::*`), which may
/// bring in any name. A glob of an enum's variants (`use Rule::*`) does
/// not count: Rust variants are CamelCase.
pub(in crate::analysis) fn may_name_constant(source: &str, name: &str) -> bool {
    let masked = mask_comments_and_strings(source);
    let declared = whole_word_offsets(&masked, name).into_iter().any(|offset| {
        let before = masked[..offset].trim_end();
        let before = before.strip_suffix("mut").map_or(before, str::trim_end);
        before.ends_with("const") || before.ends_with("static")
    });
    declared
        || flattened_use_paths(source).iter().any(|import| {
            let mut segments = import.path.rsplit("::");
            let last = segments.next().unwrap_or("");
            match &import.alias {
                Some(alias) => alias == name,
                None if last == "*" => !segments.next().is_some_and(is_variant_name),
                None => last == name,
            }
        })
}

/// The variant names of the one enum named `type_name` that `source`
/// declares (#7543). `None` when the file may bind that name to anything
/// else: it declares no such enum or more than one, declares another item
/// of that name, imports the name, glob-imports a module (which may bring
/// a same-named struct with constants), or calls an item macro (whose
/// output is not read). So an uncased `Kind::A` stays unreadable unless the
/// file itself shows `A` is a variant of `Kind`.
fn declared_enum_variants(
    source: &str,
    type_name: &str,
    owner: &str,
) -> Option<std::collections::BTreeSet<String>> {
    use ra_ap_syntax::{AstNode, ast, ast::HasName};
    // Cheap text test before the parse: most owner files declare no enum
    // of the scrutinee's type at all.
    let masked = mask_comments_and_strings(source);
    let declares_enum = whole_word_offsets(&masked, type_name)
        .into_iter()
        .any(|offset| masked[..offset].trim_end().ends_with("enum"));
    if !declares_enum {
        return None;
    }
    let may_rebind = flattened_use_paths(source).iter().any(|import| {
        let mut segments = import.path.rsplit("::");
        let mut last = segments.next().unwrap_or("");
        if last == "self" {
            last = segments.next().unwrap_or("");
        }
        match &import.alias {
            Some(alias) => alias == type_name,
            // `super::*` is judged on the parse below.
            None if last == "*" => {
                import.path != "super::*" && !segments.next().is_some_and(is_variant_name)
            }
            None => last == type_name,
        }
    });
    if may_rebind {
        return None;
    }
    let parse = crate::analysis::syntax::parse_clean_source_file(source)?;
    let named = |item: &dyn HasName| item.name().is_some_and(|name| name.text() == type_name);
    let mut found = None;
    for node in parse.tree().syntax().descendants() {
        if let Some(item) = ast::Enum::cast(node.clone()) {
            if !named(&item) {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(
                item.variant_list()
                    .into_iter()
                    .flat_map(|list| list.variants())
                    .filter_map(|variant| variant.name().map(|name| name.text().to_string()))
                    .collect(),
            );
        } else if ast::Struct::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::Union::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::TypeAlias::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::Trait::cast(node.clone()).is_some_and(|item| named(&item))
            || parent_glob_outside_this_file(&node)
            || may_shadow_in_owner_scope(&node, owner)
        {
            return None;
        }
    }
    found
}

/// Whether a macro's unread expansion may declare an item the function
/// `owner` sees, such as a `const p` that a bare guard binding `p` then
/// names (#7922 review): a `macro_rules!` or `include!` anywhere in the
/// file, any node [`may_shadow_in_owner_scope`] flags (an item-level macro
/// call in any module, a statement macro inside `owner`), or a file that
/// does not parse cleanly.
pub(in crate::analysis) fn may_declare_items_in_owner_scope(source: &str, owner: &str) -> bool {
    use ra_ap_syntax::AstNode;
    let masked = mask_comments_and_strings(source);
    if masked.contains("macro_rules!") || masked.contains("include!") {
        return true;
    }
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
        return true;
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .any(|node| may_shadow_in_owner_scope(&node, owner))
}

/// Whether `node` may bring an unread local item into the owner's scope:
/// a macro call at file or module item level (its expansion is not read),
/// a macro call in statement position inside a function named `owner`
/// (`shadow!();` may expand to a `struct Kind;` scoped to the match), or a
/// function named `owner` nested in a block, which sees that block's items
/// (#7686 review). Statement macros in other functions cannot reach the
/// owner's match, so they are not read. A macro in expression position
/// expands to an expression.
fn may_shadow_in_owner_scope(node: &ra_ap_syntax::SyntaxNode, owner: &str) -> bool {
    use ra_ap_syntax::{AstNode, ast, ast::HasName};
    let owner = owner.strip_prefix("r#").unwrap_or(owner);
    let named_owner = |function: &ast::Fn| {
        function.name().is_some_and(|name| {
            let text = name.text();
            text.strip_prefix("r#").unwrap_or(text) == owner
        })
    };
    if let Some(function) = ast::Fn::cast(node.clone()) {
        return named_owner(&function)
            && function
                .syntax()
                .ancestors()
                .skip(1)
                .any(|ancestor| ast::StmtList::can_cast(ancestor.kind()));
    }
    if !ast::MacroCall::can_cast(node.kind()) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind()) {
        return true;
    }
    let statement = ast::MacroExpr::can_cast(parent.kind())
        && parent.parent().is_some_and(|holder| {
            ast::ExprStmt::can_cast(holder.kind()) || ast::StmtList::can_cast(holder.kind())
        })
        || ast::MacroStmts::can_cast(parent.kind());
    statement
        && node
            .ancestors()
            .filter_map(ast::Fn::cast)
            .any(|function| named_owner(&function))
}

/// #7675: the variants of the enum `type_name` when the owner's file binds
/// that name through exactly one import, a top-level non-renamed `use` whose
/// module path the recorded `mod` edges place in exactly one workspace file
/// (`use kinds::Kind;` after `mod kinds;`, `use crate::a::b::Kind;`,
/// `use self::..`/`use super::..`), and that file declares the enum
/// (`declared_enum_variants`). An owner item or item macro that may define
/// `Kind`, an inline or `cfg`-duplicated module step, a `cfg`-gated import
/// or step, a chain that does not start at a `src/` or `crates/<name>/src/`
/// crate root, any workspace `#[path]`, a
/// re-export, an external or package-named root, an `include!` edge, or any
/// unresolved edge reads nothing, so `Kind::A` stays unreadable.
fn imported_enum_variants(
    index: &crate::analysis::rust_index::RustIndex,
    owner_file: &std::path::Path,
    type_name: &str,
    owner_name: &str,
) -> Option<std::collections::BTreeSet<String>> {
    use crate::analysis::facts::SourceRoleProvenanceEdgeKind;
    use ra_ap_syntax::{AstNode, ast, ast::HasAttrs, ast::HasName};
    // A `cfg` or `cfg_attr` on the import or a module step may compile it
    // out, leaving `Kind` to a glob or another item (#7686 review).
    // The attribute's leading name is read from its text, so a parse that
    // leaves the path off still counts.
    let cfg_gated = |item: &dyn HasAttrs| {
        item.attrs().any(|attr| {
            let text = attr.syntax().text().to_string();
            let inner = text.trim_start_matches(|ch: char| {
                ch == '#' || ch == '!' || ch == '[' || ch.is_whitespace()
            });
            let name = inner
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or_default();
            matches!(name, "cfg" | "cfg_attr")
        })
    };
    let files = index.files();
    let owner = files.get(owner_file)?;
    let parse = crate::analysis::syntax::parse_clean_source_file(&owner.source)?;
    let named = |item: &dyn HasName| item.name().is_some_and(|name| name.text() == type_name);
    let mut bindings = Vec::new();
    for node in parse.tree().syntax().descendants() {
        // The owner's own item of that name (in an inline module or behind
        // a `cfg` alternative to the import) or an item macro's unread
        // output may be what `Kind` means at the match (#7686 review).
        if ast::Struct::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::Union::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::Enum::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::TypeAlias::cast(node.clone()).is_some_and(|item| named(&item))
            || ast::Trait::cast(node.clone()).is_some_and(|item| named(&item))
            || may_shadow_in_owner_scope(&node, owner_name)
        {
            return None;
        }
        let Some(item) = ast::Use::cast(node) else {
            continue;
        };
        let top_level = item
            .syntax()
            .parent()
            .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()));
        for import in flattened_use_paths(&item.syntax().text().to_string()) {
            let last = import.path.rsplit("::").next().unwrap_or_default();
            // A glob in an inline module may bring another `Kind` into the
            // module holding the match; `super::*` there brings this file's
            // items, read above. At the file root the explicit import
            // shadows any glob.
            if !top_level && import.alias.is_none() && last == "*" && import.path != "super::*" {
                return None;
            }
            if import.alias.as_deref() == Some(type_name)
                || (import.alias.is_none() && last == type_name)
            {
                bindings.push((
                    top_level && import.alias.is_none() && !cfg_gated(&item),
                    import.path,
                ));
            }
        }
    }
    let [(true, path)] = bindings.as_slice() else {
        return None;
    };
    let segments = path.split("::").collect::<Vec<_>>();
    let (_, modules) = segments.split_last()?;
    // Whether a file's chain of `mod` edges is all resolved `mod` edges; a
    // file with an unresolved or `include!` edge is not placed.
    let placed = |file: &std::path::Path| {
        files.get(file).is_some_and(|facts| {
            let provenance = &facts.role_provenance;
            provenance.earliest_unresolved_reason.is_none()
                && provenance
                    .edges
                    .iter()
                    .all(|edge| edge.kind == SourceRoleProvenanceEdgeKind::Module)
        })
    };
    if !placed(owner_file) {
        return None;
    }
    let owner_chain = &owner.role_provenance.edges;
    // An out-of-line `mod` inside an inline module records no edge, so a
    // chain can start below the crate root; `crate::` and `super::` are
    // read only when it starts at `src/lib.rs` or `src/main.rs`. A file
    // shared by several crates (`tests/common/mod.rs`) never does. A module
    // file can carry such a path below inline modules (`mod outer { mod src
    // { mod main; } }` places `src/outer/src/main.rs`), so the root must be
    // the workspace's `src/` or a `crates/<name>/src/` root, and the
    // package manifest must not move a target root (`[lib] path =
    // "src/real.rs"` makes `src/lib.rs` a module). Default module
    // resolution reaches those paths from another crate only through a
    // `mod` of the root's stem (a `lib.rs` at the package root with `mod src
    // { mod lib; }`, a macro's `mod $name;`, or `m!(mod, lib)`) from another
    // root, and `#[path]` or `include!` reaches anywhere, so none of these
    // may appear (#7686 review).
    let root = owner_chain
        .first()
        .map_or(owner_file, |edge| edge.parent.as_path());
    let components = root
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    // The package directory holding that root: "" or `crates/<name>`.
    let package = components
        .len()
        .checked_sub(2)
        .map(|len| &components[..len]);
    let stem = components
        .last()
        .map(|file| file.trim_end_matches(".rs"))
        .unwrap_or_default();
    // The root's sibling target root (`src/main.rs` beside `src/lib.rs`)
    // reaches it with a plain `mod lib;`, which an external macro can
    // spell from an argument; any mention of the stem there refuses, and so
    // does any item macro or non-builtin attribute there, whose expansion
    // the index never reads (#7686 review).
    let sibling = root.with_file_name(if stem == "lib" { "main.rs" } else { "lib.rs" });
    let crate_root = matches!(
        components.as_slice(),
        ["src", "lib.rs" | "main.rs"] | ["crates", _, "src", "lib.rs" | "main.rs"]
    ) && index.member_crates.is_default_target_root(root)
        && !files.iter().any(|(path, facts)| {
            // A package file outside Cargo's standard target directories may be
            // a declared crate root (`[lib] path = "lib.rs"`) whose modules
            // reach `src/lib.rs`; another member's files are its own package.
            let parts = path
                .components()
                .map(|component| component.as_os_str().to_str().unwrap_or_default())
                .collect::<Vec<_>>();
            let nonstandard = package.is_some_and(|package| {
                parts.starts_with(package)
                    && !(package.is_empty() && parts.first() == Some(&"crates"))
                    && !matches!(
                        parts.get(package.len()).copied(),
                        Some("src" | "tests" | "benches" | "examples" | "build.rs")
                    )
            });
            let masked = mask_comments_and_strings(&facts.source);
            nonstandard
            || super::call_identity::has_path_attribute(&facts.source)
            // `include!` splices unindexed text, which may declare modules.
            || !whole_word_offsets(&masked, "include").is_empty()
            // A `mod` keyword that does not start a plain `mod name;` or
            // `mod name {` is macro input (`mod $name;`, `m!(mod, lib)`) and
            // may declare a module of any name.
            || whole_word_offsets(&masked, "mod")
                .into_iter()
                .any(|offset| !declares_named_module(&masked[offset + 3..]))
            || (path.as_path() == sibling.as_path()
                && (!whole_word_offsets(&facts.source, stem).is_empty()
                    || may_expand_items(&facts.source)))
            || whole_word_offsets(&masked, stem).into_iter().any(|offset| {
                let before = masked[..offset].trim_end();
                let before = before.strip_suffix("r#").unwrap_or(before).trim_end();
                before.ends_with("mod")
            })
        });
    if !crate_root {
        return None;
    }
    let (mut file, steps) = match modules.split_first()? {
        (&"crate", rest) => (root.to_path_buf(), rest),
        (&"self", rest) => (owner_file.to_path_buf(), rest),
        (&"super", rest) => (owner_chain.last()?.parent.clone(), rest),
        _ => (owner_file.to_path_buf(), modules),
    };
    for step in steps {
        if matches!(*step, "crate" | "self" | "super") {
            return None;
        }
        // The step's one `mod` in this file must be out of line and at the
        // file's top level: an inline module of that name, a `cfg`
        // alternative to it, or a nested `mod outer { mod kinds; }` beside a
        // top-level `use constants as kinds;` names a different module
        // (#7686 review).
        let parent = crate::analysis::syntax::parse_clean_source_file(&files.get(&file)?.source)?;
        let mut modules = parent
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Module::cast)
            .filter(|module| {
                module.name().is_some_and(|name| {
                    let text = name.text();
                    text.strip_prefix("r#").unwrap_or(text) == *step
                })
            });
        let only = modules.next()?;
        let top_level = only
            .syntax()
            .parent()
            .is_some_and(|holder| ast::SourceFile::can_cast(holder.kind()));
        if only.item_list().is_some() || !top_level || cfg_gated(&only) || modules.next().is_some()
        {
            return None;
        }
        let mut children = files.iter().filter_map(|(child, facts)| {
            let edge = facts.role_provenance.edges.last()?;
            (edge.parent == file
                && edge
                    .declaration
                    .split_whitespace()
                    .skip_while(|word| *word != "mod")
                    .nth(1)
                    .map(|name| name.trim_end_matches(';'))
                    == Some(*step)
                && placed(child))
            .then(|| child.clone())
        });
        let child = children.next()?;
        if children.next().is_some() {
            return None;
        }
        // An inner `#![cfg]` compiles the module file out as surely as one
        // on its `mod` item (#7686 review).
        let tree = crate::analysis::syntax::parse_clean_source_file(&files.get(&child)?.source)?;
        if cfg_gated(&tree.tree()) {
            return None;
        }
        file = child;
    }
    if file == owner_file {
        return None;
    }
    declared_enum_variants(&files.get(&file)?.source, type_name, owner_name)
}

/// Whether a file may hold items the index never reads: an item-level
/// macro call, an attribute other than a compiler builtin (a proc-macro
/// attribute or custom derive may emit any item, and `cfg_attr` may apply
/// one), or an import that may shadow a builtin. An unparsable file may.
fn may_expand_items(source: &str) -> bool {
    use ra_ap_syntax::{AstNode, ast};
    const BUILTIN_ATTRIBUTES: &[&str] = &[
        "allow",
        "cfg",
        "cold",
        "deny",
        "deprecated",
        "derive",
        "doc",
        "expect",
        "forbid",
        "ignore",
        "inline",
        "must_use",
        "no_mangle",
        "non_exhaustive",
        "repr",
        "should_panic",
        "test",
        "track_caller",
        "warn",
    ];
    const BUILTIN_DERIVES: &[&str] = &[
        "Clone",
        "Copy",
        "Debug",
        "Default",
        "Eq",
        "Hash",
        "Ord",
        "PartialEq",
        "PartialOrd",
    ];
    // A glob, or an import named like a builtin, may shadow the prelude's
    // attribute or derive (`use pm::Clone; #[derive(Clone)]`), and a
    // `cfg_attr` may apply any attribute, so neither is read as builtin.
    let shadowed = flattened_use_paths(source).iter().any(|import| {
        let name = import
            .alias
            .as_deref()
            .unwrap_or_else(|| import.path.rsplit("::").next().unwrap_or_default());
        let name = name.strip_prefix("r#").unwrap_or(name);
        name == "*" || BUILTIN_ATTRIBUTES.contains(&name) || BUILTIN_DERIVES.contains(&name)
    });
    if shadowed {
        return true;
    }
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
        return true;
    };
    parse.tree().syntax().descendants().any(|node| {
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            return call.syntax().parent().is_some_and(|parent| {
                ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
            });
        }
        let Some(attr) = ast::Attr::cast(node) else {
            return false;
        };
        let text = attr.syntax().text().to_string();
        let inner = text.trim_start_matches(|ch: char| {
            ch == '#' || ch == '!' || ch == '[' || ch.is_whitespace()
        });
        let name_end = inner
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(inner.len());
        let (name, rest) = inner.split_at(name_end);
        if !BUILTIN_ATTRIBUTES.contains(&name) || rest.trim_start().starts_with("::") {
            return true;
        }
        name == "derive"
            && rest
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
                .filter(|word| !word.is_empty())
                .any(|word| !BUILTIN_DERIVES.contains(&word))
    })
}

/// Whether `rest`, the text after a `mod` keyword, starts a plain module
/// declaration: a (raw) identifier followed by `;` or `{`.
fn declares_named_module(rest: &str) -> bool {
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("r#").unwrap_or(rest);
    let end = rest
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    end > 0 && rest[end..].trim_start().starts_with([';', '{'])
}

/// Whether a related test file outside the owner's file may bind
/// `type_name` to something other than the owner's enum: it declares an
/// item of that name, renames an import to it, or glob-imports a module
/// other than `super`, a workspace root (`my_crate::*`) or the type's own
/// variants. A test-side `struct Kind` with `const A` makes `Kind::A` a
/// constant, which a pattern compares by value.
fn test_file_may_rebind(source: &str, type_name: &str, local_roots: &[String]) -> bool {
    let masked = mask_comments_and_strings(source);
    let declares = whole_word_offsets(&masked, type_name)
        .into_iter()
        .any(|offset| {
            let before = masked[..offset].trim_end();
            [
                "struct", "enum", "union", "trait", "type", "mod", "const", "static", "fn",
            ]
            .iter()
            .any(|keyword| {
                before.ends_with(keyword)
                    && !before[..before.len() - keyword.len()]
                        .ends_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
            })
        });
    declares
        || flattened_use_paths(source).iter().any(|import| {
            let segments = import.path.split("::").collect::<Vec<_>>();
            match (&import.alias, segments.as_slice()) {
                (Some(alias), _) => alias == type_name,
                (None, [.., parent, "*"]) => {
                    *parent != type_name
                        && !(segments.len() == 2 && local_roots.iter().any(|root| root == parent))
                }
                _ => false,
            }
        })
}

/// Whether `node` is a file-root `use super::*`, which brings in another
/// file's items. In an inline module (`mod tests { use super::*; }`) it
/// brings in only this file's items and imports, which are read already.
fn parent_glob_outside_this_file(node: &ra_ap_syntax::SyntaxNode) -> bool {
    use ra_ap_syntax::{AstNode, ast};
    ast::Use::cast(node.clone()).is_some_and(|item| {
        !item
            .syntax()
            .ancestors()
            .any(|ancestor| ast::Module::can_cast(ancestor.kind()))
            && flattened_use_paths(&item.syntax().text().to_string())
                .iter()
                .any(|import| import.path == "super::*")
    })
}

/// The same reading applied to a call's input expression. A variant input
/// selects by name whatever its payload is.
fn input_head(input: &str) -> PatternHead {
    let input = input.trim();
    if let Some(literal) = literal_value(input) {
        return PatternHead::Literal(literal);
    }
    match variant_parts(input) {
        Some(VariantParts {
            qualifier, name, ..
        }) => variant_head(qualifier, name, true),
        None => PatternHead::Opaque,
    }
}

struct VariantParts<'a> {
    /// The whole path before the name (`a::Kind` in `a::Kind::A`), if
    /// any, so only a one-segment qualifier can equal the scrutinee's bare
    /// type: `presets::Kind` may be another type than the owner's `Kind`.
    qualifier: Option<&'a str>,
    name: &'a str,
    /// The payload text without its delimiters.
    payload: Option<&'a str>,
}

/// A path whose last segment reads as a variant (`Some`, `LowerCase`,
/// `Mode::Frozen`), optionally followed by one `(..)` or `{..}` payload.
/// A qualified uncased name (`Kind::A`) is returned too; `variant_head`
/// keeps it apart until the owner's file shows it is a variant.
fn variant_parts(text: &str) -> Option<VariantParts<'_>> {
    let text = text.trim();
    let path_end = text
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(text.len());
    let path = &text[..path_end];
    let rest = text[path_end..].trim_start();
    let segments = path.split("::").collect::<Vec<_>>();
    if segments.iter().any(|segment| !is_identifier(segment)) {
        return None;
    }
    let (name, qualifier) = match segments.as_slice() {
        [.., _, name] => (*name, path.rsplit_once("::").map(|(prefix, _)| prefix)),
        [name] => (*name, None),
        [] => return None,
    };
    let uncased = qualifier.is_some()
        && name.starts_with(|ch: char| ch.is_ascii_uppercase())
        && !name.chars().any(|ch| ch.is_ascii_lowercase());
    if !is_variant_name(name) && !uncased {
        return None;
    }
    if rest.is_empty() {
        return Some(VariantParts {
            qualifier,
            name,
            payload: None,
        });
    }
    let (open, close) = match rest.as_bytes().first() {
        Some(b'(') => ('(', ')'),
        Some(b'{') => ('{', '}'),
        _ => return None,
    };
    let inner = rest.strip_prefix(open)?.strip_suffix(close)?;
    // The delimiters must enclose the whole payload: `Some(a)(b)` is not
    // one variant.
    let masked = mask_comments_and_strings(inner);
    let mut depth = 0i32;
    for byte in masked.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    (depth == 0).then_some(VariantParts {
        qualifier,
        name,
        payload: Some(inner),
    })
}

/// `_`, `..`, plain bindings (`v`, `ref v`, `mut v`) and struct shorthand
/// or `field: binding` entries. Anything that could refute (a literal, a
/// nested variant, a range) is not irrefutable.
fn payload_is_irrefutable(payload: &str) -> bool {
    let Some(entries) = split_top_level_arguments(payload) else {
        return false;
    };
    entries.into_iter().all(|entry| {
        let entry = entry.trim();
        let binding = entry
            .rsplit_once(':')
            .map_or(entry, |(_, value)| value)
            .trim();
        let binding = binding
            .strip_prefix("ref ")
            .or_else(|| binding.strip_prefix("mut "))
            .unwrap_or(binding)
            .trim();
        // `true`/`false` are literals, and a path (`limits::MAX`) is a
        // constant, not a binding: both refute.
        let is_binding = is_identifier(binding)
            && binding.starts_with(|ch: char| ch.is_ascii_lowercase())
            && !matches!(binding, "true" | "false");
        !entry.contains("::")
            && (binding.is_empty() || binding == "_" || binding == ".." || is_binding)
    })
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// CamelCase only: an uppercase first letter and at least one lowercase
/// letter. `SCREAMING` names and single letters may be constants (which a
/// pattern compares by value, not by name), so a qualified one is a
/// variant only when the owner's file declares it (`UncasedVariant`), and
/// lowercase names are bindings or functions. `variant_parts`,
/// `variant_head` and `result_value` all branch on this rule, so a change
/// to it moves them together.
fn is_variant_name(name: &str) -> bool {
    name.starts_with(|ch: char| ch.is_ascii_uppercase())
        && name.chars().any(|ch| ch.is_ascii_lowercase())
}

fn literal_value(text: &str) -> Option<LiteralValue> {
    let text = text.trim();
    match text {
        "true" => return Some(LiteralValue::Bool(true)),
        "false" => return Some(LiteralValue::Bool(false)),
        _ => {}
    }
    // Cooked and raw strings (`"a"`, `r#"a=>b"#`) compare by their decoded
    // value, through the same lexer reveal's literal rule uses. The text
    // must be exactly one literal.
    if text.starts_with(['"', 'r']) {
        let literals = lex_strings(text);
        if let [(0, end, Some(value))] = literals.as_slice()
            && *end == text.len()
        {
            return Some(LiteralValue::Str(value.clone()));
        }
        if text.starts_with('"') {
            return None;
        }
    }
    if let Some(inner) = text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return (!inner.contains(['\'', '\\']) && inner.chars().count() == 1)
            .then(|| LiteralValue::Char(inner.to_string()));
    }
    integer_value(text).map(LiteralValue::Int)
}

/// Decimal, hex, octal or binary integer literals with `_` separators and an
/// optional integer type suffix. `0x07FF` and `0x7FF` are the same value.
fn integer_value(text: &str) -> Option<i128> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, text),
    };
    let (radix, digits) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, digits)
    };
    let digits = strip_integer_suffix(digits, radix);
    let cleaned = digits.replace('_', "");
    if cleaned.is_empty() || !cleaned.chars().all(|ch| ch.is_digit(radix)) {
        return None;
    }
    let value = i128::from_str_radix(&cleaned, radix).ok()?;
    Some(if negative { -value } else { value })
}

fn strip_integer_suffix(digits: &str, radix: u32) -> &str {
    for suffix in [
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        // A hex literal's own digits can end in what looks like a suffix
        // only through `u`/`i`, which are not hex digits, so stripping is
        // safe in every radix.
        if let Some(rest) = digits.strip_suffix(suffix)
            && (radix != 16 || !rest.is_empty())
        {
            return rest;
        }
    }
    digits
}

fn owner_has_self_parameter(owner: &FunctionSummary) -> bool {
    let signature = owner.body.lines().next().unwrap_or_default();
    let Some(open) = signature.find('(') else {
        return false;
    };
    let first = signature[open + 1..]
        .split([',', ')'])
        .next()
        .unwrap_or_default();
    // `self`, `mut self`, `&self`, `&mut self`, `&'a self`, `self: Box<Self>`.
    let name = first.split(':').next().unwrap_or_default().trim();
    let name = name.strip_prefix('&').unwrap_or(name).trim_start();
    let name = match name.strip_prefix('\'') {
        Some(rest) => rest
            .split_once(char::is_whitespace)
            .map_or("", |(_, rest)| rest)
            .trim_start(),
        None => name,
    };
    name.strip_prefix("mut ").unwrap_or(name).trim() == "self"
}

/// `name` from `name`, `mut name`, or a destructuring-free parameter.
fn parameter_name(parameter: &str) -> &str {
    parameter
        .trim()
        .strip_prefix("mut ")
        .unwrap_or(parameter.trim())
        .trim()
}

/// The enclosing `match` of the arm that starts on `arm_line`: its
/// scrutinee and the patterns of the arms before it, in source order.
/// `None` when the arm's enclosing brace is not a `match`, the line does
/// not start an arm, or an earlier arm cannot be delimited.
struct EnclosingMatch {
    scrutinee: String,
    earlier_patterns: Vec<String>,
    reached: bool,
    /// `(pattern, body)` of the changed arm and every arm after it, or
    /// `None` when they cannot be delimited up to the match's closing brace
    /// or were not asked for.
    arms_from_changed: Option<Vec<(String, String)>>,
    /// Whether only the owner body's closing `}` follows the match.
    match_is_tail: bool,
}

/// `with_later_arms` asks for the changed and later arms' bodies, which only
/// a changed pattern's moved-input credit reads.
fn enclosing_match(
    owner: &FunctionSummary,
    arm_line: usize,
    with_later_arms: bool,
) -> Option<EnclosingMatch> {
    let relative = arm_line.checked_sub(owner.start_line)?;
    let masked = mask_comments_and_strings(&owner.body);
    let source = without_comments(&owner.body)?;
    let arm_offset = masked
        .split_inclusive('\n')
        .take(relative)
        .map(str::len)
        .sum::<usize>();
    if arm_offset == 0 || arm_offset > masked.len() {
        return None;
    }
    let before = &masked[..arm_offset];
    let mut depth = 0usize;
    let mut brace = None;
    for (index, byte) in before.bytes().enumerate().rev() {
        match byte {
            b'}' | b')' | b']' => depth += 1,
            b'{' | b'(' | b'[' => {
                if depth == 0 {
                    brace = (byte == b'{').then_some(index);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let brace = brace?;
    let head = &before[..brace];
    let keyword = *whole_word_offsets(head, "match").last()?;
    let scrutinee = head[keyword + "match".len()..].trim();
    if scrutinee.is_empty() || scrutinee.contains(['{', '}', ';']) {
        return None;
    }
    if !(scrutinee == "*self"
        || is_identifier(scrutinee) && !is_variant_name(scrutinee)
        || integer_scrutinee(scrutinee).is_some()
        || scrutinee.split_once('.').is_some_and(|(base, field)| {
            is_identifier(base) && !is_variant_name(base) && is_identifier(field)
        }))
    {
        return None;
    }
    let earlier_patterns = earlier_arm_patterns(&masked, &source, brace + 1, arm_offset)?;
    let close = with_later_arms
        .then(|| matching_close_brace(&masked, brace))
        .flatten();
    let arms_from_changed =
        close.and_then(|close| arms_with_bodies(&masked, &source, arm_offset, close));
    let match_is_tail = close
        .and_then(|close| masked.get(close + 1..))
        .is_some_and(|rest| rest.trim() == "}");
    Some(EnclosingMatch {
        scrutinee: scrutinee.to_string(),
        earlier_patterns,
        reached: match_always_runs(&masked[..keyword]),
        arms_from_changed,
        match_is_tail,
    })
}

const INTEGER_TYPES: [&str; 12] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// `name / N` or `name % N` (decision 6): a plain lowercase name, one
/// operator and a positive integer literal. `self` and anything longer
/// are not read.
fn integer_scrutinee(scrutinee: &str) -> Option<(&str, ScrutineeArithmetic)> {
    let (name, operator, literal) = match (scrutinee.split_once('/'), scrutinee.split_once('%')) {
        (Some((name, literal)), None) => (name, '/', literal),
        (None, Some((name, literal))) => (name, '%', literal),
        _ => return None,
    };
    let name = name.trim();
    if !is_identifier(name) || is_variant_name(name) || name == "self" {
        return None;
    }
    let divisor = integer_value(literal.trim()).filter(|divisor| *divisor > 0)?;
    Some((
        name,
        if operator == '/' {
            ScrutineeArithmetic::Div(divisor)
        } else {
            ScrutineeArithmetic::Rem(divisor)
        },
    ))
}

/// Whether the test's own statement on `line` starts with an assertion and
/// stands directly in the body of a `for` whose pattern binds `name`, with
/// nothing before it in that body that may skip a row (`continue`, `break`,
/// `return`, `?`) and no block, closure or call around it. An assertion a
/// check helper lends (#6482) sits outside the test body and never reads
/// the test's table (decision 7).
fn runs_in_table_loop(test: &TestSummary, line: usize, name: &str) -> bool {
    let Some(relative) = line.checked_sub(test.start_line) else {
        return false;
    };
    let masked = mask_comments_and_strings(&test.body);
    let offset = masked
        .split_inclusive('\n')
        .take(relative)
        .map(str::len)
        .sum::<usize>();
    if relative == 0 || offset >= masked.len() {
        return false;
    }
    let statement = masked[offset..].trim_start();
    if !(statement.starts_with("assert_eq!")
        || statement.starts_with("std::assert_eq!")
        || statement.starts_with("core::assert_eq!"))
    {
        return false;
    }
    let statement_start = masked.len() - statement.len();
    whole_word_offsets(&masked[..statement_start], "for")
        .into_iter()
        .rev()
        .any(|keyword| {
            let after = &masked[keyword + "for".len()..statement_start];
            let Some(in_at) = whole_word_offsets(after, "in").first().copied() else {
                return false;
            };
            if whole_word_offsets(&after[..in_at], name).is_empty() {
                return false;
            }
            let Some(open) = masked[keyword..statement_start]
                .find('{')
                .map(|at| keyword + at)
            else {
                return false;
            };
            let Some(close) = matching_close_brace(&masked, open) else {
                return false;
            };
            if close < statement_start {
                return false;
            }
            let before = &masked[open + 1..statement_start];
            let mut depth = 0i32;
            for byte in before.bytes() {
                match byte {
                    b'{' | b'(' | b'[' => depth += 1,
                    b'}' | b')' | b']' => depth -= 1,
                    _ => {}
                }
            }
            depth == 0
                && !before.contains('?')
                && ["continue", "break", "return"]
                    .iter()
                    .all(|word| whole_word_offsets(before, word).is_empty())
                && (before.trim().is_empty() || before.trim_end().ends_with([';', '}']))
        })
}

/// The offset of the `}` that closes the `{` at `open`.
fn matching_close_brace(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (byte == b'}').then_some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// `(pattern, body)` of each arm in `masked[start..end]`, delimited like
/// `earlier_arm_patterns`. The scan must consume the range exactly.
fn arms_with_bodies(
    masked: &str,
    source: &str,
    start: usize,
    end: usize,
) -> Option<Vec<(String, String)>> {
    let bytes = masked.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = start;
    let mut index = start;
    let mut depth = 0usize;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b'=' if depth == 0 && bytes.get(index + 1) == Some(&b'>') => {
                // The last arm may end at the closing brace without a `,`.
                let body_end = match arm_expression_end(masked, index + 2, end) {
                    Some(body_end) => body_end,
                    None if !masked.get(index + 2..end)?.contains("=>") => end,
                    None => return None,
                };
                arms.push((
                    source.get(arm_start..index)?.trim().to_string(),
                    source.get(index + 2..body_end)?.trim().to_string(),
                ));
                index = body_end;
                arm_start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    (depth == 0 && masked.get(arm_start..end)?.trim().is_empty()).then_some(arms)
}

/// Whether the owner's body always reaches a `match` whose keyword ends
/// `masked_before_match`: everything between the body's opening `{` and
/// the keyword is complete `let` statements with no block, closure, branch,
/// short circuit, loop, early exit or macro, and the match's own statement
/// starts directly with it or with exactly `let <name> =`.
fn match_always_runs(masked_before_match: &str) -> bool {
    let Some(body_open) = masked_before_match.find('{') else {
        return false;
    };
    let prefix = &masked_before_match[body_open + 1..];
    let control = [
        "if", "else", "match", "loop", "while", "for", "return", "break", "continue",
    ];
    if prefix.contains(['{', '}', '?', '|', '!', '#'])
        || prefix.contains("=>")
        || prefix.contains("&&")
        || control
            .iter()
            .any(|keyword| !whole_word_offsets(prefix, keyword).is_empty())
    {
        return false;
    }
    let (statements, head) = prefix.rsplit_once(';').unwrap_or(("", prefix));
    let head = head.trim();
    let head_runs = head.is_empty()
        || head
            .strip_prefix("let ")
            .and_then(|binding| binding.strip_suffix('='))
            .is_some_and(|pattern| {
                let pattern = pattern.split_once(':').map_or(pattern, |(name, _)| name);
                let pattern = pattern.trim();
                is_identifier(pattern.strip_prefix("mut ").unwrap_or(pattern).trim())
            });
    head_runs
        && statements
            .split(';')
            .map(str::trim)
            .all(|statement| statement.is_empty() || statement.starts_with("let "))
}

/// Arm patterns in `masked[start..end]`, the part of a match body before
/// the changed arm. Each arm is `pattern => expression` ended by a
/// top-level `,`, or `pattern => { block }` with an optional `,`. The scan
/// must end exactly at an arm boundary, or the changed line does not start
/// an arm of this match.
fn earlier_arm_patterns(
    masked: &str,
    source: &str,
    start: usize,
    end: usize,
) -> Option<Vec<String>> {
    let bytes = masked.as_bytes();
    let mut patterns = Vec::new();
    let mut arm_start = start;
    let mut index = start;
    let mut depth = 0usize;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b'=' if depth == 0 && bytes.get(index + 1) == Some(&b'>') => {
                patterns.push(source.get(arm_start..index)?.trim().to_string());
                index = arm_expression_end(masked, index + 2, end)?;
                arm_start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    (depth == 0 && masked.get(arm_start..end)?.trim().is_empty()).then_some(patterns)
}

/// The offset just past an arm expression that starts at `from`: past its
/// block (and an optional `,`) or past its top-level `,`.
fn arm_expression_end(masked: &str, from: usize, end: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut index = from;
    while index < end && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let block = bytes.get(index) == Some(&b'{');
    let mut depth = 0usize;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if block && depth == 0 {
                    let mut next = index + 1;
                    while next < end && bytes[next].is_ascii_whitespace() {
                        next += 1;
                    }
                    if bytes.get(next) == Some(&b',') {
                        next += 1;
                    }
                    return Some(next);
                }
            }
            b',' if depth == 0 => return Some(index + 1),
            // A block-like body (`match`, `if .. else`, `unsafe`, a labeled
            // block) may end without a comma; a second `=>` before the
            // comma means the next arm was swallowed.
            b'=' if depth == 0 && !block && bytes.get(index + 1) == Some(&b'>') => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

/// The type segment before the variant in a qualified input path
/// (`Mode` in `Mode::Hot`, `Mode::Hot(1)`), or `None` when unqualified.
fn input_qualifier(input: &str) -> Option<&str> {
    let input = input.trim();
    let path_end = input
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(input.len());
    let mut segments = input[..path_end].rsplit("::");
    segments.next()?;
    segments.next()
}

/// The first segment of a qualified input path with segments above its
/// type (`a` in `a::Mode::Hot`), or `None` for `Mode::Hot` and `Hot`. A
/// leading `::` roots the path at an extern crate, read as an empty root.
fn input_path_root(input: &str) -> Option<&str> {
    let input = input.trim();
    let path_end = input
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(input.len());
    let segments = input[..path_end].split("::").collect::<Vec<_>>();
    (segments.len() > 2).then(|| segments[0])
}

struct ParameterDeclaration {
    mutable: bool,
    type_name: Option<String>,
    /// Whether the type is written as a path (`other::Kind`).
    qualified: bool,
}

/// How `name` is declared in the owner's one-line signature: whether it is
/// `mut name` or `&mut T`, and the last path segment of its type with
/// references and generic arguments dropped (`Option` for `Option<i32>`).
fn parameter_declaration(owner: &FunctionSummary, name: &str) -> Option<ParameterDeclaration> {
    let signature = owner.body.lines().next()?;
    let open = signature.find('(')?;
    let close = matching_parenthesis(signature, open)?;
    let entries = split_top_level_arguments(&signature[open + 1..close])?;
    let entry = entries.into_iter().find(|entry| {
        entry
            .split_once(':')
            .is_some_and(|(pattern, _)| parameter_name(pattern) == name)
    })?;
    let (pattern, ty) = entry.split_once(':')?;
    let ty = ty.trim();
    let mutable = pattern.trim().starts_with("mut ")
        || ty.starts_with("&mut")
        || ty.starts_with("&'") && ty.contains(" mut ");
    let base = ty.trim_start_matches('&');
    let base = match base.strip_prefix('\'') {
        Some(rest) => rest.split_once(' ').map_or("", |(_, rest)| rest),
        None => base,
    };
    let base = base.trim().strip_prefix("mut ").unwrap_or(base.trim());
    let base = base.split('<').next().unwrap_or(base).trim();
    let type_name = base
        .rsplit("::")
        .next()
        .filter(|name| is_identifier(name))
        .map(str::to_string);
    Some(ParameterDeclaration {
        mutable,
        type_name,
        qualified: base.contains("::"),
    })
}

/// The owner's body after its signature line, masked.
fn owner_body_after_signature(owner: &FunctionSummary) -> String {
    let masked = mask_comments_and_strings(&owner.body);
    masked
        .split_once('\n')
        .map_or(String::new(), |(_, rest)| rest.to_string())
}

/// Whether the parameter may hold something other than the caller's
/// argument when the match reads it. The whole body is read, not only the
/// part before the arm, because a loop can reassign it after the match. A
/// binding position (`let x`, `Some(x) =`, `for x in`, `|x|`, `W { f: x }`),
/// an assignment, or, for a `mut`/`&mut` parameter, any method call or
/// further use refuses. Plain reads of an immutable parameter do not. A use
/// inside call arguments (`f(x)`) also refuses: telling it from a pattern
/// position needs a parser.
fn parameter_may_change(owner: &FunctionSummary, name: &str, mutable: bool) -> bool {
    let body = owner_body_after_signature(owner);
    whole_word_offsets(&body, name).into_iter().any(|offset| {
        let prefix = body[..offset].trim_end();
        let suffix = body[offset + name.len()..].trim_start();
        if prefix.ends_with("match") && suffix.starts_with('{') {
            return false;
        }
        mutable
            || [
                "let", "mut", "ref", "for", "|", "(", ",", "{", "@", "[", "&",
            ]
            .iter()
            .any(|marker| prefix.ends_with(marker))
            || (prefix.ends_with(':') && !prefix.ends_with("::"))
            || (suffix.starts_with('=') && !suffix.starts_with("=="))
            || [
                "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>=", "@", "in ",
            ]
            .iter()
            .any(|marker| suffix.starts_with(marker))
            || (suffix.starts_with(':') && !suffix.starts_with("::"))
    })
}

/// Whether a `mut self`/`&mut self`/`self: &mut Self`/`self: Pin<&mut
/// Self>` method may change `self` before or between reads of the match:
/// any use of `self` other than the match's own scrutinee refuses. An
/// immutable receiver cannot be reassigned. The signature is read up to the
/// body's `{`, so a receiver on a later signature line is still seen.
fn receiver_may_change(owner: &FunctionSummary) -> bool {
    let masked = mask_comments_and_strings(&owner.body);
    let masked_signature = masked.split('{').next().unwrap_or_default();
    let mutable = whole_word_offsets(masked_signature, "self")
        .into_iter()
        .any(|offset| {
            if masked_signature[..offset].trim_end().ends_with("mut") {
                return true;
            }
            // A typed receiver: `mut` anywhere in its type (`&mut Self`,
            // `Pin<&mut Self>`) lets the body change the scrutinee.
            masked_signature[offset + "self".len()..]
                .trim_start()
                .strip_prefix(':')
                .is_some_and(|rest| {
                    let receiver_type = rest.split([',', ')']).next().unwrap_or_default();
                    !whole_word_offsets(receiver_type, "mut").is_empty()
                })
        });
    if !mutable {
        return false;
    }
    let body = owner_body_after_signature(owner);
    whole_word_offsets(&body, "self").into_iter().any(|offset| {
        let prefix = body[..offset].trim_end().trim_end_matches('*').trim_end();
        let suffix = body[offset + "self".len()..].trim_start();
        !(prefix.ends_with("match") && suffix.starts_with('{'))
    })
}

/// One direct owner call: `owner(..)`, `Path::owner(..)` or
/// `<receiver>.owner(..)`, with its byte span in the scanned text.
struct OwnerCall<'a> {
    span: (usize, usize),
    receiver: Option<&'a str>,
    arguments: Vec<&'a str>,
}

/// Owner calls in one line or operand. A method owner is read only through
/// `<receiver>.owner(..)` with a plain path or literal receiver; a free
/// owner only through `owner(..)` or `Path::owner(..)` whose first segment
/// is one of `local_roots`. Calls of the other form, through another root,
/// or whose receiver is computed, are left out, so a caller that counts
/// mentions sees the gap.
fn owner_calls_in<'a>(
    text: &'a str,
    owner: &str,
    method: bool,
    local_roots: &[String],
) -> Vec<OwnerCall<'a>> {
    let masked = mask_comments_and_strings(text);
    let mut calls = Vec::new();
    for offset in whole_word_offsets(&masked, owner) {
        let after = offset + owner.len();
        let open = after + (masked[after..].len() - masked[after..].trim_start().len());
        if masked.as_bytes().get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_parenthesis(text, open) else {
            continue;
        };
        let Some(arguments) = split_top_level_arguments(&text[open + 1..close]) else {
            continue;
        };
        let arguments = arguments
            .into_iter()
            .filter(|argument| !argument.is_empty())
            .collect::<Vec<_>>();
        let prefix = masked[..offset].trim_end();
        if method {
            let Some(receiver_end) = prefix.strip_suffix('.').map(str::len) else {
                continue;
            };
            let receiver_start = path_start(&masked[..receiver_end]);
            // Masking keeps byte length but not character boundaries inside
            // strings and comments, so the original text is sliced checked.
            let Some(receiver) = text.get(receiver_start..receiver_end).map(str::trim) else {
                continue;
            };
            let preceding = masked[..receiver_start].trim_end();
            if receiver.is_empty() || preceding.ends_with(['.', ')', ']', '?']) {
                continue;
            }
            calls.push(OwnerCall {
                span: (receiver_start, close + 1),
                receiver: Some(receiver),
                arguments,
            });
        } else {
            if prefix.ends_with('.') {
                continue;
            }
            let start = if prefix.ends_with("::") {
                let start = path_start(&masked[..prefix.len()]);
                let root = masked[start..offset].split("::").next().unwrap_or_default();
                if !local_roots.iter().any(|local| local == root) {
                    continue;
                }
                start
            } else {
                offset
            };
            calls.push(OwnerCall {
                span: (start, close + 1),
                receiver: None,
                arguments,
            });
        }
    }
    calls
}

/// Where the ASCII path (`a::B`) that ends `text` starts: just past the last
/// character outside `[A-Za-z0-9_:]`, on a character boundary.
fn path_start(text: &str) -> usize {
    text.char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_' || *ch == ':'))
        .map_or(0, |(index, ch)| index + ch.len_utf8())
}

fn whole_word_offsets(text: &str, word: &str) -> Vec<usize> {
    if word.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    text.match_indices(word)
        .map(|(offset, _)| offset)
        .filter(|offset| {
            let before = offset.checked_sub(1).and_then(|index| bytes.get(index));
            let after = bytes.get(offset + word.len());
            !before.is_some_and(|byte| is_word(*byte)) && !after.is_some_and(|byte| is_word(*byte))
        })
        .collect()
}

/// The distinct line starts of the lines holding `offsets`, in order.
fn line_starts_of(text: &str, offsets: &[usize]) -> Vec<usize> {
    let mut starts = offsets
        .iter()
        .map(|offset| text[..*offset].rfind('\n').map_or(0, |index| index + 1))
        .collect::<Vec<_>>();
    starts.dedup();
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation, SymbolId};

    fn owner(body: &str, name: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.to_string().into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn arm_probe(expression: &str, line: usize) -> Probe {
        Probe {
            id: ProbeId(format!("probe:src_lib_rs:{line}:match_arm")),
            location: SourceLocation::new("src/lib.rs", line, 1),
            owner: Some(SymbolId("src/lib.rs::reason".to_string())),
            family: ProbeFamily::MatchArm,
            delta: DeltaKind::Value,
            before: None,
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn test_with(body: &str) -> TestSummary {
        TestSummary {
            name: "t".to_string(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.to_string().into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    #[test]
    #[cfg(feature = "lang-rust")]
    fn field_selector_limit_refuses_ambiguous_or_computed_subjects() {
        let valid = "fn reason(lease: &Lease) -> u64 {\n    match lease.elapsed {\n        Some(ms) => ms,\n        None => 600,\n    }\n}";
        let owner = self::owner(valid, "reason");
        for line in [2, 3, 4] {
            assert_eq!(
                unresolved_parameter_field_selector(&arm_probe("", line), &owner).as_deref(),
                Some("lease.elapsed")
            );
            // Establishing syntax alone never supplies field value evidence.
            if let Some(selector) = ArmSelector::establish(&arm_probe("None => 600,", line), &owner)
            {
                assert!(
                    !selector
                        .assertion_selects("assert_eq!(reason(&Lease { elapsed: None }), 600)")
                );
            }
        }
        assert!(unresolved_parameter_field_selector(&arm_probe("", 5), &owner).is_none());
        let mut wrong_owner = arm_probe("", 3);
        wrong_owner.owner = None;
        assert!(unresolved_parameter_field_selector(&wrong_owner, &owner).is_none());
        for unsupported in [
            valid.replace("lease.elapsed", "lease.elapsed.as_ref()"),
            valid.replace("lease.elapsed", "lease.inner.elapsed"),
            valid.replace("lease.elapsed", "lease.0"),
            valid.replace("match lease.elapsed", "let lease = other();\n    match lease.elapsed"),
            valid.replace("match lease.elapsed", "if ready() { return 1; }\n    match lease.elapsed"),
            valid.replace("Some(ms) =>", "Some(ms) if ms > 0 =>"),
            valid.replace("Some(ms) => ms,", "Some(ms) => match ms.field { A => 1, B => 2 },"),
            valid.replace("Some(ms) => ms,", "Some(ms) => (|| ms)(),"),
            valid.replace("    match lease.elapsed {", "    (|| match lease.elapsed {").replace("    }\n}", "    })()\n}"),
            valid.replace("lease.elapsed", "unknown.elapsed"),
            "fn reason(lease: &Lease) -> u64 { match lease.elapsed { Some(ms) => ms, None => 600 } }".to_string(),
            "fn reason(lease: &Lease) -> u64 {\n match lease.elapsed { Some(ms) => ms, None => 600 }\n}".to_string(),
        ] {
            let owner = self::owner(&unsupported, "reason");
            for line in 1..=unsupported.lines().count() {
                assert!(unresolved_parameter_field_selector(&arm_probe("", line), &owner).is_none(), "unexpected selector on line {line}: {unsupported}");
            }
        }
    }

    #[test]
    #[cfg(feature = "lang-rust")]
    fn a_field_selector_cannot_retain_admission_from_an_older_workspace() -> Result<(), String> {
        use crate::analysis::rust_index::{FileFacts, RustIndex};
        let body = "fn reason(lease: &Lease) -> u64 {\n    match lease.elapsed {\n        Some(ms) => ms,\n        None => 600,\n    }\n}";
        let mut owner = self::owner(body, "reason");
        owner.start_line = 2;
        owner.end_line += 1;
        let selector = ArmSelector::establish(&arm_probe("None => 600,", 5), &owner)
            .ok_or("the direct field selector must establish")?;
        let index = |fallback| {
            let mut index = RustIndex::default();
            index.insert_file(
                "src/lib.rs".into(),
                FileFacts {
                    path: "src/lib.rs".into(),
                    source: format!("struct Lease {{ elapsed: Option<u64> }}\n{body}").into(),
                    used_lexical_fallback: fallback,
                    ..FileFacts::default()
                },
                true,
            );
            index
        };
        let assertion = "assert_eq!(reason(&Lease { elapsed: None }), 600)";
        let valid = selector.in_workspace(&index(false), std::iter::empty());
        assert!(valid.assertion_selects(assertion));
        for stale in [&RustIndex::default(), &index(true)] {
            let rebound = valid.clone().in_workspace(stale, std::iter::empty());
            assert!(!rebound.assertion_selects(assertion));
        }
        let imported = std::path::Path::new("tests/external.rs");
        let rebound = valid.in_workspace(&index(false), [imported]);
        assert!(!rebound.assertion_selects(assertion));
        assert!(
            rebound
                .in_workspace(&index(false), std::iter::empty())
                .assertion_selects(assertion)
        );
        Ok(())
    }

    const REASON: &str = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";

    fn reason_selector() -> Result<ArmSelector, String> {
        ArmSelector::establish(&arm_probe("None => 0,", 4), &owner(REASON, "reason"))
            .ok_or_else(|| "premise: the None arm reads its scrutinee from x".to_string())
    }

    fn oracle(line: usize, text: &str) -> crate::analysis::facts::OracleFact {
        crate::analysis::facts::OracleFact {
            line,
            text: text.to_string(),
            kind: crate::domain::OracleKind::ExactErrorVariant,
            strength: crate::domain::OracleStrength::Strong,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        }
    }

    /// #6616: an admitted exact pattern pin selects the trailing `_` arm
    /// only through an input that misses every sibling literal; a pin the
    /// admission refused selects nothing.
    #[test]
    fn an_admitted_pattern_pin_selects_the_wildcard_arm_by_its_input() -> Result<(), String> {
        let units = "pub fn parse_unit(raw: &str) -> Result<u32, UnitError> {\n    match raw {\n        \"\" => Err(UnitError::Blank),\n        \"k\" => Ok(1_000),\n        _ => Err(UnitError::Unknown),\n    }\n}\n";
        let mut probe = arm_probe("_ => Err(UnitError::Blank),", 5);
        probe.before = Some("_ => Err(UnitError::Unknown),".to_string());
        let mut test = test_with(
            "fn t() {\n    assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));\n    assert_matches!(parse_unit(\"k\"), Ok(1_000));\n    assert_matches!(parse_unit(\"\"), Err(UnitError::Blank));\n    assert_matches!(parse_unit(\"z\"), Err(UnitError::Unknown));\n}\n",
        );
        test.assertions = vec![
            oracle(
                2,
                "assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            ),
            oracle(3, "assert_matches!(parse_unit(\"k\"), Ok(1_000));"),
            oracle(
                4,
                "assert_matches!(parse_unit(\"\"), Err(UnitError::Blank));",
            ),
            oracle(
                5,
                "assert_matches!(parse_unit(\"z\"), Err(UnitError::Unknown));",
            ),
        ];
        let selector = ArmSelector::establish(&probe, &owner(units, "parse_unit"))
            .ok_or_else(|| "premise: the `_` arm reads its scrutinee from raw".to_string())?
            .with_pattern_pins([&test], |_, assertion| {
                // The admission refused line 5 (say, after a `return;`).
                (assertion.line != 5).then(|| {
                    let open = assertion.text.find('(').unwrap_or(0) + 1;
                    let close = assertion.text.find(", ").unwrap_or(open);
                    assertion.text[open..close].to_string()
                })
            });
        let selects = |index: usize| selector.pattern_pin_selects(&test, &test.assertions[index]);
        assert!(selects(0), "\"q\" misses every sibling literal");
        assert!(!selects(1), "\"k\" selects a sibling arm");
        assert!(!selects(2), "\"\" selects a sibling arm");
        assert!(!selects(3), "a refused pin selects nothing");
        // The text-only reader never reads a pattern pin as a comparison.
        assert!(!selector.assertion_selects(&test.assertions[0].text));
        Ok(())
    }

    #[test]
    fn sibling_variant_input_selects_other_arm() -> Result<(), String> {
        let selector = reason_selector()?;
        let observed = selector
            .observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n}\n",
            ))
            .ok_or_else(|| "premise: the owner call is readable".to_string())?;
        assert_eq!(observed.inputs, vec!["Some(5)".to_string()]);
        assert_eq!(observed.selection, ArmSelection::SelectsOther);
        Ok(())
    }

    #[test]
    fn matching_input_selects_and_any_selecting_call_wins() -> Result<(), String> {
        let selector = reason_selector()?;
        let observed = selector
            .observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n    assert_eq!(reason(None), 0);\n}\n",
            ))
            .ok_or_else(|| "premise: both owner calls are readable".to_string())?;
        assert_eq!(observed.selection, ArmSelection::Selects);
        assert!(selector.assertion_selects("assert_eq!(reason(None), 0);"));
        assert!(!selector.assertion_selects("assert_eq!(reason(Some(5)), 6);"));
        // A variant named only on the expected side is not an input.
        assert!(!selector.assertion_selects("assert_eq!(reason(Some(5)), None);"));
        Ok(())
    }

    #[test]
    fn opaque_or_unreadable_inputs_never_select_other() -> Result<(), String> {
        let selector = reason_selector()?;
        let variable = selector
            .observed_inputs(&test_with(
                "fn t() {\n    let x = Some(5);\n    assert_eq!(reason(x), 6);\n}\n",
            ))
            .ok_or_else(|| "premise: the call is readable".to_string())?;
        assert_eq!(variable.selection, ArmSelection::Unknown);
        // A function pointer may be called with any input.
        assert_eq!(
            selector.observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n    let _ = [None].map(reason);\n}\n",
            )),
            None
        );
        // A call split across lines is not read.
        assert_eq!(
            selector.observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(\n        Some(5)), 6);\n}\n",
            )),
            None
        );
        // No owner call at all: the test may reach the arm another way.
        assert_eq!(
            selector.observed_inputs(&test_with("fn t() {\n    assert_eq!(other(1), 2);\n}\n")),
            None
        );
        Ok(())
    }

    #[test]
    fn receiver_scrutinee_reads_glob_imported_variants() -> Result<(), String> {
        let body = "pub fn apply_to_variant(self, variant: &str) -> String {\n    match self {\n        None | PascalCase => variant.to_owned(),\n        LowerCase => str::to_ascii_lowercase(variant),\n        UpperCase => variant.to_ascii_uppercase(),\n    }\n}\n";
        let mut apply = owner(body, "apply_to_variant");
        apply.impl_context = crate::analysis::facts::FunctionImplContext::Impl {
            self_type: "RenameRule".to_string(),
        };
        let selector = ArmSelector::establish(
            &arm_probe("LowerCase => str::to_ascii_lowercase(variant),", 4),
            &apply,
        )
        .ok_or_else(|| "premise: the arm reads its scrutinee from self".to_string())?;
        assert!(
            selector.assertion_selects("assert_eq!(LowerCase.apply_to_variant(original), lower);")
        );
        assert!(selector.assertion_selects(
            "assert_eq!(RenameRule::LowerCase.apply_to_variant(original), lower);"
        ));
        assert!(
            !selector.assertion_selects("assert_eq!(UpperCase.apply_to_variant(original), upper);")
        );
        // A same-named variant of another enum is not this scrutinee's input.
        assert!(!selector.assertion_selects(
            "assert_eq!(OtherRule::LowerCase.apply_to_variant(original), lower);"
        ));
        // A computed receiver is not an input this module reads.
        assert!(
            !selector.assertion_selects("assert_eq!(rule().apply_to_variant(original), lower);")
        );
        // A same-named type outside the workspace is another type.
        assert!(!selector.assertion_selects(
            "assert_eq!(other_crate::RenameRule::LowerCase.apply_to_variant(original), lower);"
        ));
        assert!(!selector.assertion_selects(
            "assert_eq!(::other_crate::RenameRule::LowerCase.apply_to_variant(original), lower);"
        ));
        assert!(selector.assertion_selects(
            "assert_eq!(crate::RenameRule::LowerCase.apply_to_variant(original), lower);"
        ));
        Ok(())
    }

    #[test]
    fn an_inequality_never_selects() -> Result<(), String> {
        let selector = reason_selector()?;
        assert!(selector.assertion_selects("assert_eq!(reason(None), 0);"));
        // `None => 0` changed to `None => 1` passes `!= 2` either way.
        assert!(!selector.assertion_selects("assert_ne!(reason(None), 2);"));
        assert!(!selector.assertion_selects("debug_assert_ne!(reason(None), 2);"));
        assert!(!selector.assertion_selects("prop_assert_ne!(reason(None), 2);"));
        assert!(selector.assertion_selects("debug_assert_eq!(reason(None), 0);"));
        // A bound owner result may stand on the expected side.
        assert!(selector.binds_owner_result(&test_with(
            "fn t() {\n    let expected = reason(None);\n    assert_eq!(reason(None), expected);\n}\n"
        )));
        assert!(!selector.binds_owner_result(&test_with(
            "fn t() {\n    let expected = 0;\n    assert_eq!(reason(None), expected);\n}\n"
        )));
        // Both sides run the arm, so the comparison holds whatever it yields.
        assert!(!selector.assertion_selects("assert_eq!(reason(None), (reason(None)));"));
        assert!(!selector.assertion_selects("assert_eq!(reason(None), reason(None) + 0);"));
        Ok(())
    }

    #[test]
    fn a_bare_variant_imported_from_another_enum_is_not_read() -> Result<(), String> {
        let body = "pub fn apply_to_variant(self, variant: &str) -> String {\n    match self {\n        None | PascalCase => variant.to_owned(),\n        LowerCase => str::to_ascii_lowercase(variant),\n        UpperCase => variant.to_ascii_uppercase(),\n    }\n}\n";
        let mut apply = owner(body, "apply_to_variant");
        apply.impl_context = crate::analysis::facts::FunctionImplContext::Impl {
            self_type: "RenameRule".to_string(),
        };
        let mut selector = ArmSelector::establish(
            &arm_probe("LowerCase => str::to_ascii_lowercase(variant),", 4),
            &apply,
        )
        .ok_or_else(|| "premise: the arm reads its scrutinee from self".to_string())?;
        let test = test_with(
            "fn t() {\n    assert_eq!(LowerCase.apply_to_variant(original), lower);\n}\n",
        );
        let with_imports = |selector: &mut ArmSelector, source: &str| {
            selector.test_imports = Some(
                [(test.file.clone(), flattened_use_paths(source))]
                    .into_iter()
                    .collect(),
            );
        };
        with_imports(&mut selector, "use crate::RenameRule::*;\nuse super::*;\n");
        assert_eq!(
            selector
                .observed_inputs(&test)
                .map(|observed| observed.selection),
            Some(ArmSelection::Selects)
        );
        with_imports(
            &mut selector,
            "use crate::{RenameRule::{LowerCase, UpperCase}};\n",
        );
        assert_eq!(
            selector
                .observed_inputs(&test)
                .map(|observed| observed.selection),
            Some(ArmSelection::Selects)
        );
        for foreign in [
            "use other_crate::OtherRule::LowerCase;\n",
            "use other_crate::{OtherRule::{LowerCase}};\n",
            "use other_crate::OtherRule::*;\n",
            "use crate::RenameRule::UpperCase as LowerCase;\n",
            "use other_crate::RenameRule::LowerCase;\n",
            "use other_crate::RenameRule::*;\n",
        ] {
            with_imports(&mut selector, foreign);
            assert_eq!(selector.observed_inputs(&test), None, "{foreign}");
        }
        // A test file whose imports were not read is ambiguous.
        selector.test_imports = Some(Default::default());
        assert_eq!(selector.observed_inputs(&test), None);
        // A same-named type imported from outside the workspace is another type.
        let qualified = test_with(
            "fn t() {\n    assert_eq!(RenameRule::LowerCase.apply_to_variant(original), lower);\n}\n",
        );
        let with_qualified_imports = |selector: &mut ArmSelector, source: &str| {
            selector.test_imports = Some(
                [(qualified.file.clone(), flattened_use_paths(source))]
                    .into_iter()
                    .collect(),
            );
        };
        with_qualified_imports(&mut selector, "use other_crate::RenameRule;\n");
        assert_eq!(selector.observed_inputs(&qualified), None);
        with_qualified_imports(&mut selector, "use crate::RenameRule;\n");
        assert_eq!(
            selector
                .observed_inputs(&qualified)
                .map(|observed| observed.selection),
            Some(ArmSelection::Selects)
        );
        Ok(())
    }

    #[test]
    fn a_lowercase_name_the_owner_file_may_resolve_to_a_constant_is_not_a_binding()
    -> Result<(), String> {
        let body = "pub fn pick(n: u8) -> u8 {\n    match n {\n        target => 10,\n    }\n}\n";
        let selector = ArmSelector::establish(&arm_probe("target => 10,", 3), &owner(body, "pick"))
            .ok_or_else(|| "premise: the arm reads its scrutinee from n".to_string())?;
        let test = test_with("fn t() {\n    assert_eq!(pick(2), 10);\n}\n");
        let judged = |source: Option<&str>| {
            let mut selector = selector.clone();
            selector.demote_constant_bindings(source);
            selector
                .observed_inputs(&test)
                .map(|observed| observed.selection)
        };
        assert_eq!(judged(Some(body)), Some(ArmSelection::Selects));
        for constant in [
            "const target: u8 = 1;\n",
            "static target: u8 = 1;\n",
            "use crate::limits::target;\n",
            "use crate::limits::*;\n",
        ] {
            let source = format!("{constant}{body}");
            assert_eq!(
                judged(Some(&source)),
                Some(ArmSelection::Unknown),
                "{constant}"
            );
        }
        // An enum glob brings in CamelCase variants only.
        assert_eq!(
            judged(Some(&format!("use Rule::*;\n{body}"))),
            Some(ArmSelection::Selects)
        );
        // An item macro may declare the constant (#7863).
        for macro_item in [
            "macro_rules! def_const { ($n:ident) => { const $n: u8 = 0; }; }\ndef_const!(target);\n",
            "def_const!(target);\n",
        ] {
            let source = format!("{macro_item}{body}");
            assert_eq!(
                judged(Some(&source)),
                Some(ArmSelection::Unknown),
                "{macro_item}"
            );
        }
        // An owner file that was not read may declare anything.
        assert_eq!(judged(None), Some(ArmSelection::Unknown));
        Ok(())
    }

    fn resolve_with(selector: &ArmSelector, source: Option<&str>) -> ArmSelector {
        let mut selector = selector.clone();
        let declared =
            source
                .zip(selector.bare_scrutinee_type.clone())
                .and_then(|(source, type_name)| {
                    declared_enum_variants(source, &type_name, &selector.owner)
                });
        selector.resolve_uncased_variants(declared);
        selector
    }

    fn uncased_selection_of(selector: &ArmSelector, owner_call: &str) -> Option<ArmSelection> {
        selector
            .observed_inputs(&test_with(&format!(
                "fn t() {{\n    assert_eq!({owner_call}, 0);\n}}\n"
            )))
            .map(|observed| observed.selection)
    }

    const KIND: &str = "pub fn f(k: Kind) -> i32 {\n    match k {\n        Kind::A => 1,\n        _ => 0,\n    }\n}\n";

    /// #7670: an input that moved from the changed arm to a later uncased
    /// arm earns moved-input credit once that later arm reads as the
    /// declared variant, and none while `Kind` is undeclared.
    #[test]
    fn a_later_uncased_arm_resolves_for_moved_input_credit() -> Result<(), String> {
        let body = "pub fn f(k: Kind) -> i32 {\n    match k {\n        Kind::A => 1,\n        Kind::B => 2,\n        _ => 0,\n    }\n}\n";
        let mut narrowed = arm_probe("Kind::A => 1,", 3);
        narrowed.before = Some("Kind::A | Kind::B =>".to_string());
        let selector = ArmSelector::establish(&narrowed, &owner(body, "f"))
            .ok_or_else(|| "premise: the narrowed arm is readable".to_string())?;
        let declared = format!("pub enum Kind {{\n    A,\n    B,\n    C,\n}}\n{body}");
        let resolved = resolve_with(&selector, Some(&declared));
        assert!(resolved.assertion_selects("assert_eq!(f(Kind::B), 2);"));
        // An input that stayed in this arm earns nothing from moving.
        assert!(!resolved.assertion_selects("assert_eq!(f(Kind::C), 0);"));
        let undeclared = resolve_with(&selector, Some(body));
        assert!(!undeclared.assertion_selects("assert_eq!(f(Kind::B), 2);"));
        Ok(())
    }

    #[test]
    fn uncased_variants_read_only_when_the_owner_file_declares_the_enum() -> Result<(), String> {
        let selector = ArmSelector::establish(&arm_probe("_ => 0,", 4), &owner(KIND, "f"))
            .ok_or_else(|| "premise: the arm reads its scrutinee from k".to_string())?;
        let judged = |source: Option<&str>, input: &str| {
            uncased_selection_of(&resolve_with(&selector, source), &format!("f({input})"))
        };
        let declared = format!("pub enum Kind {{\n    A,\n    B,\n}}\n{KIND}");
        assert_eq!(
            judged(Some(&declared), "Kind::A"),
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            judged(Some(&declared), "Kind::B"),
            Some(ArmSelection::Selects)
        );
        // Every source below declares `enum Kind { A, .. }` too, except the
        // first two, so each refusal is what keeps `Kind::A` unreadable: it
        // may be a constant, which a pattern compares by value.
        for source in [
            None,
            Some(KIND.to_string()),
            Some(format!(
                "pub enum Kind {{ A, B }}\npub struct Kind(u8);\n{KIND}"
            )),
            Some(format!(
                "pub enum Kind {{ A, B }}\npub union Kind {{ a: u8 }}\n{KIND}"
            )),
            Some(format!(
                "pub enum Kind {{ A, B }}\npub trait Kind {{}}\n{KIND}"
            )),
            Some(format!("enum Kind {{ A, B }}\ntype Kind = u8;\n{KIND}")),
            Some(format!("use other::Kind;\nenum Kind {{ A, B }}\n{KIND}")),
            Some(format!(
                "use other::Thing as Kind;\nenum Kind {{ A, B }}\n{KIND}"
            )),
            Some(format!(
                "use other::Kind::{{self}};\nenum Kind {{ A, B }}\n{KIND}"
            )),
            Some(format!(
                "use crate::flags::*;\nenum Kind {{ A, B }}\n{KIND}"
            )),
            Some(format!(
                "bitflags! {{ struct Kind: u8 {{ const A = 1; }} }}\nmod legacy {{\n    pub enum Kind {{ A, B }}\n}}\n{KIND}"
            )),
            Some(format!(
                "mod a {{\n    enum Kind {{ A }}\n}}\nmod b {{\n    enum Kind {{ A, B }}\n}}\n{KIND}"
            )),
            Some(format!("enum Kind {{ Alpha, B }}\n{KIND}")),
        ] {
            assert_eq!(
                judged(source.as_deref(), "Kind::A"),
                Some(ArmSelection::Unknown),
                "{source:?}"
            );
        }
        // A file-root `use super::*` brings in another file's items.
        assert_eq!(
            judged(Some(&format!("use super::*;\n{declared}")), "Kind::A"),
            Some(ArmSelection::Unknown)
        );
        // In an inline test module it brings in only this file's items.
        let inline_tests = format!("{declared}#[cfg(test)]\nmod tests {{\n    use super::*;\n}}\n");
        assert_eq!(
            judged(Some(&inline_tests), "Kind::A"),
            Some(ArmSelection::SelectsOther)
        );
        // A variant glob of the enum itself binds no type name.
        assert_eq!(
            judged(Some(&format!("use Kind::*;\n{declared}")), "Kind::A"),
            Some(ArmSelection::SelectsOther)
        );
        // Another type's uncased name is never this enum's variant.
        assert_eq!(
            judged(Some(&declared), "Other::A"),
            Some(ArmSelection::Unknown)
        );
        Ok(())
    }

    #[test]
    fn an_uncased_pattern_reads_only_through_the_bare_scrutinee_type() -> Result<(), String> {
        let declared = "pub enum Method {\n    GET,\n    POST,\n    PUT,\n}\n";
        let judged = |arms: &str, input: &str| -> Result<Option<ArmSelection>, String> {
            let body = format!(
                "pub fn f(m: Method) -> i32 {{\n    match m {{\n{arms}\n        _ => 0,\n    }}\n}}\n"
            );
            let selector = ArmSelector::establish(&arm_probe("_ => 0,", 4), &owner(&body, "f"))
                .ok_or_else(|| "premise: the arm reads its scrutinee from m".to_string())?;
            let source = format!("{declared}{body}");
            Ok(uncased_selection_of(
                &resolve_with(&selector, Some(&source)),
                &format!("f({input})"),
            ))
        };
        // A multi-letter uncased variant.
        assert_eq!(
            judged("        Method::GET => 1,", "Method::GET")?,
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            judged("        Method::GET => 1,", "Method::POST")?,
            Some(ArmSelection::Selects)
        );
        // `Other::GET` and `presets::Method::GET` may be constants of other
        // types, which could equal `Method::POST`.
        for earlier in [
            "        Other::GET => 1,",
            "        presets::Method::GET => 1,",
        ] {
            assert_eq!(
                judged(earlier, "Method::POST")?,
                Some(ArmSelection::Unknown),
                "{earlier}"
            );
        }
        Ok(())
    }

    #[test]
    fn self_names_the_scrutinee_enum_only_in_its_own_impl() -> Result<(), String> {
        let judged = |impl_type: &str, body: &str, input: &str| -> Result<_, String> {
            let mut function = owner(body, "f");
            function.impl_context = crate::analysis::facts::FunctionImplContext::Impl {
                self_type: impl_type.to_string(),
            };
            let selector = ArmSelector::establish(&arm_probe("_ => 0,", 4), &function)
                .ok_or_else(|| "premise: the arm reads its scrutinee".to_string())?;
            let source = format!("pub enum Kind {{ A, B }}\nimpl {impl_type} {{\n{body}}}\n");
            Ok(uncased_selection_of(
                &resolve_with(&selector, Some(&source)),
                input,
            ))
        };
        let on_parameter = "pub fn f(k: Kind) -> i32 {\n    match k {\n        Self::A => 1,\n        _ => 0,\n    }\n}\n";
        // In `impl Router`, `Self::A` is a `Router` constant: it may equal
        // `Kind::B`, so a `Kind::B` input is not shown to reach `_`.
        assert_eq!(
            judged("Router", on_parameter, "Router::f(Kind::B)")?,
            Some(ArmSelection::Unknown)
        );
        assert_eq!(
            judged("Kind", on_parameter, "Kind::f(Kind::B)")?,
            Some(ArmSelection::Selects)
        );
        // A test-side `Self::B` names the test's own impl type, which the
        // owner's `impl Kind` says nothing about.
        assert_eq!(
            judged("Kind", on_parameter, "Kind::f(Self::B)")?,
            Some(ArmSelection::Unknown)
        );
        let on_receiver = "pub fn f(self) -> i32 {\n    match self {\n        Self::A => 1,\n        _ => 0,\n    }\n}\n";
        assert_eq!(
            judged("Kind", on_receiver, "Kind::A.f()")?,
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            judged("Kind", on_receiver, "Kind::B.f()")?,
            Some(ArmSelection::Selects)
        );
        Ok(())
    }

    #[test]
    fn uncased_variants_need_an_unqualified_scrutinee_type() -> Result<(), String> {
        let body = KIND.replace("k: Kind", "k: crate::Kind");
        let selector = ArmSelector::establish(&arm_probe("_ => 0,", 4), &owner(&body, "f"))
            .ok_or_else(|| "premise: the arm reads its scrutinee from k".to_string())?;
        let source = format!("pub enum Kind {{ A, B }}\n{body}");
        assert_eq!(
            uncased_selection_of(&resolve_with(&selector, Some(&source)), "f(Kind::B)"),
            Some(ArmSelection::Unknown)
        );
        Ok(())
    }

    #[test]
    fn a_changed_uncased_pattern_resolves_both_sides() -> Result<(), String> {
        let body = "pub fn f(k: Kind) -> i32 {\n    match k {\n        Kind::A | Kind::B => 1,\n        _ => 0,\n    }\n}\n";
        let source = format!("pub enum Kind {{ A, B, C, D }}\n{body}");
        let changed = |before: &str| -> Result<ArmSelector, String> {
            let mut probe = arm_probe("Kind::A | Kind::B => 1,", 3);
            probe.before = Some(before.to_string());
            ArmSelector::establish(&probe, &owner(body, "f"))
                .ok_or_else(|| "premise: the arm reads its scrutinee from k".to_string())
        };
        let selector = changed("Kind::A | Kind::C => 1,")?;
        let resolved = resolve_with(&selector, Some(&source));
        // Neither pattern takes `Kind::D`; the old one took `Kind::C`.
        assert_eq!(
            uncased_selection_of(&resolved, "f(Kind::D)"),
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            uncased_selection_of(&resolved, "f(Kind::C)"),
            Some(ArmSelection::Unknown)
        );
        assert_eq!(
            uncased_selection_of(&resolve_with(&selector, None), "f(Kind::D)"),
            Some(ArmSelection::Unknown)
        );
        // An original pattern that cannot be read stays unreadable.
        let unreadable = changed("Kind::A => 1, Kind::C => 1,")?;
        if unreadable.changed_from != Some(None) {
            return Err(format!(
                "premise: a two-arm old line is unreadable, got {:?}",
                unreadable.changed_from
            ));
        }
        let resolved = resolve_with(&unreadable, Some(&source));
        assert_eq!(resolved.changed_from, Some(None));
        assert_eq!(
            uncased_selection_of(&resolved, "f(Kind::D)"),
            Some(ArmSelection::Unknown)
        );
        Ok(())
    }

    #[test]
    fn a_test_file_that_may_rebind_the_type_is_refused() {
        let roots = ["crate", "self", "super", "Self", "my_crate"].map(str::to_string);
        let rebinds = |source: &str| test_file_may_rebind(source, "Kind", &roots);
        for source in [
            "struct Kind;\nimpl Kind { const A: u8 = 1; }\n",
            "pub(crate) enum Kind { A }\n",
            "type Kind = my_crate::Other;\n",
            "use my_crate::Other as Kind;\n",
            "use my_crate::presets::*;\n",
            "use other::*;\n",
        ] {
            assert!(rebinds(source), "{source}");
        }
        for source in [
            "use my_crate::{Kind, f};\nfn t() { f(Kind::A); }\n",
            "use my_crate::*;\n",
            "use super::*;\n",
            "use my_crate::Kind::*;\n",
            "struct KindHolder;\nfn kind() {}\n// struct Kind\n",
        ] {
            assert!(!rebinds(source), "{source}");
        }
    }

    #[test]
    fn integer_arms_compare_by_value() -> Result<(), String> {
        let body = "fn max_scalar_value(nbytes: usize) -> u32 {\n    match nbytes {\n        1 => 0x007F,\n        2 => 0x7FF,\n        3 => 0xFFFF,\n        _ => unreachable!(),\n    }\n}\n";
        let selector = ArmSelector::establish(
            &arm_probe("2 => 0x7FF,", 4),
            &owner(body, "max_scalar_value"),
        )
        .ok_or_else(|| "premise: the arm reads its scrutinee from nbytes".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(max_scalar_value(2), 0x07FF);"));
        assert!(selector.assertion_selects("assert_eq!(max_scalar_value(0x2usize), 2047);"));
        assert!(!selector.assertion_selects("assert_eq!(max_scalar_value(3), 0xFFFF);"));
        assert_eq!(
            selector
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(max_scalar_value(3), 0xFFFF);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        Ok(())
    }

    #[test]
    fn char_literal_arms_compare_by_value() -> Result<(), String> {
        // The arm-head reader once masked char literals as comments, so
        // `'C' | 'S' =>` read as `|` and no char arm established.
        let body = "pub fn suit_color(suit: char) -> &'static str {\n    match suit {\n        'H' | 'D' => \"red\",\n        'S' | 'C' => \"black\",\n        _ => \"unknown\",\n    }\n}\n";
        let selector = ArmSelector::establish(
            &arm_probe("'S' | 'C' => \"black\",", 4),
            &owner(body, "suit_color"),
        )
        .ok_or_else(|| "premise: the char arm reads its scrutinee from suit".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(suit_color('S'), \"black\");"));
        assert!(selector.assertion_selects("assert_eq!(suit_color('C'), \"black\");"));
        assert!(!selector.assertion_selects("assert_eq!(suit_color('H'), \"red\");"));
        Ok(())
    }

    #[test]
    fn integer_scrutinee_arithmetic_evaluates_the_input() -> Result<(), String> {
        // RIPR-SPEC-0229 decision 6: `status / 100` reads `301` as `3`.
        let body = "pub fn http_class(status: u16) -> &'static str {\n    match status / 100 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n";
        let selector = ArmSelector::establish(
            &arm_probe("3 => \"redirect\",", 4),
            &owner(body, "http_class"),
        )
        .ok_or_else(|| "premise: the arm reads status / 100".to_string())?;
        assert_eq!(selector.scrutinee(), "status");
        assert!(selector.assertion_selects("assert_eq!(http_class(301), \"redirect\");"));
        assert!(selector.assertion_selects("assert_eq!(http_class(399), \"redirect\");"));
        assert!(!selector.assertion_selects("assert_eq!(http_class(404), \"other\");"));
        assert!(!selector.assertion_selects("assert_eq!(http_class(code), \"redirect\");"));
        let remainder = "pub fn weekday(day: i64) -> &'static str {\n    match day % 7 {\n        0 => \"sun\",\n        6 => \"sat\",\n        _ => \"weekday\",\n    }\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("6 => \"sat\",", 4), &owner(remainder, "weekday"))
                .ok_or_else(|| "premise: the arm reads day % 7".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(weekday(13), \"sat\");"));
        assert!(!selector.assertion_selects("assert_eq!(weekday(14), \"sun\");"));
        // Rust's remainder keeps the dividend's sign: -1 % 7 is -1, not 6.
        assert!(!selector.assertion_selects("assert_eq!(weekday(-1), \"sat\");"));
        // Only a primitive integer parameter, one operator and a positive
        // literal divisor are read.
        for unread in [
            "pub fn http_class(status: Code) -> &'static str {\n    match status / 100 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
            "pub fn http_class(status: u16) -> &'static str {\n    match status / 0 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
            "pub fn http_class(status: u16, by: u16) -> &'static str {\n    match status / by {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
            "pub fn http_class(status: u16) -> &'static str {\n    match status / 10 / 10 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
            "pub fn http_class(status: u16) -> &'static str {\n    match status * 100 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
            "pub fn http_class(mut status: u16) -> &'static str {\n    status += 1;\n    match status / 100 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n",
        ] {
            let line = unread
                .lines()
                .position(|line| line.contains("3 =>"))
                .map_or(0, |at| at + 1);
            assert!(
                ArmSelector::establish(
                    &arm_probe("3 => \"redirect\",", line),
                    &owner(unread, "http_class")
                )
                .is_none(),
                "must not establish: {unread}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_value_equal_literal_respelling_reads_as_unchanged() -> Result<(), String> {
        let body = "pub fn http_class(status: u16) -> &'static str {\n    match status / 100 {\n        2 => \"success\",\n        0x3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n";
        let mut respelled = arm_probe("0x3 => \"redirect\",", 4);
        respelled.before = Some("3 => \"redirect\",".to_string());
        let selector = ArmSelector::establish(&respelled, &owner(body, "http_class"))
            .ok_or_else(|| "premise: the respelled arm is readable".to_string())?;
        assert_eq!(selector.changed_from, None);
        assert!(selector.assertion_selects("assert_eq!(http_class(301), \"redirect\");"));
        // A different value is a changed pattern.
        let mut changed = arm_probe("0x3 => \"redirect\",", 4);
        changed.before = Some("4 => \"redirect\",".to_string());
        let selector = ArmSelector::establish(&changed, &owner(body, "http_class"))
            .ok_or_else(|| "premise: the changed arm is readable".to_string())?;
        assert!(selector.changed_from.is_some());
        assert!(!same_literal_values("3", "3 | 4"));
        assert!(same_literal_values("1..=3 | 7", "7 | 0x1..=0x3"));
        assert!(!same_literal_values("Kind::A", "Kind::A"));
        Ok(())
    }

    #[test]
    fn a_constant_row_table_lends_its_cells_to_the_owner_call() -> Result<(), String> {
        // RIPR-SPEC-0229 decision 7.
        let body = "pub fn http_class(status: u16) -> &'static str {\n    match status / 100 {\n        2 => \"success\",\n        3 => \"redirect\",\n        _ => \"other\",\n    }\n}\n";
        let selector = ArmSelector::establish(
            &arm_probe("3 => \"redirect\",", 4),
            &owner(body, "http_class"),
        )
        .ok_or_else(|| "premise: the arm reads status / 100".to_string())?;
        let assertion = "assert_eq!(http_class(status), want, \"{status}\");";
        let selecting = test_with(
            "fn t() {\n    let cases = [(200, \"success\"), (301, \"redirect\")];\n    for (status, want) in cases {\n        assert_eq!(http_class(status), want, \"{status}\");\n    }\n}\n",
        );
        assert!(selector.table_row_selects(&selecting, assertion, 4));
        let other_rows = test_with(
            "fn t() {\n    let cases = [(200, \"success\"), (404, \"other\")];\n    for (status, want) in cases {\n        assert_eq!(http_class(status), want, \"{status}\");\n    }\n}\n",
        );
        assert!(!selector.table_row_selects(&other_rows, assertion, 4));
        // A table built at run time lends nothing.
        let computed = test_with(
            "fn t() {\n    let cases = statuses();\n    for (status, want) in cases {\n        assert_eq!(http_class(status), want, \"{status}\");\n    }\n}\n",
        );
        assert!(!selector.table_row_selects(&computed, assertion, 4));
        // `assert_ne!` passes for many arm results.
        assert!(!selector.table_row_selects(
            &selecting,
            "assert_ne!(http_class(status), want, \"{status}\");",
            4
        ));
        // Only the test's own statement directly in the loop body reads the
        // table: a borrowed helper assertion on another line, a skipped
        // row, a conditional or a closure never does.
        assert!(!selector.table_row_selects(&selecting, assertion, 7));
        for unrun in [
            "fn t() {\n    let cases = [(200, \"success\"), (301, \"redirect\")];\n    for (status, want) in cases {\n        if status == 301 { continue; }\n        assert_eq!(http_class(status), want, \"{status}\");\n    }\n}\n",
            "fn t() {\n    let cases = [(200, \"success\"), (301, \"redirect\")];\n    for (status, want) in cases {\n        if status < 300 {\n        assert_eq!(http_class(status), want, \"{status}\");\n        }\n    }\n}\n",
            "fn t() {\n    let cases = [(200, \"success\"), (301, \"redirect\")];\n    for (status, want) in cases {\n        let check = ||\n        assert_eq!(http_class(status), want, \"{status}\");\n    }\n}\n",
        ] {
            let line = unrun
                .lines()
                .position(|line| line.trim_start().starts_with("assert_eq!"))
                .map_or(0, |at| at + 1);
            assert!(
                !selector.table_row_selects(&test_with(unrun), assertion, line),
                "must not read the table: {unrun}"
            );
        }
        Ok(())
    }

    #[test]
    fn refutable_or_unprovable_patterns_stay_unknown() -> Result<(), String> {
        let body = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(0) => 9,\n        Some(v) => v + 1,\n        _ => 0,\n    }\n}\n";
        // `Some(0)` refutes on its payload: `Some(5)` is not provably selected.
        let some_zero =
            ArmSelector::establish(&arm_probe("Some(0) => 9,", 3), &owner(body, "reason"))
                .ok_or_else(|| "premise: the Some(0) arm is readable".to_string())?;
        assert!(!some_zero.assertion_selects("assert_eq!(reason(Some(0)), 9);"));
        // RIPR-SPEC-0229 decision 2: a wildcard arm is judged by first
        // match. `Some(5)` is taken by the earlier `Some(v)` arm, `None`
        // falls through to `_`.
        let wildcard = ArmSelector::establish(&arm_probe("_ => 0,", 5), &owner(body, "reason"))
            .ok_or_else(|| "premise: the wildcard arm is readable".to_string())?;
        assert_eq!(
            wildcard
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        assert!(wildcard.assertion_selects("assert_eq!(reason(None), 0);"));
        // Only a refutable earlier arm stands before `_`: whether `Some(5)`
        // reaches `_` depends on a payload this module does not compare.
        let refutable = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(0) => 9,\n        _ => 0,\n    }\n}\n";
        let wildcard =
            ArmSelector::establish(&arm_probe("_ => 0,", 4), &owner(refutable, "reason"))
                .ok_or_else(|| "premise: the wildcard arm is readable".to_string())?;
        assert_eq!(
            wildcard
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(reason(Some(5)), 0);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::Unknown)
        );
        // A guarded arm never establishes selection.
        let guarded = "pub fn reason(x: Option<i32>) -> i32 {\n    match x {\n        None if flag() => 0,\n        _ => 1,\n    }\n}\n";
        assert!(
            ArmSelector::establish(
                &arm_probe("None if flag() => 0,", 3),
                &owner(guarded, "reason")
            )
            .is_none()
        );
        // A SCREAMING name may be a constant compared by value.
        let constant = "pub fn reason(x: u8) -> u8 {\n    match x {\n        LIMIT => 0,\n        _ => 1,\n    }\n}\n";
        let limit =
            ArmSelector::establish(&arm_probe("LIMIT => 0,", 3), &owner(constant, "reason"))
                .ok_or_else(|| "premise: the LIMIT arm is readable".to_string())?;
        assert_eq!(
            limit
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(reason(OTHER), 1);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::Unknown)
        );
        Ok(())
    }

    #[test]
    fn computed_or_rebound_scrutinee_establishes_nothing() {
        let computed = "pub fn reason(x: Option<i32>) -> i32 {\n    match x.map(|v| v * 2) {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 4), &owner(computed, "reason"))
                .is_none()
        );
        let rebound = "pub fn reason(x: Option<i32>) -> i32 {\n    let x = x.filter(|v| *v > 0);\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(rebound, "reason"))
                .is_none()
        );
        let if_let = "pub fn reason(x: Option<Option<i32>>) -> i32 {\n    if let Some(x) = x {\n        return match x {\n            Some(v) => v + 1,\n            None => 0,\n        };\n    }\n    1\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(if_let, "reason")).is_none()
        );
        // A plain read before the match is not a rebinding.
        let read = "pub fn reason(x: Option<i32>) -> i32 {\n    if x.is_some() {\n        debug_log();\n    }\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("None => 0,", 7), &owner(read, "reason")).is_some()
        );
        // An arm of a nested match reads that match's scrutinee.
        let nested = "pub fn reason(x: Option<i32>, y: bool) -> i32 {\n    match y {\n        true => match x {\n            Some(v) => v,\n            None => 0,\n        },\n        false => 1,\n    }\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(nested, "reason"));
        assert_eq!(
            selector.map(|selector| selector.binding),
            Some(ScrutineeBinding::Parameter(0))
        );
    }

    #[test]
    fn a_match_the_call_may_skip_never_credits_the_arm() -> Result<(), String> {
        let skippable = [
            // Nested in another arm: `reason(None, false)` never enters it.
            (
                "pub fn reason(x: Option<i32>, y: bool) -> i32 {\n    match y {\n        true => match x {\n            Some(v) => v,\n            None => 0,\n        },\n        false => 7,\n    }\n}\n",
                5,
                "reason(None, false)",
            ),
            // A short circuit may skip the match.
            (
                "pub fn reason(x: Option<i32>, flag: bool) -> bool {\n    let ok = flag && match x {\n        Some(v) => v > 0,\n        None => true,\n    };\n    ok\n}\n",
                4,
                "reason(None, false)",
            ),
            // After an early return that `None` takes.
            (
                "pub fn reason(x: Option<i32>) -> i32 {\n    if x.is_none() {\n        return 7;\n    }\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}\n",
                7,
                "reason(None)",
            ),
            // After a `?` that may return first.
            (
                "pub fn reason(x: Option<i32>) -> Option<i32> {\n    let y = check()?;\n    match x {\n        Some(v) => Some(v + y),\n        None => Some(0),\n    }\n}\n",
                5,
                "reason(None)",
            ),
        ];
        for (body, line, call) in skippable {
            let selector =
                ArmSelector::establish(&arm_probe("None => 0,", line), &owner(body, "reason"))
                    .ok_or_else(|| format!("premise: the None arm at line {line} is readable"))?;
            assert!(
                !selector.assertion_selects(&format!("assert_eq!({call}, 7);")),
                "a match the call may skip cannot credit its arm: {body}"
            );
            // Missing the arm's pattern still misses the arm.
            let observed = selector
                .observed_inputs(&test_with(&format!(
                    "fn t() {{\n    assert_eq!({}, 7);\n}}\n",
                    call.replace("None", "Some(1)")
                )))
                .ok_or_else(|| "premise: the call is readable".to_string())?;
            assert_eq!(observed.selection, ArmSelection::SelectsOther);
        }
        // Control: `let` statements with no control flow always run first.
        let straight = "pub fn reason(x: Option<i32>) -> i32 {\n    let base = 1;\n    let total = match x {\n        Some(v) => v + base,\n        None => 0,\n    };\n    total\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("None => 0,", 5), &owner(straight, "reason"))
                .ok_or_else(|| "premise: the None arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(reason(None), 0);"));
        Ok(())
    }

    #[test]
    fn a_changed_pattern_credits_only_a_moved_input_and_names_only_when_both_miss()
    -> Result<(), String> {
        let body = "pub fn kind(k: Kind) -> u8 {\n    match k {\n        Kind::Alpha => 1,\n        Kind::Beta => 2,\n        Kind::Gamma => 3,\n    }\n}\n";
        let mut probe = arm_probe("Kind::Beta => 2,", 4);
        // The diff's head cut of `Kind::Beta | Kind::Gamma => 2,`.
        probe.before = Some("Kind::Beta | Kind::Gamma =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(body, "kind"))
            .ok_or_else(|| "premise: the changed arm is readable".to_string())?;
        // `Kind::Beta` runs this arm under both patterns: no discriminator.
        assert!(!selector.assertion_selects("assert_eq!(kind(Kind::Beta), 2);"));
        let observed = |call: &str| {
            selector
                .observed_inputs(&test_with(&format!(
                    "fn t() {{\n    assert_eq!({call}, 0);\n}}\n"
                )))
                .map(|observed| observed.selection)
        };
        // `Kind::Gamma` moved from this arm (result 2) to `Kind::Gamma => 3`:
        // the results are unequal literals, so the input discriminates the
        // change (RIPR-SPEC-0229 example 8) and the arm is not named.
        assert_eq!(observed("kind(Kind::Gamma)"), Some(ArmSelection::Selects));
        assert!(selector.assertion_selects("assert_eq!(kind(Kind::Gamma), 3);"));
        // `Kind::Alpha` misses both patterns.
        assert_eq!(
            observed("kind(Kind::Alpha)"),
            Some(ArmSelection::SelectsOther)
        );
        // A pairing whose patterns share no alternative may be another arm
        // of a multi-line hunk (#5638 review P1): the original pattern is
        // unreadable, so no input names the arm.
        let mut mispaired = arm_probe("Kind::Beta => 2,", 4);
        mispaired.before = Some("Kind::Gamma => 3,".to_string());
        let selector = ArmSelector::establish(&mispaired, &owner(body, "kind"))
            .ok_or_else(|| "premise: the changed arm is readable".to_string())?;
        assert_eq!(
            selector
                .observed_inputs(&test_with(
                    "fn t() {\n    assert_eq!(kind(Kind::Alpha), 1);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::Unknown)
        );
        assert!(!selector.assertion_selects("assert_eq!(kind(Kind::Beta), 2);"));
        // A body-only change keeps selection credit.
        let mut body_only = arm_probe("Kind::Beta => 2,", 4);
        body_only.before = Some("Kind::Beta => 7,".to_string());
        let selector = ArmSelector::establish(&body_only, &owner(body, "kind"))
            .ok_or_else(|| "premise: the changed arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(kind(Kind::Beta), 2);"));
        // Reordered alternatives take the same inputs: the arm keeps
        // selection credit, and a partial overlap still does not.
        let zones = "pub fn shipping_zone(country: &str) -> u32 {\n    match country {\n        \"US\" => 1,\n        \"MX\" | \"CA\" => 2,\n        _ => 3,\n    }\n}\n";
        let mut reordered = arm_probe("\"MX\" | \"CA\" => 2,", 4);
        reordered.before = Some("\"CA\" | \"MX\" => 2,".to_string());
        let selector = ArmSelector::establish(&reordered, &owner(zones, "shipping_zone"))
            .ok_or_else(|| "premise: the reordered arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(shipping_zone(\"CA\"), 2);"));
        let mut narrowed = arm_probe("\"MX\" | \"CA\" => 2,", 4);
        narrowed.before = Some("\"CA\" | \"MX\" | \"US\" => 2,".to_string());
        let selector = ArmSelector::establish(&narrowed, &owner(zones, "shipping_zone"))
            .ok_or_else(|| "premise: the narrowed arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(shipping_zone(\"CA\"), 2);"));
        // A payload change is not a reorder, though both payloads share a
        // variant head.
        let options = "pub fn f(x: Option<u8>) -> u8 {\n    match x {\n        None | Some(2) => 2,\n        _ => 3,\n    }\n}\n";
        let mut repayloaded = arm_probe("None | Some(2) => 2,", 4);
        repayloaded.before = Some("Some(1) | None => 2,".to_string());
        let selector = ArmSelector::establish(&repayloaded, &owner(options, "f"))
            .ok_or_else(|| "premise: the repayloaded arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(None), 2);"));
        // A removed-line probe whose arm no longer stands on that line
        // establishes nothing.
        let mut removed = arm_probe("Kind::Delta => 2,", 4);
        removed.before = Some("Kind::Delta => 2,".to_string());
        removed.after = None;
        assert!(ArmSelector::establish(&removed, &owner(body, "kind")).is_none());
        Ok(())
    }

    const DIGIT: &str = "pub fn f(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1..=9 => \"digit\",\n        _ => \"many\",\n    }\n}\n";

    fn digit_selector(before: Option<&str>) -> Result<ArmSelector, String> {
        let mut probe = arm_probe("1..=9 => \"digit\",", 4);
        probe.before = before.map(str::to_string);
        ArmSelector::establish(&probe, &owner(DIGIT, "f"))
            .ok_or_else(|| "premise: the range arm reads its scrutinee from n".to_string())
    }

    fn selection_of(selector: &ArmSelector, call: &str) -> Option<ArmSelection> {
        selector
            .observed_inputs(&test_with(&format!(
                "fn t() {{\n    assert_eq!({call}, \"x\");\n}}\n"
            )))
            .map(|observed| observed.selection)
    }

    /// RIPR-SPEC-0229 example 7: a range arm whose body changed is judged
    /// by value, so inputs outside it select another arm and an input
    /// inside it credits the arm.
    #[test]
    fn integer_range_arms_select_by_value() -> Result<(), String> {
        let selector = digit_selector(Some("1..=9 => \"digits\","))?;
        assert_eq!(
            selection_of(&selector, "f(0)"),
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            selection_of(&selector, "f(42)"),
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(
            selection_of(&selector, "f(10)"),
            Some(ArmSelection::SelectsOther)
        );
        assert_eq!(selection_of(&selector, "f(9)"), Some(ArmSelection::Selects));
        assert!(selector.assertion_selects("assert_eq!(f(1), \"digit\");"));
        assert!(!selector.assertion_selects("assert_eq!(f(0), \"zero\");"));
        // The lower bound is read: 4 is below `5..=9`, 5 is inside it.
        let low = PatternHead::IntRange {
            start: Some(5),
            end: Some(9),
        };
        assert_eq!(
            judge_alternative(&low, &PatternHead::Literal(LiteralValue::Int(4))),
            ArmSelection::SelectsOther
        );
        assert_eq!(
            judge_alternative(&low, &PatternHead::Literal(LiteralValue::Int(5))),
            ArmSelection::Selects
        );
        // A non-integer input against a range is not judged.
        assert_eq!(
            selection_of(&selector, "f('a')"),
            Some(ArmSelection::Unknown)
        );
        // Exclusive, half-open and negative bounds.
        assert_eq!(
            integer_range("0..10"),
            Some(PatternHead::IntRange {
                start: Some(0),
                end: Some(9)
            })
        );
        assert_eq!(
            integer_range("..=-1"),
            Some(PatternHead::IntRange {
                start: None,
                end: Some(-1)
            })
        );
        assert_eq!(
            integer_range("10u8.."),
            Some(PatternHead::IntRange {
                start: Some(10),
                end: None
            })
        );
        // Not integer ranges: a char range, a path bound, a bare `..`, an
        // inclusive range without an end.
        for text in ["'a'..='z'", "MIN..=MAX", "..", "1..="] {
            assert_eq!(integer_range(text), None, "{text}");
        }
        assert_eq!(
            pattern_head("\"a..b\""),
            PatternHead::Literal(LiteralValue::Str("a..b".into()))
        );
        Ok(())
    }

    /// RIPR-SPEC-0229 example 8: widening `1..=8` to `1..=9`. The input 9
    /// moved from `_ => "many"` to this arm, whose results differ, so it
    /// credits the arm; 5 runs this arm under both patterns and earns
    /// nothing; 0 and 42 miss both, so the arm may be named.
    #[test]
    fn an_input_that_moved_to_an_unequal_result_credits_a_changed_pattern() -> Result<(), String> {
        let selector = digit_selector(Some("1..=8 =>"))?;
        assert_eq!(selection_of(&selector, "f(9)"), Some(ArmSelection::Selects));
        assert!(selector.assertion_selects("assert_eq!(f(9), \"many\");"));
        assert!(selector.assertion_selects("assert_eq!(f(9), \"digit\");"));
        assert_eq!(selection_of(&selector, "f(5)"), Some(ArmSelection::Unknown));
        assert!(!selector.assertion_selects("assert_eq!(f(5), \"digit\");"));
        assert_eq!(
            selection_of(&selector, "f(42)"),
            Some(ArmSelection::SelectsOther)
        );
        // Narrowing: 9 moved out of this arm to `_`, unequal results too.
        let mut narrowed = arm_probe("1..=8 => \"digit\",", 4);
        narrowed.before = Some("1..=9 =>".to_string());
        let narrow = DIGIT.replace("1..=9", "1..=8");
        let selector = ArmSelector::establish(&narrowed, &owner(&narrow, "f"))
            .ok_or_else(|| "premise: the narrowed arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(f(9), \"many\");"));
        assert!(!selector.assertion_selects("assert_eq!(f(8), \"digit\");"));
        Ok(())
    }

    /// RIPR-SPEC-0229 example 9 and the fail-closed edges of moved-input
    /// credit: a computed or equal result, an old line with another body,
    /// a guarded arm in the fall-through path, or an unreached `match`.
    #[test]
    fn moved_input_credit_needs_provably_unequal_results() -> Result<(), String> {
        let computed = "pub fn f(n: u32) -> u32 {\n    match n {\n        0..=4 => 1 + 1,\n        _ => 2,\n    }\n}\n";
        let mut probe = arm_probe("0..=4 => 1 + 1,", 3);
        probe.before = Some("0..=3 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(computed, "f"))
            .ok_or_else(|| "premise: the computed arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(4), 2);"));
        let equal = computed.replace("1 + 1", "2");
        let mut probe = arm_probe("0..=4 => 2,", 3);
        probe.before = Some("0..=3 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(&equal, "f"))
            .ok_or_else(|| "premise: the equal arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(4), 2);"));
        // An old whole line whose body differs is not this arm with only
        // its pattern changed, even when the patterns share an alternative
        // and the moved input's results are unequal.
        let selector = digit_selector(Some("1..=8 => \"digits\","))?;
        assert!(!selector.assertion_selects("assert_eq!(f(9), \"many\");"));
        let kinds = "pub fn kind(k: Kind) -> u8 {\n    match k {\n        Kind::Alpha => 1,\n        Kind::Beta => 2,\n        Kind::Gamma => 3,\n    }\n}\n";
        let mut rebodied = arm_probe("Kind::Beta => 2,", 4);
        rebodied.before = Some("Kind::Beta | Kind::Gamma => 5,".to_string());
        let selector = ArmSelector::establish(&rebodied, &owner(kinds, "kind"))
            .ok_or_else(|| "premise: the rebodied arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(kind(Kind::Gamma), 3);"));
        // A whole old line with an equal body is not a head cut either: the
        // diff keeps whole lines when another arm of the hunk changed.
        rebodied.before = Some("Kind::Beta | Kind::Gamma => 2,".to_string());
        let selector = ArmSelector::establish(&rebodied, &owner(kinds, "kind"))
            .ok_or_else(|| "premise: the whole old arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(kind(Kind::Gamma), 3);"));
        // An old head that shares no leading text with the current pattern
        // is not an anchored cut of this line (#5992 review: `0 =>` paired
        // with `1..=9 => 2,` from another arm of the hunk).
        let selector = digit_selector(Some("0 =>"))?;
        assert!(!selector.assertion_selects("assert_eq!(f(5), \"many\");"));
        assert_eq!(selection_of(&selector, "f(5)"), Some(ArmSelection::Unknown));
        // A `match` whose value is bound and read later is not the return
        // value: `if t > 0` maps unequal results to one value.
        let bound = "pub fn f(n: u32) -> u32 {\n    let t = match n {\n        0..=4 => 1,\n        _ => 2,\n    };\n    if t > 0 { 7 } else { 0 }\n}\n";
        let mut probe = arm_probe("0..=4 => 1,", 3);
        probe.before = Some("0..=3 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(bound, "f"))
            .ok_or_else(|| "premise: the bound match is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(4), 7);"));
        // A guarded arm between this one and `_` may take the input.
        let guarded = "pub fn f(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1..=9 => \"digit\",\n        k if k > 100 => \"huge\",\n        _ => \"many\",\n    }\n}\n";
        let mut probe = arm_probe("1..=9 => \"digit\",", 4);
        probe.before = Some("1..=8 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(guarded, "f"))
            .ok_or_else(|| "premise: the guarded match is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(9), \"many\");"));
        // Constructor results compare by name and literal payload.
        let options = "pub fn f(n: u32) -> Option<u8> {\n    match n {\n        0..=4 => Some(1),\n        _ => None,\n    }\n}\n";
        let mut probe = arm_probe("0..=4 => Some(1),", 3);
        probe.before = Some("0..=3 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(options, "f"))
            .ok_or_else(|| "premise: the option arm is readable".to_string())?;
        assert!(selector.assertion_selects("assert_eq!(f(4), None);"));
        let some = |text: &str| result_value(text);
        assert!(
            some("Some(1)")
                .zip(some("Some(2),"))
                .is_some_and(|(l, r)| provably_unequal(&l, &r))
        );
        assert!(
            !some("Some(1)")
                .zip(some("Some(1u8)"))
                .is_some_and(|(l, r)| provably_unequal(&l, &r))
        );
        assert!(
            !some("Kind::Alpha")
                .zip(some("Alpha"))
                .is_some_and(|(l, r)| provably_unequal(&l, &r))
        );
        assert!(
            !some("Quiet")
                .zip(some("Level::Low"))
                .is_some_and(|(l, r)| provably_unequal(&l, &r))
        );
        assert!(
            some("Level::High")
                .zip(some("Level::Low"))
                .is_some_and(|(l, r)| provably_unequal(&l, &r))
        );
        assert_eq!(some("Some(x)"), None);
        // A CamelCase call with literal arguments is not a variant: `Make(1)`
        // and `Make(2)` may return the same value (#5992 review).
        assert_eq!(some("Make(1)"), None);
        assert_eq!(some("Kind::Make(2)"), None);
        assert!(some("Option::Some(1)").is_some());
        assert_eq!(some("{ 1 }"), None);
        // An uncased name may be an associated constant, and two constants
        // may hold one value, so `Kind::A` and `Kind::B` are never one value
        // each (#7543 merged with #5992).
        assert_eq!(some("Kind::A"), None);
        assert_eq!(some("Kind::MAX"), None);
        // A `match` the call may skip credits nothing.
        let skipped = "pub fn f(n: u32) -> &'static str {\n    if n == 7 { return \"seven\"; }\n    match n {\n        0 => \"zero\",\n        1..=9 => \"digit\",\n        _ => \"many\",\n    }\n}\n";
        let mut probe = arm_probe("1..=9 => \"digit\",", 5);
        probe.before = Some("1..=8 =>".to_string());
        let selector = ArmSelector::establish(&probe, &owner(skipped, "f"))
            .ok_or_else(|| "premise: the skipped match is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(f(9), \"many\");"));
        Ok(())
    }

    #[test]
    fn a_test_names_the_owner_through_its_body_or_an_imported_alias() -> Result<(), String> {
        let body = "pub fn kind(k: Kind) -> u8 {\n    match k {\n        Kind::Alpha => 1,\n        Kind::Beta => 2,\n    }\n}\n";
        let mut selector =
            ArmSelector::establish(&arm_probe("Kind::Beta => 2,", 4), &owner(body, "kind"))
                .ok_or_else(|| "premise: the Beta arm is readable".to_string())?;
        let wrapper_only = test_with("fn t() {\n    assert_eq!(f(Kind::Alpha), 1);\n}\n");
        assert!(selector.mentioned_by(&test_with(
            "fn t() {\n    assert_eq!(kind(Kind::Beta), 2);\n}\n"
        )));
        assert!(!selector.mentioned_by(&wrapper_only));
        let mut with_imports = |source: &str| {
            selector.test_imports = Some(
                [(wrapper_only.file.clone(), flattened_use_paths(source))]
                    .into_iter()
                    .collect(),
            );
            selector.mentioned_by(&wrapper_only)
        };
        // `f` is the owner under another name, so the test calls it.
        assert!(with_imports("use crate::{Kind, kind as f};\n"));
        // A plain import of the owner is not a call through another name.
        assert!(!with_imports("use crate::{Kind, kind};\n"));
        // A file whose imports were not read may alias the owner.
        selector.test_imports = Some(Default::default());
        assert!(selector.mentioned_by(&wrapper_only));
        Ok(())
    }

    #[test]
    fn an_earlier_arm_that_may_match_blocks_selection() -> Result<(), String> {
        // An earlier guarded arm may take `2` first.
        let guarded = "fn width(n: u8) -> u8 {\n    match n {\n        v if v > 1 => {\n            v\n        }\n        2 => 20,\n        _ => 0,\n    }\n}\n";
        let selector = ArmSelector::establish(&arm_probe("2 => 20,", 6), &owner(guarded, "width"))
            .ok_or_else(|| "premise: the 2 arm is readable".to_string())?;
        assert!(!selector.assertion_selects("assert_eq!(width(2), 20);"));
        // A non-matching input still provably misses the changed arm.
        assert_eq!(
            selector
                .observed_inputs(&test_with("fn t() {\n    assert_eq!(width(3), 3);\n}\n"))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        // Earlier arms that provably miss, including a block arm, let the
        // changed arm select.
        let plain = "fn width(n: u8) -> u8 {\n    match n {\n        1 => {\n            10\n        }\n        3 | 4 => 30,\n        2 => 20,\n        _ => 0,\n    }\n}\n";
        let selector = ArmSelector::establish(&arm_probe("2 => 20,", 7), &owner(plain, "width"))
            .ok_or_else(|| "premise: the 2 arm is readable".to_string())?;
        assert_eq!(selector.scrutinee(), "n");
        assert!(selector.assertion_selects("assert_eq!(width(2), 20);"));
        // The changed line must start an arm of the enclosing match.
        assert!(ArmSelector::establish(&arm_probe("10", 4), &owner(plain, "width")).is_none());
        Ok(())
    }

    #[test]
    fn non_ascii_receivers_and_mut_self_methods_are_read_safely() -> Result<(), String> {
        let body = "pub fn flip(&mut self, mode: Mode) -> u8 {\n    match mode {\n        Mode::Cold => 0,\n        Mode::Hot => 1,\n    }\n}\n";
        let selector =
            ArmSelector::establish(&arm_probe("Mode::Hot => 1,", 4), &owner(body, "flip"))
                .ok_or_else(|| "premise: the Hot arm reads its scrutinee from mode".to_string())?;
        // `&mut self` is not counted: `mode` is the call's first argument.
        assert_eq!(selector.binding, ScrutineeBinding::Parameter(0));
        assert!(selector.assertion_selects("assert_eq!(obj.flip(Mode::Hot), 1);"));
        // A non-ASCII receiver is read on character boundaries.
        assert!(selector.assertion_selects("assert_eq!(café.flip(Mode::Hot), 1);"));
        assert!(!selector.assertion_selects("assert_eq!(\"é\".flip(Mode::Cold), 0);"));
        assert_eq!(
            selector
                .observed_inputs(&test_with(
                    "fn t() {\n    let mut café = Thing;\n    assert_eq!(café.flip(Mode::Cold), 0);\n}\n"
                ))
                .map(|observed| observed.selection),
            Some(ArmSelection::SelectsOther)
        );
        Ok(())
    }

    #[test]
    fn typed_mutable_receivers_establish_nothing() -> Result<(), String> {
        let shape = |signature: &str| {
            format!(
                "pub fn level({signature}) -> u8 {{\n    self.bump();\n    match *self {{\n        Mode::Cold => 0,\n        Mode::Hot => 1,\n    }}\n}}\n"
            )
        };
        // Control: an immutable typed receiver cannot change between reads.
        let shared = shape("self: &Self");
        let selector =
            ArmSelector::establish(&arm_probe("Mode::Hot => 1,", 5), &owner(&shared, "level"))
                .ok_or_else(|| "premise: `self: &Self` reads the receiver".to_string())?;
        assert_eq!(selector.binding, ScrutineeBinding::Receiver);
        // `self.bump()` may change a mutable typed receiver before the match.
        for signature in ["self: &mut Self", "self: Pin<&mut Self>", "mut self: Self"] {
            let body = shape(signature);
            assert!(
                ArmSelector::establish(&arm_probe("Mode::Hot => 1,", 5), &owner(&body, "level"))
                    .is_none(),
                "{signature}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_free_owner_called_through_a_foreign_path_is_not_read() -> Result<(), String> {
        let selector = reason_selector()?;
        // `other_crate::reason` may be another function: never credited,
        // and a test that calls it gives no readable inputs.
        assert!(!selector.assertion_selects("assert_eq!(other_crate::reason(None), 0);"));
        assert_eq!(
            selector.observed_inputs(&test_with(
                "fn t() {\n    assert_eq!(reason(Some(5)), 6);\n    assert_eq!(other_crate::reason(None), 0);\n}\n"
            )),
            None
        );
        assert!(!selector.assertion_selects("assert_eq!(::reason(None), 0);"));
        // Workspace-rooted paths still read.
        assert!(selector.assertion_selects("assert_eq!(crate::reason(None), 0);"));
        assert!(selector.assertion_selects("assert_eq!(super::reason(None), 0);"));
        assert!(!selector.assertion_selects("assert_eq!(my_pkg::reason(None), 0);"));
        let packages = std::collections::BTreeSet::from(["my-pkg".to_string()]);
        let selector = selector.with_workspace_packages(&packages);
        assert!(selector.assertion_selects("assert_eq!(my_pkg::reason(None), 0);"));
        Ok(())
    }

    /// Review findings on #5638: each shape below once read as `Selects`
    /// for an input Rust routes to a different arm.
    #[test]
    fn review_false_credit_shapes_stay_unjudged() -> Result<(), String> {
        // `true`/`false` in a payload refute; they are not bindings.
        let bools = "fn f(x: Option<bool>) -> u8 {\n    match x {\n        Some(true) => 1,\n        Some(false) => 2,\n        None => 0,\n    }\n}\n";
        let some_true =
            ArmSelector::establish(&arm_probe("Some(true) => 1,", 3), &owner(bools, "f"))
                .ok_or_else(|| "premise: the Some(true) arm is readable".to_string())?;
        assert!(!some_true.assertion_selects("assert_eq!(f(Some(false)), 2);"));
        // A comma-less block-like body must not swallow the guard arm after it.
        let swallowed = "fn f(n: u8, flag: bool) -> u8 {\n    match n {\n        0 => match flag { true => 1, false => 2 }\n        x if x > 5 => 9,\n        7 => 70,\n        _ => 0,\n    }\n}\n";
        let seven = ArmSelector::establish(&arm_probe("7 => 70,", 5), &owner(swallowed, "f"));
        assert!(
            seven.is_none_or(|selector| !selector.assertion_selects("assert_eq!(f(7, true), 9);"))
        );
        // A `for` loop, a field-rename destructure, or a mutating method
        // rebinds or changes the parameter.
        for body in [
            "fn f(x: Option<u8>, xs: Vec<Option<u8>>) -> u8 {\n    for x in xs {\n        match x {\n            Some(v) => v,\n            None => 100,\n        };\n    }\n    1\n}\n",
            "fn f(x: Option<u8>, w: W) -> u8 {\n    let W { inner: x } = w;\n    match x {\n        Some(v) => v,\n        None => 100,\n    }\n}\n",
            "fn f(mut x: Option<u8>) -> u8 {\n    let _ = x.take();\n    match x {\n        Some(v) => v,\n        None => 100,\n    }\n}\n",
        ] {
            let line = body
                .lines()
                .position(|line| line.contains("None => 100"))
                .unwrap_or(0)
                + 1;
            assert!(
                ArmSelector::establish(&arm_probe("None => 100,", line), &owner(body, "f"))
                    .is_none(),
                "{body}"
            );
        }
        // A loop that reassigns the scrutinee after the match.
        let looped = "fn run(mut s: State) -> u8 {\n    loop {\n        match s {\n            Start => return 1,\n            Idle => s = Start,\n        }\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("Start => return 1,", 4), &owner(looped, "run"))
                .is_none()
        );
        // A `&mut self` method that assigns `*self` before the match.
        let reassigned = "fn f(&mut self) -> u8 {\n    *self = Mode::Off;\n    match self {\n        On => 1,\n        Off => 0,\n    }\n}\n";
        assert!(
            ArmSelector::establish(&arm_probe("On => 1,", 4), &owner(reassigned, "f")).is_none()
        );
        // Two arms on one line: which one changed is not known.
        assert!(ArmSelector::establish(&arm_probe("None => 0, Some(v) => v + 1,", 3), &owner("fn f(x: Option<u8>) -> u8 {\n    match x {\n        None => 0, Some(v) => v + 1,\n    }\n}\n", "f")).is_none());
        // Another enum's variant with the same name is not this one's.
        let typed = "fn f(level: Level) -> u8 {\n    match level {\n        Level::Low => 1,\n        Level::High => 2,\n    }\n}\n";
        let low = ArmSelector::establish(&arm_probe("Level::Low => 1,", 3), &owner(typed, "f"))
            .ok_or_else(|| "premise: the Low arm is readable".to_string())?;
        assert!(low.assertion_selects("assert_eq!(f(Level::Low), 1);"));
        assert!(!low.assertion_selects("assert_eq!(f(Priority::Low), 1);"));
        Ok(())
    }
}
