//! Change classifier for the TypeScript preview adapter.

use super::*;

// ── Observation guard (RIPR-SPEC-0098) ───────────────────────────────────────

/// Tokenize an expression string into identifier tokens longer than 3 chars.
///
/// Only ASCII alphanumeric / underscore segments are kept. Segments adjacent
/// to a dot (`.`) or path separator (`:`) are excluded — both the qualifier
/// and the qualified member — so that shared names (e.g. `"amount"` from both
/// `props.amount * 9` and an unrelated bare `amount * 2`) do not spuriously
/// confirm: a dot-qualified name may belong to a different receiver.
fn identifier_tokens(expr: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in expr.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(segment_start) = start.take() {
            push_identifier_token(expr, segment_start, idx, &mut tokens);
        }
    }
    if let Some(segment_start) = start {
        push_identifier_token(expr, segment_start, expr.len(), &mut tokens);
    }
    tokens
}

/// Push `expr[segment_start..segment_end]` as a confirmation token unless it
/// is dot/path-adjacent (see [`identifier_tokens`]) or too short.
fn push_identifier_token(
    expr: &str,
    segment_start: usize,
    segment_end: usize,
    tokens: &mut Vec<String>,
) {
    let preceded_by_qualifier = expr[..segment_start]
        .chars()
        .next_back()
        .is_some_and(|ch| ch == '.' || ch == ':');
    let followed_by_qualifier = expr[segment_end..]
        .chars()
        .next()
        .is_some_and(|ch| ch == '.' || ch == ':');
    if preceded_by_qualifier || followed_by_qualifier {
        return;
    }
    let token = &expr[segment_start..segment_end];
    if token.len() > 3 {
        tokens.push(token.to_string());
    }
}

/// Synthesized template words that `typescript_call_effect_discriminator`
/// interpolates into its English-sentence discriminator (`call … includes …`,
/// `call … occurs`, `mock interaction … is called`, `log contains …`). They
/// are generator vocabulary, not changed code, so they must never serve as
/// confirmation tokens for the RIPR-SPEC-0098 observation guard.
const CALL_EFFECT_TEMPLATE_WORDS: &[&str] = &[
    "includes",
    "occurs",
    "matching",
    "called",
    "interaction",
    "contains",
    "mock",
    "with",
    "call",
    "log",
];

/// Strip the synthesized prefix that `typescript_missing_discriminator_value`
/// adds so we recover the raw changed sub-expression.
fn strip_synthesized_prefix(discriminator_value: &str) -> &str {
    // Prefixes added by probe_shape.rs:
    //   "return value == "  (ReturnValue)
    //   "<lhs> == "         (Predicate/FieldConstruction)
    //   "call ... includes " (SideEffect)
    //   "log contains "     (SideEffect console.log)
    //   "call "             (SideEffect bare call name)
    //   "throws "           (ErrorPath)
    for prefix in &["return value == ", "log contains ", "call ", "throws "] {
        if let Some(rest) = discriminator_value.strip_prefix(prefix) {
            return rest;
        }
    }
    // "<lhs> == <rhs>" form from boundary/field discriminator
    if let Some(idx) = discriminator_value.find(" == ") {
        // Return the right-hand side (the changed value)
        return &discriminator_value[idx + 4..];
    }
    discriminator_value
}

