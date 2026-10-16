# Contabo consumer source and qualification handoff

Source owner: #4988. Bounded workload/pool qualification: #7164.
Admission and integration adapters remain #4986 / #4836. Settings activation
remains #4838; service-created merge-group proof remains #4840.

## Staged consumer

`contabo-qualification.yml` has only an explicit manual dispatch. It creates
**Contabo Consumer Qualification**, a non-required rehearsal, and calls the public
`EffortlessMetrics/em-ci-workflows/.github/workflows/rust.yml` at
`8ff77e5876b4a7808a58e053aa6248b1020e02d0`, with `profile: standard`, full history,
and `force_hosted: false`. The caller refuses allocation unless the effective
`expected_sha` equals the actual dispatch SHA and
`EM_CI_GITHUB_HOSTED_ENABLED` variable is exactly `false`; this condition runs
before reusable-job allocation or any Cargo bootstrap. Absent observation or
unknown policy cannot authorize recovery. No inventory polling, physical-host
selection, inherited secrets, or caller-owned scratch cleanup occurs here.

The caller now prepares `artifact_lane: ripr-product`, `upload_artifacts: true`,
`.ci/contabo-product.sh` and `.ci/contabo-result.sh` together. Central source
`em-ci#236` merged at `8334119f938f545df95158bd157f9da318beb017`; generated
public `em-ci-workflows#23` merged at the immutable pin above. This is source
preparation, not live runner admission or a qualified product result.

**Execution blockers:** the caller retains `actions: read` and `contents: read`.
The reusable workflow additionally declares `checks: read` and
`pull-requests: read`; explicit approval of these two read-only scope additions
is pending. They are deliberately recorded as comments, not granted by this
candidate. The actual new-pin live runner allowance must also be observed after
its separate pending approval. Do not dispatch until both conditions, default
workflow registration and reviewed #7164 admission hold. No permissions, runner
settings, secrets, protection or image activation are changed here.

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
`ab3daa37c93d604092f24b483aba294c8c6c0d96`. The earlier `7c24ed92e...` and
`0598981b9...` bases and their proof remain historical epochs; current main merged
cleanly and preserved its `ci-policy` composition. Landed #7968 repairs both
scalar projection fallback tuples to `(call, expected, None)`, resolving the two
E0308 errors reproduced on exact `7c24ed92e...` main without manufacturing direct
adaptor call text. Existing run `38606464827`'s **Ripr Rust Small Result** job
`116023617352` reads back successful; that legacy required result does not qualify
this governed consumer or a Contabo slot.

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

## Actual RIPR product outputs and prepared transport

The earlier `a3125de...` source lacked product transport. The prepared public
pin now supplies bounded no-follow packaging, sanitization, native producer and
source/run/attempt authentication, mandatory import and blocking fourteen-day
product custody. The separate central result envelope remains seven days.
A green envelope alone cannot establish product custody. The real portfolio is:

| Actual output | Proposition and producing owner |
| --- | --- |
| `target/nextest/ci/junit.xml`, `run-context.txt` | Fresh nonempty CI nextest execution, actual checkout/run/attempt, exit and tool/config identities; Rust Gates' Required Rust tests block |
| `target/ripr/pr/check.json` | Native diff analysis; not a repository-wide debt census |
| `target/ripr/pr/repo-exposure.json`, `.md`, `canonical-delta.json`, `pr.diff` | Exact base/head diff packet and available causal-comparison observations; PR evidence owner |
| `target/ripr/review/comments.json`, `.md`, `run-receipt.json`, `annotations.txt` | Guidance/annotations and their actual subject; annotations belong under `review/`, and may truthfully be empty |
| `target/xtask/impacted-evidence/latest.json`, `.md` | Selected changed surfaces and disclosed routing evidence; impacted-evidence owner |
| `target/ripr/reports/policy-preflight.json`, `ci-policy.json`, `.md` and applicable policy/fixture reports | Actual source/toolchain-bound policy and selected fixture outcomes; existing report owners |
| Native adapter logs plus central job/result context | Command-owner blob, exact subject/tree/base, original step exits/timing, setup refusal and actual workflow/run/attempt |

The machine-readable proposed contract is
[`.ci/contabo-product-contract.json`](../../.ci/contabo-product-contract.json),
schema `ripr-swarm-product-members/v1`. It declares **19 exact native core
members plus the adapter's terminal `target/ripr/contabo/commands.json`** on
complete product success. It also names retain-if-produced policy/golden/fixture
reports, source-derived verdict-corpus membership, and the two existing
producer-owned test/calibration retention sets. Central must resolve their
bounded path selections from the exact owner blob, not collect an entire target
or cache tree. The contract proposes fourteen-day retention and limits of 512
regular members / 8 GiB expanded including context, with 128 MiB per member.
These transport bounds are not measured product/storage fit or a reservation.

