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
    expr_ending_at_has_type, impl_self_type_name, let_binding_mentions_type, let_binding_type_head,
    owner_trait_name,
};
use super::reveal::contains_as_whole_word;
use crate::analysis::facts::TestFact;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::*;

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
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
        // A test seen calling the owner already decided reach; this rule is
        // only for the "no test is seen calling" reading.
        || finding.ripr.reach.state != StageState::Weak
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
            uses_syntax_of(trait_name, &code)
                && binds_impl_value(test, &code, trait_name, self_type)
        }
        ImplicitCall::Dispatch => {
            constructs(&code, self_type)
                && dispatch_binds_impl_value(test, &code, &owner.name, self_type)
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
struct OperatorSite {
    at: usize,
    len: usize,
    operand: SelfOperand,
    /// The operator writes its result back into the left operand (`+=`).
    assigns: bool,
}

/// Whether the test spells the trait's syntax on a value of `self_type` and
/// a strong assertion observes the result. `uses_syntax_of` only finds the
/// syntax; a type named in one statement and an operator used on integers in
/// another do not make the impl's method run, so a gap there stays.
fn binds_impl_value(test: &TestFact, code: &str, trait_name: &str, self_type: &str) -> bool {
    let Some(sites) = operator_sites(trait_name, code) else {
        // Drop glue, iteration and auto-deref have no operator to bind: the
        // test must build a value of the type, and drop or iterate it.
        return constructs(code, self_type)
            && match trait_name {
                "Drop" => drops_at_scope_end(code),
                "Iterator" | "IntoIterator" | "DoubleEndedIterator" | "ExactSizeIterator" => {
                    iterates(code)
                }
                _ => true,
            };
    };
    sites.iter().any(|site| {
        let self_operand = match site.operand {
            SelfOperand::Left => expr_ending_at_has_type(code, site.at, self_type),
            SelfOperand::Right => operand_after_has_type(code, site.at + site.len, self_type),
        };
        let assigned = site.assigns.then(|| ident_before(code, site.at)).flatten();
        self_operand && observed_by_strong_assertion(test, code, site.at, assigned)
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
    let assign = |op: &str| -> Vec<OperatorSite> {
        operator_positions(code, op, true)
            .into_iter()
            .map(|site| OperatorSite {
                assigns: true,
                ..site
            })
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
        "AddAssign" => assign("+="),
        "SubAssign" => assign("-="),
        "MulAssign" => assign("*="),
        "DivAssign" => assign("/="),
        "RemAssign" => assign("%="),
        "BitAndAssign" => assign("&="),
        "BitOrAssign" => assign("|="),
        "BitXorAssign" => assign("^="),
        "ShlAssign" => assign("<<="),
        "ShrAssign" => assign(">>="),
        "Neg" => operator_positions(code, "-", false),
        "Not" => operator_positions(code, "!", false),
        "Index" | "IndexMut" => code
            .match_indices('[')
            .filter(|(at, _)| operand_char_before(code, *at))
            .map(|(at, _)| OperatorSite {
                at,
                len: 1,
                operand: SelfOperand::Left,
                assigns: false,
            })
            .collect(),
        "PartialOrd" => binary(&["<", ">"]),
        _ => return None,
    })
}

/// Occurrences of `op` as a binary (`binary`) or unary operator. A binary
/// operator has an operand before it and is not part of a longer operator
/// (`->`, `=>`, `&&`, `||`, `<=` is still ordering) or a closure parameter
/// list; a unary one has none.
fn operator_positions(code: &str, op: &str, binary: bool) -> Vec<OperatorSite> {
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
            assigns: false,
        })
        .collect()
}

/// Whether the nearest non-space character before `at` ends an operand.
fn operand_char_before(code: &str, at: usize) -> bool {
    code[..at]
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']'))
}

/// Whether the unary operand starting at `start` has type `ty`: a constructor
/// or unit value whose path head is `ty`, or one identifier bound to `ty`. A
/// method chain or field access after it may change the type, so it fails
/// closed.
fn operand_after_has_type(code: &str, start: usize, ty: &str) -> bool {
    let rest = code[start..].trim_start();
    let rest = rest.strip_prefix('&').map_or(rest, str::trim_start);
    let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
    let path_len = rest
        .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | ':')))
        .unwrap_or(rest.len());
    let path = &rest[..path_len];
    if path.is_empty() || rest[path_len..].trim_start().starts_with('.') {
        return false;
    }
    let lets = code[..start].rfind(';').map_or("", |at| &code[..=at]);
    path.split("::").next() == Some(ty)
        || (!path.contains("::") && let_binding_mentions_type(lets, path, ty))
}

