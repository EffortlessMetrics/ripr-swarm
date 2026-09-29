use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    BehaviorEvidenceWitnessV1, InputCurrentness, RelationWitness, StageWitness, SubjectSet,
    sort_unique,
};

/// Result of comparing one paired diff/repo witness row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ParityDisposition {
    Equal,
    ExplainedScopeDifference,
    Contradiction,
    NotComparable,
}

impl ParityDisposition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::ExplainedScopeDifference => "explained_scope_difference",
            Self::Contradiction => "contradiction",
            Self::NotComparable => "not_comparable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StagePair {
    pub(crate) diff: String,
    pub(crate) repo: String,
    pub(crate) diff_digest_fragment: String,
    pub(crate) repo_digest_fragment: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ParityRow {
    pub(crate) case_id: String,
    pub(crate) portable_item_id: String,
    pub(crate) owner: String,
    pub(crate) family: String,
    pub(crate) diff_scope: String,
    pub(crate) repo_scope: String,
    pub(crate) candidate_relation_identities: Vec<String>,
    pub(crate) established_relation_identities: Vec<String>,
    pub(crate) reach: StagePair,
    pub(crate) activation: StagePair,
    pub(crate) propagation: StagePair,
    pub(crate) observation: StagePair,
    pub(crate) discrimination: StagePair,
    pub(crate) required_discriminator: String,
    pub(crate) diff_target: String,
    pub(crate) repo_target: String,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) public_class_diff: String,
    pub(crate) public_class_repo: String,
    pub(crate) diff_semantic_digest: String,
    pub(crate) repo_semantic_digest: String,
    pub(crate) disposition: ParityDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ParityReport {
    pub(crate) schema_version: u16,
    pub(crate) rows: Vec<ParityRow>,
}

impl ParityReport {
    pub(crate) fn from_rows(mut rows: Vec<ParityRow>) -> Self {
        rows.sort_by(|left, right| left.case_id.cmp(&right.case_id));
        Self {
            schema_version: super::SCHEMA_VERSION,
            rows,
        }
    }

    pub(crate) fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub(crate) fn to_markdown(&self) -> String {
        let mut out = String::from("# Behavior evidence parity\n\n");
        out.push_str(
            "| case | identity | owner | family | disposition | diff class | repo class |\n",
        );
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for row in &self.rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                row.case_id,
                row.portable_item_id,
                row.owner,
                row.family,
                row.disposition.as_str(),
                row.public_class_diff,
                row.public_class_repo
            ));
        }
        out
    }
}

pub(crate) fn compare_pair(
    case_id: &str,
    diff: &BehaviorEvidenceWitnessV1,
    repo: &BehaviorEvidenceWitnessV1,
) -> ParityRow {
    let disposition = disposition_for(diff, repo);
    let mut limitations = diff.limitations.clone();
    limitations.extend(repo.limitations.iter().cloned());
    sort_unique(&mut limitations);
    let mut non_claims = diff.non_claims.clone();
    non_claims.extend(repo.non_claims.iter().cloned());
    sort_unique(&mut non_claims);
    let mut candidate = relation_identities(&diff.candidate_relations);
    candidate.extend(relation_identities(&repo.candidate_relations));
    sort_unique(&mut candidate);
    let mut established = relation_identities(&diff.established_relations);
    established.extend(relation_identities(&repo.established_relations));
    sort_unique(&mut established);
    let portable_item_id = if diff.portable_item_id == repo.portable_item_id {
        diff.portable_item_id.clone()
    } else {
        format!("{}|{}", diff.portable_item_id, repo.portable_item_id)
    };
    ParityRow {
        case_id: case_id.to_string(),
        portable_item_id,
        owner: diff.owner.identity.clone(),
        family: diff.family.clone(),
        diff_scope: scope_tokens(diff).join(","),
        repo_scope: scope_tokens(repo).join(","),
        candidate_relation_identities: candidate,
        established_relation_identities: established,
        reach: stage_pair(&diff.reach, &repo.reach),
        activation: stage_pair(&diff.activation, &repo.activation),
        propagation: stage_pair(&diff.propagation, &repo.propagation),
        observation: stage_pair(&diff.observation, &repo.observation),
        discrimination: stage_pair(&diff.discrimination, &repo.discrimination),
        required_discriminator: diff.required_discriminator.identity.clone(),
        diff_target: diff.selected_target.kind().to_string(),
        repo_target: repo.selected_target.kind().to_string(),
        limitations,
        non_claims,
        public_class_diff: diff.public_class.clone(),
        public_class_repo: repo.public_class.clone(),
        diff_semantic_digest: diff.semantic_digest.clone(),
        repo_semantic_digest: repo.semantic_digest.clone(),
        disposition,
    }
}

