//! Bounded, producer-owned propagation facts for the Rust classifier.
//!
//! This is the first migration slice for #3161.  It deliberately does not
//! decide `propagate` or an exposure class.  The existing classifier remains
//! authoritative until a later slice can require a complete witness.  The
//! adapter is useful now because it gives the next consumer a typed, portable
//! identity instead of asking it to infer a path from a compatible sink kind.

use crate::domain::{FlowSinkFact, FlowSinkKind, Probe, ProbeFamily, SymbolId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod collection_argument;
pub(in crate::analysis) use collection_argument::assertion_observes_collection_argument;

const SCHEMA_VERSION: u16 = 1;

/// A versioned internal witness.  This type is intentionally scoped to the
/// analysis crate; it is not a public output or protocol contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct PropagationWitnessV1 {
    pub(in crate::analysis) schema_version: u16,
    pub(in crate::analysis) behavior: BehaviorIdentity,
    pub(in crate::analysis) source: PropagationEndpoint,
    pub(in crate::analysis) edges: Vec<PropagationEdge>,
    /// Declaration/callsite identity receipts only. These do not complete an
    /// edge or contribute relation, activation, propagation or oracle credit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::analysis) selected_helper_calls:
        Vec<super::helper_transfer::SelectedInherentCall>,
    /// Exact rule-7a observer provenance; absent preserves existing digests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::analysis) fallible_check: Option<super::FallibleCheckReceipt>,
    pub(in crate::analysis) sink: PropagationEndpoint,
    pub(in crate::analysis) completeness: PathCompleteness,
    pub(in crate::analysis) limitations: Vec<String>,
    pub(in crate::analysis) semantic_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct BehaviorIdentity {
    pub(in crate::analysis) owner: SymbolId,
    pub(in crate::analysis) family: String,
    pub(in crate::analysis) delta: String,
    pub(in crate::analysis) expression: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct PropagationEndpoint {
    pub(in crate::analysis) kind: String,
    pub(in crate::analysis) identity: String,
    /// Source coordinates are useful for diagnostics, but are intentionally
    /// excluded from the semantic digest so harmless line movement is stable.
    pub(in crate::analysis) line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct PropagationEdge {
    pub(in crate::analysis) kind: PropagationEdgeKind,
    pub(in crate::analysis) status: EdgeStatus,
    pub(in crate::analysis) from: String,
    pub(in crate::analysis) to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::analysis) enum PropagationEdgeKind {
    DirectReturn,
    ErrorVariant,
    StructField,
    EffectTarget,
}

impl PropagationEdgeKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DirectReturn => "direct_return",
            Self::ErrorVariant => "error_variant",
            Self::StructField => "struct_field",
            Self::EffectTarget => "effect_target",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::analysis) enum EdgeStatus {
    Established,
    Candidate,
}

impl EdgeStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Established => "established",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::analysis) enum PathCompleteness {
    /// The bounded direct source-to-sink path is fully established for one of
    /// the sink families integrated by PR-B, or for the direct collection
    /// StateWrite family (#4575).
    Complete,
    /// A local source-to-sink fact exists, but observer binding is not modeled
    /// by this adapter except for the direct collection StateWrite family.
    /// It cannot by itself justify `propagate=yes`.
    Partial,
    Unresolved,
}

impl PathCompleteness {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unresolved => "unresolved",
        }
    }
}

/// Adapt the current Rust local-flow producer into a bounded witness.
///
/// The adapter fails closed unless the sink is non-unknown, belongs to the
/// exact owner, has a family-compatible kind, and shares a semantic token with
/// the changed expression.  In particular, a compatible oracle or a sibling
/// sink is not enough to manufacture a witness.
pub(in crate::analysis) fn current_path_witness(
    probe: &Probe,
    flow_sinks: &[FlowSinkFact],
) -> Option<PropagationWitnessV1> {
    let owner = probe.owner.clone()?;
    // #6695: a statement-level `.ok_or(Type::Variant)?` returns exactly
    // `Err(Type::Variant)` from the owner. The flow producer emits that sink
    // only after confirming the `?` returns from the owner itself, so an
    // equal sink identity is an established error edge (the closure in
    // `ok_or_else(|| Type::Variant)` is the argument's own constant thunk,
    // not an opaque path).
    // Only when the line has no literal `Err(..)` construction: flow emits the
    // owner-checked `ok_or?` sink exactly then. A line holding both
    // (`x.map(|_| Err(E::V)).ok_or(E::V)?`) gets the unchecked
    // `result_error_text` sink with the same text, which must not borrow the
    // bypass (PR #6786 review).
    let question_mark_sink = (matches!(probe.family, ProbeFamily::ErrorPath)
        && super::text::exact_error_variant(&probe.expression).is_none())
    .then(|| super::text::question_mark_error_variant(&probe.expression))
    .flatten()
    .map(|variant| format!("Result::Err({variant})"));
    let is_question_mark_sink = |sink: &FlowSinkFact| {
        sink.kind == FlowSinkKind::ErrorVariant
            && question_mark_sink.as_deref() == Some(normalize_semantic_text(&sink.text).as_str())
    };
    let sink = flow_sinks.iter().find(|sink| {
        sink.owner.as_ref() == Some(&owner)
            && sink.kind != FlowSinkKind::Unknown
            && family_accepts_sink(&probe.family, &sink.kind)
            // A line the strict `ok_or?` reader parses completes only through
            // the owner-checked sink. When flow refused that sink (a closure,
            // a converting owner error type), its fallback text sink must not
            // complete through the generic identity rule (PR #6786 review,
            // CodeRabbit).
            && (is_question_mark_sink(sink)
                || (question_mark_sink.is_none()
                    && source_sink_tokens_overlap(&probe.expression, &sink.text)
                    && !opaque_path_text(&probe.expression)
                    && !opaque_path_text(&sink.text)
                    || unchanged_closure_tail(probe, sink)
                    // #7063: the payload-carrying source and its
                    // variant-named sink share no path identity.
                    || constructs_its_error_on_its_line(probe, sink)))
    })?;

    let edge_kind = edge_kind_for_sink(&sink.kind)?;
    let source_identity = normalize_semantic_text(&probe.expression);
    let sink_identity = normalize_semantic_text(&sink.text);
    let edge_status = if is_question_mark_sink(sink)
        || returns_itself_on_its_line(probe, sink)
        || constructs_its_error_on_its_line(probe, sink)
    {
        EdgeStatus::Established
    } else {
        edge_status_for_kind(&edge_kind, &source_identity, &sink_identity)
    };
    let collection_direct = is_direct_collection_state_write(probe, sink);
    let completeness = if collection_direct && edge_status == EdgeStatus::Established {
        PathCompleteness::Complete
    } else {
        completeness_for_edge(&edge_kind, &edge_status)
    };
    let limitations = if collection_direct {
        vec![
            "direct_collection_state_write".to_string(),
            "static_local_flow_only".to_string(),
            "observer_reassignment_not_modeled".to_string(),
        ]
    } else {
        vec![
            "observer_binding_not_modeled".to_string(),
            "static_local_flow_only".to_string(),
        ]
    };
    let mut witness = PropagationWitnessV1 {
        schema_version: SCHEMA_VERSION,
        behavior: BehaviorIdentity {
            owner: owner.clone(),
            family: probe.family.as_str().to_string(),
            delta: probe.delta.as_str().to_string(),
            expression: source_identity.clone(),
        },
        source: PropagationEndpoint {
            kind: "changed_behavior".to_string(),
            identity: source_identity.clone(),
            line: probe.location.line,
        },
        selected_helper_calls: Vec::new(),
        fallible_check: None,
        edges: vec![PropagationEdge {
            kind: edge_kind.clone(),
            status: edge_status,
            from: source_identity,
            to: sink_identity.clone(),
        }],
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().to_string(),
            identity: sink_identity,
            line: sink.line,
        },
        completeness,
        limitations,
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
}

/// A literal has no semantic tokens. Only the complete rule-7a source/value
/// receipt can adapt its exact local owner return into a complete witness.
pub(in crate::analysis) fn fallible_literal_witness(
    probe: &Probe,
    sinks: &[FlowSinkFact],
    receipt: &super::FallibleCheckReceipt,
) -> Option<PropagationWitnessV1> {
    if !receipt.valid_for(probe) {
        return None;
    }
    let sink = sinks.iter().find(|sink| {
        sink.owner == probe.owner
            && sink.kind == FlowSinkKind::ReturnValue
            && sink.line == probe.location.line
            && normalize_semantic_text(&sink.text) == normalize_semantic_text(&probe.expression)
    })?;
    let identity = normalize_semantic_text(&probe.expression);
    let mut witness = PropagationWitnessV1 {
        schema_version: SCHEMA_VERSION,
        behavior: BehaviorIdentity {
            owner: probe.owner.clone()?,
            family: probe.family.as_str().into(),
            delta: probe.delta.as_str().into(),
            expression: identity.clone(),
        },
        source: PropagationEndpoint {
            kind: "changed_behavior".into(),
            identity: identity.clone(),
            line: probe.location.line,
        },
        edges: vec![PropagationEdge {
            kind: PropagationEdgeKind::DirectReturn,
            status: EdgeStatus::Established,
            from: identity.clone(),
            to: identity.clone(),
        }],
        selected_helper_calls: Vec::new(),
        fallible_check: Some(receipt.clone()),
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().into(),
            identity,
            line: sink.line,
        },
        completeness: PathCompleteness::Complete,
        limitations: vec![
            "bounded_fallible_literal_check".into(),
            "static_local_flow_only".into(),
        ],
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
}

/// A constant owner tail (#6617): the owner's whole body is one expression
/// built only from literals, unary and binary operators and parentheses
/// (`42`, `40 + 2`, `-(1 << 3)`), and that expression is the changed one.
/// Such an expression has no semantic tokens, so [`current_path_witness`]
/// cannot pair it with its sink, yet nothing sits between it and the owner's
/// return: every value it produces is the value the owner returns. Only that
/// exact shape completes a DirectReturn witness. A statement, a binding, a
/// name, a call, a cast or a macro anywhere in the body answers `None`, as
/// does a sink on another line or with other text (fail closed).
pub(in crate::analysis) fn constant_tail_witness(
    probe: &Probe,
    owner: &crate::analysis::facts::FunctionSummary,
    sinks: &[FlowSinkFact],
) -> Option<PropagationWitnessV1> {
    if probe.family != ProbeFamily::ReturnValue || probe.owner.as_ref() != Some(&owner.id) {
        return None;
    }
    let identity = normalize_semantic_text(&probe.expression);
    // RIPR-SPEC-0265 rule 3: an admitted iterator-adaptor tail that is the
    // owner's whole body is, like a constant tail, the value the owner
    // returns; whether the pinned input runs its closure is the owner pin's
    // question (rule 2), not this edge's.
    // RIPR-SPEC-0197 short-circuit tails (#8037): an admitted `&&`/`||`
    // chain that is the owner's whole body is likewise the value the owner
    // returns; whether a pinned run evaluates every operand is the owner
    // pin's run check, not this edge's.
    let (tail_line, limitation) = match constant_owner_tail_line(owner, &identity) {
        Some(line) => (line, "constant_owner_tail"),
        None => match super::owner_pin::closure_adaptor::owner_adaptor_tail(owner)
            .filter(|tail| tail.tail_is(&probe.expression))
        {
            Some(tail) => (
                owner.start_line + tail.tail_line_offset,
                "iterator_adaptor_owner_tail",
            ),
            None => (
                short_circuit_owner_tail_line(owner, &probe.expression, &identity)?,
                "short_circuit_owner_tail",
            ),
        },
    };
    if tail_line != probe.location.line {
        return None;
    }
    let sink = sinks.iter().find(|sink| {
        sink.owner.as_ref() == Some(&owner.id)
            && sink.kind == FlowSinkKind::ReturnValue
            && sink.line == probe.location.line
            && normalize_semantic_text(&sink.text) == identity
    })?;
    let mut witness = PropagationWitnessV1 {
        schema_version: SCHEMA_VERSION,
        behavior: BehaviorIdentity {
            owner: owner.id.clone(),
            family: probe.family.as_str().into(),
            delta: probe.delta.as_str().into(),
            expression: identity.clone(),
        },
        source: PropagationEndpoint {
            kind: "changed_behavior".into(),
            identity: identity.clone(),
            line: probe.location.line,
        },
        edges: vec![PropagationEdge {
            kind: PropagationEdgeKind::DirectReturn,
            status: EdgeStatus::Established,
            from: identity.clone(),
            to: identity.clone(),
        }],
        selected_helper_calls: Vec::new(),
        fallible_check: None,
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().into(),
            identity,
            line: sink.line,
        },
        completeness: PathCompleteness::Complete,
        limitations: vec![limitation.into(), "static_local_flow_only".into()],
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
}

/// The line of the owner's tail when the owner's whole body is one
/// admitted short-circuit chain (`ShortCircuitTail`) whose normalized text
/// is `identity`. Its operands are comparisons of parameters with literals
/// and make no calls, so the chain's value is the owner's return.
fn short_circuit_owner_tail_line(
    owner: &crate::analysis::facts::FunctionSummary,
    changed: &str,
    identity: &str,
) -> Option<usize> {
    use ra_ap_syntax::{AstNode, ast};
    super::owner_pin::short_circuit::ShortCircuitTail::parse(&owner.body, changed)?;
    let parse = crate::analysis::syntax::parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let tail = list.tail_expr()?;
    if normalize_semantic_text(&tail.syntax().text().to_string()) != identity {
        return None;
    }
    let offset = usize::from(tail.syntax().text_range().start());
    let before = owner.body.get(..offset)?;
    Some(owner.start_line + before.matches('\n').count())
}

/// The line of the owner's body when that body is exactly one constant
/// expression whose normalized text is `identity`.
fn constant_owner_tail_line(
    owner: &crate::analysis::facts::FunctionSummary,
    identity: &str,
) -> Option<usize> {
    use ra_ap_syntax::{AstNode, SyntaxKind, ast};
    let parse = crate::analysis::syntax::parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let tail = list.tail_expr()?;
    let constant = tail.syntax().descendants().all(|node| {
        matches!(
            node.kind(),
            SyntaxKind::LITERAL
                | SyntaxKind::BIN_EXPR
                | SyntaxKind::PREFIX_EXPR
                | SyntaxKind::PAREN_EXPR
        )
    });
    if !constant || normalize_semantic_text(&tail.syntax().text().to_string()) != identity {
        return None;
    }
    let offset = usize::from(tail.syntax().text_range().start());
    let before = owner.body.get(..offset)?;
    Some(owner.start_line + before.matches('\n').count())
}