/// Returns `true` when there is static evidence that a strong assertion in one
/// of the oracle-eligible related tests observes the specific changed
/// sub-expression — not just some other part of the same function body.
///
/// This is the RIPR-SPEC-0098 observation guard, scoped to **effect families**
/// (SideEffect and CallDeletion) where the false-exposed pattern is clearest:
/// a call side-effect (e.g. `console.log("audit", amount*9)`) was being promoted
/// to Exposed by value-based `toBe`/`toEqual` assertions in the same test body
/// that assert the owner's UNCHANGED return value.
///
/// ### What the guard checks
///
/// The confirmation decision keys on **`oracle_kind`** — which the live oracle
/// extractor always populates (`oracle.rs`) — rather than relying on
/// `observed_expression`, which is optional metadata. For SideEffect /
/// CallDeletion families a strong assertion CONFIRMS (returns `true`,
/// stays Exposed) only when it actually witnesses a call effect:
/// 1. It is an effect-shape oracle
///    (`MockExpectation` | `Snapshot` | `WholeObjectEquality`) — these capture
///    mock-call expectations, serialized snapshots, or persisted whole-object
///    state, all of which observe side effects directly; OR
/// 2. It carries an `observed_expression` that either shares an exact changed
///    identifier token (> 3 chars, dot/path-adjacent segments excluded, same
///    `identifier_tokens` rules on both sides) or names a side-channel (an
///    expression that does NOT name the owner — e.g. a closure-local
///    side-effect variable or a captured mock).
///
/// Value-shaped strong oracles (`ExactValue` / `ExactErrorVariant`) that observe
/// the owner's RETURN VALUE do NOT witness a `console.log`/side-effect change, so
/// they do not confirm. When the only strong oracles are value-shaped (or no
/// observed-expression metadata is available to prove a side-channel), the guard
/// **fails closed** and returns `false`, downgrading to WeaklyExposed.
///
/// For all other families the guard always returns `true` (pre-guard behaviour).
///
/// ### Fail-closed default (RIPR-SPEC-0098 hardening, #1235)
///
/// Earlier revisions returned `true` ("confirmed") when no strong assertion
/// carried `observed_expression` metadata. That was a fail-OPEN fallback:
/// "can't tell what's observed → assume observed → exposed", which is exactly the
/// over-claim this guard exists to kill. It is removed. The decision now keys on
/// `oracle_kind` (always populated live) plus an optional `observed_expression`
/// token / side-channel check; absence of `observed_expression` no longer
/// re-promotes a swallowed side effect to Exposed.
pub(crate) fn ts_changed_value_is_observed(
    probe_shape: &TypeScriptProbeShape,
    line_text: &str,
    owner_name: &str,
    candidates: &[TypeScriptRelatedCandidate<'_>],
) -> bool {
    // Only apply the guard to SideEffect / CallDeletion families.
    // All other families keep the pre-guard (always-confirmed) behaviour.
    if !matches!(
        probe_shape.family,
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion
    ) {
        return true;
    }

    // Collect changed identifier tokens from the discriminator value.
    let raw_discriminator = typescript_missing_discriminator_value(&probe_shape.family, line_text);
    let changed_tokens: Vec<String> = if let Some(ref disc) = raw_discriminator {
        let raw_expr = strip_synthesized_prefix(disc);
        identifier_tokens(raw_expr)
            .into_iter()
            .filter(|tok| !CALL_EFFECT_TEMPLATE_WORDS.contains(&tok.as_str()))
            .collect()
    } else {
        Vec::new()
    };

    // Fail-CLOSED: confirmation must be affirmatively established by at least one
    // strong assertion that actually witnesses a call effect. We scan every
    // strong assertion in the oracle-eligible candidates and return `true` the
    // moment one of them qualifies. If none qualify, we downgrade — including the
    // case where no `observed_expression` metadata is available at all (the
    // former fail-OPEN `return true` fallback is deliberately gone: absence of
    // proof is not proof of observation).
    for candidate in candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        for assertion in &candidate.test.assertions {
            if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
                continue;
            }
            // (1) Effect-shape oracle kinds confirm unconditionally: these ARE
            // effect observers (mock call expectation, snapshot, whole-object
            // equality capturing persisted state). This decision keys ONLY on
            // `oracle_kind`, which the live extractor always populates.
            if matches!(
                assertion.oracle_kind,
                OracleKind::MockExpectation
                    | OracleKind::Snapshot
                    | OracleKind::WholeObjectEquality
            ) {
                return true;
            }
            // (2) Optional `observed_expression` checks — only when the live
            // extractor retained the `expect(<expr>)` argument text. These can
            // only ADD confirmations; their absence never re-promotes.
            if let Some(ref observed) = assertion.observed_expression {
                // Token match: tokenize the observed expression with the SAME
                // `identifier_tokens` rules and require EXACT token equality.
                // A raw substring `observed.contains(tok)` false-confirms on
                // synthesized template words (e.g. a `.includes(...)` call in
                // the test) and on tokens merely embedded in a larger
                // identifier.
                if !changed_tokens.is_empty() {
                    let observed_tokens = identifier_tokens(observed);
                    if changed_tokens
                        .iter()
                        .any(|tok| observed_tokens.contains(tok))
                    {
                        return true;
                    }
                }
                // Side-channel: the observed expression does NOT name the owner,
                // so it is asserting something other than the owner return value
                // (a closure-local side-effect variable, a captured mock result,
                // or any side-channel) — conservatively treat it as observing
                // the effect.
                if !observed.contains(owner_name) {
                    return true;
                }
            }
            // Otherwise this strong assertion is value-shaped (ExactValue /
            // ExactErrorVariant) observing the owner return value, or carries no
            // observed_expression to prove a side-channel: it does NOT witness
            // the call effect. Keep scanning for a qualifying assertion.
        }
    }

    // No strong assertion witnessed the call effect: fail closed → downgrade.
    false
}

/// Build the named limitation message for the RIPR-SPEC-0098 downgrade arm.
///
/// Only fires for SideEffect / CallDeletion families (the guard is scoped to
/// effect families only). Emits a `propagation_unknown` limitation.
pub(crate) fn ts_observation_guard_limitation(
    probe_shape: &TypeScriptProbeShape,
    line_text: &str,
) -> String {
    // Describe the non-escaping sink where possible.
    let sink_hint = typescript_missing_discriminator_value(&probe_shape.family, line_text)
        .map(|disc| {
            let raw = strip_synthesized_prefix(&disc);
            format!(" (`{raw}`)")
        })
        .unwrap_or_default();
    format!(
        "propagation_unknown: changed value sinks to a non-escaping call effect{sink_hint}; \
         all strong assertions observe the owner return value, not this call effect; \
         propagation unknown"
    )
}

// ── Predicate boundary witness (RIPR-SPEC-0027) ──────────────────────────────

/// Returns `true` when static evidence shows that a strong, family-matching
/// assertion observes an owner call that sits at the changed predicate's
/// boundary. Non-predicate families are not gated here (`true`).
///
/// A strong exact assertion somewhere in a related test does not show that the
/// changed comparison is discriminated: `shippingFee(60)` and `shippingFee(10)`
/// both behave identically under `total > 50` and `total >= 50`. Only an input
/// at the boundary (`total == 50`) tells the two apart, so `Exposed` requires
/// an assertion whose observed owner call carries that boundary:
///
/// - **One literal operand** (`total >= 50`, `status === 'paid'`): an argument
///   of the observed owner call contains the literal as a value token.
/// - **No literal operand** (`count <= limit`): the adapter has no owner
///   parameter facts to map operands to argument positions, so it accepts the
///   same textual witness the Rust boundary evidence accepts without value
///   resolution — two identical arguments in one observed owner call
///   (`isAllowed(5, 5)`) — or an object-literal argument that names both
///   operands with identical values (`PriceLabel({ amount: 100, threshold:
///   100 })`). Otherwise the boundary is not witnessed.
/// - **No parseable comparison** (`if (Number.isNaN(n))`, `case 'gold':`) or an
///   ambiguous fallback shape (`}`): nothing statically ties a test to the
///   changed branch, so this fails closed.
///
/// Observation keys on `observed_expression` (the `expect(<expr>)` argument).
/// When it is absent, or names a local such as `result`, the witness fails
/// closed; the finding then takes the existing weak path.
///
/// Receiver resolution: a member match (`pricing.applyDiscount(...)`) only
/// counts when the receiver is bound to the owner's own module in this test
/// (a namespace import such as `import * as pricing from "../src/pricing"`).
/// A same-named method on an unrelated receiver (`other.total(50)`) never
/// witnesses, even when its arguments carry the boundary literal.
pub(crate) fn ts_predicate_boundary_is_witnessed(
    probe_shape: &TypeScriptProbeShape,
    line_text: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    // An ambiguous fallback shape (`}`, an unrecognised statement) never names
    // a behavior an assertion could be shown to observe.
    if !probe_shape.specific {
        return false;
    }
    if probe_shape.family != ProbeFamily::Predicate {
        return true;
    }
    let Some(boundary) = typescript_boundary_discriminator(line_text) else {
        return false;
    };
    let Some((left, right)) = boundary.split_once(" == ") else {
        return false;
    };
    let literals: Vec<&str> = [left, right]
        .into_iter()
        .filter(|operand| is_boundary_literal(operand))
        .collect();

    for candidate in candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        let owner_receivers =
            owner_namespace_receivers(&candidate.test, owner, alias_map, workspace_root);
        for assertion in &candidate.test.assertions {
            if assertion.oracle_strength.rank() < OracleStrength::Strong.rank()
                || !ts_oracle_kind_matches_seam(&assertion.oracle_kind, &ProbeFamily::Predicate)
            {
                continue;
            }
            let Some(observed) = assertion.observed_expression.as_deref() else {
                continue;
            };
            for arguments in owner_call_arguments(observed, &owner.name, &owner_receivers) {
                let witnessed = if literals.is_empty() {
                    call_has_identical_arguments(&arguments)
                        || object_argument_pins_operands_equal(&arguments, left, right)
                } else {
                    literals
                        .iter()
                        .any(|literal| arguments_contain_literal(&arguments, literal))
                };
                if witnessed {
                    return true;
                }
            }
        }
    }
    false
}

