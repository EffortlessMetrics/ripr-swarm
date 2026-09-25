//! TypeScript preview → complete repair-packet projection (RIPR-SPEC-0087 §PR7).
//!
//! This module implements the **projection** side of the
//! TypeScript-actionability gate: it reads evidence lines from a
//! `preview`-language finding and — when every precondition in §1.2 holds —
//! builds a `GapRecord` that can be fed to the **shared** Rust validator
//! `validate_agent_gap_record_packet` to determine `repair_packet_ready`.
//!
//! Architectural constraints (non-negotiable):
//! - The validator lives in `output::agent_seam_packets`; this module may call
//!   it because both modules are inside `output/`.
//! - Analysis modules (`analysis/**`) must NOT import `crate::output`, so the
//!   projection CANNOT live there.
//! - No parallel TypeScript-specific completeness validator is introduced.
//!   The only flip gate is `validate_agent_gap_record_packet(..) == Ok(())`.

use crate::agent::loop_commands::shell_arg;
use crate::domain::Finding;
use crate::output::gap_decision_ledger::{
    GapAnchor, GapRecord, GapRepairRoute, ProjectionEligibility,
};
use std::collections::BTreeMap;

/// The authority-boundary string carried by all TypeScript preview packets.
const TS_AUTHORITY_BOUNDARY: &str = "preview_advisory_only";

/// The target assertion shape projected for a TypeScript repair packet,
/// together with the static reachability verdict that produced it (#4105).
///
/// A complete packet must not present the observed call input as the shape for
/// the missing discriminator when that input statically cannot reach the named
/// boundary: an agent following the packet verbatim would duplicate a
/// non-discriminating assertion while the packet claims "complete and
/// delegatable".
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TargetAssertionShape {
    /// The observed oracle shape stands: the observed call input either hits
    /// the missing-discriminator boundary, or its reachability is not
    /// statically decidable from the available evidence (fail-open is allowed
    /// only where non-reach cannot be proven).
    Observed { shape: String },
    /// The observed call input provably does NOT reach the named boundary.
    /// `shape` carries an explicit boundary placeholder instead of the
    /// observed input, and the packet must fail closed through the shared
    /// validator.
    Unreachable {
        shape: String,
        observed_call: String,
        discriminator: String,
    },
}

impl TargetAssertionShape {
    fn shape(&self) -> &str {
        match self {
            Self::Observed { shape } => shape,
            Self::Unreachable { shape, .. } => shape,
        }
    }

    /// Shared-validator ineligibility reason when the observed call input
    /// provably cannot reach the named boundary. `None` keeps the packet
    /// eligible through the normal complete-contract projection.
    fn packet_ineligibility_reason(&self) -> Option<String> {
        match self {
            Self::Observed { .. } => None,
            Self::Unreachable {
                observed_call,
                discriminator,
                ..
            } => Some(format!(
                "observed call input `{observed_call}` does not reach the missing \
                 discriminator `{discriminator}`; derive an input that hits the \
                 boundary before the packet is delegatable"
            )),
        }
    }
}

/// Derive the packet's target assertion shape from the observed oracle
/// evidence (issue #4105).
///
/// - No observed expression, no parseable discriminator comparison, or no
///   statically decidable verdict → the observed shape stands unchanged.
/// - The observed call input hits the boundary → the observed shape stands
///   (landed behavior for boundary-hitting fixtures).
/// - The observed call input provably does NOT hit the boundary → a
///   placeholder shape naming the boundary (repo convention
///   `/* boundary input for <discriminator> */`) replaces the observed input;
///   the caller fails the packet closed via the shared validator.
pub(crate) fn typescript_target_assertion_shape(
    oracle_observed: &str,
    oracle_expected: &str,
    missing_discriminator: Option<&str>,
    owner_name: Option<&str>,
) -> TargetAssertionShape {
    let observed_shape = format!("expect({oracle_observed}).toBe({oracle_expected})");
    let observed = || TargetAssertionShape::Observed {
        shape: observed_shape.clone(),
    };
    let Some(discriminator) = missing_discriminator
        .map(str::trim)
        .filter(|d| !d.is_empty())
    else {
        return observed();
    };
    let Some(call) = parse_static_call_expression(oracle_observed) else {
        return observed();
    };
    if !call_callee_is_owner(&call.callee, owner_name) {
        return observed();
    }
    let Some(comparison) = parse_static_comparison(discriminator) else {
        return observed();
    };
    match static_argument_reaches_boundary(&call, &comparison) {
        Some(true) | None => observed(),
        Some(false) => TargetAssertionShape::Unreachable {
            shape: format!(
                "{}(/* boundary input for {discriminator} */).toBe({oracle_expected})",
                call.callee
            ),
            observed_call: oracle_observed.to_string(),
            discriminator: discriminator.to_string(),
        },
    }
}

/// A statically parseable call expression: `callee(arg, ...)`.
struct StaticCall {
    callee: String,
    args: Vec<String>,
}

/// Parse a plain call expression `name(a, b)` (dotted callee allowed).
///
/// Returns `None` for anything else — variables (`result`), member tails
/// (`login('alice').length`), or wrapped calls (`wrap(login('alice'))`) — so
/// reachability stays undecided instead of binding the wrong expression.
fn parse_static_call_expression(expr: &str) -> Option<StaticCall> {
    let expr = expr.trim();
    let open = expr.find('(')?;
    let close = expr.rfind(')')?;
    if close != expr.len() - 1 || close < open {
        return None;
    }
    let callee = expr[..open].trim();
    let mut callee_chars = callee.chars();
    let first_ok = callee_chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$');
    if !first_ok || !callee_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '$'))
    {
        return None;
    }
    let inner = &expr[open + 1..close];
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level_args(inner)
    };
    Some(StaticCall {
        callee: callee.to_string(),
        args,
    })
}

