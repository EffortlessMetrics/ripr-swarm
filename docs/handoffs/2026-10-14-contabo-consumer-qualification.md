# Contabo consumer source and qualification handoff

Source owner: #4988. Bounded workload/pool qualification: #7164.
Admission and integration adapters remain #4986 / #4836. Settings activation
remains #4838; service-created merge-group proof remains #4840.

## Staged consumer

`contabo-qualification.yml` has only an explicit manual dispatch. It creates
**Contabo Consumer Qualification**, a non-required rehearsal, and calls the public
`EffortlessMetrics/em-ci-workflows/.github/workflows/rust.yml` at
`a3125de962a74e7ec2e127d41d7532f66c69334d`, with `profile: standard`, full history,
and `force_hosted: false`. The caller refuses allocation unless the effective
`EM_CI_GITHUB_HOSTED_ENABLED` variable is exactly `false`; absent observation or
unknown policy cannot authorize recovery. No inventory polling, physical-host
selection, inherited secrets, or caller-owned scratch cleanup occurs here.

The immutable standard job has a **75-minute** limit. The repository cannot
extend it. Its classifier and finalizer also use standard capacity; this source
therefore does **not** establish independent control availability under saturation.
The full canonical fleet audit is NOT_PROVEN while organization variable/package
observations are unavailable. Repository access in group 10 is only admission
configuration, not workload or fleet qualification.

The Rust adapter executes the selected actual blocks from `rust-gates.yml`:
formatting, policy, dependency preparation, evidence honesty, dependency/process/
network policy, workspace/all-target and all-feature compilation, strict Clippy,
nextest, doctests, goldens, fixtures, verdict corpora and integration-diff evidence.
The ten PR-evidence commands retain their producer/check distinction. It keeps
step command, exit and elapsed-time rows in native logs and stops on the first
failure. Changes to the shared policy preflight owner (including #7918) are consumed
from that workflow rather than copied into a second command portfolio.

For rehearsal, PR evidence evaluates the detached subject against its reviewed
actual ancestor base, with no fabricated PR payload or constituent identities.
It is combined-tree diff evidence only, not constituent review/admission proof.
The adapter refuses stale dispatcher SHA, wrong checkout, missing/invalid identity,
non-ancestor base, unsupported owner expressions or incomplete command selection.
It records exact tree, owner blob, run and attempt. GitHub native jobs and the
central result envelope remain the authority for success/cancellation/timeout.

`CARGO_TARGET_DIR` must equal `$RUNNER_TEMP/cargo-target`. Repository linker temp,
nextest JUnit store (`.config/nextest.toml`), and RIPR reports intentionally remain
workspace-relative under `target/`; these are not assumed to contain Cargo binaries.
The adapter creates that local temp/report root and does not delete shared caches.
Central runner supervision/decontamination remains the cleanup owner.

The legacy `routed-rust.yml` / `rust-gates.yml` required producer remains available
as bounded compatibility pending coherent activation. Its Ready/main routing and
old inventory/fallback are historical live behavior, not the desired route.
This rehearsal does not claim to complete their retirement, admission, exact-G
aggregation, cache-writer continuity or required-context migration. #4986 / #4836 /
#7827 / #4838 must bind those changes before cutover. No protection changes occur.

## Parent-owned execution admission

Do not dispatch from Draft, Ready, synchronize, an ordinary label or an automatic
retry. First substantively review the exact committed source and applicable local
proof. Parent then binds the execution generation and safe cold/warm/concurrent
resource envelope under #7164, including image/toolchain, actual permission reads,
slot count, reserved headroom, queue/control and refusal/failure/cancellation/
cleanup fixtures. A complete product success alone cannot close #7164.

The new dispatch workflow must first exist on the default branch for GitHub's
workflow-dispatch registration. This source PR stays Draft pending its own review;
parent coordinates the source landing and subsequent bounded execution. Do not
replace this with a Ready transition, speculative dispatch or settings activation.

After source registration and full admission, resolve and retain exact `SUBJECT`
and its reviewed `BASE`, then use:

```bash
gh workflow run contabo-qualification.yml --repo EffortlessMetrics/ripr-swarm \
  --ref "$SUBJECT" -f expected_sha="$SUBJECT" -f base_sha="$BASE"
```

Read back the native workflow/run/attempt, actual checkout/tree, selected command
rows and complete reusable result (including receipt artifact). Record queue delay
separately from execution and preserve every refused/failed/cancelled attempt.
No execution has been dispatched by this source change. Unsupported measurements
or inaccessible observations remain unknown. #4834/#4838 consume only a complete
bounded qualification packet; #4840 separately observes actual native G later.

## Validation and rollback

Focused adapter tests exercise exact dispatch identity, the current command owner,
missing-proof refusal and real-shell failure propagation. Source checks, local
fixtures and independent review do not establish a real owned workload result.
The four-line Bash shim is the required central entrypoint; Rust owns the adapter.
Revert this staged caller, adapter, shim and scoped policy/doc additions to remove
rehearsal without changing the existing required producer or live settings.