/// #6578: a `return_value` probe whose changed expression is the owner's
/// whole body, one `format!("literal", args..)` tail. `format!` evaluates
/// every argument once and writes each into the `String` it returns (an
/// unused argument does not compile), so the changed argument reaches the
/// owner's return with nothing in between, as a constant tail does
/// ([`constant_tail_witness`]). Only the bare standard `format!` with a
/// string-literal format and arguments free of macros, closures, blocks,
/// branches, loops, `?`, `return`, `break`, `continue`, `.await` and
/// `yield`; the owner's file
/// must hold no binding site that may rebind `format` (a `macro_rules!`, an
/// import, `#[macro_use]`, `#![no_implicit_prelude]` or a foreign glob).
/// Whether formatting keeps two values apart (`{:.0}`, a lossy `Display`)
/// is not settled here, as integer division is not for an arithmetic tail.
pub(in crate::analysis) fn format_tail_witness(
    probe: &Probe,
    owner: &crate::analysis::facts::FunctionSummary,
    owner_source: Option<&str>,
    sinks: &[FlowSinkFact],
) -> Option<PropagationWitnessV1> {
    if probe.family != ProbeFamily::ReturnValue || probe.owner.as_ref() != Some(&owner.id) {
        return None;
    }
    let identity = normalize_semantic_text(&probe.expression);
    let tail_line = format_owner_tail_line(owner, &identity)?;
    if tail_line != probe.location.line {
        return None;
    }
    // The file parse runs only for an owner whose tail already matched.
    let source = crate::analysis::syntax::parse_clean_source_file(owner_source?)?;
    if crate::analysis::syntax::macro_binding_candidates(&source.tree()).may_bind("format") {
        return None;
    }
    let sink = sinks.iter().find(|sink| {
        sink.owner.as_ref() == Some(&owner.id)
            && sink.kind == FlowSinkKind::ReturnValue
            && sink.line == probe.location.line
            && normalize_semantic_text(&sink.text) == identity
    })?;
    let mut witness = PropagationWitnessV1 {
        schema_version: SCHEMA_VERSION,
        behavior: BehaviorIdentity {
            owner: owner.id.clone(),
            family: probe.family.as_str().into(),
            delta: probe.delta.as_str().into(),
            expression: identity.clone(),
        },
        source: PropagationEndpoint {
            kind: "changed_behavior".into(),
            identity: identity.clone(),
            line: probe.location.line,
        },
        edges: vec![PropagationEdge {
            kind: PropagationEdgeKind::DirectReturn,
            status: EdgeStatus::Established,
            from: identity.clone(),
            to: identity.clone(),
        }],
        selected_helper_calls: Vec::new(),
        fallible_check: None,
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().into(),
            identity,
            line: sink.line,
        },
        completeness: PathCompleteness::Complete,
        limitations: vec!["format_owner_tail".into(), "static_local_flow_only".into()],
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
}

/// The line of the owner's body when that body is exactly one bare
/// `format!` call with a string-literal format and plain arguments, whose
/// normalized text is `identity`.
fn format_owner_tail_line(
    owner: &crate::analysis::facts::FunctionSummary,
    identity: &str,
) -> Option<usize> {
    use ra_ap_syntax::{AstNode, SyntaxKind, ast};
    let parse = crate::analysis::syntax::parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let ast::Expr::MacroExpr(tail) = list.tail_expr()? else {
        return None;
    };
    let call = tail.macro_call()?;
    if call.path()?.syntax().text() != "format" {
        return None;
    }
    let tree = call.token_tree()?;
    // Only the parenthesized `format!(..)` spelling.
    tree.l_paren_token()?;
    // The arguments as a tuple, `("literal", a, b)`, or a lone format
    // string, `("{name}")`, which parses as a parenthesized expression.
    let arguments = crate::analysis::syntax::parse_clean_source_file(&format!(
        "fn ripr_format() {{ let _ = {}; }}",
        tree.syntax().text()
    ))?;
    let fields: Vec<ast::Expr> = match arguments
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()?
    {
        ast::Expr::TupleExpr(tuple) => tuple.fields().collect(),
        ast::Expr::ParenExpr(lone) => vec![lone.expr()?],
        _ => return None,
    };
    let mut fields = fields.into_iter();
    let ast::Expr::Literal(format) = fields.next()? else {
        return None;
    };
    if format
        .syntax()
        .first_token()
        .is_none_or(|token| token.kind() != SyntaxKind::STRING)
    {
        return None;
    }
    let plain = fields.all(|field| {
        field.syntax().descendants().all(|node| {
            !matches!(
                node.kind(),
                SyntaxKind::MACRO_CALL
                    | SyntaxKind::MACRO_EXPR
                    | SyntaxKind::CLOSURE_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::MATCH_EXPR
                    | SyntaxKind::LOOP_EXPR
                    | SyntaxKind::WHILE_EXPR
                    | SyntaxKind::FOR_EXPR
                    | SyntaxKind::TRY_EXPR
                    | SyntaxKind::RETURN_EXPR
                    | SyntaxKind::BREAK_EXPR
                    | SyntaxKind::CONTINUE_EXPR
                    | SyntaxKind::AWAIT_EXPR
                    | SyntaxKind::YIELD_EXPR
            )
        })
    });
    if !plain || normalize_semantic_text(&tail.syntax().text().to_string()) != identity {
        return None;
    }
    let offset = usize::from(tail.syntax().text_range().start());
    let before = owner.body.get(..offset)?;
    Some(owner.start_line + before.matches('\n').count())
}

/// A field-to-push route may contain a macro initializer that the complete
/// local adapter refuses. Retain its exact local field as a candidate only;
/// the independently parsed returned-ledger route never completes this path.
pub(in crate::analysis) fn returned_ledger_witness(
    probe: &Probe,
    sinks: &[FlowSinkFact],
) -> Option<PropagationWitnessV1> {
    if probe.family != ProbeFamily::FieldConstruction {
        return None;
    }
    let owner = probe.owner.clone()?;
    let identity = normalize_semantic_text(&probe.expression);
    let sink = sinks.iter().find(|sink| {
        sink.owner.as_ref() == Some(&owner)
            && sink.kind == FlowSinkKind::StructField
            && normalize_semantic_text(&sink.text) == identity
    })?;
    let mut witness = PropagationWitnessV1 {
        schema_version: SCHEMA_VERSION,
        behavior: BehaviorIdentity {
            owner,
            family: probe.family.as_str().to_string(),
            delta: probe.delta.as_str().to_string(),
            expression: identity.clone(),
        },
        source: PropagationEndpoint {
            kind: "changed_behavior".to_string(),
            identity: identity.clone(),
            line: probe.location.line,
        },
        selected_helper_calls: Vec::new(),
        fallible_check: None,
        edges: vec![PropagationEdge {
            kind: PropagationEdgeKind::StructField,
            status: EdgeStatus::Candidate,
            from: identity.clone(),
            to: identity.clone(),
        }],
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().to_string(),
            identity,
            line: sink.line,
        },
        completeness: PathCompleteness::Partial,
        limitations: vec!["observer_binding_not_modeled".to_string()],
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
}

/// Retain identity-only helper transfer in the existing witness. The pushed
/// field is a candidate element, and no indexed observer has been established.
/// This cannot satisfy the complete-direct-witness gate.
pub(in crate::analysis) fn retain_returned_ledger(
    witness: &mut PropagationWitnessV1,
    owner: &SymbolId,
    parameter: &str,
    caller: &SymbolId,
    local: &str,
) {
    let parameter_identity = format!("{}::{parameter}", owner.0);
    let local_identity = format!("{}::{local}", caller.0);
    witness.edges.push(PropagationEdge {
        kind: PropagationEdgeKind::EffectTarget,
        status: EdgeStatus::Candidate,
        from: witness.sink.identity.clone(),
        to: parameter_identity.clone(),
    });
    witness.edges.push(PropagationEdge {
        kind: PropagationEdgeKind::EffectTarget,
        status: EdgeStatus::Established,
        from: parameter_identity,
        to: local_identity.clone(),
    });
    witness.edges.push(PropagationEdge {
        kind: PropagationEdgeKind::DirectReturn,
        status: EdgeStatus::Established,
        from: local_identity,
        to: format!("{}::return {local}", caller.0),
    });
    witness.completeness = PathCompleteness::Partial;
    witness
        .limitations
        .push("returned_mutable_ledger_observer_unresolved".to_string());
    witness.semantic_digest = witness.compute_semantic_digest();
}

fn completeness_for_edge(kind: &PropagationEdgeKind, status: &EdgeStatus) -> PathCompleteness {
    // PR-B intentionally does not promote constructor-wrapped text such as
    // `Ok(amount)` or `Err(error)`: those remain candidate/partial until a
    // later bounded adapter can prove the wrapper payload relationship.
    if matches!(
        kind,
        PropagationEdgeKind::DirectReturn
            | PropagationEdgeKind::ErrorVariant
            | PropagationEdgeKind::StructField
    ) && *status == EdgeStatus::Established
    {
        PathCompleteness::Complete
    } else {
        PathCompleteness::Partial
    }
}

fn edge_status_for_kind(kind: &PropagationEdgeKind, source: &str, sink: &str) -> EdgeStatus {
    if *kind == PropagationEdgeKind::ErrorVariant
        && canonical_error_identity(source) == canonical_error_identity(sink)
    {
        return EdgeStatus::Established;
    }
    if *kind == PropagationEdgeKind::DirectReturn
        && direct_return_identity(source)
            .is_some_and(|identity| identity == normalize_semantic_text(sink))
    {
        return EdgeStatus::Established;
    }
    edge_status(source, sink)
}

/// Identity of an **unwrapped** direct return (`return amount` → `amount`).
///
/// The `return` keyword is a syntactic wrapper, not a value transformation, so
/// stripping it before the identity comparison lets the supported
/// direct-return path complete instead of staying a candidate. Constructor-
/// wrapped forms (`return Ok(amount)`, `return Err(error)`) and call results
/// keep their parentheses and are not normalized here, so a sink on another
/// line fails closed as a candidate edge; [`returns_itself_on_its_line`]
/// completes a call-free constructor only when the sink is the same
/// expression on the probe's own line.
fn direct_return_identity(text: &str) -> Option<String> {
    let normalized = normalize_semantic_text(text);
    let payload = normalized.strip_prefix("return ")?.trim();
    (!payload.is_empty() && !payload.contains('(')).then(|| payload.to_string())
}

/// RIPR-SPEC-0231 example 23: `return Some(1 + 2);` whose return sink is
/// `Some(1 + 2)` on the changed line itself is one expression, so the edge is
/// established as it is for the same tail. Only a `Some(..)`, `Ok(..)` or
/// `Err(..)` constructor around a call-free payload qualifies: a call result
/// (`return f(x);`, `return Ok(self.persist(x));`) may not carry the changed
/// value at all. The sink must sit on the probe's line: an identical
/// constructor returned elsewhere in the owner is a different exit.
fn returns_itself_on_its_line(probe: &Probe, sink: &FlowSinkFact) -> bool {
    let source = normalize_semantic_text(&probe.expression);
    let call_free_constructor = |payload: &str| {
        ["Some(", "Ok(", "Err("].iter().any(|head| {
            payload
                .strip_prefix(head)
                .and_then(|rest| rest.strip_suffix(')'))
                .is_some_and(|inner| !inner.trim().is_empty() && !inner.contains(['(', ')']))
        })
    };
    sink.kind == FlowSinkKind::ReturnValue
        && sink.line == probe.location.line
        && source.strip_prefix("return ").is_some_and(|payload| {
            let payload = payload.trim();
            call_free_constructor(payload) && payload == normalize_semantic_text(&sink.text)
        })
}