pub(crate) fn pair_by_portable_id(
    lefts: &[BehaviorEvidenceWitnessV1],
    rights: &[BehaviorEvidenceWitnessV1],
) -> ParityReport {
    let mut rights_by_id: BTreeMap<String, Vec<&BehaviorEvidenceWitnessV1>> = BTreeMap::new();
    for right in rights {
        rights_by_id
            .entry(right.portable_item_id.clone())
            .or_default()
            .push(right);
    }
    let mut rows = Vec::new();
    let mut used_right_ids = Vec::new();
    for left in lefts {
        let case_id = format!("join:{}", left.portable_item_id);
        match rights_by_id.get(&left.portable_item_id) {
            Some(matches) if matches.len() == 1 && !left.portable_item_id.is_empty() => {
                used_right_ids.push(left.portable_item_id.clone());
                rows.push(compare_pair(&case_id, left, matches[0]));
            }
            _ => {
                rows.push(unmatched_row(&case_id, left, None));
            }
        }
    }
    for right in rights {
        if right.portable_item_id.is_empty() || !used_right_ids.contains(&right.portable_item_id) {
            rows.push(unmatched_row(
                &format!("join:unmatched:{}", right.portable_item_id),
                right,
                None,
            ));
        }
    }
    ParityReport::from_rows(rows)
}

fn unmatched_row(
    case_id: &str,
    witness: &BehaviorEvidenceWitnessV1,
    other: Option<&BehaviorEvidenceWitnessV1>,
) -> ParityRow {
    let empty = other;
    let absent = super::StageWitness::absent("absent");
    ParityRow {
        case_id: case_id.to_string(),
        portable_item_id: witness.portable_item_id.clone(),
        owner: witness.owner.identity.clone(),
        family: witness.family.clone(),
        diff_scope: scope_tokens(witness).join(","),
        repo_scope: empty.map(scope_tokens).unwrap_or_default().join(","),
        candidate_relation_identities: relation_identities(&witness.candidate_relations),
        established_relation_identities: relation_identities(&witness.established_relations),
        reach: stage_pair(&witness.reach, &absent),
        activation: stage_pair(&witness.activation, &absent),
        propagation: stage_pair(&witness.propagation, &absent),
        observation: stage_pair(&witness.observation, &absent),
        discrimination: stage_pair(&witness.discrimination, &absent),
        required_discriminator: witness.required_discriminator.identity.clone(),
        diff_target: witness.selected_target.kind().to_string(),
        repo_target: "absent".to_string(),
        limitations: witness.limitations.clone(),
        non_claims: witness.non_claims.clone(),
        public_class_diff: witness.public_class.clone(),
        public_class_repo: String::new(),
        diff_semantic_digest: witness.semantic_digest.clone(),
        repo_semantic_digest: String::new(),
        disposition: ParityDisposition::NotComparable,
    }
}

fn disposition_for(
    left: &BehaviorEvidenceWitnessV1,
    right: &BehaviorEvidenceWitnessV1,
) -> ParityDisposition {
    if left.portable_item_id.is_empty()
        || right.portable_item_id.is_empty()
        || left.semantic_digest.is_empty()
        || right.semantic_digest.is_empty()
        || !left.digest_matches()
        || !right.digest_matches()
    {
        return ParityDisposition::NotComparable;
    }
    if left.language != right.language {
        if has_language_scope_limit(left) || has_language_scope_limit(right) {
            return ParityDisposition::ExplainedScopeDifference;
        }
        return ParityDisposition::NotComparable;
    }
    if cores_equal(left, right) {
        if explaining_scope_tokens(left) == explaining_scope_tokens(right) {
            return ParityDisposition::Equal;
        }
        return ParityDisposition::ExplainedScopeDifference;
    }
    if explaining_scope_tokens(left) != explaining_scope_tokens(right) {
        return ParityDisposition::ExplainedScopeDifference;
    }
    ParityDisposition::Contradiction
}