/// A value of `ty` is built in the test: `ty(..)`, `ty { .. }`, `ty::..` or a
/// unit value in expression position. A declaration (`struct ty`) or a type in
/// an annotation is not.
fn constructs(code: &str, ty: &str) -> bool {
    let bytes = code.as_bytes();
    code.match_indices(ty).any(|(at, _)| {
        let end = at + ty.len();
        let boundary = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(end).is_none_or(|b| !is_ident_byte(*b));
        if !boundary {
            return false;
        }
        let before = code[..at].trim_end();
        let declared = [
            "struct", "enum", "impl", "for", "trait", "type", "use", "dyn", "as",
        ]
        .iter()
        .any(|keyword| {
            before.ends_with(keyword)
                && before[..before.len() - keyword.len()]
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !ch.is_alphanumeric() && ch != '_')
        });
        let annotation = before.ends_with([':', '<', '>']) && !before.ends_with("::");
        let after = code[end..].trim_start();
        let built = after.starts_with(['(', '{']) || after.starts_with("::");
        let unit = before.ends_with(['=', '(', ',']) && !after.starts_with(['<', ':']);
        !declared && !annotation && (built || unit)
    })
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The test iterates a value: a `for` loop or an adapter that calls `next`.
fn iterates(code: &str) -> bool {
    code.contains("for ")
        || [
            "collect", "next", "sum", "count", "fold", "map", "take", "last", "nth", "product",
            "min", "max", "rev",
        ]
        .iter()
        .any(|name| calls_method_named(code, name))
}

/// Whether the dispatched `.method(` is called on a value that could be the
/// impl's: a call whose receiver is a binding of another type is not.
fn dispatch_binds_impl_value(test: &TestFact, code: &str, method: &str, self_type: &str) -> bool {
    method_call_dots(code, method).into_iter().any(|dot| {
        let lets = code[..dot].rfind(';').map_or("", |at| &code[..=at]);
        let other_type = ident_before(code, dot)
            .and_then(|receiver| let_binding_type_head(lets, receiver))
            .is_some_and(|head| head != self_type);
        !other_type && observed_by_strong_assertion(test, code, dot, None)
    })
}

/// Whether the statement holding byte `at` is checked by a strong assertion:
/// it is inside an assertion, or binds a name (or assigns `assigned`) that a
/// strong assertion mentions. `let _ = a + b;` discards the result.
fn observed_by_strong_assertion(
    test: &TestFact,
    code: &str,
    at: usize,
    assigned: Option<&str>,
) -> bool {
    let start = code[..at].rfind([';', '{', '}']).map_or(0, |i| i + 1);
    let end = code[at..].find(';').map_or(code.len(), |i| at + i);
    let statement = code[start..end].trim();
    if statement.contains("assert") {
        return true;
    }
    let bound = statement.strip_prefix("let ").and_then(|rest| {
        let rest = rest
            .trim_start()
            .strip_prefix("mut ")
            .unwrap_or(rest)
            .trim_start();
        let len = rest
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..len];
        (!name.is_empty() && name != "_").then_some(name)
    });
    bound.into_iter().chain(assigned).any(|name| {
        test.assertions.iter().any(|assertion| {
            assertion.strength == OracleStrength::Strong
                && contains_as_whole_word(&assertion.text, name)
        })
    })
}

/// The identifier ending just before byte `end`, if the expression there is a
/// plain name (not a field, call result or index).
fn ident_before(code: &str, end: usize) -> Option<&str> {
    let head = code[..end].trim_end();
    let start = head
        .rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .map_or(0, |i| i + 1);
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
            let whole = at == 0 || !is_ident_byte(code.as_bytes()[at - 1]);
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
    fn constructs_reads_a_value_not_a_declaration() {
        assert!(constructs("let g = Guard::new();", "Guard"));
        assert!(constructs("let _g = Guard;", "Guard"));
        assert!(constructs("assert_eq!(Money(2).0, 2);", "Money"));
        assert!(constructs("Square { side: 3 }", "Square"));
        assert!(!constructs("struct Money(i64);", "Money"));
        assert!(!constructs("impl Drop for Guard {", "Guard"));
        assert!(!constructs("let m: Money = other;", "Money"));
        assert!(!constructs("fn f(g: Guard) {}", "Guard"));
        assert!(!constructs("let v: Vec<Guard> = x;", "Guard"));
        assert!(!constructs("MoneyBag(2)", "Money"));
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
}