/// Build the named limitation for a predicate whose boundary no strong
/// assertion reaches (RIPR-SPEC-0027 boundary witness).
pub(crate) fn ts_predicate_boundary_limitation(
    probe_shape: &TypeScriptProbeShape,
    line_text: &str,
    owner_name: &str,
) -> String {
    let boundary = if probe_shape.specific {
        typescript_boundary_discriminator(line_text)
    } else {
        None
    };
    match boundary {
        Some(boundary) => format!(
            "Related tests reach `{owner_name}` with a strong oracle, but no strong assertion observes a `{owner_name}(...)` call at the changed predicate boundary `{boundary}`; inputs on one side of the boundary do not discriminate the changed comparison."
        ),
        None => format!(
            "Related tests reach `{owner_name}` with a strong oracle, but the changed line has no statically comparable predicate boundary; static evidence cannot tie an assertion to the changed branch."
        ),
    }
}

fn is_boundary_literal(operand: &str) -> bool {
    let operand = operand.trim();
    numeric_literal_value(operand).is_some()
        || matches!(operand, "true" | "false" | "null" | "undefined")
        || (operand.len() >= 2
            && ['"', '\'', '`']
                .iter()
                .any(|quote| operand.starts_with(*quote) && operand.ends_with(*quote)))
}

/// Canonical decimal text of a numeric literal token (`50`, `50.0`, `1_000`,
/// `10n`), or `None` when the token is not a plain decimal literal.
fn numeric_literal_value(token: &str) -> Option<String> {
    let token = token.trim().trim_end_matches('n').replace('_', "");
    if !token.starts_with(|ch: char| ch.is_ascii_digit())
        || !token.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        || token.matches('.').count() > 1
    {
        return None;
    }
    let (integer, fraction) = token.split_once('.').unwrap_or((token.as_str(), ""));
    let integer = match integer.trim_start_matches('0') {
        "" => "0",
        digits => digits,
    };
    let fraction = fraction.trim_end_matches('0');
    Some(if fraction.is_empty() {
        integer.to_string()
    } else {
        format!("{integer}.{fraction}")
    })
}