/// #7063: `return Err(E::V { line: idx + 1 });` on the changed line is one
/// expression whose error-variant sink the flow producer names by its variant
/// alone (`Result::Err(E::V)`), so the payload-carrying source and the sink
/// never share an identity. The edge is established exactly as for
/// [`returns_itself_on_its_line`]: the sink is the error variant on the
/// probe's own line, the source is one `Err(..)` constructor (optionally
/// after `return`) whose exact variant is the sink's, and its payload is
/// call-free (no call, macro, closure or `?` that may drop the changed
/// value). Whether a test observes the payload is reveal's question: a
/// variant-only oracle does not (`payload_pin_required` in reveal).
fn constructs_its_error_on_its_line(probe: &Probe, sink: &FlowSinkFact) -> bool {
    if sink.kind != FlowSinkKind::ErrorVariant || sink.line != probe.location.line {
        return false;
    }
    let source = normalize_semantic_text(&probe.expression);
    let source = source.trim().trim_end_matches(';').trim_end();
    let payload = source.strip_prefix("return ").unwrap_or(source).trim();
    let Some(inner) = payload
        .strip_prefix("Err(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    let call_free = !inner.trim().is_empty()
        && !inner.contains(['(', ')', '!', '|', '?', ';', '"', '\''])
        && inner.matches('{').count() <= 1
        && inner.matches('{').count() == inner.matches('}').count();
    call_free
        && super::text::exact_error_variant(payload).is_some_and(|variant| {
            normalize_semantic_text(&sink.text) == format!("Result::Err({variant})")
        })
}

fn canonical_error_identity(text: &str) -> String {
    let normalized_text = normalize_semantic_text(text);
    let normalized = without_err_turbofish(normalized_text.trim_start_matches("return "));
    let normalized = normalized.as_str();
    if normalized.starts_with("Err(") {
        format!("Result::{normalized}")
    } else {
        normalized.to_string()
    }
}

pub(in crate::analysis) fn complete_direct_witness(
    probe: &Probe,
    witness: Option<&PropagationWitnessV1>,
) -> bool {
    let Some(witness) = witness else {
        return false;
    };
    let Some(owner) = probe.owner.as_ref() else {
        return false;
    };
    witness.digest_matches()
        && witness
            .fallible_check
            .as_ref()
            .is_none_or(|receipt| receipt.valid_for(probe))
        && witness.schema_version == SCHEMA_VERSION
        && witness.behavior.owner == *owner
        && witness.behavior.family == probe.family.as_str()
        && witness.behavior.delta == probe.delta.as_str()
        && witness.behavior.expression == normalize_semantic_text(&probe.expression)
        && witness.source.identity == normalize_semantic_text(&probe.expression)
        && witness.source.kind == "changed_behavior"
        && witness.completeness == PathCompleteness::Complete
        && witness.edges.len() == 1
        && witness.edges[0].status == EdgeStatus::Established
        && witness.edges[0].from == witness.source.identity
        && witness.edges[0].to == witness.sink.identity
        && complete_direct_edge(probe, witness)
        && witness.sink.kind != FlowSinkKind::Unknown.as_str()
}

fn complete_direct_edge(probe: &Probe, witness: &PropagationWitnessV1) -> bool {
    let Some(edge) = witness.edges.first() else {
        return false;
    };
    match edge.kind {
        PropagationEdgeKind::DirectReturn
        | PropagationEdgeKind::ErrorVariant
        | PropagationEdgeKind::StructField => true,
        PropagationEdgeKind::EffectTarget => {
            witness.sink.kind == FlowSinkKind::StateWrite.as_str()
                && direct_collection_mutation_receiver(&probe.expression).is_some()
        }
    }
}

pub(in crate::analysis) fn valid_owner_bound_partial_witness(
    probe: &Probe,
    witness: Option<&PropagationWitnessV1>,
) -> bool {
    let Some(witness) = witness else {
        return false;
    };
    let Some(owner) = probe.owner.as_ref() else {
        return false;
    };
    let Some(edge) = witness.edges.first() else {
        return false;
    };
    witness.digest_matches()
        && witness.fallible_check.is_none()
        && witness.schema_version == SCHEMA_VERSION
        && witness.behavior.owner == *owner
        && witness.behavior.family == probe.family.as_str()
        && witness.behavior.delta == probe.delta.as_str()
        && witness.behavior.expression == normalize_semantic_text(&probe.expression)
        && witness.source.kind == "changed_behavior"
        && witness.source.identity == normalize_semantic_text(&probe.expression)
        && witness.completeness == PathCompleteness::Partial
        && witness.edges.len() == 1
        && edge.status == EdgeStatus::Candidate
        && edge.from == witness.source.identity
        && edge.to == witness.sink.identity
}

fn edge_kind_for_sink(kind: &FlowSinkKind) -> Option<PropagationEdgeKind> {
    match kind {
        FlowSinkKind::ReturnValue => Some(PropagationEdgeKind::DirectReturn),
        FlowSinkKind::ErrorVariant => Some(PropagationEdgeKind::ErrorVariant),
        FlowSinkKind::StructField => Some(PropagationEdgeKind::StructField),
        FlowSinkKind::EventCall
        | FlowSinkKind::StateWrite
        | FlowSinkKind::Persistence
        | FlowSinkKind::LogMessage
        | FlowSinkKind::ConfigChange
        | FlowSinkKind::CallEffect => Some(PropagationEdgeKind::EffectTarget),
        FlowSinkKind::MatchArm | FlowSinkKind::Unknown => None,
    }
}

fn edge_status(source: &str, sink: &str) -> EdgeStatus {
    if normalize_semantic_text(source) == normalize_semantic_text(sink) {
        EdgeStatus::Established
    } else {
        EdgeStatus::Candidate
    }
}

/// Bare identifier mutated by a collection StateWrite method (`items.push(x)`).
///
/// This is the #4575 family: a directly passed mutable collection, not
/// `self.field.push`, a call result, or a helper/dynamic receiver.
pub(in crate::analysis) fn direct_collection_mutation_receiver(expression: &str) -> Option<&str> {
    let trimmed = expression.trim().trim_end_matches([';', ',']).trim();
    let dot = trimmed.find('.')?;
    let receiver = trimmed[..dot].trim();
    if !is_bare_ident(receiver) {
        return None;
    }
    let after = trimmed.get(dot + 1..)?;
    let method_end = after.find('(')?;
    let method = after[..method_end].trim();
    (method == "push").then_some(receiver)
}

fn is_bare_ident(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && text
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
}

pub(in crate::analysis) fn is_direct_collection_state_write(
    probe: &Probe,
    sink: &FlowSinkFact,
) -> bool {
    matches!(
        probe.family,
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion
    ) && sink.kind == FlowSinkKind::StateWrite
        && direct_collection_mutation_receiver(&probe.expression).is_some()
        && direct_collection_mutation_receiver(&sink.text)
            == direct_collection_mutation_receiver(&probe.expression)
}

/// True when the assertion's primary observed subject is `receiver`.
///
/// `assert_eq!(items, expected)` observes `items`. `assert_eq!(other, items)`
/// observes `other` and must not credit a token on the expected side. The
/// subject must be exactly `receiver`, `receiver[..]` (or another index that
/// ends there), or `receiver.<value-read>(..)`. A trailing operator after the
/// first read, or `capacity`, does not observe the collection (#7135).
pub(in crate::analysis) fn assertion_observes_direct_collection(
    assertion_text: &str,
    receiver: &str,
) -> bool {
    if !is_bare_ident(receiver) {
        return false;
    }
    let Some(subject) = assertion_primary_subject(assertion_text) else {
        return false;
    };
    subject_names_collection(&subject, receiver)
}

/// The field path of a collection write on `self` (`log` for
/// `self.log.push(x)`, `stock` for `self.stock.insert(k, v)`, `a.b` for
/// `self.a.b.push(x)`). RIPR-SPEC-0228 extends the direct-collection
/// observer to these receivers; any other expression gives `None`.
pub(in crate::analysis) fn field_collection_write_path(expression: &str) -> Option<&str> {
    let trimmed = expression.trim().trim_end_matches([';', ',']).trim();
    let rest = trimmed.strip_prefix("self.")?;
    let (target, _) = rest.split_once('(')?;
    let (path, method) = target.rsplit_once('.')?;
    let path = path.trim();
    (matches!(method.trim(), "push" | "insert")
        && path.split('.').all(|segment| is_bare_ident(segment.trim())))
    .then_some(path)
}

/// How an assertion's primary subject reads the field collection `path`:
/// from the local binding `binding`, directly (`inv.log`, `inv.log[0]`,
/// `inv.log.len()`) or through a method with no arguments
/// (`inv.history()`), named in `getter`.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::analysis) struct FieldCollectionRead {
    pub(in crate::analysis) binding: String,
    pub(in crate::analysis) getter: Option<String>,
}

/// The reads of the field collection `path` by `assertion_text`'s compared
/// sides: its primary subject (`assert_eq!(subject, ..)`, `assert!(subject)`)
/// and, for `assert_eq!`, the second operand too, since either side may hold
/// the actual value. A side that is not rooted at a local binding read of
/// that field or of one argument-free method reads nothing. Neither does a
/// side compared with an operand that names the same binding
/// (`assert_eq!(o.entries(), o.entries())` cannot fail), nor, unless
/// `count_reads`, a side that reads only how many elements there are
/// (`.len()`, `.is_empty()`): a changed argument leaves that count alone.
pub(in crate::analysis) fn assertion_field_collection_reads(
    assertion_text: &str,
    path: &str,
    count_reads: bool,
) -> Vec<FieldCollectionRead> {
    let subjects = compared_sides(assertion_text);
    subjects
        .iter()
        .enumerate()
        .filter(|(_, subject)| count_reads || !reads_only_count(subject))
        .filter_map(|(index, subject)| {
            let read = subject_field_collection_read(subject, path)?;
            let other = subjects
                .get(1 - index.min(1))
                .filter(|_| subjects.len() == 2);
            (!other.is_some_and(|other| names_ident(other, &read.binding))).then_some(read)
        })
        .collect()
}

/// The compared sides of `assertion_text`: its primary subject
/// (`assert_eq!(subject, ..)`, `assert!(subject)`) and, for `assert_eq!`,
/// the second operand too, since either side may hold the actual value.
fn compared_sides(assertion_text: &str) -> Vec<String> {
    let mut subjects: Vec<String> = assertion_primary_subject(assertion_text)
        .into_iter()
        .collect();
    if let Some(payload) = macro_payload(assertion_text, "assert_eq") {
        let payload = payload.trim_start();
        if let Some(second) = subjects
            .first()
            .and_then(|first| payload.strip_prefix(first.as_str()))
            .and_then(|rest| rest.trim_start().strip_prefix(','))
            .and_then(first_call_argument)
        {
            subjects.push(second);
        }
    }
    subjects
}

/// A compared side that reads one entry of a field collection through a
/// one-argument method of a local binding: `inv.on_hand("BOLT-M8")` gives
/// binding `inv`, getter `on_hand` and key `"BOLT-M8"`. Whether `on_hand`
/// really looks that key up in the written field is for the caller to
/// verify (`builder_override::getter_plainly_reads_key`).
#[derive(Debug, PartialEq, Eq)]
pub(in crate::analysis) struct KeyedFieldRead {
    pub(in crate::analysis) binding: String,
    pub(in crate::analysis) getter: String,
    pub(in crate::analysis) key: String,
    /// The other `assert_eq!` operand: the value the read is expected to
    /// equal.
    pub(in crate::analysis) expected: String,
}

/// RIPR-SPEC-0228 keyed reads: the `assert_eq!` sides of `assertion_text`
/// that are exactly `binding.getter(key)`, with one argument, compared with
/// an operand that does not name the same binding. A projection of the
/// result (`inv.on_hand("A") + 1`, `inv.on_hand("A").max(1)`) reads
/// nothing, and neither does `assert!` or `assert_ne!`: only an equality
/// says which value the entry must hold.
pub(in crate::analysis) fn assertion_keyed_getter_reads(
    assertion_text: &str,
) -> Vec<KeyedFieldRead> {
    let subjects = compared_sides(assertion_text);
    // Only `assert_eq!` has a second side; without one `get` gives `None`.
    subjects
        .iter()
        .enumerate()
        .filter_map(|(index, subject)| {
            let (binding, getter, key) = subject_keyed_getter_read(subject)?;
            let other = subjects.get(1 - index.min(1))?;
            (!names_ident(other, &binding)).then(|| KeyedFieldRead {
                binding,
                getter,
                key,
                expected: other.trim().to_string(),
            })
        })
        .collect()
}

fn subject_keyed_getter_read(subject: &str) -> Option<(String, String, String)> {
    let bare = subject.trim();
    let (binding, after) = bare.split_once('.')?;
    let (getter, arguments) = after.split_once('(')?;
    if !is_bare_ident(binding) || !is_bare_ident(getter) {
        return None;
    }
    let inner = balanced_inner(arguments)?;
    // The call must be the whole side: nothing may follow its `)`.
    if arguments
        .get(inner.len() + 1..)
        .is_none_or(|tail| !tail.trim().is_empty())
    {
        return None;
    }
    let key = first_call_argument(inner)?;
    (key == inner.trim() && !names_ident(&key, binding))
        .then(|| (binding.to_string(), getter.to_string(), key))
}

/// RIPR-SPEC-0228 keyed reads: the owner parameter that keys a field-path
/// map insert, `self.<path>.insert(KEY, value)`, when `KEY` is that
/// parameter itself or an owned copy of it (`sku`, `sku.to_string()`,
/// `sku.to_owned()`, `sku.clone()`, `sku.into()`, `String::from(sku)`).
/// Any other key gives `None`.
pub(in crate::analysis) fn field_collection_insert_key(expression: &str) -> Option<String> {
    let trimmed = expression.trim().trim_end_matches([';', ',']).trim();
    let rest = trimmed.strip_prefix("self.")?;
    let (target, arguments) = rest.split_once('(')?;
    let (path, method) = target.rsplit_once('.')?;
    if method.trim() != "insert" || !path.split('.').all(|segment| is_bare_ident(segment.trim())) {
        return None;
    }
    let inner = balanced_inner(arguments)?;
    if arguments
        .get(inner.len() + 1..)
        .is_none_or(|tail| !tail.trim().is_empty())
    {
        return None;
    }
    let key = first_call_argument(inner)?;
    let parameter = key
        .strip_prefix("String::from(")
        .and_then(|rest| rest.strip_suffix(')'))
        .or_else(|| {
            [".to_string()", ".to_owned()", ".clone()", ".into()"]
                .iter()
                .find_map(|suffix| key.strip_suffix(suffix))
        })
        .unwrap_or(&key)
        .trim();
    is_bare_ident(parameter).then(|| parameter.to_string())
}

/// Whether `subject` ends in a count of a collection (`.len()`,
/// `.is_empty()`) rather than a read of its elements.
fn reads_only_count(subject: &str) -> bool {
    let compact: String = subject.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact.ends_with(".len()") || compact.ends_with(".is_empty()")
}

/// Whether `text` names `ident` as a whole identifier outside comments and
/// strings.
fn names_ident(text: &str, ident: &str) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    masked.match_indices(ident).any(|(at, name)| {
        !masked[..at].chars().next_back().is_some_and(is_ident)
            && !masked[at + name.len()..]
                .chars()
                .next()
                .is_some_and(is_ident)
    })
}

fn subject_field_collection_read(subject: &str, path: &str) -> Option<FieldCollectionRead> {
    let mut bare = subject.trim();
    bare = bare.strip_prefix('&').map_or(bare, str::trim);
    bare = bare.strip_prefix("mut ").map_or(bare, str::trim);
    let (binding, after) = bare.split_once('.')?;
    if !is_bare_ident(binding) {
        return None;
    }
    if subject_names_collection(bare, &format!("{binding}.{path}")) {
        return Some(FieldCollectionRead {
            binding: binding.to_string(),
            getter: None,
        });
    }
    let method = collection_read_method(&format!(".{after}"))?.to_string();
    let getter_call = format!("{binding}.{method}()");
    subject_names_collection(bare, &getter_call).then(|| FieldCollectionRead {
        binding: binding.to_string(),
        getter: Some(method),
    })
}

/// #7131: the field path `self.<path>` whose `push` takes, as its whole
/// argument, the struct or enum literal that encloses `expression` on `line`
/// of `body` (the owner body, from its `fn`, starting at `start_line`):
/// `self.log.push(Event::Shipped { sku: sku.to_owned(), qty })` gives `log`
/// for `sku: sku.to_owned()`. Any other placement gives `None`, and so do:
/// - an `expression` that occurs twice on its line, since the probe could be
///   either one;
/// - a push that is not a top-level statement of the body (inside a branch,
///   loop, block or closure), which a test may never run;
/// - anything later in the body that may write the field again: a use of
///   `self.<path>` other than another `push`, a use of a parent field of
///   `<path>` (`self.state` for `state.log`), a bare `self`, or a call
///   `self.m(..)` whose body (from `method_body`, through further `self`
///   calls) may do so. A method `method_body` cannot resolve counts as a
///   write.
pub(in crate::analysis) fn pushed_literal_collection_path(
    body: &str,
    start_line: usize,
    line: usize,
    expression: &str,
    method_body: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let line_start = masked
        .split_inclusive('\n')
        .take(line.checked_sub(start_line)?)
        .map(str::len)
        .sum::<usize>();
    let line_text = body.get(line_start..)?.lines().next()?;
    let expression = expression.trim();
    if expression.is_empty() || line_text.matches(expression).count() != 1 {
        return None;
    }
    let probe_at = line_start + line_text.find(expression)?;
    // The group opened at `from` must close after the probe: a net count of
    // delimiters would let a later `Ok(` hold an already closed `push(` open.
    let encloses_probe = |from: usize, open: char, close: char| {
        delimited_closer(&masked[from + 1..], open, close)
            .is_some_and(|closer| from + 1 + closer > probe_at)
    };
    masked[..probe_at]
        .rmatch_indices(".push(")
        .find_map(|(at, call)| {
            let paren = at + call.len() - 1;
            if !encloses_probe(paren, '(', ')') || !top_level_statement(&masked, at) {
                return None;
            }
            let argument = &masked[paren + 1..];
            let literal_path = argument.trim_start();
            let path_len = literal_path
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
                .unwrap_or(literal_path.len());
            let brace = paren + 1 + (argument.len() - literal_path.len()) + path_len;
            let brace = brace + masked[brace..].len() - masked[brace..].trim_start().len();
            if path_len == 0 || !masked[brace..].starts_with('{') || brace >= probe_at {
                return None;
            }
            if !encloses_probe(brace, '{', '}') {
                return None;
            }
            let literal_end = brace + 1 + delimited_closer(&masked[brace + 1..], '{', '}')?;
            let after_literal = masked[literal_end + 1..].trim_start();
            if !after_literal.starts_with(')') {
                return None;
            }
            let call_line_start = masked[..at].rfind('\n').map_or(0, |index| index + 1);
            let path = field_collection_write_path(body.get(call_line_start..=paren)?)?;
            let call_end = masked.len() - after_literal.len();
            let mut visited = Vec::new();
            (!may_write_field(&masked[call_end..], path, method_body, &mut visited))
                .then(|| path.to_string())
        })
}

