# RIPR-SPEC-0439: Manual security comparison subjects

Status: proposed

Owner: test-infra

Linked issues: #4987 (security portfolio), #9067 (Dependabot acceptance).

## Problem

Retiring auxiliary PR subscriptions removes automatic candidate-specific
dependency security checks. A scheduled scan of main cannot establish the
security result of a different candidate. Dependency Review also requires an
explicit base/head pair when invoked outside a PR event.

## Behavior

The existing security workflow accepts optional `base_sha` and `head_sha`
manual inputs. A candidate comparison requires both inputs as distinct,
lowercase, full 40-character commit IDs. Partial pairs, mutable refs, equal
subjects and malformed input fail before security consumers run.

For that explicit invocation, cargo-deny checks out `head_sha`; Dependency
Review compares `base_sha` to the same head. Both jobs verify their actual
checkout before running their consumer, keep failures visible, and record the
selected subject. Existing PR Dependency Review uses the PR event's own pair.

With neither manual input, cargo-deny retains the immutable workflow-event
source and Dependency Review is not selected. Scheduled scanning and the
schedule-only deny-red tracker retain their separate purpose. The capability
adds no automatic event or required-check membership and remains usable after
auxiliary PR/push event retirement.

The retained policy is cargo-deny's advisories/licenses/bans/sources checks
under `deny.toml`, and the pinned Dependency Review action's default `runtime`
scope, high-severity threshold and existing denied-license list. This change
does not expand Dependency Review to development dependencies. Provider
snapshot warnings remain visible limitations, not proof of complete coverage.

A manual Actions run keeps its workflow ref's native head identity. Candidate
evidence uses the verified selected SHA pair, workflow revision, both consumer
results and run URL without relabeling that native identity.

## Required Evidence

- Literal selector controls distinguish a candidate head from the workflow-run
  head and preserve no-input manual/scheduled subjects.
- Partial, mutable, equal and injected inputs are refused without publishing
  a selected subject; PR context and manual inputs remain distinct.
- Both actual checkout verifiers refuse a mismatch, and removal controls expose
  the false receipt when verification is bypassed.
- Consumer-binding controls reject lost input bindings, main instead of the
  selected checkout, missing comparison refs and weakened security commands.
- Source/control evidence does not claim native crate integration or a live
  cargo-deny/Dependency Review result. Operated candidate evidence must retain
  the actual native outcomes, subject identities and provider limitations.

## Non-Goals

- Queue qualification, protected result publication or branch-setting changes.
- A reusable caller profile, new automatic subscribers or full queue cutover.
- Broader security policy, provider snapshot repair or advisory suppression.

## Acceptance Examples

1. Manual base A/head B, with workflow source C: both jobs consume B; Dependency
   Review compares A to B. A successful scan of C cannot stand in for that work.
2. Only head B, base `main`, equal A/A or a newline-bearing value: refuse.
3. No inputs: cargo-deny scans the event SHA and no dependency comparison runs.
4. Checkout returns C after selecting B: fail before the security consumer.
5. A provider failure or incomplete snapshot never becomes a claim that the
   candidate meets #9067 merely because an explicit invocation was selected.

## Test Mapping

`tools/repo-policy/tests/security_subject.rs`:

- `cargo_scan_binds_candidate_and_preserves_unselected_event_subjects`
- `manual_security_refuses_partial_mutable_equal_and_injected_subjects`
- `dependency_comparison_uses_manual_pair_or_actual_pr_context`
- `both_consumers_refuse_the_wrong_checkout`
- `selected_subjects_reach_checkout_and_the_real_security_consumers`

## Implementation Mapping

- `.github/workflows/security.yml`: manual inputs, selectors, checkout guards
  and existing security consumers.
- `docs/CI.md`: invocation, retained policy and evidence limits.

## Metrics

- `unit_test_pass_rate`: all mapped subject and consumer controls pass.
- No runtime, cost or full-portfolio security improvement is claimed by source
  publication alone.
