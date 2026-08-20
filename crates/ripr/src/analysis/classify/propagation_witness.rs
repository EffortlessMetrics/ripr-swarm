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

const SCHEMA_VERSION: u16 = 1;

/// A versioned internal witness.  This type is intentionally scoped to the
/// analysis crate; it is not a public output or protocol contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct PropagationWitnessV1 {
    pub(in crate::analysis) schema_version: u16,
    pub(in crate::analysis) behavior: BehaviorIdentity,
    pub(in crate::analysis) source: PropagationEndpoint,
    pub(in crate::analysis) edges: Vec<PropagationEdge>,
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
    /// A local source-to-sink fact exists, but observer binding is not modeled
    /// by this PR.  It cannot by itself justify `propagate=yes`.
    Partial,
    Unresolved,
}

impl PathCompleteness {
    fn as_str(&self) -> &'static str {
        match self {
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
    let sink = flow_sinks.iter().find(|sink| {
        sink.owner.as_ref() == Some(&owner)
            && sink.kind != FlowSinkKind::Unknown
            && family_accepts_sink(&probe.family, &sink.kind)
            && source_sink_tokens_overlap(&probe.expression, &sink.text)
            && !opaque_path_text(&probe.expression)
            && !opaque_path_text(&sink.text)
    })?;

    let edge_kind = edge_kind_for_sink(&sink.kind)?;
    let source_identity = normalize_semantic_text(&probe.expression);
    let sink_identity = normalize_semantic_text(&sink.text);
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
        edges: vec![PropagationEdge {
            kind: edge_kind,
            status: edge_status(&source_identity, &sink_identity),
            from: source_identity,
            to: sink_identity.clone(),
        }],
        sink: PropagationEndpoint {
            kind: sink.kind.as_str().to_string(),
            identity: sink_identity,
            line: sink.line,
        },
        completeness: PathCompleteness::Partial,
        limitations: vec![
            "observer_binding_not_modeled".to_string(),
            "static_local_flow_only".to_string(),
        ],
        semantic_digest: String::new(),
    };
    witness.semantic_digest = witness.compute_semantic_digest();
    Some(witness)
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

fn opaque_path_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("dyn ")
        || lower.contains("box<dyn")
        || lower.contains("ffi")
        || lower.contains("extern ")
        || has_macro_invocation(&lower)
        || has_closure_syntax(&lower)
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
    let source_path = qualified_path_identity(source);
    let sink_path = qualified_path_identity(sink);
    !source_tokens.is_empty()
        && receiver_identity(source) == receiver_identity(sink)
        && paths_are_compatible(source, sink, source_path.as_deref(), sink_path.as_deref())
        && semantic_tokens(sink)
            .iter()
            .any(|token| source_tokens.iter().any(|source| source == token))
}

fn qualified_path_identity(text: &str) -> Option<String> {
    let path = text
        .split_once('(')
        .map_or(text, |(callee, _)| callee)
        .trim();
    (path.contains('.') || path.contains("::")).then(|| normalize_semantic_text(path))
}

fn paths_are_compatible(
    source: &str,
    sink: &str,
    source_path: Option<&str>,
    sink_path: Option<&str>,
) -> bool {
    match (source_path, sink_path) {
        (Some(source), Some(sink)) => source == sink,
        (None, None) => true,
        (None, Some(path)) => is_variant_wrapper(source, path),
        (Some(path), None) => is_variant_wrapper(sink, path),
    }
}

fn is_variant_wrapper(text: &str, qualified_path: &str) -> bool {
    let Some((callee, _)) = text.split_once('(') else {
        return false;
    };
    let Some(last_path_component) = qualified_path.rsplit("::").next() else {
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
    (!receiver.is_empty()).then(|| receiver.to_ascii_lowercase())
}

fn semantic_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
        .filter(|token| !is_keyword_or_noise(token))
        .map(|token| token.to_ascii_lowercase())
        .collect()
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

fn normalize_semantic_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl PropagationWitnessV1 {
    pub(in crate::analysis) fn digest_matches(&self) -> bool {
        self.semantic_digest == self.compute_semantic_digest()
    }

    fn compute_semantic_digest(&self) -> String {
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
        append_canonical_section_header(&mut canonical, "sink", 2);
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
    use crate::analysis::facts::{FunctionSummary, ReturnFact};
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use std::path::PathBuf;

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
            body: "fn calculate() { status: amount; }".to_string(),
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
            is_test: false,
            attrs: Vec::new(),
        };
        let flow_sinks = super::super::local_flow_sinks(probe, Some(&owner));
        current_path_witness(probe, &flow_sinks)
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
                EdgeStatus::Candidate,
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
            assert_eq!(witness.completeness, PathCompleteness::Partial);
            assert_eq!(witness.semantic_digest, witness.compute_semantic_digest());
        }
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
                &probe(ProbeFamily::ReturnValue, "x"),
                &[sink(FlowSinkKind::ReturnValue, "Ok(x)", 14)]
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
}