/// Whether the statement at `at` in a body (masked, from its `fn`) sits
/// directly in the body's block, not in a nested block, branch, loop or
/// closure.
fn top_level_statement(masked: &str, at: usize) -> bool {
    let before = &masked[..at];
    let depth = before.matches('{').count() as isize - before.matches('}').count() as isize;
    let statement_start = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
    depth == 1 && !before[statement_start..].contains('|')
}

/// Whether masked `code` may write the field `self.<path>` other than by
/// another `push`: see [`pushed_literal_collection_path`]. `visited` holds
/// the `self` methods already followed, so recursion ends.
fn may_write_field(
    code: &str,
    path: &str,
    method_body: &dyn Fn(&str) -> Option<String>,
    visited: &mut Vec<String>,
) -> bool {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let field = format!(".{path}");
    code.match_indices("self").any(|(at, word)| {
        let rest = &code[at + word.len()..];
        if code[..at].chars().next_back().is_some_and(ident) || rest.starts_with(ident) {
            return false;
        }
        if let Some(after) = rest.strip_prefix(field.as_str())
            && !after.starts_with(ident)
        {
            return !after.starts_with(".push(");
        }
        let Some(member) = rest.strip_prefix('.') else {
            // A bare `self`: passed on, reborrowed or assigned through.
            return true;
        };
        let name_len = member.find(|ch: char| !ident(ch)).unwrap_or(member.len());
        let (name, after) = member.split_at(name_len);
        if name.is_empty() {
            return true;
        }
        if !after.trim_start().starts_with('(') {
            // A parent of the tracked path (`self.state` for `state.log`)
            // may replace the collection whole; any other field cannot.
            return path.starts_with(&format!("{name}."));
        }
        if visited.iter().any(|seen| seen == name) {
            return false;
        }
        visited.push(name.to_string());
        method_body(name).is_none_or(|callee| {
            let masked = crate::analysis::extract::mask_comments_and_strings(&callee);
            // The callee's own signature names `self`; scan its block only.
            let block = masked.find('{').map_or("", |open| &masked[open..]);
            may_write_field(block, path, method_body, visited)
        })
    })
}

/// The offset in `after_open` of the `close` that balances an `open` just
/// before it.
fn delimited_closer(after_open: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in after_open.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
    }
    None
}

/// #7131: the read of the whole field collection `path` that an
/// `assert_eq!` compares with its other operand: `inv.log` or a getter
/// call `inv.history()`, optionally borrowed. A projection (`.len()`,
/// `[0]`, `.iter().map(..)`) is not a whole read, and neither is a side
/// whose other operand names the same binding: that operand may copy the
/// owner's value and cannot fail on it.
///
/// Both sides may qualify (`assert_eq!(expected.log, actual.log)`), so every
/// admissible read is returned and the caller decides which binding ran the
/// owner.
pub(in crate::analysis) fn assertion_whole_collection_equality(
    assertion_text: &str,
    path: &str,
) -> Vec<FieldCollectionRead> {
    let Some(payload) = macro_payload(assertion_text, "assert_eq").map(str::trim_start) else {
        return Vec::new();
    };
    let Some(first) = first_call_argument(payload) else {
        return Vec::new();
    };
    let Some(second) = payload
        .strip_prefix(first.as_str())
        .and_then(|rest| rest.trim_start().strip_prefix(','))
        .and_then(first_call_argument)
    else {
        return Vec::new();
    };
    [(&first, &second), (&second, &first)]
        .into_iter()
        .filter_map(|(side, other)| {
            let read = whole_collection_read(side, path)?;
            (!names_ident(other, &read.binding)).then_some(read)
        })
        .collect()
}

fn whole_collection_read(side: &str, path: &str) -> Option<FieldCollectionRead> {
    let mut bare = side.trim();
    bare = bare.strip_prefix('&').map_or(bare, str::trim);
    bare = bare.strip_prefix("mut ").map_or(bare, str::trim);
    let (binding, after) = bare.split_once('.')?;
    if !is_bare_ident(binding) {
        return None;
    }
    if after == path {
        return Some(FieldCollectionRead {
            binding: binding.to_string(),
            getter: None,
        });
    }
    let getter = after
        .strip_suffix("()")
        .filter(|name| is_bare_ident(name))?;
    Some(FieldCollectionRead {
        binding: binding.to_string(),
        getter: Some(getter.to_string()),
    })
}

fn assertion_primary_subject(text: &str) -> Option<String> {
    let payload = macro_payload(text, "assert_eq")
        .or_else(|| macro_payload(text, "assert_ne"))
        .or_else(|| macro_payload(text, "assert"))?;
    first_call_argument(payload)
}

fn macro_payload<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let trimmed = text.trim_start();
    let needle = format!("{name}!");
    if !trimmed.starts_with(&needle) {
        return None;
    }
    let after = trimmed.get(needle.len()..)?.trim_start();
    let after = after.strip_prefix('(')?;
    balanced_inner(after)
}

fn balanced_inner(after_open: &str) -> Option<&str> {
    after_open.get(..delimited_group_closer_index(after_open)?)
}

fn first_call_argument(payload: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in payload.char_indices() {
        let rest_after = payload
            .get(index.saturating_add(character.len_utf8())..)
            .unwrap_or("");
        if in_quoted_literal(&mut quote, &mut escaped, character, rest_after) {
            continue;
        }
        match character {
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return payload.get(..index).map(|text| text.trim().to_string()),
            _ => {}
        }
    }
    let trimmed = payload.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn subject_names_collection(subject: &str, receiver: &str) -> bool {
    let mut subject = subject.trim();
    subject = subject.strip_prefix('&').map_or(subject, str::trim);
    subject = subject.strip_prefix("mut ").map_or(subject, str::trim);
    if subject == receiver {
        return true;
    }
    let Some(rest) = subject.strip_prefix(receiver) else {
        return false;
    };
    if rest.starts_with('[') {
        return remainder_after_delimited_group(rest).is_some_and(|tail| tail.trim().is_empty());
    }
    collection_value_read_ends(rest)
}

/// True when `rest` is exactly `.<value-read>(..)` with no trailing operator.
fn collection_value_read_ends(rest: &str) -> bool {
    let Some(method) = collection_read_method(rest) else {
        return false;
    };
    if !is_collection_read_method(method) {
        return false;
    }
    let Some(after_name) = rest
        .strip_prefix('.')
        .and_then(|text| text.get(method.len()..))
        .map(str::trim_start)
    else {
        return false;
    };
    remainder_after_delimited_group(after_name).is_some_and(|tail| tail.trim().is_empty())
}

fn collection_read_method(rest: &str) -> Option<&str> {
    let rest = rest.strip_prefix('.')?;
    let end = rest
        .char_indices()
        .find(|(_, character)| !character.is_ascii_alphanumeric() && *character != '_')
        .map(|(index, _)| index)
        .unwrap_or(rest.len());
    rest.get(..end).filter(|name| !name.is_empty())
}

fn remainder_after_delimited_group(text: &str) -> Option<&str> {
    let open = text.chars().next()?;
    if !matches!(open, '(' | '[' | '{') {
        return None;
    }
    let after_open = text.get(open.len_utf8()..)?;
    let closer_at = delimited_group_closer_index(after_open)?;
    let closer_len = after_open.get(closer_at..)?.chars().next()?.len_utf8();
    after_open.get(closer_at.checked_add(closer_len)?..)
}

/// Byte index of the closer that returns depth to 0 in `after_open`.
/// Quoted literals are scanned as units so a `)` inside a string is not a closer.
fn delimited_group_closer_index(after_open: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in after_open.char_indices() {
        let rest_after = after_open
            .get(index.saturating_add(character.len_utf8())..)
            .unwrap_or("");
        if in_quoted_literal(&mut quote, &mut escaped, character, rest_after) {
            continue;
        }
        match character {
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// True when `character` is inside a quoted literal and must not count as a delimiter.
///
/// A `'` starts a character literal only when lookahead shows `'\…'` or `'x'`.
/// Lifetime ticks such as `'static` are not quotes.
fn in_quoted_literal(
    quote: &mut Option<char>,
    escaped: &mut bool,
    character: char,
    rest_after: &str,
) -> bool {
    if let Some(q) = *quote {
        if *escaped {
            *escaped = false;
            return true;
        }
        if character == '\\' {
            *escaped = true;
            return true;
        }
        if character == q {
            *quote = None;
        }
        return true;
    }
    if character == '"' || (character == '\'' && starts_char_literal(rest_after)) {
        *quote = Some(character);
        return true;
    }
    false
}

fn starts_char_literal(after_tick: &str) -> bool {
    let mut chars = after_tick.chars();
    match chars.next() {
        Some('\\') => true,
        Some(_) => chars.next() == Some('\''),
        None => false,
    }
}

fn is_collection_read_method(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "is_empty"
            | "contains"
            | "get"
            | "first"
            | "last"
            | "iter"
            | "as_slice"
            | "as_ref"
            | "to_vec"
    )
}

fn family_accepts_sink(family: &ProbeFamily, kind: &FlowSinkKind) -> bool {
    match family {
        ProbeFamily::ReturnValue => {
            matches!(kind, FlowSinkKind::ReturnValue | FlowSinkKind::ErrorVariant)
        }
        ProbeFamily::ErrorPath => matches!(kind, FlowSinkKind::ErrorVariant),
        ProbeFamily::FieldConstruction => matches!(kind, FlowSinkKind::StructField),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => is_effect_sink_kind(kind),
        ProbeFamily::Predicate => matches!(
            kind,
            FlowSinkKind::ReturnValue
                | FlowSinkKind::ErrorVariant
                | FlowSinkKind::StructField
                | FlowSinkKind::EventCall
                | FlowSinkKind::StateWrite
                | FlowSinkKind::Persistence
                | FlowSinkKind::LogMessage
                | FlowSinkKind::ConfigChange
                | FlowSinkKind::CallEffect
        ),
        ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => false,
    }
}

fn is_effect_sink_kind(kind: &FlowSinkKind) -> bool {
    matches!(
        kind,
        FlowSinkKind::EventCall
            | FlowSinkKind::StateWrite
            | FlowSinkKind::Persistence
            | FlowSinkKind::LogMessage
            | FlowSinkKind::ConfigChange
            | FlowSinkKind::CallEffect
    )
}

/// #6578: a `return_value` probe whose changed line is the owner's returned
/// expression, identical to the return sink, where the only opaque syntax
/// is a closure the diff leaves unchanged and the changed part sits outside
/// every closure (`it.map(|l| l.cents).sum::<u64>() + 50` → `.. + 25 * 2`).
/// The closure runs the same way in both versions, so the changed operand
/// reaches the returned value as in a closure-free tail.
fn unchanged_closure_tail(probe: &Probe, sink: &FlowSinkFact) -> bool {
    if probe.family != ProbeFamily::ReturnValue
        || sink.kind != FlowSinkKind::ReturnValue
        || sink.line != probe.location.line
        || normalize_semantic_text(&probe.expression) != normalize_semantic_text(&sink.text)
    {
        return false;
    }
    let lower = probe.expression.to_ascii_lowercase();
    let only_closures = !(lower.contains("dyn ")
        || lower.contains("box<dyn")
        || names_ffi_boundary(&lower)
        || lower.contains("extern ")
        || has_macro_invocation(&lower));
    only_closures
        && super::changed_span::ChangedSpan::of(&probe.expression, probe.before.as_deref())
            .is_some_and(|span| !span.under_closure())
        // The changed part must also run on every input: `x.map_or(51, |v|
        // v)` reaches the return with `51` only when `x` is `None`.
        && !super::owner_pin::changed_part_evaluates_conditionally(
            &probe.expression,
            probe.before.as_deref(),
        )
}

fn opaque_path_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("dyn ")
        || lower.contains("box<dyn")
        || names_ffi_boundary(&lower)
        || lower.contains("extern ")
        || has_macro_invocation(&lower)
        || has_closure_syntax(&lower)
}

/// Whether a (lowercased) text names an FFI boundary: an identifier or path
/// segment that IS `ffi` or carries it as an underscore-separated word
/// (`std::ffi::CStr`, `ffi_call`, `sys_ffi`). A plain substring check also
/// matched ordinary words (`Insufficient`, `efficient`, `traffic`) and
/// refused their error-variant witnesses (#6673).
fn names_ffi_boundary(lower: &str) -> bool {
    lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|identifier| identifier.split('_').any(|word| word == "ffi"))
}

fn has_macro_invocation(text: &str) -> bool {
    text.char_indices().any(|(index, character)| {
        character == '!'
            && text[index + character.len_utf8()..]
                .trim_start()
                .chars()
                .next()
                .is_some_and(|next| matches!(next, '(' | '[' | '{'))
    })
}

fn has_closure_syntax(text: &str) -> bool {
    let mut pipes = text.match_indices('|');
    if pipes.next().is_none() {
        return false;
    }
    // Conservative: any paired pipes, including `||`, are an opaque closure
    // boundary.
    pipes.next().is_some()
}

fn source_sink_tokens_overlap(source: &str, sink: &str) -> bool {
    let source_tokens = semantic_tokens(source);
    let source_path = qualified_path_identity(&path_identity_text(source));
    let sink_path = qualified_path_identity(&path_identity_text(sink));
    !source_tokens.is_empty()
        && receiver_identity(source) == receiver_identity(sink)
        && paths_are_compatible(source, sink, source_path.as_deref(), sink_path.as_deref())
        && semantic_tokens(sink)
            .iter()
            .any(|token| source_tokens.iter().any(|source| source == token))
}

/// `Err::<T, E>(payload)` and `Result::Err::<T, E>(payload)` spelled as
/// `Err(payload)`: the turbofish names types, not a different constructor,
/// so error identities compare equal with the flow sink, which
/// `exact_error_variant` already reduces to `Result::Err(payload)` (PR #6786
/// review, Devin). Nested generics are balanced. Any other text is returned
/// unchanged.
fn without_err_turbofish(text: &str) -> String {
    let Some(generics) = text
        .strip_prefix("Result::")
        .unwrap_or(text)
        .strip_prefix("Err::<")
    else {
        return text.to_string();
    };
    let mut depth = 1i32;
    for (offset, ch) in generics.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let rest = &generics[offset + 1..];
                    return if rest.starts_with('(') {
                        format!("Err{rest}")
                    } else {
                        text.to_string()
                    };
                }
            }
            _ => {}
        }
    }
    text.to_string()
}