/// Return the argument lists (one per call) of every `<owner_name>(...)` call
/// inside `observed`, split at top-level commas.
/// Receiver names in `test` that bind to the owner's own module through a
/// namespace import (`import * as pricing from "../src/pricing"`). A member
/// call on such a receiver (`pricing.applyDiscount(...)`) IS an owner call,
/// while a same-named method on any other receiver (`other.total(...)`) is
/// not — this is the receiver resolution that keeps genuine namespace-import
/// witnesses credited without crediting unrelated receivers.
fn owner_namespace_receivers(
    test: &TypeScriptTest,
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> Vec<String> {
    test.imports_in_file
        .iter()
        .filter(|import| {
            import.namespace
                && import_source_matches_owner(import, &test.file, owner, alias_map, workspace_root)
        })
        .map(|import| import.local.clone())
        .collect()
}

/// The trailing identifier segment immediately before the final `.` of
/// `before_match` — the receiver of a member call (`expect(pricing.` →
/// `pricing`).
fn receiver_before_dot(before_match: &str) -> String {
    let trimmed = before_match.trim_end();
    let without_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
    without_dot
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Return the argument lists (one per call) of every `<owner_name>(...)` call
/// inside `observed`, split at top-level commas.
///
/// A member match (`pricing.applyDiscount(...)`) is kept only when the
/// receiver is bound to the owner's own module via a namespace import
/// (`owner_receivers`); otherwise the match is a same-named method on an
/// unrelated receiver and is skipped.
fn owner_call_arguments(
    observed: &str,
    owner_name: &str,
    owner_receivers: &[String],
) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
    if owner_name.is_empty() {
        return calls;
    }
    for (idx, _) in observed.match_indices(owner_name) {
        // Skip matches embedded in a longer identifier (`otherShippingFee`):
        // the match must start at a real owner call.
        let Some(before) = observed.get(..idx) else {
            continue;
        };
        let preceded_by_identifier = before
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$');
        if preceded_by_identifier {
            continue;
        }
        // Member access (`receiver.owner(...)`): keep only when the receiver
        // resolves to the owner's own module (namespace import); a same-named
        // method on an unrelated receiver (`other.total(50)`) must not
        // witness the owner's boundary.
        if before.trim_end().chars().next_back() == Some('.')
            && !owner_receivers
                .iter()
                .any(|receiver| receiver == &receiver_before_dot(before))
        {
            continue;
        }
        let Some(inner) = observed
            .get(idx + owner_name.len()..)
            .and_then(|rest| rest.trim_start().strip_prefix('('))
        else {
            continue;
        };
        if let Some(arguments) = balanced_call_arguments(inner) {
            calls.push(arguments);
        }
    }
    calls
}

/// Split the text after a call's opening parenthesis into its top-level
/// arguments, stopping at the matching closing parenthesis.
fn balanced_call_arguments(inner: &str) -> Option<Vec<String>> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut current = String::new();
    let mut arguments = Vec::new();
    for ch in inner.chars() {
        if let Some(open) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' if depth == 0 => {
                let last = current.trim();
                if !last.is_empty() {
                    arguments.push(last.to_string());
                }
                return Some(arguments);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                arguments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    None
}

fn call_has_identical_arguments(arguments: &[String]) -> bool {
    arguments.iter().enumerate().any(|(idx, left)| {
        !left.is_empty() && arguments.iter().skip(idx + 1).any(|right| right == left)
    })
}

/// `true` when an object-literal argument binds both comparison operands
/// (matched by their last member segment, e.g. `props.amount` → `amount`) to
/// the same non-empty value text.
fn object_argument_pins_operands_equal(arguments: &[String], left: &str, right: &str) -> bool {
    let key_of = |operand: &str| operand.rsplit('.').next().unwrap_or(operand).to_string();
    let (left_key, right_key) = (key_of(left), key_of(right));
    if left_key.is_empty() || left_key == right_key {
        return false;
    }
    arguments.iter().any(|argument| {
        let Some(body) = argument
            .trim()
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        else {
            return false;
        };
        let Some(fields) = balanced_call_arguments(&format!("{body})")) else {
            return false;
        };
        let value_of = |key: &str| {
            fields.iter().find_map(|field| {
                let (name, value) = field.split_once(':')?;
                (name.trim().trim_matches(['"', '\'']) == key).then(|| value.trim().to_string())
            })
        };
        matches!(
            (value_of(&left_key), value_of(&right_key)),
            (Some(left_value), Some(right_value))
                if !left_value.is_empty() && left_value == right_value
        )
    })
}

fn arguments_contain_literal(arguments: &[String], literal: &str) -> bool {
    let literal = literal.trim();
    if let Some(expected) = numeric_literal_value(literal) {
        return arguments.iter().any(|argument| {
            argument
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .filter_map(numeric_literal_value)
                .any(|value| value == expected)
        });
    }
    if let Some(body) = literal
        .strip_prefix(['"', '\'', '`'])
        .and_then(|rest| rest.strip_suffix(['"', '\'', '`']))
    {
        return arguments.iter().any(|argument| {
            ['"', '\'', '`']
                .iter()
                .any(|quote| argument.contains(&format!("{quote}{body}{quote}")))
        });
    }
    arguments.iter().any(|argument| {
        argument
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
            .any(|token| token == literal)
    })
}

// ── Family↔oracle-kind matching (RIPR-SPEC-0104) ─────────────────────────────

/// Returns `true` when `oracle_kind` can observe the seam identified by
/// `probe_family`.
///
/// This is the single source of truth for the TS-adapter assertion-level
/// family↔kind filter. It mirrors `oracle_kind_matches_seam_kind` from
/// `test_grip_evidence.rs` (which operates on Rust `SeamKind`), adapted for
/// the TypeScript `ProbeFamily` vocabulary.
///
/// ### Mapping table (RIPR-SPEC-0104 §3)
///
/// | ProbeFamily | Excluded OracleKinds (Strong-rank only — the honesty fix) |
/// |---|---|
/// | `ErrorPath` | `ExactValue`, `RelationalCheck`, `MockExpectation` |
/// | `ReturnValue`, `Predicate`, `FieldConstruction`, `MatchArm` | `ExactErrorVariant`, `BroadError`, `MockExpectation` |
/// | `SideEffect`, `CallDeletion` | *(handled by RIPR-SPEC-0098 observation guard — no additional exclusion here)* |
/// | `StaticUnknown` | *(fail-open — all oracles admitted)* |
///
/// ### Design: fail-closed only at the cross-domain seam boundary
///
/// The HONESTY BUG addressed by this function is specifically the case where a
/// **Strong wrong-domain** oracle is credited to a seam whose family it cannot
/// observe:
///
/// - An `ExactErrorVariant` oracle (`.toThrow(DiscountError)`) is an error-path
///   discriminator. It MUST NOT count as a strong match for a `ReturnValue` or
///   `Predicate` seam — these seams emit a value, not an error.
/// - An `ExactValue` oracle (`.toBe(90)`) is a value discriminator. It MUST NOT
///   count as a strong match for an `ErrorPath` seam — the seam throws, not returns.
///
/// ### Composing with RIPR-SPEC-0098 (SideEffect observation guard)
///
/// For `SideEffect` and `CallDeletion` families, RIPR-SPEC-0098 already provides
/// a dedicated observation guard (`ts_changed_value_is_observed`) that correctly
/// handles the case where a value-shaped `ExactValue` assertion observes the owner
/// RETURN value (not the call effect). That guard fires in the
/// `else if strongest_strength >= Strong.rank()` arm of the classifier.
/// To preserve that path, `ExactValue` is NOT excluded from `SideEffect` /
/// `CallDeletion` by this function — SPEC-0098 handles those seams independently.
/// The two specs compose: SPEC-0104 handles cross-domain value↔error exclusion;
/// SPEC-0098 handles the same-domain but wrong-target SideEffect case.
///
/// ### Weak oracle kinds are always admitted
///
/// `SmokeOnly`, `Unknown`, `BroadError` (for non-error families),
/// `RelationalCheck` (for non-error families) are admitted for all concrete
/// families because they are weak/absent and do NOT contradict any family.
/// Their rank (Smoke / Unknown / Weak) prevents them from triggering
/// `Exposed` promotion regardless. Admitting them preserves accurate
/// per-kind messaging in `weak_oracle_missing_summary`.
pub(crate) fn ts_oracle_kind_matches_seam(
    oracle_kind: &OracleKind,
    probe_family: &ProbeFamily,
) -> bool {
    match probe_family {
        // ErrorPath seams: exclude STRONG VALUE oracles (ExactValue, RelationalCheck)
        // and MockExpectation.  BroadError, ExactErrorVariant, Snapshot all observe
        // an error-path change.  SmokeOnly/Unknown are weak and admitted.
        ProbeFamily::ErrorPath => !matches!(
            oracle_kind,
            OracleKind::ExactValue | OracleKind::RelationalCheck | OracleKind::MockExpectation
        ),
        // Value-family seams: exclude STRONG ERROR oracle (ExactErrorVariant)
        // and BroadError, MockExpectation.  ExactValue, WholeObjectEquality,
        // Snapshot, RelationalCheck, SmokeOnly, Unknown all admitted.
        // THIS IS THE PRIMARY FIX: ExactErrorVariant must not credit a ReturnValue seam.
        ProbeFamily::ReturnValue
        | ProbeFamily::Predicate
        | ProbeFamily::FieldConstruction
        | ProbeFamily::MatchArm => !matches!(
            oracle_kind,
            OracleKind::ExactErrorVariant | OracleKind::BroadError | OracleKind::MockExpectation
        ),
        // SideEffect / CallDeletion: defer to RIPR-SPEC-0098 observation guard.
        // All oracle kinds admitted here so that `strongest_strength` reflects
        // the actual oracle rank, letting the observation guard run in the
        // `else if strongest_strength >= Strong.rank()` arm.
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => true,
        // Fail-open for genuinely-unknown family: we cannot determine the
        // match — do not over-correct by blocking a potentially valid oracle.
        ProbeFamily::StaticUnknown => true,
    }
}

/// Compute the strongest oracle kind and strength that MATCHES the changed
/// seam's probe family, by iterating assertions at the assertion level across
/// oracle-eligible candidates (RIPR-SPEC-0104).
///
/// This is the family-aware replacement for the former per-test max over
/// `related: Vec<RelatedTest>` (which collapsed each test to one
/// `oracle_kind` via `strongest_assertion`). The collapsed kind may be
/// wrong-family (e.g., a test with both `.toThrow(DiscountError)` and
/// `.toBeGreaterThan(0)` collapses to `ExactErrorVariant` / Strong — but
/// `ExactErrorVariant` does NOT match a `ReturnValue` seam).
///
/// By filtering at the assertion level before taking the max, a multi-assertion
/// test can still contribute its family-matching assertion even when its
/// overall-strongest assertion is wrong-family — the anti-over-correction
/// invariant (RIPR-SPEC-0104 control 4).
///
/// Returns `(rank: u8, kind: OracleKind)` where rank is the
/// `oracle_strength.rank()` of the best matching assertion, and kind is its
/// `oracle_kind`. Returns `(0, OracleKind::Unknown)` when there are no
/// oracle-eligible candidates or no family-matching assertion.
pub(crate) fn strongest_family_matching_oracle(
    probe_family: &ProbeFamily,
    candidates: &[TypeScriptRelatedCandidate<'_>],
) -> (u8, OracleKind) {
    let mut best_rank: u8 = 0;
    let mut best_kind = OracleKind::Unknown;

    for candidate in candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        for assertion in &candidate.test.assertions {
            if !ts_oracle_kind_matches_seam(&assertion.oracle_kind, probe_family) {
                continue;
            }
            let rank = assertion.oracle_strength.rank();
            if rank > best_rank {
                best_rank = rank;
                best_kind = assertion.oracle_kind.clone();
            }
        }
    }

    (best_rank, best_kind)
}

/// Classify a changed TypeScript/JavaScript line and return a finding.
///
/// `workspace_root` enforces package-local ownership when `Some`: a test in
/// `packages/b/` will not be selected as an owner relation for a source file in
/// `packages/a/`.  Pass `None` to preserve the previous single-package
/// behaviour (used in unit tests).
///
/// `reexport_index` enables single-hop re-export tracing for test discovery.
/// Pass `&ReExportIndex::empty()` to disable (backward-compatible for unit tests).
// 8 parameters — all are structurally distinct context tokens required by the
// TypeScript classifier pipeline; bundling them would force a heap allocation
// per call.  The count is stable; no further parameters are planned.
#[allow(
    clippy::too_many_arguments,
    reason = "8 structurally-distinct context tokens; bundling forces heap allocation; count is stable"
)]
pub(crate) fn classify_change(
    file: &Path,
    line: usize,
    line_text: &str,
    owners: &[TypeScriptOwner],
    all_tests: &[TypeScriptTest],
    workspace_root: Option<&Path>,
    reexport_index: &ReExportIndex,
    alias_map: Option<&TsAliasMap>,
) -> Option<Finding> {
    let changed_file = normalized_path(file);
    let owner = owners
        .iter()
        .filter(|owner| normalized_path(&owner.file) == changed_file)
        .find(|owner| line >= owner.start_line && line <= owner.end_line)?;
    let related_candidates =
        related_test_candidates(owner, all_tests, workspace_root, reexport_index, alias_map);
    let related = find_related_tests(owner, all_tests, workspace_root, reexport_index, alias_map);
    let bun_array_buffer_facts = collect_related_bun_array_buffer_facts(&related_candidates);
    let bun_bridge_hints = collect_related_bun_bridge_hints(&bun_array_buffer_facts);
    let mock_paths = collect_related_mock_paths(owner, all_tests);
    let static_limit = static_limit_for_change(line_text, owner, &mock_paths);

    // Collect named TS-specific limitations (RIPR-SPEC-0085 §PR4 taxonomy).
    // These are ADDITIVE evidence lines; existing `static_limit_kind` is unchanged.
    let named_limitations_from_static: Vec<TypeScriptNamedLimitation> = static_limit
        .as_ref()
        .and_then(|limit| named_limitation_for_static_limit(limit, file, line))
        .into_iter()
        .collect();
    // Ownership-resolution limitations (RIPR-SPEC-0085 §PR6):
    // Emitted when a cross-package test references the owner by name but is
    // excluded by the package-local filter — the target is unresolvable.
    // Only active when workspace_root is supplied (i.e. in the live pipeline).
    let named_limitations_from_ownership: Vec<TypeScriptNamedLimitation> =
        if let Some(root) = workspace_root {
            named_limitations_for_unresolved_ownership(owner, all_tests, root)
        } else {
            Vec::new()
        };

    // Path-alias unresolved disclosure (RIPR-SPEC-0099 always-on honesty):
    // When a test has a non-relative, name-matched import that was NOT credited
    // as an owner relation, emit the limitation to explain the potential gap.
    // This fires regardless of `resolve_tsconfig_paths` (always-on disclosure).
    // The `related` vec is not yet computed at this point; use `related_candidates`
    // to derive which tests were actually credited.
    let credited_test_files: std::collections::HashSet<PathBuf> = related_candidates
        .iter()
        .map(|c| c.test.file.clone())
        .collect();
    let named_limitations_from_alias: Vec<TypeScriptNamedLimitation> =
        named_limitations_for_alias_unresolved(owner, all_tests, |test| {
            credited_test_files.contains(&test.file)
        });
    // Oracle-based limitations fire from oracle-eligible candidates even when
    // there is no static_limit. We always compute them; they are empty when there
    // are no oracle-eligible candidates or no qualifying assertions.
    let named_limitations_from_oracle =
        named_limitations_for_oracle_candidates(owner, &related_candidates);
    // Oracle metadata evidence lines (RIPR-SPEC-0085 §PR5).
    // Emitted from the strongest oracle-eligible assertion across candidates.
    // ADDITIVE: does not change oracle_kind, oracle_strength, static_limit_kind,
    // or repair_packet_ready. At most one assertion's metadata is emitted
    // (the strongest, by oracle_strength rank) to avoid redundant evidence.
    let probe_shape = classify_probe_shape_detail(line_text);
    let oracle_metadata_lines: Vec<String> =
        collect_oracle_metadata_evidence_lines(&probe_shape.family, &related_candidates);
    let has_oracle_eligible_relation = related_candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle());

    // RIPR-SPEC-0104: compute strongest_strength/strongest_kind at the
    // ASSERTION level, filtered by probe_family↔oracle_kind match.
    //
    // The former approach iterated `related: Vec<RelatedTest>` whose oracle_kind
    // was already collapsed to the test's overall-strongest assertion via
    // `strongest_assertion()`. That collapsed kind may be wrong-family:
    // e.g. a test with `.toThrow(DiscountError)` (Strong, ErrorPath-matching)
    // AND `.toBeGreaterThan(0)` (Weak, ReturnValue-matching) collapses to
    // `ExactErrorVariant/Strong`, which then falsely promoted a ReturnValue
    // seam to `Exposed / strong_oracle_observed`.
    //
    // We now iterate `related_candidates` (which carry the full `.test.assertions`
    // slice) and filter each assertion by `ts_oracle_kind_matches_seam`. This
    // lets a multi-assertion test contribute its family-matching assertion even
    // when its overall-strongest assertion is wrong-family (anti-over-correction).
    let (strongest_strength, strongest_kind) =
        strongest_family_matching_oracle(&probe_shape.family, &related_candidates);
    let mock_payload_oracle = related_mock_payload_oracle(&related);

    // Move flow_sink computation here so it is available to the observation
    // guard and the missing_discriminators gate below (RIPR-SPEC-0098).
    let flow_sink = typescript_flow_sink_for(&probe_shape, owner, line, line_text);

    // RIPR-SPEC-0098: observation guard — fires BEFORE promoting to Exposed.
    // When the strong-oracle precondition would hold, verify that at least one
    // strong assertion actually observes the changed sub-expression.  When this
    // proof fails, fall through to the downgraded WeaklyExposed arm instead.
    //
    // RIPR-SPEC-0027 boundary witness: a changed predicate is only `Exposed`
    // when a strong assertion observes an owner call at the changed boundary,
    // and an ambiguous fallback shape is never `Exposed`.
    let strong_oracle_present = strongest_strength >= OracleStrength::Strong.rank();
    let boundary_witnessed = !strong_oracle_present
        || ts_predicate_boundary_is_witnessed(
            &probe_shape,
            line_text,
            owner,
            &related_candidates,
            alias_map,
            workspace_root,
        );
    let observation_confirmed = strong_oracle_present
        && boundary_witnessed
        && ts_changed_value_is_observed(&probe_shape, line_text, &owner.name, &related_candidates);

    let (class, reach_state, observe_state, discriminate_state, mut missing) = if related.is_empty()
    {
        (
            ExposureClass::NoStaticPath,
            StageState::No,
            StageState::No,
            StageState::No,
            vec![no_static_path_missing(owner)],
        )
    } else if !has_oracle_eligible_relation {
        (
            ExposureClass::WeaklyExposed,
            StageState::Weak,
            StageState::Weak,
            StageState::Weak,
            vec![format!(
                "Only heuristic TypeScript test links were found for `{}`; verify the suggested test location or add a direct Jest/Vitest owner call with an exact-value assertion.",
                owner.name
            )],
        )
    } else if strongest_strength >= OracleStrength::Strong.rank() && observation_confirmed {
        (
            ExposureClass::Exposed,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            vec![format!(
                "Related test reaches `{}` with a `{}` oracle. Static evidence suggests the changed behavior is observed under an exact-value or exact-error-variant discriminator.",
                owner.name,
                strongest_kind.as_str()
            )],
        )
    } else if strongest_strength >= OracleStrength::Strong.rank() {
        // Strong oracle exists but observation guard failed: downgrade to
        // WeaklyExposed with a named limitation (RIPR-SPEC-0098), or the
        // predicate boundary is not witnessed (RIPR-SPEC-0027).
        let named_limitation = if boundary_witnessed {
            ts_observation_guard_limitation(&probe_shape, line_text)
        } else {
            ts_predicate_boundary_limitation(&probe_shape, line_text, &owner.name)
        };
        (
            ExposureClass::WeaklyExposed,
            StageState::Yes,
            StageState::Weak,
            StageState::Weak,
            vec![named_limitation],
        )
    } else {
        (
            ExposureClass::WeaklyExposed,
            StageState::Yes,
            StageState::Weak,
            StageState::Weak,
            vec![weak_oracle_missing_summary(
                &owner.name,
                &strongest_kind,
                &probe_shape.family,
                mock_payload_oracle.as_deref(),
            )],
        )
    };
    if let Some(limit) = &static_limit {
        missing.push(limit.missing.clone());
    }

    let missing_discriminators = if matches!(class, ExposureClass::WeaklyExposed)
        && has_oracle_eligible_relation
        && static_limit.is_none()
    {
        typescript_missing_discriminators(&probe_shape, line, line_text, flow_sink.as_ref())
    } else {
        Vec::new()
    };

    let id_path: String = file
        .display()
        .to_string()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let family = probe_shape.family.clone();
    let delta = probe_shape.delta.clone();
    let expected_sinks = if probe_shape.specific {
        probes::expected_sinks(line_text, &family)
    } else {
        Vec::new()
    };
    let required_oracles = if probe_shape.specific {
        probes::required_oracles(line_text, &family)
    } else {
        Vec::new()
    };
    let owner_sym = owner.symbol_id();
    let ts_probe_id = fingerprint_probe_id(
        "probe",
        &id_path,
        "typescript_preview",
        owner_sym.0.as_str(),
        &normalize_expression(line_text),
        1,
    );
    let probe = Probe {
        id: ts_probe_id,
        location: SourceLocation::new(file.to_string_lossy().as_ref(), line, 1),
        owner: Some(owner_sym),
        family: family.clone(),
        delta,
        before: None,
        after: Some(line_text.to_string()),
        expression: line_text.to_string(),
        expected_sinks,
        required_oracles,
    };
    let actionability = typescript_actionability_for(
        &class,
        static_limit.as_ref(),
        has_oracle_eligible_relation,
        &missing_discriminators,
        observed_evidence_label(&related),
    );
    missing.push(actionability.missing_summary());

    let related_count = related.len();
    let reach_summary = format!(
        "{} related test(s) found for owner `{}`",
        related_count, owner.name
    );
    let reach = StageEvidence::new(reach_state.clone(), Confidence::Low, &reach_summary);
    let infect = StageEvidence::new(
        StageState::Unknown,
        Confidence::Low,
        "TypeScript preview adapter does not yet model infection.",
    );
    let propagate = StageEvidence::new(
        StageState::Unknown,
        Confidence::Low,
        "TypeScript preview adapter does not yet model propagation.",
    );
    let observe_summary = format!(
        "Strongest extracted oracle kind: `{}` (rank {})",
        strongest_kind.as_str(),
        strongest_strength
    );
    let observe = StageEvidence::new(observe_state, Confidence::Low, &observe_summary);
    let discriminate_summary = if strongest_strength >= OracleStrength::Strong.rank()
        && observation_confirmed
    {
        format!(
            "Related test uses a `{}` oracle; static evidence suggests the changed behavior is discriminated.",
            strongest_kind.as_str()
        )
    } else if strongest_strength >= OracleStrength::Strong.rank() && !boundary_witnessed {
        // RIPR-SPEC-0027: strong oracle exists but no assertion reaches the
        // changed predicate boundary.
        match missing_discriminators.first() {
            Some(discriminator) => format!(
                "TypeScript preview adapter: strong oracle found but no strong assertion observes an owner call at the changed predicate boundary; missing proof: `{}`.",
                discriminator.value
            ),
            None => "TypeScript preview adapter: strong oracle found but the changed predicate boundary is not statically witnessed by any strong assertion; discriminate unknown.".to_string(),
        }
    } else if strongest_strength >= OracleStrength::Strong.rank() {
        // RIPR-SPEC-0098: strong oracle exists but observation guard failed.
        "TypeScript preview adapter: strong oracle found but no assertion's observed_expression flows from the changed sub-expression; observation_unverified — discriminate unknown.".to_string()
    } else if let Some(discriminator) = missing_discriminators.first() {
        format!(
            "TypeScript preview adapter found no strong discriminator; missing proof: `{}`.",
            discriminator.value
        )
    } else {
        "TypeScript preview adapter found no strong discriminator; use `toBe` / `toEqual` / `toStrictEqual` to escalate. TypeScript `toThrow` forms remain broad error evidence until payload inspection lands.".to_string()
    };
    let discriminate =
        StageEvidence::new(discriminate_state, Confidence::Low, &discriminate_summary);

    let recommended = if let Some(limit) = &static_limit {
        format!(
            "TypeScript preview advisory: static limit `{}`; {}; no actionable repair packet is emitted.",
            limit.kind.as_str(),
            limit.repair_route
        )
    } else {
        match &class {
        ExposureClass::Exposed => {
            "TypeScript preview advisory: changed behavior is observed under a strong oracle; verify the assertion targets the changed boundary value.".to_string()
        }
        ExposureClass::NoStaticPath => {
            no_static_path_recommendation(owner)
        }
        _ if !has_oracle_eligible_relation => {
            "TypeScript preview advisory: related-test proximity is heuristic only; add a direct owner call before treating this as an actionable repair target.".to_string()
        }
        _ if let Some(discriminator) = missing_discriminators.first() => {
            weak_oracle_recommendation(
                &strongest_kind,
                &discriminator.value,
                mock_payload_oracle.as_deref(),
            )
        }
        _ if owner.owner_kind == OwnerKind::ModuleFunction => {
            format!(
                "TypeScript preview advisory: related module-initializer observer reaches `{}` but no safe target shape is available; add an exact value assertion for the exported value and keep the finding advisory until repair-card fields are complete.",
                owner.name
            )
        }
        _ => {
            "TypeScript preview advisory: add a test that exercises the changed behavior with an exact-value assertion (`toBe` / `toEqual` / `toStrictEqual`); no actionable repair packet is emitted until the target shape is explicit.".to_string()
        }
        }
    };
    let confidence_value = if matches!(class, ExposureClass::Exposed) {
        0.6
    } else {
        0.4
    };

    let mut evidence = vec![format!("owner: {}", owner.name)];
    if !probe_shape.specific {
        evidence.push("probe_fact: ambiguous_fallback".to_string());
    }
    for discriminator in &missing_discriminators {
        evidence.push(format!("missing_discriminator: {}", discriminator.value));
    }
    if let Some(oracle) = &mock_payload_oracle {
        evidence.push(format!("mock_payload_evidence: {oracle}"));
    }
    for fact in &bun_array_buffer_facts {
        evidence.push(fact.evidence_line());
    }
    for hint in &bun_bridge_hints {
        evidence.extend(hint.evidence_lines());
    }
    if let Some(limit) = &static_limit {
        evidence.extend(limit.evidence.iter().cloned());
    }
    // Emit additive named limitation evidence lines (RIPR-SPEC-0085 §PR4/PR6).
    // These lines are ADDITIVE — they do not change any existing field value.
    // `named_limitations_from_alias` fires on the always-on alias-gap disclosure
    // (RIPR-SPEC-0099): non-relative name-matched imports that were not credited.
    for named_limit in named_limitations_from_static
        .iter()
        .chain(named_limitations_from_oracle.iter())
        .chain(named_limitations_from_ownership.iter())
        .chain(named_limitations_from_alias.iter())
    {
        evidence.extend(named_limit.evidence_lines());
    }
    // Emit additive oracle metadata evidence lines (RIPR-SPEC-0085 §PR5).
    // ADDITIVE: oracle_kind, oracle_strength, static_limit_kind, and
    // repair_packet_ready are unchanged. These lines surface the structured
    // metadata needed for a future repair packet.
    evidence.extend(oracle_metadata_lines);
    evidence.extend(actionability.evidence(typescript_raw_evidence_ref(
        file,
        line,
        Some(owner),
        &probe.id.0,
    )));
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.is_uncertain())
    {
        evidence.push(format!(
            "related_test_relation: {} ({})",
            candidate.relation.as_str(),
            candidate.test.name
        ));
        evidence.push(format!(
            "related_test_uncertain: {} ({})",
            candidate.relation.as_str(),
            candidate.test.name
        ));
    }
    // Resolved from the probe's own delta evidence (#3281) before the probe
    // moves into the finding: TypeScript probes are seeded from head-side
    // added lines.
    let source_currentness = crate::domain::SourceCurrentness::from_probe_delta(
        probe.before.as_deref(),
        probe.after.as_deref(),
    );
    Some(Finding {
        id: probe.id.0.clone(),
        canonical_gap: None,
        probe,
        class,
        ripr: RiprEvidence {
            reach,
            infect,
            propagate,
            reveal: RevealEvidence {
                observe,
                discriminate,
            },
        },
        confidence: confidence_value,
        evidence,
        missing,
        flow_sinks: flow_sink.into_iter().collect(),
        activation: ActivationEvidence {
            observed_values: Vec::new(),
            missing_discriminators,
        },
        stop_reasons: Vec::new(),
        related_tests: related,
        recommended_next_step: Some(recommended),
        language: Some(output_language_for(file)),
        language_status: Some(LanguageStatus::Preview),
        owner_kind: Some(owner.owner_kind),
        static_limit_kind: static_limit.map(|limit| limit.kind),
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        // Resolved above, before the probe moved into the finding (#3281).
        source_currentness,
    })
}

