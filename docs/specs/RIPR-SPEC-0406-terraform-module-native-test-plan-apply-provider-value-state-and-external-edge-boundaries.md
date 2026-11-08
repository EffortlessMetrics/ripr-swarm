# RIPR-SPEC-0406: Terraform module, native-test, plan/apply, provider, value, state, and external-edge preview boundaries

Status: proposed

Owner: language-adapter / terraform (#5603)

Created: 2026-11-02

Linked issues:

- [#5969](https://github.com/EffortlessMetrics/ripr-swarm/issues/5969) (this contract)
- Parent epic [#5603](https://github.com/EffortlessMetrics/ripr-swarm/issues/5603)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)
- Consumed authorities: [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) (language-neutral adapter boundary and shared witness/output fields)
- [RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md) (preview posture)
- [RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md) (capability cell identity)
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) (registered cross-language edges)

Support-tier impact:

- This spec does not enable Terraform, OpenTofu, generic HCL products, provider
  plugins, backends, or cloud APIs; it does not promote support tiers, arm
  default gates, badges, baselines, RIPR Zero blocking, or publication. All
  Terraform evidence governed here remains preview/advisory per capability cell
  until separate promotion work cites proof. The canonical support-tier boundary
  remains [support tiers](../status/SUPPORT_TIERS.md).
- Nomad, Packer, Consul, Vault, Boundary, and other HCL-using products MUST NOT
  inherit Terraform routing, owner namespaces, test semantics, observation
  families, or support rows from this spec.

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0406.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0406.md`.
- Register `.ripr/traceability.toml` behavior `RIPR-SPEC-0406`.
- Register `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0406.toml` as out of
  scope for runtime mutant corpora until Terraform judged corpora land (#5979).
- No adapter, parser, routing, feature gate, or `LanguageId` registration is
  introduced by this document alone.

## Problem

Terraform configuration combines module graphs, provider requirements, value
uncertainty, state/backend context, and native `.tftest.hcl` runs whose
`command = plan|apply` modes expose different observation surfaces. Generic HCL
syntax is shared with other products, but Terraform semantics, native tests, and
infrastructure claims are not interchangeable.

Epic #5603 needs a normative **preview contract** that separates project/module
substrate, block owners, native-test activation, observation witnesses, typed
limitations, and external edges before ADR selection (#5970), routing (#5972),
or adapters ship. Without it, passive analysis could treat references as
observation, collapse plan and apply, infer provider behavior from names, or
promote Nomad/Packer HCL into Terraform support.

## Behavior

### Wire language, routing, and generic-HCL non-claim

The closed wire language for this campaign is exactly:

```text
terraform
```

There is no synthetic `hcl` wire code, combined generic-HCL owner namespace, or
shared semantic support row for all HashiCorp configuration products.

File routing maps sources **after** role exclusion (see substrate) and **only**
when Terraform project evidence is present under [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541):

| Extension | Wire language | Role hint |
| --- | --- | --- |
| `.tf` | `terraform` | module configuration (production, variable, provider, check, import, moved, etc.) |
| `.tftest.hcl` | `terraform` | native test file (`test`, `run`, `assert`, and related blocks) |

**Generic HCL non-claim (normative):** Files whose syntax is HCL but whose product
context is Nomad, Packer, Consul, Vault, Boundary, or another non-Terraform
product MUST NOT route to the `terraform` wire language or inherit Terraform
capability cells. Each such product requires its own wire identity, capability
rows, and promotion path (#5603 child 12). Co-location with `.tf` modules,
shared `.hcl` extension on non-Terraform paths, or HashiCorp vendor naming alone
is insufficient for Terraform routing.

**Terraform project evidence** (static, non-executing) includes any of:

- a root module with `terraform`/`provider`/`resource`/`module` blocks in `.tf`;
- `terraform` block with `required_providers` or backend/workspace declarations;
- native test files referencing a module under test;
- explicit configuration selecting a Terraform root (per [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541)).

Without project evidence, `.tf` and `.tftest.hcl` paths remain unowned or
`generic_hcl_limited` — never silently treated as Terraform.

### Source and module roles (substrate)

Root and child modules, tests, fixtures, examples, generated, vendor, cache, and
state-sensitive paths carry explicit `source_role` values:

| Role | Typical paths / signals | Semantic adapter |
| --- | --- | --- |
| `root_module` | directory with module entry `.tf` | yes (when selected) |
| `child_module` | `module` call with local `./` or `../` source | yes |
| `native_test` | `.tftest.hcl` | test contract only |
| `fixture` | test-support `.tf` not deployed | bounded |
| `example` | documented sample | advisory only |
| `generated` | `.terraform/`, provider codegen markers | excluded from repair targets |
| `vendor` | vendored modules | read-only substrate |
| `cache` | `.terraform` plugin/module cache | excluded |
| `state_sensitive` | `terraform.tfstate*`, backend config secrets | no state read during passive analysis |

Remote module sources (registry URLs, git refs) are recorded as **declarations**
with `remote_module_unresolved` when content is not locally present. Passive
analysis MUST NOT fetch modules, run `init`, or install providers.

### Four machine-readable contracts

Implementations may store slices in one envelope, but validation and promotion
MUST keep them independent:

| Contract | `schema_version` | Owns |
| --- | --- | --- |
| Terraform project/module substrate | `terraform_project_substrate.v1` | Module graph, source roles, declared providers/versions, backend/workspace declarations, selection basis |
| Terraform block semantic evidence | `terraform_block_evidence.v1` | Changed owners, stable addresses, probes, typed limitations |
| Terraform native test evidence | `terraform_native_test_evidence.v1` | Test file registration, run activation, plan/apply mode, overrides/mocks, expect_failures |
| Terraform observation witnesses | `terraform_observation_witness.v1` | Distinct observation families and oracle bindings (reuse RIPR-SPEC-0026 witness shapes) |

Each document requires `validate()` after deserialization, uses
`deny_unknown_fields`, and records explicit `currentness` for facts that can go
stale (module path, provider version pins, backend config digest, workspace name
declaration, import/move history markers).

Substrate facts MUST NOT appear inside block or test documents except by reference
(`substrate_ref` id + digest). Block addresses, graph edges, or provider names
MUST NOT alone establish observation, reach, or oracle eligibility.

#### Shared substrate (`terraform_project_substrate.v1`)

Portable identities and currentness cover, without executing Terraform,
OpenTofu, providers, backends, or tests:

- **Module graph**: stable `module_id` (repo-relative path digest + optional
  `module.call` chain); root vs child; `module` source strings (local vs remote).
- **Declared tool context**: `terraform_version_constraint`,
  `opentofu_version_constraint` when declared; `required_providers` entries with
  source and version constraints (static strings only).
- **Backend/workspace declarations**: backend type and config keys as declared;
  `workspace` name when static; no live state read.
- **Source roles**: every routed path tagged per table above.
- **Repository-selected root**: exact module root chosen for analysis with
  `selection_basis` (`explicit_config`, `single_root`, `nearest_terraform_block`,
  `ambiguous`).

**Substrate non-reach law:** A module call, provider requirement, backend
declaration, output name, or resource type string MUST NOT alone establish that a
native test observes a changed attribute, that a plan/apply would mutate
infrastructure, or that deployed state matches configuration.

#### Block semantic contract (`terraform_block_evidence.v1`)

Stable owner identities bind to **exact source spans** and **Terraform addresses**
(module path + block type + name + optional instance key with uncertainty):

- **Core blocks**: `resource`, `data`, `module`, `variable`, `local`, `output`.
- **Provider blocks**: `terraform.required_providers` entries;
  `provider` configuration blocks (including aliases).
- **Lifecycle and meta**: `lifecycle`, `depends_on`, `count`, `for_each`,
  `provider` meta-arguments — instance keys carry `instance_key_state`
  (`known`, `unknown_count`, `unknown_for_each`, `dynamic_block`, `absent`).
- **Checks and conditions**: `check` blocks; resource `precondition` /
  `postcondition`; `validation` on variables.
- **State migration**: `import` and `moved` blocks (history markers only).
- **Expressions**: attribute assignments and references as probes only where
  spans are exact; unresolved references emit typed limitations.

**Address law:** Addresses use Terraform's block-type/name conventions plus
module path prefix. When `count`/`for_each`/dynamic expansion is unresolved,
addresses MUST use placeholder instance keys and `instance_key_state` rather than
inventing concrete indices.

**Reference and dependency law:** Explicit reference edges (attribute traversal,
`depends_on`, module output references) are **relation facts** only. They MUST NOT
promote to observation, infection, or oracle credit without a separate activated
test witness under #5542 and a matching observation family below.

#### Native test contract (`terraform_native_test_evidence.v1`)

Native `.tftest.hcl` facts are modeled per **test file** and **run block**:

- **Registration**: `test` block identity; file path; linked `module` under test.
- **Activation** ([#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542)):
  registration, selection, `expect_failures`, disabled/skipped runs, and provider
  override/mock presence are distinct fields. Oracle eligibility is impossible when
  activation is `disabled`, `ignored`, `pending`, `unselected`, `unknown`, or
  `deferred`.
- **`command` mode**: each run records `command_mode` as `plan` or `apply`
  exactly as declared. Plan and apply MUST NOT collapse into one mode; available
  observation families differ by mode (see witnesses).
- **Run inputs**: static `variables`; provider `override` / mock blocks when
  declared; `module` source under test; `expect_failures` selectors.
- **Selector limits**: partial/static `expect_failures` targets carry
  `selector_resolution` (`exact`, `partial`, `unknown`).

Provider mocks and overrides affect **activation and oracle eligibility** through
#5542, not renderer heuristics or output formatting shortcuts.

#### Observation witnesses (`terraform_observation_witness.v1`)

Observation families are **distinct** from address/reference edges and from each
other. Witness records reuse shared language-adapter witness/output field shapes
from RIPR-SPEC-0026 (`witness_kind`, `observed_subject`, `source_span`,
`strength`, `limitation_refs`).

Initial families (non-exhaustive; extend only via spec amendment):

| Family | Typical static witness | Plan vs apply |
| --- | --- | --- |
| `output_value` | expected output comparisons in asserts | both; apply may add side-effect context limits |
| `resource_attribute` | `attr`/`resource` checks in asserts | both |
| `collection_cardinality` | `length`, splat, or set comparisons | both |
| `planned_change` | plan-only change summaries when declared | plan primarily |
| `diagnostic` | expected error/warning text | both |
| `check_condition` | check/pre/postcondition outcomes | plan/apply per run mode |
| `state_transition` | bounded state assertions when statically declared | apply-biased; requires explicit limits |

Each oracle cites the **exact assert/matcher span** and the **activated run
identity** (including `command_mode`). Witnesses MUST NOT claim runtime provider
behavior, deployed infrastructure correctness, drift remediation, cost, security,
or policy outcomes.

### Value, provider, state, and plan/apply limitations

Typed stops (extend only via spec amendment):

| Limitation kind | When |
| --- | --- |
| `tf_value_unknown` | Unknown or deferred expression value |
| `tf_value_computed` | Computed attribute or provider-computed field |
| `tf_value_sensitive` | Sensitive marking or redacted value |
| `tf_provider_schema_unresolved` | Provider type/version without schema facts |
| `tf_provider_behavior_unresolved` | Provider logic, CRUD semantics, or plugin behavior |
| `tf_remote_module_unresolved` | Module source not locally present |
| `tf_dynamic_block_unresolved` | Dynamic block expansion unknown |
| `tf_instance_expansion_unresolved` | `count`/`for_each`/set/list expansion unknown |
| `tf_lifecycle_meta_unresolved` | replace_triggered_by, ignore_changes, etc. not evidenced |
| `tf_provisioner_unresolved` | local-exec/remote-exec/file provisioners |
| `tf_external_data_unresolved` | external data source or out-of-repo command |
| `tf_function_effect_unresolved` | env/time/network-sensitive functions |
| `tf_backend_state_unresolved` | backend/workspace/state content unavailable |
| `tf_import_move_history_unresolved` | import/move linkage incomplete |
| `tf_plan_apply_divergence` | plan observations cannot transfer to apply or reverse |
| `tf_version_unresolved` | Terraform/OpenTofu/provider/module version not fixed |
| `tf_test_activation_unknown` | Registered run with unknown activation |
| `tf_generic_hcl_product` | Non-Terraform HCL product context |

Unresolved provider schema, state, backend, remote module, or version context
MUST NOT become a universal exposure claim across modules or tests.

### Cross-language and external edges

Provider plugins, external commands, cloud APIs, and application runtimes reachable
only through provider behavior MUST register cross-language or external edges under
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) and
[#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559) with explicit
provenance. Provider name strings, `data.external` declarations, or provisioner
blocks alone do not establish a validated edge.

Same-language Terraform module-to-module references use substrate linkage plus
evidenced reference edges and test activation under #5542; they do not use
cross-language registry edges.

### Capability cell registry (normative preview index)

Capability cells use identity shape
`{language}/{framework}/{mode}/{family}` per RIPR-SPEC-0365. The table registers
initial Terraform and explicit generic-HCL non-claim cells for
docs/traceability/manifest joins; runtime registry entries land in later
implementation slices (#5972–#5979).

| Cell id | Support state (initial) | Tier ceiling | Notes |
| --- | --- | --- | --- |
| `terraform/module/diff/static_facts` | `not_proven` | `preview` | block/module owners |
| `terraform/native_test/diff/static_facts` | `not_proven` | `preview` | `.tftest.hcl` registration |
| `terraform/plan/diff/static_facts` | `not_proven` | `preview` | plan-mode runs |
| `terraform/apply/diff/static_facts` | `not_proven` | `preview` | apply-mode runs |
| `terraform/provider_mock/diff/static_facts` | `not_proven` | `preview` | overrides/mocks |
| `terraform/observation/diff/static_facts` | `not_proven` | `preview` | witness families |
| `terraform/any/repo/static_facts` | `not_proven` | `preview` | repo mode substrate |
| `terraform/any/diff/repair_routing` | `not_proven` | `preview` | repair routing only |
| `generic_hcl/nomad/diff/static_facts` | `not_supported` | `preview` | explicit non-claim |
| `generic_hcl/packer/diff/static_facts` | `not_supported` | `preview` | explicit non-claim |
| `generic_hcl/consul/diff/static_facts` | `not_supported` | `preview` | explicit non-claim |
| `generic_hcl/vault/diff/static_facts` | `not_supported` | `preview` | explicit non-claim |
| `generic_hcl/boundary/diff/static_facts` | `not_supported` | `preview` | explicit non-claim |

Product manifest claims for these ids are added only when
`metrics/capabilities.toml` entries explicitly cite implementation proof; this
spec registers the ids for index completeness only.

Initial posture: preview/advisory per cell; no default feature enablement; no
gate, badge, baseline, RIPR Zero, release, or publication authority from this
spec alone.

## Required Evidence

- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-artifacts`,
  `check-doc-index`, `check-traceability`, `check-static-language`, and
  `check-capabilities` pass with this spec and its index registrations.
- Acceptance examples in this document are keyed for future Terraform judged
  corpora (#5979) as `RIPR-SPEC-0406#<slug>`.
- Implementation slices (#5970–#5978) must add executable proof; this document
  alone does not satisfy runtime or adapter evidence.

## Non-Goals

- No Terraform/OpenTofu/provider/plugin/backend/plan/apply/native-test execution
  during passive analysis.
- No remote module fetch, state read/write, cloud API calls, or provisioner runs.
- No universal HCL or infrastructure-behavior reconstruction.
- No adapter code, routing, feature gates, default enablement, or support-tier
  promotion.
- No generated tests/edits, registry publication, credentials, or release action.
- No claim that OpenTofu parity or generic HCL support follows from Terraform
  preview law.

## Acceptance Examples

1. **routing-terraform-root**: A directory with `main.tf` containing `terraform {
   required_providers { … } }` routes `.tf` to `terraform` and tags `root_module`.
2. **routing-tftest**: `tests/main.tftest.hcl` routes to `terraform` with
   `source_role = native_test` when a module under test is declared.
3. **generic-hcl-nomad-non-claim**: A `job.nomad.hcl` without Terraform project
   evidence MUST NOT populate `terraform/*` cells; `generic_hcl/nomad/*` stays
   `not_supported`.
4. **negative-reference-not-observation**: A `resource "aws_s3_bucket" "x"` change
   with only a `depends_on` edge to another resource MUST NOT credit a native test
   that merely mentions the bucket name without an activated assert witness.
5. **plan-apply-separation**: Two runs differing only by `command = plan` vs
   `apply` produce distinct `command_mode` values and may differ in available
   observation families; observations do not transfer under
   `tf_plan_apply_divergence`.
6. **provider-mock-activation**: A run with `override` blocks requires explicit
   activation fields; disabled mock context yields no oracle credit.
7. **expect-failures-selector**: `expect_failures` with partial selectors sets
   `selector_resolution = partial` and refuses promotion to exact oracle credit.
8. **unknown-value-limit**: An attribute referencing a computed value names
   `tf_value_computed` rather than inferring a concrete witness.
9. **sensitive-limit**: Marked sensitive values name `tf_value_sensitive` and
   withhold attribute witnesses.
10. **count-for-each-uncertainty**: `count = var.n` without static `n` uses
    `instance_key_state = unknown_count` and refuses concrete instance addresses.
11. **dynamic-block-limit**: Dynamic nested blocks name
    `tf_dynamic_block_unresolved` until expansion is evidenced.
12. **remote-module-limit**: `module "vpc" { source = "terraform-aws-modules/vpc/aws"
    … }` without local module content names `tf_remote_module_unresolved`.
13. **backend-state-limit**: Backend configuration present without passive state
    access names `tf_backend_state_unresolved`; no deployed-state claims.
14. **external-edge-positive**: A registered #5559 edge with provenance links
    Terraform `data.external` to an application subject; without the edge, no
    cross-language credit.
15. **external-edge-negative**: Provider name `aws` alone does not establish a
    validated edge or provider behavior witness.
16. **shared-witness-shape**: Observation records use RIPR-SPEC-0026 witness fields
    and remain separate from activation registration rows.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0406`. Executable tests land
with analysis slices (#5972–#5978) and judged corpora (#5979). Until then,
`fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0406.toml` marks the spec out
of runtime mutant corpora.

## Implementation Mapping

- HCL/Terraform substrate ADR: #5970
- Identity, routing, module/test project facts: #5972
- Block/address/expression probes: #5973
- Native test run activation and plan/apply: #5974
- Observation/discriminator model: #5975
- Typed limits (value/provider/state/dynamic): #5976
- Repo mode completeness: #5977
- Output/editor/agent parity: #5978
- Corpus and support evaluation: #5979
- Support adjudication packet: #5980

## Metrics

- `terraform_evidence_contract_registered`
- `terraform_capability_cell_index_complete`
- `terraform_generic_hcl_non_claim_registered`

## Failure Modes

- Treating reference/dependency edges as observation or oracle eligibility.
- Collapsing plan and apply runs or their witness families.
- Promoting provider names, module sources, or addresses to runtime behavior.
- Routing Nomad/Packer/Consul/Vault/Boundary HCL into Terraform cells.
- Promoting preview Terraform findings to gate or badge eligibility without
  promotion policy and proof (#5972–#5979).