fn path_identity_text(text: &str) -> String {
    let normalized_text = normalize_semantic_text(text);
    let normalized = without_err_turbofish(normalized_text.trim_start_matches("return "));
    let normalized = normalized.as_str();
    if let Some(payload) = normalized
        .strip_prefix("Err(")
        .and_then(|value| value.strip_suffix(')'))
    {
        payload.to_string()
    } else {
        normalized.to_string()
    }
}

fn qualified_path_identity(text: &str) -> Option<String> {
    let path = text.trim();
    (path.contains('.') || path.contains("::")).then(|| normalize_semantic_text(path))
}

fn paths_are_compatible(
    source: &str,
    sink: &str,
    source_path: Option<&str>,
    sink_path: Option<&str>,
) -> bool {
    match (source_path, sink_path) {
        (Some(source), Some(sink)) => {
            source == sink
                || qualified_payload_identity(sink).as_deref() == Some(source)
                || qualified_payload_identity(source).as_deref() == Some(sink)
        }
        (None, None) => true,
        (None, Some(path)) => is_variant_wrapper(source, path),
        (Some(path), None) => is_variant_wrapper(sink, path),
    }
}

fn qualified_payload_identity(text: &str) -> Option<String> {
    let (_, payload) = text.split_once('(')?;
    let payload = payload.trim().trim_end_matches(')').trim();
    qualified_path_identity(payload)
}

fn is_variant_wrapper(text: &str, qualified_path: &str) -> bool {
    let Some((callee, _)) = text.split_once('(') else {
        return false;
    };
    let qualified_callee = qualified_path
        .split_once('(')
        .map_or(qualified_path, |(callee, _)| callee);
    let Some(last_path_component) = qualified_callee.rsplit("::").next() else {
        return false;
    };
    normalize_semantic_text(callee) == last_path_component
}

fn receiver_identity(text: &str) -> Option<String> {
    let dot = text.find('.')?;
    let receiver = text[..dot]
        .rsplit(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()?
        .trim();
    (!receiver.is_empty()).then(|| receiver.to_string())
}

fn semantic_tokens(text: &str) -> Vec<String> {
    let characters = text.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == 'r'
            && characters.get(index + 1) == Some(&'#')
            && characters
                .get(index + 2)
                .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
        {
            let start = index;
            index += 2;
            while characters
                .get(index)
                .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
            {
                index += 1;
            }
            push_semantic_token(&mut tokens, &characters[start..index]);
        } else if characters[index].is_ascii_alphanumeric() || characters[index] == '_' {
            let start = index;
            index += 1;
            while characters
                .get(index)
                .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
            {
                index += 1;
            }
            push_semantic_token(&mut tokens, &characters[start..index]);
        } else {
            index += 1;
        }
    }
    tokens
}

fn push_semantic_token(tokens: &mut Vec<String>, characters: &[char]) {
    let token = characters.iter().collect::<String>();
    if !is_keyword_or_noise(&token)
        && token.chars().any(|character| {
            character.is_ascii_alphabetic() || character == '_' || character == '#'
        })
    {
        tokens.push(token);
    }
}