fn has_language_scope_limit(witness: &BehaviorEvidenceWitnessV1) -> bool {
    witness
        .limitations
        .iter()
        .any(|token| token == "preview_language" || token == "cross_language_limit")
}

fn stage_pair(left: &StageWitness, right: &StageWitness) -> StagePair {
    StagePair {
        diff: left.state.clone(),
        repo: right.state.clone(),
        diff_digest_fragment: left
            .first_unresolved_edge
            .clone()
            .unwrap_or_else(|| left.state.clone()),
        repo_digest_fragment: right
            .first_unresolved_edge
            .clone()
            .unwrap_or_else(|| right.state.clone()),
    }
}

const SCOPE_LIMITATIONS: [&str; 8] = [
    "partial_index",
    "diff_only_subject_set",
    "workspace_complete",
    "stale_input",
    "wrong_input",
    "preview_language",
    "unspecified_input_completeness",
    "cross_language_limit",
];

fn cores_equal(left: &BehaviorEvidenceWitnessV1, right: &BehaviorEvidenceWitnessV1) -> bool {
    left.family == right.family
        && left.owner.identity == right.owner.identity
        && left.expression.normalized == right.expression.normalized
        && left.required_discriminator.identity == right.required_discriminator.identity
        && left.expected_sink == right.expected_sink
        && relation_identities(&left.established_relations)
            == relation_identities(&right.established_relations)
        && relation_identities(&left.candidate_relations)
            == relation_identities(&right.candidate_relations)
        && stage_cores_equal(&left.reach, &right.reach)
        && stage_cores_equal(&left.activation, &right.activation)
        && stage_cores_equal(&left.propagation, &right.propagation)
        && stage_cores_equal(&left.observation, &right.observation)
        && stage_cores_equal(&left.discrimination, &right.discrimination)
}

fn relation_identities(relations: &[RelationWitness]) -> Vec<String> {
    relations
        .iter()
        .map(RelationWitness::semantic_identity)
        .collect()
}

fn stage_cores_equal(left: &StageWitness, right: &StageWitness) -> bool {
    left.state == right.state
        && left.established_facts == right.established_facts
        && left.candidate_facts == right.candidate_facts
        && left.first_unresolved_edge == right.first_unresolved_edge
}

fn scope_tokens(witness: &BehaviorEvidenceWitnessV1) -> Vec<String> {
    let mut tokens = vec![
        witness.subject_set.as_str().to_string(),
        witness.currentness.as_str().to_string(),
    ];
    if witness.language_status.as_deref() == Some("preview") {
        tokens.push("preview_language".to_string());
    }
    tokens.extend(
        witness
            .limitations
            .iter()
            .filter(|token| SCOPE_LIMITATIONS.contains(&token.as_str()))
            .cloned(),
    );
    sort_unique(&mut tokens);
    tokens
}

/// Scope tokens that can explain a semantic difference. Inherent
/// diff-only versus workspace-complete pairing is the normal cross-path
/// comparison, not by itself an explanation of a contradiction.
fn explaining_scope_tokens(witness: &BehaviorEvidenceWitnessV1) -> Vec<String> {
    let mut tokens = Vec::new();
    match witness.subject_set {
        SubjectSet::PartialIndex | SubjectSet::Unspecified => {
            tokens.push(witness.subject_set.as_str().to_string());
        }
        SubjectSet::DiffOnly | SubjectSet::WorkspaceComplete => {}
    }
    match witness.currentness {
        InputCurrentness::Stale | InputCurrentness::Wrong | InputCurrentness::Unspecified => {
            tokens.push(witness.currentness.as_str().to_string());
        }
        InputCurrentness::Current => {}
    }
    if witness.language_status.as_deref() == Some("preview") {
        tokens.push("preview_language".to_string());
    }
    tokens.extend(
        witness
            .limitations
            .iter()
            .filter(|token| {
                matches!(
                    token.as_str(),
                    "partial_index"
                        | "stale_input"
                        | "wrong_input"
                        | "preview_language"
                        | "unspecified_input_completeness"
                        | "cross_language_limit"
                )
            })
            .cloned(),
    );
    sort_unique(&mut tokens);
    tokens
}