The adapter's fresh exclusive terminal receipt records actual entered commands,
SHA256 of their Bash source, original exits/durations, all unentered steps as
`not_executed`, source/base/tree/owner/contract blobs and native run/attempt.
It performs no upload/import. Receipt publication failure after a product failure
preserves the original product exit; after product success it refuses completion.
It never overwrites a previous attempt. Setup refusal, cancellation or timeout
may leave no terminal receipt; central pre-proof/failure diagnostics and native
job status must retain that absence, never invent a completed command portfolio.

Empty `annotations.txt` and `pr.diff` can be legitimate. The nextest producer
still enforces fresh JUnit with at least one executed test. `canonical-delta.json`
may disclose unavailable comparison snapshots. The three PR summary files include
`target/ripr/reports/pr-evidence-summary.{json,md}`; that v1 pair reads optional
report inputs distinct from `pr/repo-exposure.json` and may honestly report
unavailable analysis. File presence is not an established causal comparison,
repository census or analysis completeness. Do not fabricate the optional inputs.

On partial product failure retain only fresh declared files actually produced;
later members are expected to be absent and cannot turn the native failure green.
Central packaging must reject links/escapes/stale files and bind member digests,
command receipt, current run/attempt, actual native producer and defining pin.
Before a consumer result importer treats this as complete, it must authenticate
the native successful producer, all 17 rows and mandatory membership without
rebuilding the portfolio or converting transport success into domain evidence.
This RIPR-specific contract is distinct from both Perl's ten-file canonical packet
and `em-ci#229`'s unsafe-review nonfatal diagnostic packet. Central implementation and generated publication are now merged. Native
upload/import execution, actual admission and product fit remain unproved.

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

The prepared caller selects the distinct `ripr-product` lane rather than the
incompatible older `ripr` lane. The immutable central transport recognizes the
unchanged member-contract blob `bf03be85b64fab0927ddb9f8d61083e39f33972d`, adapter
blob `351b83e04c28359115eb4b60e1fce3e8614bcfb8` and source-derived corpus mapping.
These exact code/contract bytes must remain unchanged unless the central owner
reviews and publishes a corresponding transport update. New consumer tests live
in `xtask/tests/contabo_result_policy.rs`, outside that pinned adapter blob.

The lightweight result policy consumes only the centrally authenticated import
at `EM_CI_ARTIFACT_DIR`. It binds the selected successful owned standard result
and imported product verdict to the current repository, subject/tree/base,
run/attempt and defining public pin. Central import remains the authority for
native producer, command rows, file membership and digests; the policy does not
reimplement authentication, compile the product or fabricate missing analysis.
The complete reusable call, including packaging, import, policy and final result,
must succeed; its selected proof envelope alone is insufficient.

The source compatibility preflight reads genuine immutable Git objects through
the exact decoded public transport: the existing contract resolves 19 core
members and all 17 command blocks, and its adapter/contract/source checks pass.
This is source compatibility, not a real product packet or runtime qualification.
The former retained-preflight receipt publication was cancelled by the user; it
remains excluded. Current source and sanitized new proof use only this authorized
repository candidate. No duplicate uploader, package, live allowance, image
activation or new authentication credential is supplied here.

The legacy `routed-rust.yml` / `rust-gates.yml` required producer remains available
as bounded compatibility pending coherent activation. Its Ready/main routing and
old inventory/fallback are historical live behavior, not the desired route.
This rehearsal does not claim to complete their retirement, admission, exact-G
aggregation, cache-writer continuity or required-context migration. #4986 / #4836 /
#7827 / #4838 must bind those changes before cutover. No protection changes occur.

## Coordinated routing and cleanup retirement

The source path below prepares retirement; it does not perform live activation,
cancel queued Scratch GC run `38606711391`, delete old-host state, relabel jobs or
withdraw runner registrations. Preserve current required producers until their
replacement coverage and rollback generation are accepted together.

