# RIPR-SPEC-0376: One shared resolver decides a language's availability state, and `ripr doctor --json` publishes it

Status: proposed

Owner: product / analysis

Created: 2026-11-01

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0021](../adr/0021-static-and-executed-assurance-axes.md)

Linked plan:

- #2615

Linked issues:

- #2961
- #12130

Linked PRs:

- #11675 (resolver and DTO)
- #11834 (config and check consumers)
- #12016 (doctor `detected_languages[]` fields)

Support-tier impact:

- No tier change. Availability never promotes a support tier. See
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `.ripr/traceability.d/`,
  `docs/spec-index.d/` and the four verdict corpora's `spec-scopes/`.
- No new output field: this spec records the contract of fields and a resolver
  that already exist.

## Problem

Whether a language is analyzed depends on three independent facts: the project
contains it, the binary was built with its adapter, and the configuration (or
auto-detection) enables it. Before the shared resolver each surface re-derived
that answer from `is_available()` alone, so a disabled, uncompiled or merely
auto-enabled language could read differently in `check`, config validation and
`doctor`. The resolver
(`crates/ripr/src/domain/language_availability.rs`) now owns the decision, and
`ripr doctor --json` is its first public projection. Neither had a spec, so the
policy, the state vocabulary and the non-claims were pinned only by unit tests.

## Behavior

`resolve_language_availability` takes observed facts (project detected, adapter
compiled, configured intent, auto-detection policy, optional scope and adapter
results) and adds none of its own.

Policy:

1. An explicit disable always wins. A disabled language is never effectively
   enabled, whatever is detected or compiled.
2. An explicit enable needs the compiled adapter. Without it the state is
   `configured_enabled_but_unavailable`, never silent.
3. With no configured intent, a language is enabled only under the
   `when_detected` policy, with the project detected and the adapter compiled.
   Under the `never` policy detection is reported and enables nothing.
4. Effective enablement never implies the adapter ran.

State vocabulary (closed; the schema enum is these nine plus `null`):

| State | Meaning |
| --- | --- |
| `detected_compiled_enabled` | Detected, compiled, effectively enabled |
| `detected_compiled_disabled` | Detected and compiled, explicitly disabled |
| `detected_not_compiled` | Detected, adapter not in this binary |
| `configured_enabled_not_detected` | Enabled and compiled, nothing detected |
| `configured_enabled_but_unavailable` | Enabled in configuration, adapter not compiled |
| `auto_detected_enabled` | Enabled by auto-detection, not by configuration |
| `supported_but_no_current_scope` | Enabled, no relevant files in the diff scope |
| `adapter_ran_with_limitations` | Adapter ran and disclosed limitations |
| `not_enabled` | Not enabled and nothing to analyze; never a clean result |

Each report carries ordered `remediation` steps, complete for the state and
empty when none are needed, and `non_claims`: availability does not mean the
adapter ran, and availability does not promote the support tier. Those two are
always present. A third, that a disabled or unavailable language is not
analyzed so silence is not a clean result, is added only when the language is
not effectively enabled.

`ripr doctor --json` `detected_languages[]` entries carry `availability_state`
and `remediation` from this resolver:

- With no `ripr.toml` loaded, Python or TypeScript enabled by marker detection
  resolve as `auto_detected_enabled` (unspecified intent, `when_detected`
  policy), not as explicit configuration.
- When the configuration could not be loaded, enablement is unknown: the
  resolver has no unknown state, so `availability_state` is `null` and
  `remediation` is empty rather than a definite `not_enabled` plus an enable
  step.
- Both fields are declared in `schemas/ripr/doctor.schema.json` and optional
  within schema 0.4, so documents emitted before they existed still validate;
  the current producer always emits them.

## Required Evidence

- Resolver unit tests cover each state, explicit-disable precedence, the
  uncompiled enable refusal, auto-detection visibility, independent states in
  a mixed workspace, and that available states carry no remediation.
- `doctor_json_conforms_to_the_published_schema` validates live
  `ripr doctor --json` bytes against the schema and asserts every
  `detected_languages[]` entry carries both fields.
- A doctor test asserts the no-`ripr.toml` Python case reads as
  `auto_detected_enabled` and an explicit configuration does not.

## Known Limits

- Doctor is the only surface that sets the `when_detected` policy. Config
  validation (`config.rs`) and the check/diff advisory path
  (`analysis/mod.rs`) resolve with the default `never` policy, so
  `auto_detected_enabled` is reachable only from doctor today. This is a known
  follow-up, not an assumption that the surfaces agree.
- Remediation order follows the resolver: the language's enable prerequisite
  (today only Perl's fact-packet step) is appended after the state's own steps,
  and only when the adapter is compiled and the state is
  `detected_compiled_disabled`, `detected_compiled_enabled`,
  `auto_detected_enabled` or `configured_enabled_not_detected`. The
  not-compiled states and `not_enabled` get no prerequisite step.
- The human doctor screen does not print the state.

## Non-Goals

- No claim that an adapter ran, that analysis succeeded, or that the support
  tier moved.
- No new state, unknown state, or change to enablement policy.
- No runtime or mutation truth.

## Acceptance Examples

1. Python project, no `ripr.toml`, Python adapter compiled:
   `auto_detected_enabled`.
2. Same project with `ripr.toml` listing `python`:
   `detected_compiled_enabled`.
3. `ripr.toml` disabling a detected, compiled language:
   `detected_compiled_disabled`, with a `[languages]` enable step.
4. Detected language whose adapter is not compiled: `detected_not_compiled`,
   remediation names the build route and never enables it.
5. Configuration that failed to load: `availability_state` is `null`,
   `remediation` is empty.

## Test Mapping

See `.ripr/traceability.d/language-availability-0376.toml`.

## Implementation Mapping

- `crates/ripr/src/domain/language_availability.rs`: resolver, states,
  remediation, non-claims.
- `crates/ripr/src/cli/commands/doctor.rs`: `detected_language_entries`.
- `crates/ripr/src/output/doctor.rs`: `DoctorDetectedLanguage` fields.
- `schemas/ripr/doctor.schema.json` and `docs/OUTPUT_SCHEMA.md`: published
  shape.

## Metrics

- `unit_test_pass_rate`.
