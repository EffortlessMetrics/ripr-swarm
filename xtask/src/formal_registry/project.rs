use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimIndexEntry {
    claim_id: String,
    claim_generation: u64,
    criticality: String,
    phase: String,
    provider: String,
    route: ProviderRoute,
    owners: Vec<String>,
    source_closure: Vec<String>,
    authority_paths: Vec<String>,
    subjects: Vec<String>,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimIndex {
    generation: String,
    global_authority_paths: Vec<String>,
    claims: Vec<ClaimIndexEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimIndexDocument {
    schema: &'static str,
    repository: String,
    claim_index: ClaimIndex,
    execution_authority: &'static str,
    native_assurance: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ScoreboardClaim {
    claim_id: String,
    claim_generation: u64,
    criticality: String,
    phase: String,
    provider: String,
    route: ProviderRoute,
    status: &'static str,
    reasons: Vec<&'static str>,
    subjects: Vec<String>,
    decision_digest: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ScoreboardSummary {
    registered_claims: usize,
    established: usize,
    failed: usize,
    not_established: usize,
    stale: usize,
    unsupported: usize,
    not_proven: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct Scoreboard {
    schema: &'static str,
    repository: String,
    registry_generation: String,
    native_result_join: &'static str,
    native_assurance: &'static str,
    summary: ScoreboardSummary,
    claims: Vec<ScoreboardClaim>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryCheck {
    schema: &'static str,
    repository: String,
    registry_generation: String,
    registry_digest: String,
    active_claims: usize,
    retired_claims: usize,
    valid: bool,
    native_assurance: &'static str,
    execution_authority: &'static str,
}

fn claim_index(registry: &LoadedRegistry) -> ClaimIndexDocument {
    let claims = registry
        .claims
        .iter()
        .map(|loaded| ClaimIndexEntry {
            claim_id: loaded.claim.claim_id.clone(),
            claim_generation: loaded.claim.claim_generation,
            criticality: loaded.claim.criticality.clone(),
            phase: loaded.claim.phase.clone(),
            provider: loaded.claim.provider.clone(),
            route: loaded.route,
            owners: sorted(&loaded.claim.owners),
            source_closure: sorted(&loaded.claim.source_closure),
            authority_paths: sorted(&loaded.authority_paths),
            subjects: sorted(&loaded.claim.subjects),
            limitations: sorted(&loaded.claim.non_claims),
        })
        .collect();
    ClaimIndexDocument {
        schema: "ripr.formal-claim-index.v1",
        repository: registry.manifest.repository.clone(),
        claim_index: ClaimIndex {
            generation: registry.generation.clone(),
            global_authority_paths: sorted(&registry.manifest.global_authority_paths),
            claims,
        },
        execution_authority: "not_granted",
        native_assurance: "not_assessed",
    }
}

fn scoreboard(registry: &LoadedRegistry) -> Scoreboard {
    let mut unsupported = 0usize;
    let claims = registry
        .claims
        .iter()
        .map(|loaded| {
            let (status, reasons) = match loaded.route {
                ProviderRoute::Unsupported => {
                    unsupported += 1;
                    (
                        "UNSUPPORTED",
                        vec![
                            "The claim is applicable but has no admitted provider route.",
                            "No exact current execution or manual acceptance was joined.",
                        ],
                    )
                }
                ProviderRoute::Manual => (
                    "NOT_ESTABLISHED",
                    vec![
                        "The manual route has no accepted evidence joined to this registry-only scoreboard.",
                        "Currentness, negative controls and transition acceptance remain unassessed.",
                    ],
                ),
                ProviderRoute::Native => (
                    "NOT_ESTABLISHED",
                    vec![
                        "No exact admitted native proof run was supplied to this registry-only scoreboard.",
                        "Negative-control execution and currentness are not joined by this command.",
                        "Provider, toolchain, source-closure, merge-group and runner identities are not established.",
                    ],
                ),
            };
            ScoreboardClaim {
                claim_id: loaded.claim.claim_id.clone(),
                claim_generation: loaded.claim.claim_generation,
                criticality: loaded.claim.criticality.clone(),
                phase: loaded.claim.phase.clone(),
                provider: loaded.claim.provider.clone(),
                route: loaded.route,
                status,
                reasons,
                subjects: sorted(&loaded.claim.subjects),
                decision_digest: loaded.decision_digest.clone(),
            }
        })
        .collect::<Vec<_>>();
    Scoreboard {
        schema: "ripr.formal-assurance-scoreboard.v1",
        repository: registry.manifest.repository.clone(),
        registry_generation: registry.generation.clone(),
        native_result_join: "not_supplied",
        native_assurance: "not_established",
        summary: ScoreboardSummary {
            registered_claims: claims.len(),
            established: 0,
            failed: 0,
            not_established: claims.len().saturating_sub(unsupported),
            stale: 0,
            unsupported,
            not_proven: 0,
        },
        claims,
    }
}

fn registry_check(registry: &LoadedRegistry) -> RegistryCheck {
    RegistryCheck {
        schema: "ripr.formal-claim-registry-check.v1",
        repository: registry.manifest.repository.clone(),
        registry_generation: registry.generation.clone(),
        registry_digest: registry.registry_digest.clone(),
        active_claims: registry.claims.len(),
        retired_claims: registry.manifest.retirements.len(),
        valid: true,
        native_assurance: "not_assessed",
        execution_authority: "not_granted",
    }
}

fn markdown_check(value: &RegistryCheck) -> String {
    format!(
        "# Formal claim registry check\n\n- Repository: `{}`\n- Generation: `{}`\n- Digest: `{}`\n- Active claims: {}\n- Retired claims: {}\n- Native assurance: `{}`\n- Execution authority: `{}`\n",
        value.repository,
        value.registry_generation,
        value.registry_digest,
        value.active_claims,
        value.retired_claims,
        value.native_assurance,
        value.execution_authority,
    )
}

fn markdown_index(value: &ClaimIndexDocument) -> String {
    let mut output = format!(
        "# Formal claim index\n\nRepository: `{}`  \nGeneration: `{}`  \nExecution authority: `{}`  \nNative assurance: `{}`\n\n",
        value.repository,
        value.claim_index.generation,
        value.execution_authority,
        value.native_assurance,
    );
    output.push_str(
        "| Claim | Generation | Criticality | Phase | Provider route | Subjects |\n\
         | --- | ---: | --- | --- | --- | ---: |\n",
    );
    for claim in &value.claim_index.claims {
        output.push_str(&format!(
            "| `{}` | {} | `{}` | `{}` | `{}/{}` | {} |\n",
            claim.claim_id,
            claim.claim_generation,
            claim.criticality,
            claim.phase,
            claim.provider,
            claim.route.as_str(),
            claim.subjects.len(),
        ));
    }
    output
}

fn markdown_scoreboard(value: &Scoreboard) -> String {
    let mut output = format!(
        "# Formal assurance scoreboard\n\nRepository: `{}`  \nRegistry generation: `{}`  \nNative result join: `{}`  \nNative assurance: `{}`\n\n",
        value.repository,
        value.registry_generation,
        value.native_result_join,
        value.native_assurance,
    );
    output.push_str(
        "| Claim | Generation | Status | Provider route | Reason |\n\
         | --- | ---: | --- | --- | --- |\n",
    );
    for claim in &value.claims {
        output.push_str(&format!(
            "| `{}` | {} | `{}` | `{}/{}` | {} |\n",
            claim.claim_id,
            claim.claim_generation,
            claim.status,
            claim.provider,
            claim.route.as_str(),
            claim.reasons.join(" "),
        ));
    }
    output
}

pub(super) fn render_check(
    registry: &LoadedRegistry,
    format: OutputFormat,
) -> RegistryResult<String> {
    let value = registry_check(registry);
    render(&value, format, || markdown_check(&value))
}

pub(super) fn render_index(
    registry: &LoadedRegistry,
    format: OutputFormat,
) -> RegistryResult<String> {
    let value = claim_index(registry);
    render(&value, format, || markdown_index(&value))
}

pub(super) fn render_scoreboard(
    registry: &LoadedRegistry,
    format: OutputFormat,
) -> RegistryResult<String> {
    let value = scoreboard(registry);
    render(&value, format, || markdown_scoreboard(&value))
}