| Current source owner | Replacement / release condition |
| --- | --- |
| `routed-rust.yml` hosted inventory, cx43/cpx42/cx53 selection and hosted scratch-tempfail recovery | #4986 supplies explicit reviewed H admission; #4836 binds actual G/native result. The reviewed central standard caller queues on qualified capability with no ordinary hosted chooser/fallback. No Ready/push/synchronize qualification. Remove inventory/token references only after rederiving genuine consumers; do not remove a credential live. |
| `rust-gates.yml` shared command and native artifact owners | Preserve these 17 commands, policy-preflight/ci-policy composition, separate strict Clippy, fresh nonempty JUnit, doctests and real corpus/PR outcomes. Transfer actual retention through the distinct product contract above. Keep the command owner; provider migration does not fork or copy the product portfolio. |
| `Ripr Rust Small Result`, docs/no-heavy result and `pr-staleness-watchdog.yml` | #4836 binds selected required children/attempt/source and truthful no-heavy finalization; #4986 supplies review/request revocation. Update actual workflow/event/head lookups and policy fixtures together. Missing/cancelled/stale evidence cannot become pass. Independent control capacity remains a #7164 proof obligation. |
| `.github/actions/ci-scratch-lease`, inline legacy scratch and `scratch-gc.yml` | Remain exclusively legacy `/mnt/ci-scratch` lease owners and `/mnt/ci-cache/sccache` users. GC retains `em-ci-small` and its old label matrix; **never relabel it onto Contabo**. After no new legacy allocations, inventory exact in-flight jobs/leases and let the existing owner settle them. Only after verified ownership/coverage transfer may a reviewed source change remove the obsolete schedule/action/policy entries. No age-based or foreign-tree deletion. |
| `ci-lane-whitelist`, repo-policy routing/Ready/fallback checks, scratch behavior tests, command-count and watchdog fixtures, `docs/CI.md` | Reconcile with the same admitted generation and tested rollback. A literal label replacement cannot transfer admission, result, artifacts, cache-writer continuity or cleanup coverage. #7827 owns main-cache writer/reader continuity; #4838 owns controlled activation/readback. |

Source inspection of `em-ci@ab217deb99fb673103975a4fbea6b66f62d56a38`
`host/em-ci-slot` assigns container/cycle-directory/registration cleanup to the
central slot supervisor, outside candidate code. Their failures latch cleanup
state; recovery is checked before fresh admission/JIT registration. Shared Cargo
and sccache mounts are outside consumer cleanup. Qualify bound started/completed
hook receipts, post-cleanup container/cycle/registration absence, failure ->
offline -> no registration -> verified recovery, and sibling/shared-state safety.

The central cycle receipt is written **before cleanup** and cannot attest teardown.
Receipt write failure only warns, and some disk-size/pruning errors are not uniformly
latched. `worker_seen`/runner exit alone are telemetry. These are source observations,
not actual fleet proof or a claim that every filesystem failure forces offline state;
central owns any repair and the actual lifecycle receipts under #7164.

## Provider-neutral NixOS preparation

The caller selects the immutable central Rust workflow and `standard` contract;
it has no Contabo machine label, NixOS label, provider selector or host filesystem
cleanup. Keep this one consumer when future NixOS capacity is enrolled. Central
owns runtime, image/toolchain, trust, resource isolation and group/workflow admission;
new capacity must meet the same product/custody/lifecycle contract before eligibility.
`em-ci#152` describes a CI-disabled appliance foundation, not admitted Rust capacity.
No host setup, new credentials, runner registration or separate consumer workflow
is introduced. A new host/provider alone neither proves 75-minute fit nor supplies
independent control availability. Unsupported or unenrolled capacity stays unavailable,
without automatic hosted fallback or consumer-specific placement logic.

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
the actual new-pin allowance, read-scope approval, workflow registration and reviewed admission must be resolved first.

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
After the landed prerequisite was reconciled, actual
`cargo test --locked -p xtask --example contabo_product_proof` passed all eleven
tests (none failed/ignored, native exit 0) on merge checkpoint
`ba6636476bf96f906b563f8c083bda481cb2ddf1`. This supersedes the missing Cargo
dimension of the earlier direct-rustc seam proof; the original failed baseline
and repaired overwrite control remain separate epochs. The documentation-only
basis refresh leaves adapter/contract/caller/command-owner bytes unchanged.
Exact-head workspace results and source/review custody are retained in Draft
PR #7929; neither local compile proof nor the upstream required result supplies
central artifact transport, #7164 pool/lifecycle or #4840 native-G evidence.
The minimal Bash product shim is the required central entrypoint; Rust owns the adapter. The bounded Bash/jq result script avoids heavy bootstrap in the control job.
Revert this staged caller, adapter, shim and scoped policy/doc additions to remove
rehearsal without changing the existing required producer or live settings.