/// Split call arguments on top-level commas, respecting nesting and quotes.
fn split_top_level_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut current = String::new();
    for ch in inner.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                }
                current.push(ch);
            }
            None => match ch {
                '\'' | '"' | '`' => {
                    quote = Some(ch);
                    current.push(ch);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    current.push(ch);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    current.push(ch);
                }
                ',' if depth == 0 => {
                    args.push(current.trim().to_string());
                    current.clear();
                }
                _ => current.push(ch),
            },
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// The observed oracle expression counts as owner evidence only when its
/// callee resolves to the finding's owner short name (dotted callees compare
/// by their last segment). Without an owner identity the reachability verdict
/// is refused — never judged.
fn call_callee_is_owner(callee: &str, owner_name: Option<&str>) -> bool {
    let Some(owner) = owner_name.map(str::trim).filter(|o| !o.is_empty()) else {
        return false;
    };
    callee
        .rsplit('.')
        .next()
        .is_some_and(|short| short == owner)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StaticComparisonOp {
    Equal,        // == / ===
    NotEqual,     // != / !==
    GreaterEqual, // >=
    LessEqual,    // <=
    Greater,      // >
    Less,         // <
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StaticLiteral {
    Int(i64),
    Str(String),
}

struct StaticComparison {
    receiver_is_length: bool,
    op: StaticComparisonOp,
    boundary: StaticLiteral,
}

/// Parse a discriminator like `user.length == 3` into a static comparison.
///
/// Conservative: only `<ident>` / `<ident>.length` receivers and int or plain
/// string boundary literals are recognized; anything else stays undecided.
fn parse_static_comparison(discriminator: &str) -> Option<StaticComparison> {
    const OPS: [(&str, StaticComparisonOp); 8] = [
        ("===", StaticComparisonOp::Equal),
        ("!==", StaticComparisonOp::NotEqual),
        ("==", StaticComparisonOp::Equal),
        ("!=", StaticComparisonOp::NotEqual),
        (">=", StaticComparisonOp::GreaterEqual),
        ("<=", StaticComparisonOp::LessEqual),
        (">", StaticComparisonOp::Greater),
        ("<", StaticComparisonOp::Less),
    ];
    let mut best: Option<(usize, usize, StaticComparisonOp)> = None;
    for (symbol, op) in OPS {
        if let Some(position) = discriminator.find(symbol) {
            let better = match best {
                None => true,
                Some((best_position, best_len, _)) => {
                    position < best_position
                        || (position == best_position && symbol.len() > best_len)
                }
            };
            if better {
                best = Some((position, symbol.len(), op));
            }
        }
    }
    let (position, len, op) = best?;
    let receiver = discriminator[..position].trim();
    let boundary_raw = discriminator[position + len..].trim();
    let receiver_is_length = parse_static_receiver(receiver)?;
    let boundary = parse_static_literal(boundary_raw)?;
    Some(StaticComparison {
        receiver_is_length,
        op,
        boundary,
    })
}

/// Recognize `<ident>` or `<ident>.length` receivers; other shapes (member
/// chains, index expressions) are not statically decidable. Returns whether
/// the receiver is a `.length` probe.
fn parse_static_receiver(receiver: &str) -> Option<bool> {
    if is_plain_identifier(receiver) {
        return Some(false);
    }
    let stem = receiver.strip_suffix(".length")?;
    if is_plain_identifier(stem) {
        Some(true)
    } else {
        None
    }
}

fn is_plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$');
    first_ok && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// A quoted string literal without escape sequences. Escapes make the static
/// length and byte-for-byte equality unreliable, so they fail open instead.
fn parse_plain_string_literal(raw: &str) -> Option<String> {
    let quote = raw.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    if raw.chars().count() < 2 || !raw.ends_with(quote) {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    if inner.contains('\\') {
        return None;
    }
    Some(inner.to_string())
}

fn parse_static_literal(raw: &str) -> Option<StaticLiteral> {
    if let Ok(value) = raw.parse::<i64>() {
        return Some(StaticLiteral::Int(value));
    }
    parse_plain_string_literal(raw).map(StaticLiteral::Str)
}

fn apply_static_comparison(op: StaticComparisonOp, lhs: i64, rhs: i64) -> bool {
    match op {
        StaticComparisonOp::Equal => lhs == rhs,
        StaticComparisonOp::NotEqual => lhs != rhs,
        StaticComparisonOp::GreaterEqual => lhs >= rhs,
        StaticComparisonOp::LessEqual => lhs <= rhs,
        StaticComparisonOp::Greater => lhs > rhs,
        StaticComparisonOp::Less => lhs < rhs,
    }
}

/// Statically decide whether the observed call's argument satisfies the
/// discriminator comparison: `Some(true)` reaches the boundary, `Some(false)`
/// provably does not, `None` is undecided.
///
/// The discriminator receiver binds to the observed argument only when the
/// call has exactly one argument: without signature evidence a multi-argument
/// binding would be a guess, and a wrong "cannot reach" verdict is worse than
/// no verdict.
fn static_argument_reaches_boundary(
    call: &StaticCall,
    comparison: &StaticComparison,
) -> Option<bool> {
    if call.args.len() != 1 {
        return None;
    }
    let argument = call.args[0].trim();
    if comparison.receiver_is_length {
        let text = parse_plain_string_literal(argument)?;
        let StaticLiteral::Int(boundary) = comparison.boundary else {
            return None;
        };
        let length = text.chars().count() as i64;
        return Some(apply_static_comparison(comparison.op, length, boundary));
    }
    match &comparison.boundary {
        StaticLiteral::Int(boundary) => {
            let value = argument.parse::<i64>().ok()?;
            Some(apply_static_comparison(comparison.op, value, *boundary))
        }
        StaticLiteral::Str(boundary) => {
            let value = parse_plain_string_literal(argument)?;
            match comparison.op {
                StaticComparisonOp::Equal => Some(value == *boundary),
                StaticComparisonOp::NotEqual => Some(value != *boundary),
                // Relational comparisons over strings are not statically
                // decided here.
                _ => None,
            }
        }
    }
}

/// Project a TypeScript preview finding into a `GapRecord` for validator
/// consumption, applying all §1.2 preconditions (G-A through G-G).
///
/// Returns `None` whenever ANY precondition fails — the false case requires
/// no positive proof. Only `Ok(())` from the shared validator leads to a flip.
///
/// # Preconditions checked (fail-closed)
/// - G-A: `actionability_category == "incomplete_repair_packet"`
/// - G-C: no named TypeScript limitation, plus a non-dynamic oracle with
///   `expected_value_or_variant` present (`typescript_oracle_expected` evidence)
/// - G-D: oracle-eligible relation (not `ambiguous_related_test`; verified by G-A)
/// - G-E: non-empty `missing_discriminators` list (a target shape exists)
/// - G-F: no cross-language bridge limitation evidence present
/// - G-G (#4105): the observed oracle call input must reach — or be statically
///   undecidable against — the named discriminator boundary. A provably
///   non-reaching input keeps the record projected with a boundary placeholder
///   shape but fails the packet closed (`agent_packet` ineligible).
pub(crate) fn typescript_gap_record_for(finding: &Finding) -> Option<GapRecord> {
    // G-A: only the terminal `incomplete_repair_packet` branch is eligible.
    let category = evidence_value(finding, "actionability_category: ")?;
    if category != "incomplete_repair_packet" {
        return None;
    }

    if has_named_typescript_limitation(finding) {
        return None;
    }

    // G-C: non-dynamic oracle: `typescript_oracle_expected` must be present.
    let oracle_expected = evidence_value(finding, "typescript_oracle_expected: ")?;
    if oracle_expected.is_empty() {
        return None;
    }
    // Fail-closed: if any dynamic-assertion limitation was emitted, stay preview.
    if finding
        .evidence
        .iter()
        .any(|line| line == "typescript_limitation: typescript_dynamic_assertion_unresolved")
    {
        return None;
    }

    // G-D: oracle-eligible relation is guaranteed by G-A (ambiguous_related_test
    // is a different category and is excluded above). Verify also that there is
    // actually a related test with an oracle-eligible file we can use.
    let related_test = finding
        .related_tests
        .iter()
        .max_by_key(|t| t.oracle_strength.rank())?;
    let test_file = related_test.file.display().to_string().replace('\\', "/");
    if test_file.is_empty() {
        return None;
    }

    // G-E: the finding must have at least one named missing discriminator
    // (the `missing_target_shape` guard in actionability.rs already passed,
    // but we double-check here so the projection is self-contained).
    if finding.activation.missing_discriminators.is_empty() {
        return None;
    }

    // G-F: no unresolved cross-language oracle visibility limitation.
    if finding.evidence.iter().any(|line| {
        line.starts_with("route_cross_language_oracle_visibility_limitation:")
            || line.starts_with("typescript_bun_bridge_verdict:")
            || line
                .starts_with("typescript_limitation: typescript_cross_language_bridge_unresolved")
    }) {
        return None;
    }

    // Derive `canonical_gap_id` from the finding id (§3.2).
    // The finding id is already content-addressed with path normalized \→/ in
    // `fingerprint_probe_id` — we reuse the last two segments (family:fp8)
    // and prepend `gap:typescript:`.
    // F13: normalize before hashing (the finding id already has / separators,
    // but we defensively strip any remaining backslashes from the id string).
    let canonical_gap_id = typescript_canonical_gap_id(&finding.id);

    // Build verify command from evidence (§3.1 — existing producer).
    let verify_command = evidence_value(finding, "typescript_verify_command: ")?;
    if verify_command.is_empty() {
        return None;
    }

    // Build receipt command (§3.2 new producer F6/F7):
    // A fixed `ripr outcome … target/ripr/receipts/<canonical_gap_id>.targeted-test-outcome.json`
    // command — no external provider, no interpolation of free text.
    let receipt_command = typescript_receipt_command(&canonical_gap_id, verify_command);

    // Build repair_route from the finding (§3.1 — test file from related test).
    // The route_kind is derived from the probe family / missing discriminator.
    let missing_discriminator = finding
        .activation
        .missing_discriminators
        .first()
        .map(|d| d.value.clone());
    let owner_name = finding
        .probe
        .owner
        .as_ref()
        .and_then(|sym| sym.0.rsplit("::").next())
        .or_else(|| evidence_value(finding, "owner: "))
        .map(ToString::to_string);
    let route_kind = typescript_route_kind_for(&finding.probe.family);
    // Target shape + static reachability of the observed call input against
    // the named discriminator boundary (#4105): a packet must not present a
    // provably non-reaching observed input as the shape for the boundary —
    // an agent following it verbatim would duplicate a non-discriminating
    // assertion.
    let target_shape = evidence_value(finding, "typescript_oracle_observed: ").map(|observed| {
        typescript_target_assertion_shape(
            observed,
            oracle_expected,
            missing_discriminator.as_deref(),
            owner_name.as_deref(),
        )
    });
    let assertion_shape = target_shape.as_ref().map(|shape| shape.shape().to_string());
    let mut stop_conditions = vec![
        "Stop if the gap record is no longer present or loses agent-packet eligibility."
            .to_string(),
        "Stop if the verification command cannot run from this workspace.".to_string(),
    ];
    if let Some(boundary_stop) = target_shape.as_ref().and_then(|shape| match shape {
        TargetAssertionShape::Observed { .. } => None,
        TargetAssertionShape::Unreachable { discriminator, .. } => Some(format!(
            "Do not reuse the observed call input; it cannot reach the missing \
             discriminator `{discriminator}`. Derive an input that hits the \
             boundary first."
        )),
    }) {
        stop_conditions.push(boundary_stop);
    }

    let repair_route = GapRepairRoute {
        route_kind: route_kind.to_string(),
        target_file: Some(test_file.clone()),
        related_test: Some(format!("{test_file}::{}", related_test.name)),
        assertion_shape,
        missing_discriminator,
        changed_behavior: None,
        target_line: if related_test.line > 0 {
            Some(related_test.line as u64)
        } else {
            None
        },
        inspection_command: None,
        stop_conditions,
    };

    // Anchor: probe location + owner from the finding.
    let probe_file = finding
        .probe
        .location
        .file
        .display()
        .to_string()
        .replace('\\', "/");
    let probe_line = finding.probe.location.line as u64;
    let anchor = GapAnchor {
        file: Some(probe_file),
        line: Some(probe_line),
        owner: owner_name,
        dedupe_fingerprint: Some(finding.id.clone()),
    };

    // Projection eligibility: agent_packet eligible through the normal
    // complete-contract path, unless the observed call input provably does
    // not reach the named discriminator boundary (#4105). Then the shared
    // validator fails the packet closed while the placeholder shape still
    // names the boundary — a complete packet must not instruct a duplicate
    // of a non-discriminating assertion.
    let boundary_ineligibility = target_shape
        .as_ref()
        .and_then(TargetAssertionShape::packet_ineligibility_reason);
    let mut projection_eligibility = BTreeMap::new();
    projection_eligibility.insert(
        "agent_packet".to_string(),
        ProjectionEligibility {
            eligible: boundary_ineligibility.is_none(),
            reason: boundary_ineligibility.unwrap_or_else(|| {
                "TypeScript preview complete-contract projection (RIPR-SPEC-0087)".to_string()
            }),
        },
    );

    // Evidence IDs: the finding's own id.
    let evidence_ids = vec![finding.id.clone()];

    Some(GapRecord {
        source_currentness: Some(finding.source_currentness.as_str().to_string()),
        gap_id: finding.id.clone(),
        canonical_gap_id,
        seam_id: None,
        kind: "typescript_preview_boundary".to_string(),
        language: "typescript".to_string(),
        language_status: "preview".to_string(),
        scope: "diff".to_string(),
        evidence_class: "weakly_exposed".to_string(),
        gap_state: "advisory".to_string(),
        policy_state: "preview".to_string(),
        repairability: "repairable".to_string(),
        repair_route: Some(repair_route),
        static_limit_kind: None,
        static_limit_detail: None,
        static_limits: Vec::new(),
        anchor: Some(anchor),
        evidence_ids,
        projection_eligibility,
        verification_commands: vec![verify_command.to_string()],
        command_specs: None,
        receipt_command: Some(receipt_command),
        regeneration_commands: Vec::new(),
        receipt: None,
        safe_gate_predicate: None,
        authority_boundary: TS_AUTHORITY_BOUNDARY.to_string(),
    })
}

/// Derive the content-addressed `gap:typescript:<probe_family>:<fp8>` canonical
/// gap id from the finding's existing content-addressed id (§3.2 / F13).
///
/// The finding id format is `probe:<path>:<family>:<fp8>`. We extract the
/// `<family>` and `<fp8>` segments and build `gap:typescript:<family>:<fp8>`.
/// This avoids introducing a new hash domain and reuses the existing SHA-256 fp8.
pub(crate) fn typescript_canonical_gap_id(finding_id: &str) -> String {
    // finding_id: "probe:src_discount.ts:typescript_preview:1a2b3c4d"
    // Normalize backslashes (defensive, finding ids should already use /).
    let normalized = finding_id.replace('\\', "/");
    // Split on `:` and pick the last two segments as family:fp8.
    let segments: Vec<&str> = normalized.splitn(4, ':').collect();
    if segments.len() == 4 {
        // segments[0]=probe, [1]=path, [2]=family, [3]=fp8
        let family = segments[2];
        let fp8 = segments[3];
        format!("gap:typescript:{family}:{fp8}")
    } else {
        // Fallback: use the whole normalized id as a slug.
        format!("gap:typescript:{normalized}")
    }
}

/// Derive the receipt command for a TypeScript preview finding (§3.2 / F6/F7).
///
/// The command is a fixed `ripr outcome …` invocation that mirrors the Rust
/// receipt shape without any external provider call, curl, or http request.
pub(crate) fn typescript_receipt_command(canonical_gap_id: &str, verify_command: &str) -> String {
    // F7: fixed `ripr outcome` shape only — no external provider or curl.
    // The receipt path uses the canonical_gap_id as a slug (slashes replaced
    // with underscores so the path is a single filename component).
    let slug = canonical_gap_id
        .chars()
        .map(|c| if c == ':' || c == '/' { '_' } else { c })
        .collect::<String>();
    let receipt_path = format!("target/ripr/receipts/{slug}.targeted-test-outcome.json");
    // Route both operator-supplied values through the shared bash encoder
    // rather than wrapping them in double quotes here: a verify command
    // containing `$`, a backtick, or a redirect would otherwise execute when
    // this advisory string is copied into a shell (#2347).
    format!(
        "ripr outcome --before <baseline> --after <repair> --verify-cmd {} --out {}",
        shell_arg(verify_command),
        shell_arg(&receipt_path)
    )
}

/// Map the probe family to a `GapRepairRoute` route_kind (§3.2).
///
/// Routes stay consistent with the Rust taxonomy — no new TS-only route kind.
fn typescript_route_kind_for(family: &crate::domain::ProbeFamily) -> &'static str {
    use crate::domain::ProbeFamily;
    match family {
        ProbeFamily::Predicate => "AddBoundaryAssertion",
        ProbeFamily::ReturnValue => "AddValueAssertion",
        ProbeFamily::ErrorPath => "AddErrorDiscriminator",
        ProbeFamily::FieldConstruction => "AddValueAssertion",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => "AddBoundaryAssertion",
        ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => "AddBoundaryAssertion",
    }
}

/// Extract a value from the finding's evidence vec by prefix.
fn evidence_value<'a>(finding: &'a Finding, prefix: &str) -> Option<&'a str> {
    finding
        .evidence
        .iter()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn has_named_typescript_limitation(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line.starts_with("typescript_limitation: "))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────────────────────────────────────
    // §7.4 Validator-parity test: proves the flip is driven by the shared
    // validator and not a parallel TypeScript-specific path.
    // ──────────────────────────────────────────────────────────────────────

    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, LanguageId,
        LanguageStatus, MissingDiscriminatorFact, OracleKind, OracleStrength, OwnerKind, Probe,
        ProbeFamily, ProbeId, RelatedTest, RevealEvidence, RiprEvidence, SourceLocation,
        StageEvidence, StageState, SymbolId,
    };
    use crate::output::agent_seam_packets::{
        gap_record_packet_do_not_do, validate_agent_gap_record_packet,
    };
    use std::path::PathBuf;

    fn complete_finding() -> Finding {
        Finding {
            id: "probe:src_discount.ts:typescript_preview:a1b2c3d4".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src_discount.ts:typescript_preview:a1b2c3d4".to_string()),
                location: SourceLocation::new("src/discount.ts", 3, 1),
                owner: Some(SymbolId(
                    "typescript:src/discount.ts::applyDiscount".to_string(),
                )),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: Some("  if (amount >= threshold) {".to_string()),
                expression: "  if (amount >= threshold) {".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: StageEvidence::new(StageState::Yes, Confidence::Low, "1 related test"),
                infect: StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    "TypeScript preview adapter does not yet model infection.",
                ),
                propagate: StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    "TypeScript preview adapter does not yet model propagation.",
                ),
                reveal: RevealEvidence {
                    observe: StageEvidence::new(StageState::Weak, Confidence::Low, "weak oracle"),
                    discriminate: StageEvidence::new(
                        StageState::Weak,
                        Confidence::Low,
                        "weak discriminator",
                    ),
                },
            },
            confidence: 0.4,
            evidence: vec![
                "owner: applyDiscount".to_string(),
                "gap_state: advisory".to_string(),
                "actionability_category: incomplete_repair_packet".to_string(),
                "why_not_actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract".to_string(),
                "repair_route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available".to_string(),
                "evidence_needed_to_promote: canonical gap identity, repair kind, target test shape, related observer, verify command, receipt command, raw evidence refs, and edit constraints".to_string(),
                "raw_evidence_ref: leg=rust_seam;file=src/discount.ts;line=3;kind=typescript_preview_probe;source_id=probe:src_discount.ts:typescript_preview:a1b2c3d4;owner=applyDiscount".to_string(),
                "typescript_package_root: .".to_string(),
                "typescript_workspace_root: .".to_string(),
                "typescript_framework_hint: jest".to_string(),
                "typescript_runner_hint: npm".to_string(),
                "typescript_package_confidence: high".to_string(),
                "typescript_verify_command: jest tests/discount.test.ts".to_string(),
                "typescript_oracle_observed: applyDiscount(100, 100)".to_string(),
                "typescript_oracle_expected: 90".to_string(),
                "typescript_oracle_confidence: high".to_string(),
                "typescript_oracle_evidence_ref: tests/discount.test.ts:5".to_string(),
                "missing_discriminator: amount >= threshold".to_string(),
            ],
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence {
                observed_values: Vec::new(),
                missing_discriminators: vec![MissingDiscriminatorFact {
                    value: "amount >= threshold".to_string(),
                    reason: "changed TypeScript equality-boundary at line 3 lacks a concrete preview discriminator".to_string(),
                    flow_sink: None,
                }],
            },
            stop_reasons: Vec::new(),
            related_tests: vec![RelatedTest {
                name: "applyDiscount applies discount".to_string(),
                file: PathBuf::from("tests/discount.test.ts"),
                line: 4,
                oracle_strength: OracleStrength::Weak,
                oracle_kind: OracleKind::ExactValue,
                oracle: Some("expect(...).toBe(...)".to_string()),
                relation_reason: None,
                relation_confidence: None,
            }],
            recommended_next_step: None,
            language: Some(LanguageId::TypeScript),
            language_status: Some(LanguageStatus::Preview),
            owner_kind: Some(OwnerKind::Function),
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    /// §7.4 Validator-parity test (architectural assertion):
    /// The GapRecord produced by `typescript_gap_record_for` for the complete
    /// fixture MUST pass `validate_agent_gap_record_packet`. Any future fork
    /// of the logic would break this test.
    #[test]
    fn validator_parity_complete_finding_passes_shared_validator() {
        let finding = complete_finding();
        let maybe_record = typescript_gap_record_for(&finding);
        assert!(
            maybe_record.is_some(),
            "complete finding must produce a GapRecord"
        );
        let record = match maybe_record {
            Some(r) => r,
            None => return,
        };
        let result = validate_agent_gap_record_packet(&record);
        assert!(
            result.is_ok(),
            "shared validator must accept complete TS GapRecord, got: {:?}",
            result
        );
    }

    /// §7.4: Missing verify command → validator fails (cond. 3), stays preview.
    #[test]
    fn validator_parity_missing_verify_command_returns_none() {
        let mut finding = complete_finding();
        finding
            .evidence
            .retain(|l| !l.starts_with("typescript_verify_command:"));
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "missing verify command must return None (stay preview)"
        );
    }

    /// §7.4: Missing oracle expected value → G-C fails, returns None.
    #[test]
    fn validator_parity_missing_oracle_expected_returns_none() {
        let mut finding = complete_finding();
        finding
            .evidence
            .retain(|l| !l.starts_with("typescript_oracle_expected:"));
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "missing oracle expected value must return None (G-C)"
        );
    }

    /// §7.4: Dynamic oracle limitation → G-C fails, returns None.
    #[test]
    fn validator_parity_dynamic_oracle_returns_none() {
        let mut finding = complete_finding();
        finding
            .evidence
            .push("typescript_limitation: typescript_dynamic_assertion_unresolved".to_string());
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "dynamic oracle limitation must return None (G-C)"
        );
    }

    /// Named TypeScript limitations fail closed before packet validation.
    #[test]
    fn validator_parity_named_typescript_limitation_returns_none() {
        let mut finding = complete_finding();
        finding
            .evidence
            .push("typescript_limitation: typescript_custom_matcher_unresolved".to_string());
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "named TypeScript limitation must return None before packet validation"
        );
    }

    /// §7.4: Wrong category → G-A fails, returns None.
    #[test]
    fn validator_parity_wrong_category_returns_none() {
        let mut finding = complete_finding();
        for line in finding.evidence.iter_mut() {
            if line.starts_with("actionability_category: ") {
                *line = "actionability_category: strong_oracle_observed".to_string();
            }
        }
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "non-incomplete-repair-packet category must return None (G-A)"
        );
    }

    /// §7.4: No related tests → projection fails, returns None.
    #[test]
    fn validator_parity_no_related_tests_returns_none() {
        let mut finding = complete_finding();
        finding.related_tests.clear();
        let record = typescript_gap_record_for(&finding);
        assert!(record.is_none(), "no related tests must return None (G-D)");
    }

    /// §7.4: Empty missing_discriminators → G-E fails, returns None.
    #[test]
    fn validator_parity_no_missing_discriminators_returns_none() {
        let mut finding = complete_finding();
        finding.activation.missing_discriminators.clear();
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "empty missing_discriminators must return None (G-E)"
        );
    }

    /// §7.4: Cross-language bridge limitation → G-F fails, returns None.
    #[test]
    fn validator_parity_cross_language_bridge_returns_none() {
        let mut finding = complete_finding();
        finding
            .evidence
            .push("typescript_limitation: typescript_cross_language_bridge_unresolved".to_string());
        let record = typescript_gap_record_for(&finding);
        assert!(
            record.is_none(),
            "cross-language bridge limitation must return None (G-F)"
        );
    }

    #[test]
    fn canonical_gap_id_derives_from_finding_id() {
        let id = "probe:src_discount.ts:typescript_preview:a1b2c3d4";
        let gap_id = typescript_canonical_gap_id(id);
        assert_eq!(gap_id, "gap:typescript:typescript_preview:a1b2c3d4");
    }

    #[test]
    fn canonical_gap_id_normalizes_backslashes() {
        let id = r"probe:src\discount.ts:typescript_preview:a1b2c3d4";
        let gap_id = typescript_canonical_gap_id(id);
        // After backslash normalization, result still starts with gap:typescript:
        assert!(gap_id.starts_with("gap:typescript:"));
    }

    #[test]
    fn receipt_command_is_ripr_outcome_shape() {
        let cmd = typescript_receipt_command(
            "gap:typescript:typescript_preview:a1b2c3d4",
            "jest tests/discount.test.ts",
        );
        assert!(
            cmd.starts_with("ripr outcome "),
            "must start with ripr outcome"
        );
        assert!(
            cmd.contains("target/ripr/receipts/"),
            "must reference receipts path"
        );
        assert!(!cmd.contains("curl"), "F7: must not contain curl");
        assert!(!cmd.contains("http"), "F7: must not contain http");
    }

    /// §3.2 / F14: The shared `gap_record_packet_do_not_do` function must include
    /// the preview-language clause when `language_status == "preview"`.
    /// This proves the TS packet reuses the shared boundary list (not a fork).
    #[test]
    fn must_not_change_includes_preview_clause_via_shared_function() {
        let finding = complete_finding();
        let maybe_record = typescript_gap_record_for(&finding);
        assert!(
            maybe_record.is_some(),
            "complete finding must produce a GapRecord for do_not_do test"
        );
        let record = match maybe_record {
            Some(r) => r,
            None => return,
        };
        let do_not_do = gap_record_packet_do_not_do(&record);
        let has_preview_clause = do_not_do
            .iter()
            .any(|s| s.contains("preview-language evidence"));
        assert!(
            has_preview_clause,
            "F14: gap_record_packet_do_not_do must include preview clause for language_status=preview; got: {do_not_do:?}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // #4105: observed-input vs discriminator-boundary reachability.
    // A complete packet must not present an observed call input that provably
    // cannot reach the named boundary as the shape for that boundary.
    // ──────────────────────────────────────────────────────────────────────

    /// A complete finding shaped like the #4105 repro: owner `login` with the
    /// changed boundary `user.length >= 3`, a related test asserting on
    /// `login(<observed>)`, and the missing discriminator naming the boundary.
    fn boundary_finding(observed: &str, expected: &str, discriminator: &str) -> Finding {
        let mut finding = complete_finding();
        finding.id = "probe:src_auth.ts:typescript_preview:23e9836b".to_string();
        finding.probe.owner = Some(SymbolId("typescript:src/auth.ts::login".to_string()));
        for line in finding.evidence.iter_mut() {
            if line.starts_with("typescript_oracle_observed: ") {
                *line = format!("typescript_oracle_observed: {observed}");
            } else if line.starts_with("typescript_oracle_expected: ") {
                *line = format!("typescript_oracle_expected: {expected}");
            } else if line.starts_with("missing_discriminator: ") {
                *line = format!("missing_discriminator: {discriminator}");
            }
        }
        finding.activation.missing_discriminators[0].value = discriminator.to_string();
        finding
    }

    /// Wrongfam: the observed input `'alice'` has length 5, so it provably
    /// does NOT reach the `user.length == 3` boundary. The shape must become
    /// an explicit boundary placeholder and the packet must fail closed.
    #[test]
    fn observed_input_not_reaching_boundary_downgrades_to_placeholder_and_fails_closed()
    -> Result<(), String> {
        let finding = boundary_finding("login('alice')", "'session-for-alice'", "user.length == 3");
        let record = typescript_gap_record_for(&finding)
            .ok_or_else(|| "record must still project for the wrongfam finding".to_string())?;
        let route = record
            .repair_route
            .as_ref()
            .ok_or_else(|| "repair route must be present".to_string())?;
        let shape = route
            .assertion_shape
            .as_deref()
            .ok_or_else(|| "shape must be present".to_string())?;
        assert_eq!(
            shape, "login(/* boundary input for user.length == 3 */).toBe('session-for-alice')",
            "shape must name the boundary, not the observed input"
        );
        assert!(
            !shape.contains("'alice'"),
            "placeholder shape must not reuse the observed input: {shape}"
        );
        assert!(
            route.stop_conditions.iter().any(|stop| {
                stop.contains("Do not reuse the observed call input")
                    && stop.contains("user.length == 3")
            }),
            "a stop condition must forbid reusing the observed input: {:?}",
            route.stop_conditions
        );
        let error = match validate_agent_gap_record_packet(&record) {
            Err(error) => error,
            Ok(()) => {
                return Err(
                    "packet must fail closed when the observed input cannot reach the boundary"
                        .to_string(),
                );
            }
        };
        assert!(
            error.contains("user.length == 3") && error.contains("login('alice')"),
            "validator reason must name the boundary and the observed input: {error}"
        );
        Ok(())
    }

    /// Hon-style control: an observed input that DOES hit the boundary
    /// (`'abc'` has length 3) keeps today's derived shape and the packet
    /// stays delegatable through the shared validator.
    #[test]
    fn observed_input_hitting_boundary_keeps_observed_shape_and_validator_pass()
    -> Result<(), String> {
        let finding = boundary_finding("login('abc')", "'session-for-alice'", "user.length == 3");
        let record =
            typescript_gap_record_for(&finding).ok_or_else(|| "record must project".to_string())?;
        let route = record
            .repair_route
            .as_ref()
            .ok_or_else(|| "repair route must be present".to_string())?;
        assert_eq!(
            route.assertion_shape.as_deref(),
            Some("expect(login('abc')).toBe('session-for-alice')"),
            "boundary-hitting observed input keeps the derived shape"
        );
        if let Err(error) = validate_agent_gap_record_packet(&record) {
            return Err(format!(
                "boundary-hitting input keeps the packet delegatable; got: {error}"
            ));
        }
        Ok(())
    }

    /// Multi-argument call: without signature evidence the argument binding
    /// would be a guess, so the observed shape stands (undecided, fail-open).
    #[test]
    fn undecidable_argument_binding_keeps_observed_shape_and_validator_pass() -> Result<(), String>
    {
        let finding = boundary_finding(
            "login('alice', true)",
            "'session-for-alice'",
            "user.length == 3",
        );
        let record =
            typescript_gap_record_for(&finding).ok_or_else(|| "record must project".to_string())?;
        let route = record
            .repair_route
            .as_ref()
            .ok_or_else(|| "repair route must be present".to_string())?;
        assert_eq!(
            route.assertion_shape.as_deref(),
            Some("expect(login('alice', true)).toBe('session-for-alice')")
        );
        if let Err(error) = validate_agent_gap_record_packet(&record) {
            return Err(format!(
                "undecided reachability must not block the packet; got: {error}"
            ));
        }
        Ok(())
    }

    /// A callee that does not resolve to the owner is never judged.
    #[test]
    fn non_owner_callee_is_not_judged_for_reachability() -> Result<(), String> {
        let finding =
            boundary_finding("helper('alice')", "'session-for-alice'", "user.length == 3");
        let record =
            typescript_gap_record_for(&finding).ok_or_else(|| "record must project".to_string())?;
        let route = record
            .repair_route
            .as_ref()
            .ok_or_else(|| "repair route must be present".to_string())?;
        assert_eq!(
            route.assertion_shape.as_deref(),
            Some("expect(helper('alice')).toBe('session-for-alice')")
        );
        if let Err(error) = validate_agent_gap_record_packet(&record) {
            return Err(format!(
                "a non-owner callee must not be judged for reachability; got: {error}"
            ));
        }
        Ok(())
    }

    #[test]
    fn target_shape_numeric_boundary_miss_downgrades_to_placeholder() {
        let shape =
            typescript_target_assertion_shape("limit(5)", "0", Some("amount == 3"), Some("limit"));
        assert_eq!(
            shape,
            TargetAssertionShape::Unreachable {
                shape: "limit(/* boundary input for amount == 3 */).toBe(0)".to_string(),
                observed_call: "limit(5)".to_string(),
                discriminator: "amount == 3".to_string(),
            }
        );
    }

    #[test]
    fn target_shape_numeric_boundary_hit_keeps_observed() {
        let shape =
            typescript_target_assertion_shape("limit(3)", "0", Some("amount == 3"), Some("limit"));
        assert_eq!(
            shape,
            TargetAssertionShape::Observed {
                shape: "expect(limit(3)).toBe(0)".to_string(),
            }
        );
    }

    #[test]
    fn target_shape_string_equality_boundary_respects_operator() {
        let miss = typescript_target_assertion_shape(
            "greet('bob')",
            "'hi'",
            Some("name == 'admin'"),
            Some("greet"),
        );
        assert!(matches!(miss, TargetAssertionShape::Unreachable { .. }));
        let hit = typescript_target_assertion_shape(
            "greet('admin')",
            "'hi'",
            Some("name == 'admin'"),
            Some("greet"),
        );
        assert!(matches!(hit, TargetAssertionShape::Observed { .. }));
        // `!=` flips both verdicts.
        let ne_miss = typescript_target_assertion_shape(
            "greet('admin')",
            "'hi'",
            Some("name != 'admin'"),
            Some("greet"),
        );
        assert!(matches!(ne_miss, TargetAssertionShape::Unreachable { .. }));
        let ne_hit = typescript_target_assertion_shape(
            "greet('bob')",
            "'hi'",
            Some("name != 'admin'"),
            Some("greet"),
        );
        assert!(matches!(ne_hit, TargetAssertionShape::Observed { .. }));
    }

    #[test]
    fn target_shape_length_comparison_operators_are_evaluated() {
        // `'ab'.length == 3` is false → downgrade.
        let miss = typescript_target_assertion_shape(
            "pad('ab')",
            "'x'",
            Some("user.length == 3"),
            Some("pad"),
        );
        assert!(matches!(miss, TargetAssertionShape::Unreachable { .. }));
        // `'abcd'.length < 3` is false → downgrade.
        let miss_lt = typescript_target_assertion_shape(
            "pad('abcd')",
            "'x'",
            Some("user.length < 3"),
            Some("pad"),
        );
        assert!(matches!(miss_lt, TargetAssertionShape::Unreachable { .. }));
        // `'ab'.length < 3` is true → keep the observed shape.
        let hit_lt = typescript_target_assertion_shape(
            "pad('ab')",
            "'x'",
            Some("user.length < 3"),
            Some("pad"),
        );
        assert!(matches!(hit_lt, TargetAssertionShape::Observed { .. }));
    }

    #[test]
    fn target_shape_escaped_literal_is_undecided() {
        // Escape sequences make the static length unreliable — fail open.
        let shape = typescript_target_assertion_shape(
            "login('a\\t')",
            "'x'",
            Some("user.length == 3"),
            Some("login"),
        );
        assert!(
            matches!(shape, TargetAssertionShape::Observed { .. }),
            "escaped literals must stay undecided: {shape:?}"
        );
    }

    #[test]
    fn target_shape_without_discriminator_or_owner_keeps_observed() {
        let no_discriminator =
            typescript_target_assertion_shape("login('alice')", "'x'", None, Some("login"));
        assert!(matches!(
            no_discriminator,
            TargetAssertionShape::Observed { .. }
        ));
        let no_owner = typescript_target_assertion_shape(
            "login('alice')",
            "'x'",
            Some("user.length == 3"),
            None,
        );
        assert!(matches!(no_owner, TargetAssertionShape::Observed { .. }));
        // Non-literal boundaries (e.g. `amount >= threshold`) are undecided.
        let non_literal = typescript_target_assertion_shape(
            "applyDiscount(100, 100)",
            "90",
            Some("amount >= threshold"),
            Some("applyDiscount"),
        );
        assert!(matches!(non_literal, TargetAssertionShape::Observed { .. }));
    }
}