fn is_keyword_or_noise(token: &str) -> bool {
    if token == "_" {
        return true;
    }
    matches!(
        token,
        "as" | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

pub(in crate::analysis) fn normalize_semantic_text(text: &str) -> String {
    text.trim()
        .trim_end_matches([',', ';'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl PropagationWitnessV1 {
    pub(in crate::analysis) fn digest_matches(&self) -> bool {
        self.semantic_digest == self.compute_semantic_digest()
    }

    pub(in crate::analysis) fn compute_semantic_digest(&self) -> String {
        let mut canonical = Vec::new();
        append_canonical_section_header(&mut canonical, "witness", 1);
        append_canonical_field(&mut canonical, "propagation-witness-v1");
        append_canonical_section_header(&mut canonical, "behavior", 5);
        append_canonical_field(&mut canonical, &self.schema_version.to_string());
        append_canonical_field(&mut canonical, &self.behavior.owner.0);
        append_canonical_field(&mut canonical, &self.behavior.family);
        append_canonical_field(&mut canonical, &self.behavior.delta);
        append_canonical_field(&mut canonical, &self.behavior.expression);
        append_canonical_section_header(&mut canonical, "source", 2);
        append_canonical_field(&mut canonical, &self.source.kind);
        append_canonical_field(&mut canonical, &self.source.identity);
        append_canonical_section_header(&mut canonical, "edges", self.edges.len());
        for edge in &self.edges {
            append_canonical_section_header(&mut canonical, "edge", 4);
            append_canonical_field(&mut canonical, edge.kind.as_str());
            append_canonical_field(&mut canonical, edge.status.as_str());
            append_canonical_field(&mut canonical, &edge.from);
            append_canonical_field(&mut canonical, &edge.to);
        }
        // Source coordinates remain diagnostic, as for existing endpoints.
        // Empty receipts preserve the historical digest; identities are bound
        // only when the declaration/callsite producer actually supplies them.
        if !self.selected_helper_calls.is_empty() {
            append_canonical_section_header(
                &mut canonical,
                "selected-helper-calls",
                self.selected_helper_calls.len(),
            );
            for call in &self.selected_helper_calls {
                append_canonical_section_header(&mut canonical, "selected-helper-call", 4);
                append_canonical_field(
                    &mut canonical,
                    &call.test_declaration.file.to_string_lossy(),
                );
                append_canonical_field(&mut canonical, &call.test_name);
                append_canonical_field(&mut canonical, &call.callee.0);
                append_canonical_field(&mut canonical, &call.helper.0);
            }
        }
        append_canonical_section_header(&mut canonical, "sink", 2);
        if let Some(receipt) = &self.fallible_check {
            append_canonical_section_header(&mut canonical, "fallible-check", 1);
            append_canonical_field(&mut canonical, &receipt.semantic_digest);
        }
        append_canonical_field(&mut canonical, &self.sink.kind);
        append_canonical_field(&mut canonical, &self.sink.identity);
        append_canonical_section_header(&mut canonical, "path", 1);
        append_canonical_field(&mut canonical, self.completeness.as_str());
        let mut limitations = self.limitations.clone();
        limitations.sort();
        limitations.dedup();
        append_canonical_section_header(&mut canonical, "limitations", limitations.len());
        for limitation in limitations {
            append_canonical_field(&mut canonical, &limitation);
        }
        let digest = Sha256::digest(canonical.as_slice());
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        format!("sha256:{hex}")
    }
}

fn append_canonical_field(canonical: &mut Vec<u8>, value: &str) {
    canonical.extend_from_slice(&(value.len() as u64).to_le_bytes());
    canonical.extend_from_slice(value.as_bytes());
}

fn append_canonical_section_header(canonical: &mut Vec<u8>, tag: &str, count: usize) {
    append_canonical_field(canonical, tag);
    append_canonical_field(canonical, &count.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::facts::{FunctionSummary, ReturnFact};
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use std::path::PathBuf;

    /// #6673: `Insufficient` contains the letters `ffi`; only an `ffi`
    /// identifier word names an FFI boundary.
    #[test]
    fn ffi_boundary_is_an_identifier_word_not_a_substring() {
        for text in [
            "return Err(PayError::Insufficient)",
            "Result::Err(PayError::Insufficient)",
            "return efficient_total(traffic)",
        ] {
            assert!(!opaque_path_text(text), "{text}");
        }
        for text in [
            "std::ffi::CStr::from_ptr(raw)",
            "ffi_call(handle)",
            "sys_ffi::open(path)",
        ] {
            assert!(opaque_path_text(text), "{text}");
        }
    }

    /// `return Result::Err::<T, E>(E::Bad)` and `return Err::<T, E>(..)`
    /// reach the same canonical sink as `return Err(E::Bad)` through the
    /// production flow facts, nested generics included.
    #[test]
    fn qualified_and_unqualified_err_turbofish_complete_the_error_witness() -> Result<(), String> {
        for expression in [
            "return Result::Err::<u8, E>(E::Bad);",
            "return Err::<u8, E>(E::Bad);",
            "return Result::Err::<Vec<Option<u8>>, E>(E::Bad);",
            "return Err(E::Bad);",
        ] {
            let witness = production_witness(&probe(ProbeFamily::ErrorPath, expression), None)
                .ok_or_else(|| format!("no witness for {expression}"))?;
            assert_eq!(
                witness.edges[0].status,
                EdgeStatus::Established,
                "{expression}"
            );
            assert_eq!(
                witness.completeness,
                PathCompleteness::Complete,
                "{expression}"
            );
        }
        assert_eq!(
            without_err_turbofish("Result::Err::<Vec<Option<u8>>, E>(E::Bad)"),
            "Err(E::Bad)"
        );
        assert_eq!(
            without_err_turbofish("Result::Err(E::Bad)"),
            "Result::Err(E::Bad)"
        );
        Ok(())
    }

    /// #7063: `return Err(E::V { line: n });` on the changed line reaches its
    /// own error-variant sink, which the flow producer names by the variant
    /// alone. A sink on another line, a sibling variant, or a payload that
    /// calls, closes over or `?`s something stays a candidate.
    #[test]
    fn a_payload_carrying_err_on_its_own_line_completes_the_error_witness() {
        let complete = |expression: &str, sink_text: &str, sink_line: usize| {
            current_path_witness(
                &probe(ProbeFamily::ErrorPath, expression),
                &[sink(FlowSinkKind::ErrorVariant, sink_text, sink_line)],
            )
            .is_some_and(|witness| {
                witness.completeness == PathCompleteness::Complete
                    && complete_direct_witness(
                        &probe(ProbeFamily::ErrorPath, expression),
                        Some(&witness),
                    )
            })
        };
        for expression in [
            "return Err(ConfigError::MissingEquals { line: idx + 1 });",
            "return Err(ConfigError::MissingEquals { line: line_no });",
            "Err(ConfigError::MissingEquals { line: idx + 1 })",
        ] {
            assert!(
                complete(expression, "Result::Err(ConfigError::MissingEquals)", 10),
                "{expression}"
            );
        }
        let changed = "return Err(ConfigError::MissingEquals { line: idx + 1 });";
        // The same variant returned on another line is a different exit.
        assert!(!complete(
            changed,
            "Result::Err(ConfigError::MissingEquals)",
            11
        ));
        // A sibling variant's sink is not this construction's.
        assert!(!complete(changed, "Result::Err(ConfigError::EmptyKey)", 10));
        for expression in [
            "return Err(ConfigError::MissingEquals { line: next(idx) });",
            "return Err(ConfigError::MissingEquals { line: m!(idx) });",
            "return Err(ConfigError::MissingEquals { line: f()? });",
            "return Err(ConfigError::MissingEquals { line: (|| idx)() });",
            "return Err(ConfigError::Bad { inner: Inner { line: idx } });",
            // A tuple payload's parentheses read as a call: not established.
            "return Err(ConfigError::MissingEquals(idx + 1));",
            "return Err(error);",
        ] {
            assert!(
                !complete(expression, "Result::Err(ConfigError::MissingEquals)", 10),
                "{expression}"
            );
        }
    }

    /// The owner-checked `ok_or?` bypass keys on the probe line having no
    /// literal `Err(..)`; a line with both must use the ordinary edge rule.
    #[test]
    fn question_mark_bypass_refuses_a_line_that_also_constructs_err() {
        let both = probe(
            ProbeFamily::ErrorPath,
            "let v = x.map(|_| Err(E::V)).ok_or(E::V)?;",
        );
        let sinks = [sink(FlowSinkKind::ErrorVariant, "Result::Err(E::V)", 10)];
        let witness = current_path_witness(&both, &sinks);
        assert!(
            witness
                .as_ref()
                .is_none_or(|witness| witness.edges[0].status != EdgeStatus::Established),
            "{witness:?}"
        );
        // Positive control: the plain `ok_or?` line keeps the bypass.
        let plain = probe(ProbeFamily::ErrorPath, "let v = x.ok_or(E::V)?;");
        let witness = current_path_witness(&plain, &sinks);
        assert!(
            witness
                .as_ref()
                .is_some_and(|witness| witness.edges[0].status == EdgeStatus::Established),
            "{witness:?}"
        );
    }

    fn probe(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:fixture:1".to_string()),
            location: SourceLocation::new("fixture/one/src/lib.rs", 10, 2),
            owner: Some(SymbolId("owner:calculate".to_string())),
            family,
            delta: DeltaKind::Value,
            before: Some(expression.to_string()),
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: vec!["equality".to_string()],
        }
    }

    fn probe_at(family: ProbeFamily, expression: &str, line: usize) -> Probe {
        let mut probe = probe(family, expression);
        probe.location = SourceLocation::new("src/lib.rs", line, 5);
        probe
    }

    fn sink(kind: FlowSinkKind, text: &str, line: usize) -> FlowSinkFact {
        FlowSinkFact {
            kind,
            text: text.to_string(),
            line,
            owner: Some(SymbolId("owner:calculate".to_string())),
        }
    }

    fn production_witness(
        probe: &Probe,
        return_text: Option<&str>,
    ) -> Option<PropagationWitnessV1> {
        let owner = FunctionSummary {
            id: probe.owner.clone()?,
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn calculate() { status: amount; }".into(),
            calls: Vec::new(),
            returns: return_text
                .map(|text| {
                    vec![ReturnFact {
                        line: 14,
                        text: text.to_string(),
                    }]
                })
                .unwrap_or_default(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let flow_sinks = super::super::local_flow_sinks(probe, Some(&owner));
        current_path_witness(probe, &flow_sinks)
    }

    fn constant_owner(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("owner:calculate".to_string()),
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: body.into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    /// #6617: only a body that is exactly one literal-only expression is a
    /// constant tail.
    #[test]
    fn constant_owner_tail_is_the_whole_literal_only_body() {
        for (body, expression) in [
            ("fn calculate() -> u32 {\n    40 + 2\n}", "40 + 2"),
            ("fn calculate() -> u32 {\n    7\n}", "7"),
            ("fn calculate() -> i32 {\n    -(1 << 3)\n}", "-(1 << 3)"),
            (
                "fn calculate() -> &'static str {\n    \"ripr\"\n}",
                "\"ripr\"",
            ),
        ] {
            assert_eq!(
                constant_owner_tail_line(&constant_owner(body), expression),
                Some(2),
                "{body}"
            );
        }
        for (body, expression) in [
            // A statement before the tail.
            (
                "fn calculate() -> u32 {\n    let _x = 1;\n    40 + 2\n}",
                "40 + 2",
            ),
            // A name, a call, a method, a cast, a closure, a macro, a block.
            ("fn calculate() -> u32 {\n    BASE + 2\n}", "BASE + 2"),
            ("fn calculate() -> u32 {\n    base() + 2\n}", "base() + 2"),
            (
                "fn calculate() -> u32 {\n    40u32.pow(1)\n}",
                "40u32.pow(1)",
            ),
            (
                "fn calculate() -> u32 {\n    -1i64 as u32\n}",
                "-1i64 as u32",
            ),
            ("fn calculate() -> u32 {\n    (|| 42)()\n}", "(|| 42)()"),
            ("fn calculate() -> u32 {\n    m!(42)\n}", "m!(42)"),
            ("fn calculate() -> u32 {\n    { 42 }\n}", "{ 42 }"),
            (
                "fn calculate() -> u32 {\n    if true { 42 } else { 0 }\n}",
                "if true { 42 } else { 0 }",
            ),
            // The changed expression is not the whole tail.
            ("fn calculate() -> u32 {\n    40 + 2\n}", "2"),
        ] {
            assert_eq!(
                constant_owner_tail_line(&constant_owner(body), expression),
                None,
                "{body}"
            );
        }
    }

    #[test]
    fn constant_tail_witness_binds_owner_line_and_sink() {
        let owner = constant_owner("fn calculate() -> u32 {\n    40 + 2\n}");
        let probe = probe_at(ProbeFamily::ReturnValue, "40 + 2", 2);
        let sinks = super::super::local_flow_sinks(&probe, Some(&owner));
        let witness = constant_tail_witness(&probe, &owner, &sinks);
        assert!(
            complete_direct_witness(&probe, witness.as_ref()),
            "{witness:?} {sinks:?}"
        );
        // Another line, another family, or another owner gets nothing.
        let elsewhere = probe_at(ProbeFamily::ReturnValue, "40 + 2", 3);
        let sinks = super::super::local_flow_sinks(&elsewhere, Some(&owner));
        assert!(constant_tail_witness(&elsewhere, &owner, &sinks).is_none());
        let predicate = probe_at(ProbeFamily::Predicate, "40 + 2", 2);
        let sinks = super::super::local_flow_sinks(&predicate, Some(&owner));
        assert!(constant_tail_witness(&predicate, &owner, &sinks).is_none());
        let mut other = probe_at(ProbeFamily::ReturnValue, "40 + 2", 2);
        other.owner = Some(SymbolId("owner:other".to_string()));
        let sinks = super::super::local_flow_sinks(&other, Some(&owner));
        assert!(!sinks.is_empty());
        assert!(constant_tail_witness(&other, &owner, &sinks).is_none());
    }

    /// #8037: an admitted short-circuit chain that is the owner's whole body
    /// completes a DirectReturn witness; anything `ShortCircuitTail` refuses
    /// does not.
    #[test]
    fn short_circuit_owner_tail_completes_only_an_admitted_chain() {
        let body = "fn is_weekend(day: u8) -> bool {\n    day % 7 == 6 || day % 7 == 0\n}";
        let owner = constant_owner(body);
        let probe = probe_at(ProbeFamily::ReturnValue, "day % 7 == 6 || day % 7 == 0", 2);
        let sinks = super::super::local_flow_sinks(&probe, Some(&owner));
        let witness = constant_tail_witness(&probe, &owner, &sinks);
        assert!(
            complete_direct_witness(&probe, witness.as_ref()),
            "{witness:?} {sinks:?}"
        );
        assert!(witness.as_ref().is_some_and(|w| {
            w.limitations
                .iter()
                .any(|l| l == "short_circuit_owner_tail")
        }));
        for (body, expression) in [
            // A closure operand, mixed operators, a call, an earlier statement.
            (
                "fn f(x: u8, y: u8) -> bool {\n    g(|| x) || y == 1\n}",
                "g(|| x) || y == 1",
            ),
            (
                "fn f(a: bool, b: bool, c: bool) -> bool {\n    a && b || c\n}",
                "a && b || c",
            ),
            (
                "fn f(s: &str, y: u8) -> bool {\n    s.len() == 0 || y == 1\n}",
                "s.len() == 0 || y == 1",
            ),
            (
                "fn f(x: u8, y: u8) -> bool {\n    let x = x + 1;\n    x == 0 || y == 1\n}",
                "x == 0 || y == 1",
            ),
            // The changed expression is one operand, not the whole tail.
            (
                "fn f(x: u8, y: u8) -> bool {\n    x == 0 || y == 1\n}",
                "y == 1",
            ),
        ] {
            let owner = constant_owner(body);
            let line = if body.contains("let x") { 3 } else { 2 };
            let probe = probe_at(ProbeFamily::ReturnValue, expression, line);
            let sinks = super::super::local_flow_sinks(&probe, Some(&owner));
            assert!(
                constant_tail_witness(&probe, &owner, &sinks).is_none(),
                "{body}"
            );
        }
        // A predicate-family probe on the admitted chain gets nothing.
        let predicate = probe_at(ProbeFamily::Predicate, "day % 7 == 6 || day % 7 == 0", 2);
        let sinks = super::super::local_flow_sinks(&predicate, Some(&owner));
        assert!(constant_tail_witness(&predicate, &owner, &sinks).is_none());
    }

    /// #6578: a `format!` tail writes every argument into the `String` the
    /// owner returns; only the bare standard macro with plain arguments.
    #[test]
    fn format_owner_tail_line_reads_only_a_plain_format_tail() {
        let positive = [
            "format!(\"{name}: {}\", score + 1)",
            "format!(\"{}-{}\", a.len(), b)",
            // A lone format string with captured identifiers.
            "format!(\"{name}\")",
        ];
        for expression in positive {
            let body = format!("fn calculate() -> String {{\n    {expression}\n}}");
            assert_eq!(
                format_owner_tail_line(&constant_owner(&body), expression),
                Some(2),
                "{body}"
            );
        }
        for (body, expression) in [
            // A statement before the tail.
            (
                "fn calculate() -> String {\n    let _x = 1;\n    format!(\"{}\", x)\n}",
                "format!(\"{}\", x)",
            ),
            // A path-qualified, bracketed or other macro.
            (
                "fn calculate() -> String {\n    std::format!(\"{}\", x)\n}",
                "std::format!(\"{}\", x)",
            ),
            (
                "fn calculate() -> String {\n    format![\"{}\", x]\n}",
                "format![\"{}\", x]",
            ),
            (
                "fn calculate() -> String {\n    print!(\"{}\", x)\n}",
                "print!(\"{}\", x)",
            ),
            // A non-literal format string.
            (
                "fn calculate() -> String {\n    format!(concat!(\"{}\"), x)\n}",
                "format!(concat!(\"{}\"), x)",
            ),
            // A lone argument that is not a string literal.
            (
                "fn calculate() -> String {\n    format!(concat!(\"{x}\"))\n}",
                "format!(concat!(\"{x}\"))",
            ),
            (
                "fn calculate() -> String {\n    format!(name)\n}",
                "format!(name)",
            ),
            // Arguments that may not evaluate the changed value plainly.
            (
                "fn calculate() -> String {\n    format!(\"{}\", m!(x))\n}",
                "format!(\"{}\", m!(x))",
            ),
            (
                "fn calculate() -> String {\n    format!(\"{}\", if a { x } else { y })\n}",
                "format!(\"{}\", if a { x } else { y })",
            ),
            (
                "fn calculate() -> String {\n    format!(\"{}\", (|| x)())\n}",
                "format!(\"{}\", (|| x)())",
            ),
            (
                "fn calculate() -> String {\n    format!(\"{}\", f(x)?)\n}",
                "format!(\"{}\", f(x)?)",
            ),
            // The changed expression is not the whole tail.
            (
                "fn calculate() -> String {\n    format!(\"{}\", x + 1)\n}",
                "x + 1",
            ),
        ] {
            assert_eq!(
                format_owner_tail_line(&constant_owner(body), expression),
                None,
                "{body}"
            );
        }
    }

    /// #6578: an owner tail whose only opaque syntax is a closure the diff
    /// leaves unchanged completes like a closure-free tail.
    #[test]
    fn an_unchanged_closure_in_a_returned_tail_completes_the_witness() {
        let tail = |fee: &str| format!("lines.iter().map(|l| l.cents).sum::<u64>() + {fee}");
        let changed = |after: &str, before: Option<&str>, family: ProbeFamily| {
            let mut probe = probe(family, after);
            probe.before = before.map(str::to_string);
            probe
        };
        let witness = |probe: &Probe, sink_text: &str, sink_line: usize| {
            current_path_witness(
                probe,
                &[sink(FlowSinkKind::ReturnValue, sink_text, sink_line)],
            )
            .is_some_and(|witness| {
                witness.completeness == PathCompleteness::Complete
                    && witness.edges[0].status == EdgeStatus::Established
            })
        };
        let after = tail("25 * 2");
        let before = tail("50");
        let positive = changed(&after, Some(&before), ProbeFamily::ReturnValue);
        assert!(witness(&positive, &after, 10));
        // Not without the old text, on another line, for a sink that is not
        // the whole changed expression, or for another family.
        assert!(!witness(
            &changed(&after, None, ProbeFamily::ReturnValue),
            &after,
            10
        ));
        assert!(!witness(&positive, &after, 11));
        assert!(!witness(
            &positive,
            "lines.iter().map(|l| l.cents).sum::<u64>()",
            10
        ));
        assert!(!witness(
            &changed(&after, Some(&before), ProbeFamily::Predicate),
            &after,
            10
        ));
        // The change inside the closure, a macro, `dyn` elsewhere, or a
        // changed operand a combinator may skip.
        for (after, before) in [
            (
                "lines.iter().map(|l| l.cents + 1).sum::<u64>()",
                "lines.iter().map(|l| l.cents + 2).sum::<u64>()",
            ),
            (
                "lines.iter().map(|l| l.cents).sum::<u64>() + m!(51)",
                "lines.iter().map(|l| l.cents).sum::<u64>() + m!(50)",
            ),
            ("f(&|l: &dyn Fn()| l) + 51", "f(&|l: &dyn Fn()| l) + 50"),
            // A combinator decides whether the changed default runs.
            ("x.map_or(51, |v| v) + 3", "x.map_or(50, |v| v) + 3"),
            // The change removed a closure that may have given the same
            // value.
            (
                "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
                "lines.iter().map(|l| l.cents).sum::<u64>() + (|| 51)()",
            ),
            // The changed receiver or argument flows through a closure.
            (
                "items.iter().map(|l| l.cents).sum::<u64>() + 50",
                "lines.iter().map(|l| l.cents).sum::<u64>() + 50",
            ),
            (
                "lines.iter().fold(51, |acc, l| acc + l.cents)",
                "lines.iter().fold(50, |acc, l| acc + l.cents)",
            ),
        ] {
            let probe = changed(after, Some(before), ProbeFamily::ReturnValue);
            assert!(!witness(&probe, after, 10), "{after}");
        }
    }

    #[test]
    fn format_tail_witness_refuses_a_file_that_may_rebind_format() {
        let expression = "format!(\"{}\", x * 3)";
        let body = format!("fn calculate() -> String {{\n    {expression}\n}}");
        let owner = constant_owner(&body);
        let probe = probe_at(ProbeFamily::ReturnValue, expression, 2);
        let sinks = super::super::local_flow_sinks(&probe, Some(&owner));
        let witness = format_tail_witness(&probe, &owner, Some(&body), &sinks);
        assert!(
            complete_direct_witness(&probe, witness.as_ref()),
            "{witness:?} {sinks:?}"
        );
        assert_eq!(
            witness.map(|witness| witness.limitations),
            Some(vec![
                "format_owner_tail".to_string(),
                "static_local_flow_only".to_string()
            ])
        );
        // No owner source, or a file that may bind another `format`.
        assert!(format_tail_witness(&probe, &owner, None, &sinks).is_none());
        for prefix in [
            "macro_rules! format { ($($t:tt)*) => { String::new() }; }\n",
            "use other::format;\n",
            "#[macro_use]\nextern crate other;\n",
        ] {
            let source = format!("{prefix}{body}");
            assert!(
                format_tail_witness(&probe, &owner, Some(&source), &sinks).is_none(),
                "{source}"
            );
        }
        // Another family or line gets nothing.
        let predicate = probe_at(ProbeFamily::Predicate, expression, 2);
        let sinks = super::super::local_flow_sinks(&predicate, Some(&owner));
        assert!(format_tail_witness(&predicate, &owner, Some(&body), &sinks).is_none());
        let elsewhere = probe_at(ProbeFamily::ReturnValue, expression, 3);
        let sinks = super::super::local_flow_sinks(&elsewhere, Some(&owner));
        assert!(format_tail_witness(&elsewhere, &owner, Some(&body), &sinks).is_none());
    }

    #[test]
    fn production_local_flow_facts_produce_direct_return_error_and_field_edges()
    -> Result<(), String> {
        let cases = [
            (
                ProbeFamily::ReturnValue,
                "amount",
                Some("Ok(amount)"),
                PropagationEdgeKind::DirectReturn,
                EdgeStatus::Candidate,
            ),
            (
                ProbeFamily::ErrorPath,
                "Err(Boundary)",
                None,
                PropagationEdgeKind::ErrorVariant,
                EdgeStatus::Established,
            ),
            (
                ProbeFamily::FieldConstruction,
                "status: amount",
                None,
                PropagationEdgeKind::StructField,
                EdgeStatus::Established,
            ),
        ];

        for (family, expression, return_text, expected_edge, expected_status) in cases {
            let witness = production_witness(&probe(family, expression), return_text)
                .ok_or_else(|| "exact owner and producer local-flow fact was absent".to_string())?;
            assert_eq!(witness.schema_version, SCHEMA_VERSION);
            assert_eq!(witness.edges[0].kind, expected_edge);
            assert_eq!(witness.edges[0].status, expected_status);
            let expected_completeness = if expected_status == EdgeStatus::Established {
                PathCompleteness::Complete
            } else {
                PathCompleteness::Partial
            };
            assert_eq!(witness.completeness, expected_completeness);
            assert_eq!(witness.semantic_digest, witness.compute_semantic_digest());
        }
        Ok(())
    }

    #[test]
    fn unwrapped_direct_return_completes_and_wrapped_on_another_line_stays_candidate()
    -> Result<(), String> {
        // `return amount;` is the same value as the `amount` sink: the keyword
        // is normalized before the identity comparison so the supported
        // direct-return path completes (XQFa).
        let direct_probe = probe(ProbeFamily::ReturnValue, "return amount;");
        let witness = current_path_witness(
            &direct_probe,
            &[sink(FlowSinkKind::ReturnValue, "amount", 14)],
        )
        .ok_or_else(|| "direct return witness was absent".to_string())?;
        assert_eq!(witness.edges[0].kind, PropagationEdgeKind::DirectReturn);
        assert_eq!(witness.edges[0].status, EdgeStatus::Established);
        assert_eq!(witness.completeness, PathCompleteness::Complete);
        assert!(complete_direct_witness(&direct_probe, Some(&witness)));

        // A constructor-wrapped return whose sink sits on another line keeps
        // the adapter fail-closed: that sink may be a different exit.
        let wrapped = current_path_witness(
            &probe(ProbeFamily::ReturnValue, "return Ok(amount);"),
            &[sink(FlowSinkKind::ReturnValue, "Ok(amount)", 14)],
        )
        .ok_or_else(|| "wrapped return witness was absent".to_string())?;
        assert_eq!(wrapped.edges[0].status, EdgeStatus::Candidate);
        assert_eq!(wrapped.completeness, PathCompleteness::Partial);
        assert!(!complete_direct_witness(
            &probe(ProbeFamily::ReturnValue, "return Ok(amount);"),
            Some(&wrapped),
        ));
        Ok(())
    }

    #[test]
    fn a_wrapped_return_completes_only_on_its_own_line() -> Result<(), String> {
        // RIPR-SPEC-0231 example 23: `return Some(1 + 2);` returns the
        // `Some(1 + 2)` sink on its own line, so the edge is established.
        let wrapped = probe(ProbeFamily::ReturnValue, "return Some(1 + 2);");
        let witness = current_path_witness(
            &wrapped,
            &[sink(FlowSinkKind::ReturnValue, "Some(1 + 2)", 10)],
        )
        .ok_or_else(|| "same-line wrapped return witness was absent".to_string())?;
        assert_eq!(witness.edges[0].status, EdgeStatus::Established);
        assert_eq!(witness.completeness, PathCompleteness::Complete);
        assert!(complete_direct_witness(&wrapped, Some(&witness)));

        // The same constructor returned on another line is another exit.
        let elsewhere = current_path_witness(
            &wrapped,
            &[sink(FlowSinkKind::ReturnValue, "Some(1 + 2)", 14)],
        )
        .ok_or_else(|| "other-line wrapped return witness was absent".to_string())?;
        assert_eq!(elsewhere.edges[0].status, EdgeStatus::Candidate);
        assert!(!complete_direct_witness(&wrapped, Some(&elsewhere)));

        // A call result returned on its own line stays a candidate: the
        // call may not carry the changed value (`propagate_value_returned`).
        let call_return = probe(ProbeFamily::ReturnValue, "return self.persist(amount * 9);");
        let call_witness = current_path_witness(
            &call_return,
            &[sink(
                FlowSinkKind::ReturnValue,
                "self.persist(amount * 9)",
                10,
            )],
        )
        .ok_or_else(|| "same-line call return witness was absent".to_string())?;
        assert_eq!(call_witness.edges[0].status, EdgeStatus::Candidate);

        // A call-free constructor completes against itself on its own line.
        let ok_return = probe(ProbeFamily::ReturnValue, "return Ok(amount);");
        let ok_witness = current_path_witness(
            &ok_return,
            &[sink(FlowSinkKind::ReturnValue, "Ok(amount)", 10)],
        )
        .ok_or_else(|| "same-line Ok return witness was absent".to_string())?;
        assert_eq!(ok_witness.edges[0].status, EdgeStatus::Established);
        assert!(complete_direct_witness(&ok_return, Some(&ok_witness)));

        // A different payload on the same line stays a candidate.
        let other = current_path_witness(
            &wrapped,
            &[sink(FlowSinkKind::ReturnValue, "Some(1 + 3)", 10)],
        )
        .ok_or_else(|| "different-payload witness was absent".to_string())?;
        assert_eq!(other.edges[0].status, EdgeStatus::Candidate);
        Ok(())
    }

    #[test]
    fn missing_owner_or_flow_fact_fails_closed() {
        let mut without_owner = probe(ProbeFamily::ReturnValue, "amount");
        without_owner.owner = None;
        assert!(current_path_witness(&without_owner, &[]).is_none());
        assert!(current_path_witness(&probe(ProbeFamily::ReturnValue, "amount"), &[]).is_none());
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "amount"),
                &[sink(FlowSinkKind::Unknown, "amount", 14)]
            )
            .is_none()
        );
    }

    #[test]
    fn forged_same_owner_family_witness_fails_exact_identity_binding() -> Result<(), String> {
        let probe = probe(ProbeFamily::FieldConstruction, "status: amount");
        let sinks = vec![sink(FlowSinkKind::StructField, "status: amount", 14)];
        let Some(mut witness) = current_path_witness(&probe, &sinks) else {
            return Err("established fixture witness was absent".to_string());
        };
        assert!(complete_direct_witness(&probe, Some(&witness)));

        witness.behavior.expression = "status: sibling".to_string();
        witness.semantic_digest = witness.compute_semantic_digest();
        assert!(!complete_direct_witness(&probe, Some(&witness)));

        let mut forged_delta = current_path_witness(&probe, &sinks)
            .ok_or_else(|| "established fixture witness was absent".to_string())?;
        forged_delta.behavior.delta = "effect".to_string();
        forged_delta.semantic_digest = forged_delta.compute_semantic_digest();
        assert!(!complete_direct_witness(&probe, Some(&forged_delta)));
        Ok(())
    }

    #[test]
    fn sibling_orthogonal_dynamic_and_compatible_oracle_controls_fail_closed() {
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "amount"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(amount)", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "amount"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(Amount)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::Predicate, "amount > 0"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(0)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "x"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(x)", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "r#type"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(r#type)", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "r#type"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(r#match)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::FieldConstruction, "amount"),
                &[sink(FlowSinkKind::StructField, "foo().amount", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::FieldConstruction, "foo().amount"),
                &[sink(FlowSinkKind::StructField, "foo().amount", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "if"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(if)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ErrorPath, "Result::Err(error)"),
                &[sink(FlowSinkKind::ErrorVariant, "Result::Err(error)", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ErrorPath, "AuthError::RevokedToken"),
                &[sink(
                    FlowSinkKind::ErrorVariant,
                    "Result::Err(AuthError::RevokedToken)",
                    14,
                )]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(
                    ProbeFamily::ErrorPath,
                    "return Err(AuthError::RevokedToken);"
                ),
                &[sink(
                    FlowSinkKind::ErrorVariant,
                    "Result::Err(AuthError::RevokedToken)",
                    14,
                )]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "self.handle(value)"),
                &[sink(FlowSinkKind::CallEffect, "self.handle(value)", 14)]
            )
            .is_some()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "Ok(value)"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(value)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::CallDeletion, "self.handle(value)"),
                &[sink(FlowSinkKind::ErrorVariant, "self.handle(value)", 14)]
            )
            .is_none()
        );
        let owner = probe(ProbeFamily::FieldConstruction, "amount");
        assert!(
            current_path_witness(
                &owner,
                &[sink(FlowSinkKind::StructField, "status: sibling", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "amount"),
                &[sink(FlowSinkKind::ReturnValue, "unrelated_value", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "value"),
                &[sink(
                    FlowSinkKind::ReturnValue,
                    "Box<dyn Handler>::value",
                    14
                )]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "notify!(value)"),
                &[sink(FlowSinkKind::CallEffect, "notify!(value)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "register(|value| value)"),
                &[sink(
                    FlowSinkKind::CallEffect,
                    "register(|value| value)",
                    14
                )]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "register(|| value)"),
                &[sink(FlowSinkKind::CallEffect, "register(|| value)", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "self.handle(value)"),
                &[sink(FlowSinkKind::CallEffect, "other.handle(value)", 14)]
            )
            .is_none()
        );
        let mut cross_owner = sink(FlowSinkKind::CallEffect, "self.handle(value)", 14);
        cross_owner.owner = Some(SymbolId("owner:other".to_string()));
        assert!(
            current_path_witness(
                &probe(ProbeFamily::SideEffect, "self.handle(value)"),
                &[cross_owner]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::FieldConstruction, "value"),
                &[sink(FlowSinkKind::StructField, "Other::value", 14)]
            )
            .is_none()
        );
        assert!(
            current_path_witness(
                &probe(ProbeFamily::FieldConstruction, "self.status"),
                &[sink(FlowSinkKind::StructField, "self.meta.status", 14)]
            )
            .is_none()
        );
        // An equality oracle is deliberately not an input to this adapter;
        // sink-kind compatibility alone cannot create a witness.
        assert!(
            current_path_witness(
                &probe(ProbeFamily::ReturnValue, "amount"),
                &[sink(FlowSinkKind::ReturnValue, "other_amount", 14)]
            )
            .is_none()
        );
    }

    #[test]
    fn semantic_digest_ignores_root_and_line_movement_but_detects_edge_removal()
    -> Result<(), String> {
        let mut moved = probe(ProbeFamily::ReturnValue, "amount");
        moved.location = SourceLocation::new("fixture/two/src/lib.rs", 900, 17);
        let first = current_path_witness(
            &probe(ProbeFamily::ReturnValue, "amount"),
            &[sink(FlowSinkKind::ReturnValue, "Ok(amount)", 14)],
        )
        .ok_or_else(|| "first witness was absent".to_string())?;
        let second = current_path_witness(
            &moved,
            &[sink(FlowSinkKind::ReturnValue, "Ok(amount)", 901)],
        )
        .ok_or_else(|| "moved witness was absent".to_string())?;
        assert_eq!(first.semantic_digest, second.semantic_digest);

        let mut without_edge = first.clone();
        without_edge.edges.clear();
        assert_ne!(
            first.semantic_digest,
            without_edge.compute_semantic_digest()
        );
        assert_ne!(
            without_edge.semantic_digest,
            without_edge.compute_semantic_digest()
        );

        let mut duplicated_edge = first.clone();
        duplicated_edge.edges.push(first.edges[0].clone());
        assert_ne!(
            first.compute_semantic_digest(),
            duplicated_edge.compute_semantic_digest()
        );

        let mut split_limitations = first.clone();
        split_limitations.limitations = vec!["ab".to_string(), "c".to_string()];
        let mut alternate_limitations = first.clone();
        alternate_limitations.limitations = vec!["a".to_string(), "bc".to_string()];
        assert_ne!(
            split_limitations.compute_semantic_digest(),
            alternate_limitations.compute_semantic_digest()
        );
        Ok(())
    }

    /// RIPR-SPEC-0228: a collection write on a field path of `self` names
    /// its field path; any other receiver or method does not.
    #[test]
    fn field_collection_write_path_reads_self_field_receivers() {
        assert_eq!(
            field_collection_write_path("self.log.push(x);"),
            Some("log")
        );
        assert_eq!(
            field_collection_write_path("self.stock.insert(sku.to_string(), n)"),
            Some("stock")
        );
        assert_eq!(field_collection_write_path("self.a.b.push(x)"), Some("a.b"));
        for other in [
            "items.push(x)",
            "self.log.extend(xs)",
            "self.log.push",
            "other.log.push(x)",
            "self.log().push(x)",
            "self.log[0].push(x)",
        ] {
            assert_eq!(field_collection_write_path(other), None, "{other}");
        }
    }

    /// #7131: a field credits through a pushed collection only when its
    /// literal is the whole `push` argument on a field path of `self` and
    /// nothing after the push writes that field again.
    #[test]
    fn pushed_literal_collection_path_needs_the_whole_pushed_literal() {
        let path = |body: &str, line: usize| {
            pushed_literal_collection_path(body, 1, line, "sku: sku.to_owned()", &|name| match name
            {
                "refresh" => {
                    Some("fn refresh(&mut self) { self.flag = self.count() > 0; }".to_string())
                }
                "count" => Some("fn count(&self) -> usize { self.stock.len() }".to_string()),
                "normalize" => Some(
                    "fn normalize(&mut self) { for l in &mut self.log { l.sku.clear(); } }"
                        .to_string(),
                ),
                "deep" => Some("fn deep(&mut self) { self.normalize(); }".to_string()),
                _ => None,
            })
        };
        let one_line = "fn add(&mut self) {\n    self.log.push(Event::Shipped { sku: sku.to_owned(), qty });\n}";
        assert_eq!(path(one_line, 2).as_deref(), Some("log"));
        let split = "fn add(&mut self) {\n    self.a.b.push(Line {\n        sku: sku.to_owned(),\n        qty,\n    });\n    self.a.b.push(other);\n}";
        assert_eq!(path(split, 3).as_deref(), Some("a.b"));
        // A later call that resolves to code leaving the field alone keeps it.
        let refreshed = "fn ship(&mut self) {\n    self.log.push(Event { sku: sku.to_owned() });\n    self.refresh();\n    self.total += 1;\n}";
        assert_eq!(path(refreshed, 2).as_deref(), Some("log"));
        for (body, line) in [
            (
                "fn add(&mut self) {\n    let e = Line { sku: sku.to_owned() };\n    self.log.push(e);\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() }.clone());\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    items.push(Line { sku: sku.to_owned() });\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    self.log.clear();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    self.log = Vec::new();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(wrap(Line { sku: sku.to_owned() }));\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(x);\n    let l = Line { sku: sku.to_owned() };\n}",
                3,
            ),
            (
                "fn ship(&mut self) {\n    self.log.push(Event::Shipped { qty });\n    Ok(Receipt {\n        sku: sku.to_owned(),\n    })\n}",
                4,
            ),
            (
                "fn ship(&mut self) {\n    self.log.push(Event::Shipped { qty }); Ok(Receipt { sku: sku.to_owned() })\n}",
                2,
            ),
            // The expression twice on its line: the probe could be either.
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() }); self.other.push(Line { sku: sku.to_owned() });\n}",
                2,
            ),
            // A push that may not run.
            (
                "fn add(&mut self) {\n    if qty > 0 { self.log.push(Line { sku: sku.to_owned() }); }\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    let f = |s: &mut Self| s.log.push(Line { sku: sku.to_owned() });\n}",
                2,
            ),
            // A later callee that writes the field, directly or through another call.
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    self.normalize();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    self.deep();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    self.unknown();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    *self = Self::default();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.log.push(Line { sku: sku.to_owned() });\n    reset(self);\n}",
                2,
            ),
            // A later write to a parent of the path replaces the collection.
            (
                "fn add(&mut self) {\n    self.a.b.push(Line { sku: sku.to_owned() });\n    self.a = State::default();\n}",
                2,
            ),
            (
                "fn add(&mut self) {\n    self.a.b.push(Line { sku: sku.to_owned() });\n    self.a.reset();\n}",
                2,
            ),
        ] {
            assert_eq!(path(body, line), None, "{body}");
        }
    }

    /// #7131: only an `assert_eq!` on the whole collection, against an
    /// operand that does not name the same binding, compares every pushed
    /// field.
    #[test]
    fn assertion_whole_collection_equality_rejects_projections_and_copies() {
        let whole = |text: &str| assertion_whole_collection_equality(text, "log");
        let read = |binding: &str, getter: Option<&str>| FieldCollectionRead {
            binding: binding.to_string(),
            getter: getter.map(str::to_string),
        };
        assert_eq!(
            whole("assert_eq!(inv.history(), &[Event::A]);"),
            vec![read("inv", Some("history"))]
        );
        assert_eq!(
            whole("assert_eq!(vec![Event::A], inv.log);"),
            vec![read("inv", None)]
        );
        // Both sides qualify: each is a candidate for the owner's binding.
        assert_eq!(
            whole("assert_eq!(expected.log, actual.log);"),
            vec![read("expected", None), read("actual", None)]
        );
        for other in [
            "assert_eq!(inv.history().len(), 1);",
            "assert_eq!(inv.log[0].qty, 2);",
            "assert_eq!(inv.log.iter().map(|e| e.qty).sum::<u32>(), 2);",
            "assert_eq!(inv.history(), inv.history());",
            "assert_eq!(inv.log, expected(&inv));",
            "assert_ne!(inv.history(), &[]);",
            "assert!(inv.history().is_empty());",
            "assert_eq!(inv.stock, vec![]);",
        ] {
            assert!(whole(other).is_empty(), "{other}");
        }
    }

    /// RIPR-SPEC-0228 keyed reads: a compared side that is exactly
    /// `binding.getter(key)` names its binding, getter and key.
    #[test]
    fn assertion_keyed_getter_reads_take_one_argument_calls_only() {
        let read = |binding: &str, getter: &str, key: &str, expected: &str| {
            vec![KeyedFieldRead {
                binding: binding.to_string(),
                getter: getter.to_string(),
                key: key.to_string(),
                expected: expected.to_string(),
            }]
        };
        assert_eq!(
            assertion_keyed_getter_reads("assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);"),
            read("inv", "on_hand", "\"BOLT-M8\"", "7")
        );
        assert_eq!(
            assertion_keyed_getter_reads("assert_eq!(7, inv.on_hand(sku));"),
            read("inv", "on_hand", "sku", "7")
        );
        assert_eq!(
            assertion_keyed_getter_reads("assert_eq!(inv.on_hand(&key(\"a,b\")), Some(&7));"),
            read("inv", "on_hand", "&key(\"a,b\")", "Some(&7)")
        );
        for other in [
            // A projection of the entry, two arguments or none, a chained
            // receiver, a key naming the binding, a comparison with the
            // same binding.
            "assert_eq!(inv.on_hand(\"A\") + 1, 8);",
            "assert_eq!(inv.on_hand(\"A\").max(1), 7);",
            "assert_eq!(inv.at(\"A\", 1), 7);",
            "assert_eq!(inv.total(), 7);",
            "assert_eq!(make().on_hand(\"A\"), 7);",
            "assert_eq!(inv.on_hand(inv.first()), 7);",
            "assert_eq!(inv.on_hand(\"A\"), inv.on_hand(\"B\"));",
            // No equality: nothing says which value the entry holds.
            "assert!(inv.on_hand(\"A\"));",
            "assert_ne!(inv.on_hand(\"A\"), 0);",
        ] {
            assert!(assertion_keyed_getter_reads(other).is_empty(), "{other}");
        }
    }

    /// RIPR-SPEC-0228 keyed reads: a field-map insert names the owner
    /// parameter its key comes from.
    #[test]
    fn field_collection_insert_key_names_the_key_parameter() {
        for (expression, key) in [
            ("self.stock.insert(sku.to_string(), remaining)", "sku"),
            ("self.stock.insert(sku.to_owned(), remaining);", "sku"),
            ("self.stock.insert(sku, n)", "sku"),
            ("self.a.b.insert(id.clone(), n)", "id"),
            ("self.stock.insert(id.into(), n)", "id"),
            ("self.stock.insert(String::from(name), n)", "name"),
        ] {
            assert_eq!(
                field_collection_insert_key(expression).as_deref(),
                Some(key),
                "{expression}"
            );
        }
        for other in [
            "self.stock.push(sku)",
            "stock.insert(sku, n)",
            "self.stock.insert(format!(\"{sku}\"), n)",
            "self.stock.insert(sku.trim().to_string(), n)",
            "self.stock.insert(\"fixed\", n)",
            "self.stock.insert(sku, n).unwrap()",
            "self.stock.entry(sku).or_insert(n)",
        ] {
            assert_eq!(field_collection_insert_key(other), None, "{other}");
        }
    }

    /// RIPR-SPEC-0228: a compared side reads the field collection directly
    /// or through one argument-free method; another field, another root, or a
    /// computed value is not such a read.
    #[test]
    fn assertion_field_collection_reads_name_binding_and_getter() {
        let direct = |binding: &str| {
            vec![FieldCollectionRead {
                binding: binding.to_string(),
                getter: None,
            }]
        };
        let through = |binding: &str, getter: &str| {
            vec![FieldCollectionRead {
                binding: binding.to_string(),
                getter: Some(getter.to_string()),
            }]
        };
        let reads = |text: &str| assertion_field_collection_reads(text, "log", false);
        assert_eq!(reads("assert_eq!(inv.log, vec![1]);"), direct("inv"));
        assert_eq!(reads("assert_eq!(&inv.log[0], &1);"), direct("inv"));
        assert_eq!(
            reads("assert_eq!(inv.history(), &[1]);"),
            through("inv", "history")
        );
        // Either side of an equality may hold the actual value.
        assert_eq!(reads("assert_eq!(vec![1], inv.log);"), direct("inv"));
        assert_eq!(
            reads("assert_eq!(&[1], inv.history());"),
            through("inv", "history")
        );
        for other in [
            "assert_eq!(inv.logs, vec![1]);",
            "assert_eq!(inv.at(0), 1);",
            "assert_eq!(make().log, vec![1]);",
            "assert_eq!(inv.history().first().copied().unwrap() + 1, 2);",
            "assert_ne!(vec![1], inv.log);",
            // Compared with itself, or with a value read from the same binding.
            "assert_eq!(inv.history(), inv.history());",
            "assert_eq!(inv.log, expected_for(&inv));",
            // A count does not read a changed element.
            "assert_eq!(inv.log.len(), 1);",
            "assert!(inv.history().is_empty());",
        ] {
            assert!(reads(other).is_empty(), "{other}");
        }
        // A deleted write changes the count, so a count reads it.
        assert_eq!(
            assertion_field_collection_reads("assert_eq!(inv.log.len(), 1);", "log", true),
            direct("inv")
        );
    }

    #[test]
    fn direct_collection_push_completes_effect_target_and_rejects_wrong_observer()
    -> Result<(), String> {
        let probe = probe(ProbeFamily::SideEffect, "items.push(5)");
        let sinks = vec![sink(FlowSinkKind::StateWrite, "items.push(5)", 14)];
        let witness = current_path_witness(&probe, &sinks)
            .ok_or_else(|| "direct collection witness was absent".to_string())?;
        assert_eq!(witness.edges[0].kind, PropagationEdgeKind::EffectTarget);
        assert_eq!(witness.edges[0].status, EdgeStatus::Established);
        assert_eq!(witness.completeness, PathCompleteness::Complete);
        assert!(complete_direct_witness(&probe, Some(&witness)));
        assert!(
            witness
                .limitations
                .contains(&"direct_collection_state_write".to_string())
        );
        assert!(
            !witness
                .limitations
                .contains(&"observer_binding_not_modeled".to_string())
        );
        assert_eq!(
            direct_collection_mutation_receiver("items.push(5)"),
            Some("items")
        );
        assert!(assertion_observes_direct_collection(
            "assert_eq!(items, expected);",
            "items"
        ));
        assert!(assertion_observes_direct_collection(
            "assert_eq!(items.len(), 1);",
            "items"
        ));
        assert!(!assertion_observes_direct_collection(
            "assert_eq!(other, expected);",
            "items"
        ));
        assert!(
            !assertion_observes_direct_collection("assert_eq!(other, items);", "items"),
            "expected-side token coincidence must not observe the mutated collection"
        );
        assert!(!assertion_observes_direct_collection(
            "assert_eq!(items_backup, expected);",
            "items"
        ));
        assert!(!assertion_observes_direct_collection(
            "assert!(result);",
            "items"
        ));
        assert!(!assertion_observes_direct_collection(
            "assert!(label.contains(\"record_effect\"));",
            "items"
        ));
        assert!(
            !assertion_observes_direct_collection("assert_eq!(items.clear(), ());", "items"),
            "a mutating method observes its unit return, not the collection contents"
        );
        assert!(
            !assertion_observes_direct_collection("assert_eq!(items.push(1), ());", "items"),
            "asserting a second mutation must not confirm the production push"
        );
        assert!(
            !assertion_observes_direct_collection(
                "assert!(label == \"assert_eq!(items, expected)\");",
                "items"
            ),
            "a quoted assert_eq! must not supply the collection observer"
        );
        Ok(())
    }

    /// #7135: a collection subject is only the root, an index/slice of the
    /// root, or one value-read of the root, and it must end there. Trailing
    /// operators and `capacity` are not discriminators.
    #[test]
    fn collection_subject_must_end_after_root_index_or_value_read() {
        for assertion in [
            "assert_eq!(items.capacity() >= 1, true);",
            "assert_eq!(items.capacity(), 4);",
            "assert_eq!(items.is_empty() || true, true);",
            "assert_eq!(items.len() * 0, 0);",
            "assert_eq!(items[0] + 1, 6);",
            "assert_eq!(items.len().min(1), 1);",
        ] {
            assert!(
                !assertion_observes_direct_collection(assertion, "items"),
                "non-discriminating subject must not observe the collection: {assertion}"
            );
        }

        assert!(
            assertion_observes_direct_collection("assert_eq!(items.len(), 1);", "items"),
            "an exact len() read must still observe the collection"
        );
        assert!(
            assertion_observes_direct_collection("assert_eq!(items[0], 5);", "items"),
            "an index into the collection must still observe it"
        );
        assert!(
            assertion_observes_direct_collection("assert_eq!(items[..], expected);", "items"),
            "a whole-slice read must still observe the collection"
        );
        assert!(
            assertion_observes_direct_collection("assert_eq!(items, expected);", "items"),
            "a whole-collection equality must still observe it"
        );
        assert!(
            assertion_observes_direct_collection("assert_eq!(&items[..], expected);", "items"),
            "a borrowed whole-slice read must still observe the collection"
        );
        assert!(
            assertion_observes_direct_collection("assert!(items.is_empty());", "items"),
            "a bare is_empty() read must still observe the collection"
        );
        assert!(
            assertion_observes_direct_collection(
                "assert_eq!(items.contains(&\")\".to_string()), true);",
                "items"
            ),
            "a value-read whose argument string holds a parenthesis must still observe the collection"
        );
        assert!(
            !assertion_observes_direct_collection(
                "assert_eq!(items.contains(&\")\".to_string()) || true, true);",
                "items"
            ),
            "a trailing operator after a string-bearing value-read must still refuse"
        );
        assert!(
            assertion_observes_direct_collection(
                "assert_eq!(items.contains(&\"(\".to_string()), true);",
                "items"
            ),
            "a value-read whose argument string holds an opening parenthesis must still observe the collection"
        );
        assert!(
            !assertion_observes_direct_collection(
                "assert_eq!(items.contains(&\"(\".to_string()) || true, true);",
                "items"
            ),
            "a trailing operator after an opening-paren string value-read must still refuse"
        );
        assert!(
            assertion_observes_direct_collection(
                "assert_eq!(items, Vec::<&'static str>::new());",
                "items"
            ),
            "a lifetime on the expected side must not prevent a whole-collection observer"
        );
        assert!(
            assertion_observes_direct_collection(
                "assert_eq!(items.contains(&')'), true);",
                "items"
            ),
            "a char-literal value-read argument must still observe the collection"
        );
    }

    #[test]
    fn self_field_event_and_helper_effects_stay_partial() {
        assert_eq!(
            direct_collection_mutation_receiver("self.items.push(5)"),
            None
        );
        assert_eq!(
            direct_collection_mutation_receiver("cache.entries.push(5)"),
            None
        );
        assert_eq!(
            direct_collection_mutation_receiver("events.publish(score)"),
            None
        );
        assert_eq!(
            direct_collection_mutation_receiver("cache.insert(\"result_key\", result)"),
            None,
            "CallDeletion cache.insert is a delivered fixture family, not this admission"
        );
        assert_eq!(
            direct_collection_mutation_receiver("items.insert(0, 5)"),
            None,
            "Vec::insert is a later sibling, not the push family"
        );
        let cache_insert = current_path_witness(
            &probe(
                ProbeFamily::CallDeletion,
                "cache.insert(\"result_key\", result)",
            ),
            &[sink(
                FlowSinkKind::StateWrite,
                "cache.insert(\"result_key\", result)",
                15,
            )],
        );
        assert!(cache_insert.is_some_and(|witness| {
            witness.completeness == PathCompleteness::Partial
                && !complete_direct_witness(
                    &probe(
                        ProbeFamily::CallDeletion,
                        "cache.insert(\"result_key\", result)",
                    ),
                    Some(&witness),
                )
        }));
        let event = current_path_witness(
            &probe(ProbeFamily::SideEffect, "self.handle(value)"),
            &[sink(FlowSinkKind::CallEffect, "self.handle(value)", 14)],
        );
        assert!(event.is_some_and(|witness| {
            witness.completeness == PathCompleteness::Partial
                && !complete_direct_witness(
                    &probe(ProbeFamily::SideEffect, "self.handle(value)"),
                    Some(&witness),
                )
        }));
    }
}
