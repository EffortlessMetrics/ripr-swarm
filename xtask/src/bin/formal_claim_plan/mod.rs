//! Passive, provider-free affected-claim planning for #14474.
//!
//! This binary consumes supplied claim-index and change-fact documents. It does
//! not inspect Git, execute a verifier, authenticate either producer, or grant
//! CI authority. Unknown or incomplete impact widens to every registered
//! critical claim; it never silently selects no work.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_INPUT_BYTES: u64 = 4 * 1_048_576;
const USAGE: &str = concat!(
    "usage: formal-claim-plan --input FILE [--format json|markdown]\n",
    "Exit 0: a passive plan was produced; 2: invalid input. ",
    "Execution authority and native assurance are never granted.",
);

type PlanResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum Criticality {
    Critical,
    Important,
    Advisory,
    Scheduled,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum Phase {
    AdmittedMergeGroup,
    OrdinaryPrCritical,
    MergeReady,
    Scheduled,
    Release,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum ProviderRoute {
    Native,
    Manual,
    Unsupported,
}

impl ProviderRoute {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Manual => "manual",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ImpactClass {
    NonSemantic,
    FormalIrrelevant,
    FormalRelevant,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Member {
    pr_number: u64,
    head: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimIndexEntry {
    claim_id: String,
    claim_generation: u64,
    criticality: Criticality,
    phase: Phase,
    provider: String,
    route: ProviderRoute,
    owners: Vec<String>,
    source_closure: Vec<String>,
    authority_paths: Vec<String>,
    subjects: Vec<String>,
    #[serde(default)]
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimIndex {
    generation: String,
    #[serde(default)]
    global_authority_paths: Vec<String>,
    claims: Vec<ClaimIndexEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChangedPath {
    path: String,
    impact: ImpactClass,
    #[serde(default)]
    semantic_subjects: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChangeSet {
    base: String,
    group: String,
    members: Vec<Member>,
    expected_registry_generation: String,
    controller_generation: String,
    producer_generation: String,
    proof_profile: String,
    classification_complete: bool,
    changes: Vec<ChangedPath>,
    #[serde(default)]
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectionInput {
    schema: String,
    repository: String,
    claim_index: ClaimIndex,
    change_set: ChangeSet,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Selected,
    NotSelected,
}

impl Disposition {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::NotSelected => "not_selected",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct PlannedClaim {
    claim_id: String,
    claim_generation: u64,
    criticality: Criticality,
    phase: Phase,
    provider: String,
    route: ProviderRoute,
    disposition: Disposition,
    reasons: Vec<String>,
    subjects: Vec<String>,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Plan {
    schema: &'static str,
    repository: String,
    base: String,
    group: String,
    members: Vec<Member>,
    registry_generation: String,
    controller_generation: String,
    producer_generation: String,
    proof_profile: String,
    selection_input_digest: String,
    classification_complete: bool,
    fallback_all_critical: bool,
    fallback_reasons: Vec<String>,
    critical_claim_count: usize,
    selected_claim_count: usize,
    claims: Vec<PlannedClaim>,
    expected_native_subjects: Vec<String>,
    zero_selection_basis: Option<String>,
    execution_authority: &'static str,
    native_assurance: &'static str,
    limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Markdown,
}

fn require(condition: bool, message: impl Into<String>) -> PlanResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn nonblank(value: &str, field: &str) -> PlanResult<()> {
    require(!value.trim().is_empty(), format!("{field}: blank value"))
}

fn stable_id(value: &str, field: &str) -> PlanResult<()> {
    let mut bytes = value.bytes();
    let valid_first = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    require(
        value.len() >= 2
            && valid_first
            && bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            }),
        format!("{field}: expected stable lowercase identifier"),
    )
}

fn git_object(value: &str, field: &str) -> PlanResult<()> {
    require(
        matches!(value.len(), 40 | 64)
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        format!("{field}: expected full lowercase Git object ID"),
    )
}

fn repository_path(value: &str, field: &str) -> PlanResult<()> {
    nonblank(value, field)?;
    require(
        !value.contains('\\')
            && !value.contains(':')
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        format!("{field}: expected repository-relative path without traversal"),
    )
}

fn unique_strings(values: &[String], field: &str, require_nonempty: bool) -> PlanResult<()> {
    require(
        !require_nonempty || !values.is_empty(),
        format!("{field}: empty list"),
    )?;
    let mut seen = BTreeSet::new();
    for value in values {
        nonblank(value, field)?;
        require(
            seen.insert(value),
            format!("{field}: duplicate value {value}"),
        )?;
    }
    Ok(())
}

fn unique_paths(values: &[String], field: &str) -> PlanResult<()> {
    unique_strings(values, field, false)?;
    for value in values {
        repository_path(value, field)?;
    }
    Ok(())
}

fn validate_claim(claim: &ClaimIndexEntry) -> PlanResult<()> {
    stable_id(&claim.claim_id, "claim_id")?;
    require(
        claim.claim_generation > 0,
        format!("{}: claim_generation must be positive", claim.claim_id),
    )?;
    nonblank(&claim.provider, "provider")?;
    unique_strings(&claim.owners, "owners", true)?;
    unique_paths(&claim.source_closure, "source_closure")?;
    unique_paths(&claim.authority_paths, "authority_paths")?;
    unique_strings(&claim.subjects, "subjects", false)?;
    unique_strings(&claim.limitations, "limitations", false)?;
    require(
        !claim.source_closure.is_empty() || !claim.authority_paths.is_empty(),
        format!(
            "{}: at least one source-closure or authority path is required",
            claim.claim_id
        ),
    )?;
    if claim.route == ProviderRoute::Native {
        require(
            claim.provider == "kani",
            format!("{}: only the Kani native route is admitted", claim.claim_id),
        )?;
        require(
            !claim.subjects.is_empty(),
            format!("{}: native route requires subjects", claim.claim_id),
        )?;
    }
    Ok(())
}

fn validate_input(input: &SelectionInput) -> PlanResult<()> {
    require(
        input.schema == "ripr.formal-claim-selection-input.v1",
        "unsupported selection-input schema",
    )?;
    nonblank(&input.repository, "repository")?;
    nonblank(&input.claim_index.generation, "registry generation")?;
    nonblank(
        &input.change_set.expected_registry_generation,
        "expected registry generation",
    )?;
    require(
        input.change_set.expected_registry_generation == input.claim_index.generation,
        "selected registry generation does not match the supplied registry",
    )?;
    nonblank(
        &input.change_set.controller_generation,
        "controller generation",
    )?;
    nonblank(&input.change_set.producer_generation, "producer generation")?;
    nonblank(&input.change_set.proof_profile, "proof profile")?;
    git_object(&input.change_set.base, "base")?;
    git_object(&input.change_set.group, "group")?;
    require(
        input.change_set.base != input.change_set.group,
        "base and group must be distinct",
    )?;
    require(
        !input.change_set.members.is_empty(),
        "ordered admitted members must be nonempty",
    )?;
    let mut member_numbers = BTreeSet::new();
    let mut member_heads = BTreeSet::new();
    for member in &input.change_set.members {
        require(member.pr_number > 0, "member PR number must be positive")?;
        git_object(&member.head, "member head")?;
        require(
            member_numbers.insert(member.pr_number),
            format!("duplicate member PR {}", member.pr_number),
        )?;
        require(
            member_heads.insert(member.head.as_str()),
            format!("duplicate member head {}", member.head),
        )?;
    }
    require(
        !member_heads.contains(input.change_set.base.as_str()),
        "integration base cannot also be an admitted member head",
    )?;
    require(
        !member_heads.contains(input.change_set.group.as_str()),
        "integration group cannot also be an admitted member head",
    )?;
    unique_paths(
        &input.claim_index.global_authority_paths,
        "global_authority_paths",
    )?;
    require(!input.claim_index.claims.is_empty(), "claim index is empty")?;
    require(
        input
            .claim_index
            .claims
            .iter()
            .any(|claim| claim.criticality == Criticality::Critical),
        "claim index has no critical claims",
    )?;
    let mut claims = BTreeSet::new();
    for claim in &input.claim_index.claims {
        validate_claim(claim)?;
        require(
            claims.insert((claim.claim_id.as_str(), claim.claim_generation)),
            format!(
                "duplicate claim identity {} generation {}",
                claim.claim_id, claim.claim_generation
            ),
        )?;
    }
    let mut latest = BTreeMap::new();
    for claim in &input.claim_index.claims {
        if let Some(previous) = latest.insert(claim.claim_id.as_str(), claim.claim_generation) {
            require(
                previous == claim.claim_generation,
                format!("multiple active generations for {}", claim.claim_id),
            )?;
        }
    }
    let mut changed_paths = BTreeSet::new();
    for changed in &input.change_set.changes {
        repository_path(&changed.path, "changed path")?;
        require(
            changed_paths.insert(changed.path.as_str()),
            format!("duplicate changed path {}", changed.path),
        )?;
        unique_strings(
            &changed.semantic_subjects,
            "changed semantic subjects",
            false,
        )?;
    }
    unique_strings(&input.change_set.limitations, "change limitations", false)?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(71);
    encoded.push_str("sha256:");
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn plan(input: &SelectionInput, input_digest: String) -> PlanResult<Plan> {
    validate_input(input)?;

    let global_paths: BTreeSet<&str> = input
        .claim_index
        .global_authority_paths
        .iter()
        .map(String::as_str)
        .collect();
    let mut fallback_reasons = BTreeSet::new();
    if input.change_set.changes.is_empty() {
        fallback_reasons.insert("no_change_facts_supplied".to_string());
    }
    if !input.change_set.classification_complete {
        fallback_reasons.insert("change_classification_incomplete".to_string());
    }
    for changed in &input.change_set.changes {
        if global_paths.contains(changed.path.as_str()) {
            fallback_reasons.insert(format!("global_authority_changed:{}", changed.path));
        }
        if changed.impact == ImpactClass::Unknown {
            fallback_reasons.insert(format!("unknown_change_impact:{}", changed.path));
        }
    }

    let mut claims = input.claim_index.claims.clone();
    claims.sort_by(|left, right| {
        left.claim_id
            .cmp(&right.claim_id)
            .then(left.claim_generation.cmp(&right.claim_generation))
    });

    let mut direct_reasons: BTreeMap<(String, u64), BTreeSet<String>> = BTreeMap::new();
    let mut matched_relevant_paths = BTreeSet::new();
    for claim in &claims {
        let key = (claim.claim_id.clone(), claim.claim_generation);
        let reasons = direct_reasons.entry(key).or_default();
        for changed in &input.change_set.changes {
            if claim.source_closure.contains(&changed.path) {
                reasons.insert(format!("source_closure_changed:{}", changed.path));
                matched_relevant_paths.insert(changed.path.as_str());
            }
            if claim.authority_paths.contains(&changed.path) {
                reasons.insert(format!("claim_authority_changed:{}", changed.path));
                matched_relevant_paths.insert(changed.path.as_str());
            }
            for subject in &changed.semantic_subjects {
                if claim.owners.contains(subject) {
                    reasons.insert(format!("owner_subject_changed:{subject}"));
                    matched_relevant_paths.insert(changed.path.as_str());
                }
            }
        }
    }
    for changed in &input.change_set.changes {
        if changed.impact == ImpactClass::FormalRelevant
            && !matched_relevant_paths.contains(changed.path.as_str())
            && !global_paths.contains(changed.path.as_str())
        {
            fallback_reasons.insert(format!("unmapped_formal_relevant_change:{}", changed.path));
        }
    }

    let fallback_all_critical = !fallback_reasons.is_empty();
    let mut planned = Vec::with_capacity(claims.len());
    let mut native_subjects = BTreeSet::new();
    let mut critical_count = 0usize;
    let mut selected_count = 0usize;
    for claim in claims {
        let key = (claim.claim_id.clone(), claim.claim_generation);
        let mut reasons = direct_reasons.remove(&key).unwrap_or_default();
        let is_critical = claim.criticality == Criticality::Critical;
        if is_critical {
            critical_count += 1;
        }
        if fallback_all_critical && is_critical {
            reasons.insert("all_critical_fallback".to_string());
            reasons.extend(fallback_reasons.iter().cloned());
        }
        let selected = is_critical && (fallback_all_critical || !reasons.is_empty());
        if selected {
            selected_count += 1;
            if claim.route == ProviderRoute::Native {
                if claim.phase == Phase::AdmittedMergeGroup {
                    native_subjects.extend(claim.subjects.iter().cloned());
                } else {
                    reasons.insert("native_phase_not_admitted_for_merge_group".to_string());
                }
            }
        } else if !is_critical {
            reasons.insert("not_a_critical_claim".to_string());
        } else {
            reasons.insert("complete_change_facts_show_no_registered_impact".to_string());
        }
        planned.push(PlannedClaim {
            claim_id: claim.claim_id,
            claim_generation: claim.claim_generation,
            criticality: claim.criticality,
            phase: claim.phase,
            provider: claim.provider,
            route: claim.route,
            disposition: if selected {
                Disposition::Selected
            } else {
                Disposition::NotSelected
            },
            reasons: reasons.into_iter().collect(),
            subjects: claim.subjects,
            limitations: claim.limitations,
        });
    }

    require(
        !fallback_all_critical || selected_count == critical_count,
        "all-critical fallback omitted a critical claim",
    )?;

    let mut limitations = input.change_set.limitations.clone();
    limitations.extend([
        concat!(
            "Input registry, change facts, Git identities and generations are supplied ",
            "declarations and are not authenticated by this tool."
        )
        .to_string(),
        "The plan executes no provider and grants no CI or merge authority.".to_string(),
        concat!(
            "Source closure, semantic classification and provider applicability must be ",
            "established by their owning producers."
        )
        .to_string(),
        concat!(
            "A selected manual or unsupported route remains visible and cannot be ",
            "converted into native success."
        )
        .to_string(),
    ]);
    limitations.sort();
    limitations.dedup();

    Ok(Plan {
        schema: "ripr.formal-claim-plan.v1",
        repository: input.repository.clone(),
        base: input.change_set.base.clone(),
        group: input.change_set.group.clone(),
        members: input.change_set.members.clone(),
        registry_generation: input.claim_index.generation.clone(),
        controller_generation: input.change_set.controller_generation.clone(),
        producer_generation: input.change_set.producer_generation.clone(),
        proof_profile: input.change_set.proof_profile.clone(),
        selection_input_digest: input_digest,
        classification_complete: input.change_set.classification_complete,
        fallback_all_critical,
        fallback_reasons: fallback_reasons.into_iter().collect(),
        critical_claim_count: critical_count,
        selected_claim_count: selected_count,
        claims: planned,
        expected_native_subjects: native_subjects.into_iter().collect(),
        zero_selection_basis: (selected_count == 0 && !fallback_all_critical).then(|| {
            concat!(
                "All supplied changes were completely classified and matched no ",
                "registered critical source, authority path or owner subject."
            )
            .to_string()
        }),
        execution_authority: "not_granted",
        native_assurance: "not_assessed",
        limitations,
    })
}

fn escape_markdown(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn render_markdown(plan: &Plan) -> String {
    let members = plan
        .members
        .iter()
        .map(|member| format!("#{} `{}`", member.pr_number, member.head))
        .collect::<Vec<_>>()
        .join(", ");
    let mut output = String::new();
    output.push_str("# Formal critical-claim plan\n\n");
    output.push_str(&format!(
        "- Repository: `{}`\n",
        escape_markdown(&plan.repository)
    ));
    output.push_str(&format!("- Base: `{}`\n", plan.base));
    output.push_str(&format!("- Group: `{}`\n", plan.group));
    output.push_str(&format!("- Ordered members: {}\n", members));
    output.push_str(&format!(
        "- Registry/controller/producer: `{}` / `{}` / `{}`\n",
        escape_markdown(&plan.registry_generation),
        escape_markdown(&plan.controller_generation),
        escape_markdown(&plan.producer_generation)
    ));
    output.push_str(&format!(
        concat!(
            "- Classification complete: `{}`\n",
            "- All-critical fallback: `{}`\n",
            "- Selected: `{}/{}` critical claims\n"
        ),
        plan.classification_complete,
        plan.fallback_all_critical,
        plan.selected_claim_count,
        plan.critical_claim_count
    ));
    output.push_str("- Execution authority: `not_granted`\n- Native assurance: `not_assessed`\n\n");
    output.push_str("| Claim | Generation | Disposition | Route | Reasons |\n");
    output.push_str("| --- | ---: | --- | --- | --- |\n");
    for claim in &plan.claims {
        output.push_str(&format!(
            "| `{}` | {} | `{}` | `{}` | {} |\n",
            escape_markdown(&claim.claim_id),
            claim.claim_generation,
            claim.disposition.as_str(),
            claim.route.as_str(),
            claim
                .reasons
                .iter()
                .map(|reason| escape_markdown(reason))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !plan.fallback_reasons.is_empty() {
        output.push_str("\n## Fallback reasons\n\n");
        for reason in &plan.fallback_reasons {
            output.push_str(&format!("- {}\n", escape_markdown(reason)));
        }
    }
    if let Some(basis) = &plan.zero_selection_basis {
        output.push_str(&format!(
            "\n## Zero-selection basis\n\n{}\n",
            escape_markdown(basis)
        ));
    }
    output.push_str("\n## Limitations\n\n");
    for limitation in &plan.limitations {
        output.push_str(&format!("- {}\n", escape_markdown(limitation)));
    }
    output
}

mod input;
#[cfg(test)]
mod tests;

#[cfg(test)]
use input::parse_input;

pub(crate) fn entry() -> std::process::ExitCode {
    input::entry()
}
