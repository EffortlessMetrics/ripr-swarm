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
`expected_sha` equals the actual dispatch SHA and
`EM_CI_GITHUB_HOSTED_ENABLED` variable is exactly `false`; this condition runs
before reusable-job allocation or any Cargo bootstrap. Absent observation or
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
from that workflow rather than copied into a second command portfolio. The
current-main reconciliation brings #7918's landed `cargo xtask ci-policy`
composition into this source; the previous `d51471b24...` checkpoint remains in
the merge ancestry. The recorded integration basis is
`0598981b9ef1ae9b5eb79a25b58e732126a6cadf`, not the earlier `92e7ea803...` basis.

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
The shim creates that local temp/report root before compiling the adapter; neither deletes shared caches.
Central runner supervision/decontamination remains the cleanup owner.

## Matching LLVM prerequisite on the current image

The inspected active image source
`EffortlessMetrics/em-ci@5ecf04b0f9f053f64713a7f203f1a23ab832510f`
installs Rust primary/compatibility toolchains with rustfmt and Clippy but not
`llvm-tools-preview`. The Linux repo-policy test
`coverage_diagnostics::tests::real_llvm_malformed_profile_under_unicode_root_is_retained`
requires `llvm-profdata` under the **selected compiler's sysroot**, so another
LLVM executable on PATH or an ignored test does not satisfy it.

After exact subject/checkout/target validation, the adapter requires the central
`RUSTUP_TOOLCHAIN` to equal the repository's full numeric toolchain pin. It runs
`rustup component add llvm-tools-preview --toolchain <exact-version>` in the
existing writable one-job image, verifies the compiler release and Linux x64
host, then executes that sysroot's `llvm-profdata --version`. Setup time and
success/refusal are logged separately. Installation/probe failure stops before
the product commands; the mandatory test remains in normal workspace nextest.
No image activation, shared-host toolchain mutation or test skip is performed.
The ephemeral tool download/install participates in the same 75-minute job and
must be included in #7164's cold/warm time and scratch-growth observations.

This source setup is not proof that the active fleet completed it. Rehearsal is
still blocked on complete execution admission and output custody below.

## Actual RIPR product outputs and transport dependency

The active `a3125de...` reusable workflow retains its central result envelope for
seven days; it does not upload the product packet. A green envelope alone cannot
establish inspectable Rust-test or RIPR evidence custody. This consumer needs:

| Actual output | Proposition and producing owner |
| --- | --- |
| `target/nextest/ci/junit.xml`, `run-context.txt` | Fresh nonempty CI nextest execution, actual checkout/run/attempt, exit and tool/config identities; Rust Gates' Required Rust tests block |
| `target/ripr/pr/check.json` | Native diff analysis; not a repository-wide debt census |
| `target/ripr/pr/repo-exposure.json`, `.md`, `canonical-delta.json`, `pr.diff` | Exact base/head diff packet and available causal-comparison observations; PR evidence owner |
| `target/ripr/review/comments.json`, `.md`, `run-receipt.json`, `annotations.txt` | Guidance/annotations and their actual subject; review-comments owner |
| `target/xtask/impacted-evidence/latest.json`, `.md` | Selected changed surfaces and disclosed routing evidence; impacted-evidence owner |
| `target/ripr/reports/policy-preflight.json`, `ci-policy.json`, `.md` and applicable policy/fixture reports | Actual source/toolchain-bound policy and selected fixture outcomes; existing report owners |
| Native adapter logs plus central job/result context | Command-owner blob, exact subject/tree/base, original step exits/timing, setup refusal and actual workflow/run/attempt |

These current outputs must be retained together under the existing central
upload/import authority, with exact source/run/attempt/command/result binding,
fresh regular-file and byte/file bounds, successful selected producer/import,
and an explicit retention/inspection window covering qualification through
activation readback. No stale prior report or alias of another proposition may
satisfy a missing member. Failed/cancelled attempts must remain discriminating.

Existing `em-ci#188` / public `em-ci-workflows#18` transport at
`15f2c4d1ff4a5ce5174645ba021454e5037fef1f` is published but not activated.
Its ten mandatory files include Perl's canonical RIPR+ and quality-gate packet,
tool identity and cgroup receipts. RIPR swarm's selected product portfolio does
not produce that same canonical analysis. Its PR packet is explicitly
`kind=pr_evidence`, `scope=diff`; it cannot be relabeled as a repository-wide
summary or fabricated gap ledger. No synthetic RIPR+ zero, empty quality pass,
or filename substitution is added merely to satisfy the importer.

Therefore this is an **explicit central dependency**, not a compatible upload
switch. The existing central transport owner must supply and review a contract
for the actual RIPR product members/propositions above, with domain result
validation that neither rebuilds the heavy portfolio nor promotes missing
analysis. Only then can the exact compatible public pin, required read scopes,
workflow allowance and native upload/import result be considered for controlled
admission. `upload_artifacts=true`, `artifact_lane=ripr` and `result_script` on
`15f2c4d...` alone do not repair that incompatibility. The active caller remains
on `a3125de...`, its read permissions unchanged. No duplicate uploader, artifact
package, image activation or new-pin allowance is implemented in this source.

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
Do not execute the current envelope-only path as complete #7164 qualification:
the compatible central product-output custody dependency must be resolved first.

After source registration and full admission, resolve the existing reviewed
`BRANCH` to exact `SUBJECT` and retain its reviewed `BASE`. GitHub's
[dispatch ref contract](https://docs.github.com/en/rest/actions/workflows#create-a-workflow-dispatch-event)
accepts a branch or tag name. Re-read branch custody immediately before dispatch;
the adapter rejects a moved event SHA before product command execution. Then use:

```bash
gh workflow run contabo-qualification.yml --repo EffortlessMetrics/ripr-swarm \
  --ref "$BRANCH" -f expected_sha="$SUBJECT" -f base_sha="$BASE"
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
The minimal Bash shim is the required central entrypoint; Rust owns the adapter.
Revert this staged caller, adapter, shim and scoped policy/doc additions to remove
rehearsal without changing the existing required producer or live settings.