fn observed_evidence_label(related: &[RelatedTest]) -> &'static str {
    if related.iter().any(|test| {
        test.oracle
            .as_deref()
            .is_some_and(|oracle| oracle.starts_with("t."))
    }) {
        "related TypeScript `t.*` evidence"
    } else {
        "related Jest/Vitest evidence"
    }
}

pub(crate) fn no_static_path_missing(owner: &TypeScriptOwner) -> String {
    match owner.owner_kind {
        OwnerKind::Method => format!(
            "No trusted TypeScript method receiver relation for `{}`. Direct `new ClassName(...)` receiver calls are supported, but factories, dependency injection, mocked modules, prototype aliases, and dynamic property access stay ambiguous in preview.",
            owner.name
        ),
        OwnerKind::ClassMethod => format!(
            "No trusted TypeScript class-method relation for `{}`. Direct same-file or imported `Class.method(...)` calls are supported, but local shadows, mocked modules, namespace chains, dynamic member access, and missing class-name context stay ambiguous in preview.",
            owner.name
        ),
        OwnerKind::ModuleFunction => format!(
            "No trusted TypeScript module-initializer observer for `{}`. Direct `expect(IMPORTED_CONST)...` and `expect(namespace.EXPORT)...` observers are supported, but helper-derived values, shadowed aliases, dynamic initialization, and non-expect references stay advisory in preview.",
            owner.name
        ),
        _ => format!(
            "No test references `{}(` — add a test that calls the changed owner.",
            owner.name
        ),
    }
}

pub(crate) fn no_static_path_recommendation(owner: &TypeScriptOwner) -> String {
    match owner.owner_kind {
        OwnerKind::Method => {
            "TypeScript preview advisory: method receiver relation is missing context; use a direct `new ClassName(...)` receiver observer when safe, and keep factories, dependency injection, mocked modules, prototype aliases, and dynamic property access advisory.".to_string()
        }
        OwnerKind::ClassMethod => {
            "TypeScript preview advisory: class-method relation is missing context; use a direct same-file or imported `Class.method(...)` observer when safe, and keep local shadows, mocked modules, namespace chains, dynamic member access, and missing class-name context advisory.".to_string()
        }
        OwnerKind::ModuleFunction => {
            "TypeScript preview advisory: module initializer observer is missing context; add a direct `expect(IMPORTED_CONST).toBe(...)` or `expect(namespace.EXPORT).toEqual(...)` observer when safe, and keep helper-derived or dynamic initialization evidence advisory.".to_string()
        }
        _ => {
            "TypeScript preview advisory: no test references the changed owner; add a test that calls the owner and asserts the changed behavior with `toBe` / `toEqual` before any repair packet is emitted.".to_string()
        }
    }
}
