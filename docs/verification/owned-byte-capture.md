# Owned byte capture for local verification

`xtask::run::capture_bytes_in_dir_with_env_budget_observed` records one actual
local child invocation through the existing `OwnedProcess` implementation. Its
result is a private, non-deserializable `ObservedByteCapture`. Retained files,
JSON declarations, a fabricated `TimedBytesOutput`, and matching hashes cannot
construct that result. A consuming callback gives the producer the actual
request and observations once; anything the producer serializes afterward is
ordinary replay data.

This is a process observation boundary in the Kani implementation programme
(#14472). It does not establish a formal claim. Protected producer custody,
source currentness, complete configuration admission, and execution authority
remain separate requirements (#14500).

## Data and authority

The callback receives these five values in order:

| Value | Meaning and limit |
| --- | --- |
| `RequestedInvocation` | Program, exact arguments, requested working directory, and effective explicit environment overrides/removals that configured the real `Command`. It does not resolve a PATH-selected executable or capture inherited environment/configuration. |
| `TimedBytesOutput` | Original stdout/stderr bytes, actual native exit status, whether timeout termination was requested, and process wait duration. Nonzero exits and timeouts remain outcomes, not success. |
| `ByteCaptureBudget` | The nonzero process timeout and each output byte limit used for this capture. Exceeding either stream limit refuses the capability. |
| `ByteCaptureCleanup` | The specific local containment settlement/observation described below. It is not a claim about descendants that escaped containment. |
| `Duration` | Total local elapsed time including the bounded pipe drains and cleanup observations, distinct from process wait duration. |

The request fields drive command construction. The environment is then read
back using `Command::get_envs`, so duplicate overrides, removals followed by
sets, and platform key handling are represented as the effective explicit
request. Raw environment values remain in memory and have no automatic debug
or serialization implementation. A producer must select approved structure and
digests for persistence; it must not dump unrelated credentials.

Successful observed capture requires both byte readers to report completion
within their caps. A zero process exit is insufficient. Launch, process
control, capture, and cleanup failures retain typed refusal categories, and no
observation capability is returned on a failed or incomplete strict capture.
Explicit output bytes are not replaced by a lossy UTF-8 conversion. The next
consumer may interpret them only after retaining their original identity.

There is no public constructor, `Clone`, `Deserialize`, or conversion from
`TimedBytesOutput` to `ObservedByteCapture`. The existing DTO/string adapters
keep their signatures and select their ordinary output from the same capture
implementation. The uncapped legacy adapter can still return its established
drain-truncation diagnostic; that diagnostic is never wrapped in a capability.

## One lifecycle owner

Both byte capture and stdout-to-file capture use `OwnedProcess::spawn_isolated`.
On supported Unix platforms the existing owner observes exit without reaping,
settles its owned process group while the leader still pins its identity, then
reaps. On Windows the existing Job Object owns the assigned descendants.
Normal primary exit runs the existing tree-settlement hook before draining.

The shared timeout path still uses the existing wait implementation. Once the
primary is reaped, confirmation is read-only. It never resends SIGKILL or runs
`kill -0` against that recycled numeric identity. A failed confirmation is a
refusal, not permission to signal again.

| Platform | Recorded cleanup evidence |
| --- | --- |
| Linux | Owned group settlement and a complete scan of the procfs-visible process group finding no live members. Unreadable, malformed, or unavailable scan input refuses confirmation. Zombies are not executing members. Visibility is bounded by this procfs/PID namespace; this is not a census outside it. |
| Windows | The retained Job Object accepted termination and the primary was reaped. Native Windows qualification is a separate required receipt. |
| Other supported Unix | Normal isolated settlement can record that termination was requested, without a post-settlement census. A timed-out path that requires read-only group confirmation refuses as unconfirmed because this owner has no such platform observer. |
| Unsupported containment platform | The existing owner refuses before spawn. |

A descendant that deliberately leaves the process group is outside this
containment guarantee. If it keeps an output pipe open, strict capture refuses
on the drain deadline. If it closes every observed pipe, pipe completion alone
cannot discover it. This limitation must remain visible to the producer and
must not become a stronger cleanup or protected-origin claim.

The process timeout is accompanied by bounded drain and cleanup phases; it is
not a claim that every OS syscall has a hard real-time bound. The owner retains
its existing platform termination/reap behavior. The recorded elapsed values
and the outer qualification deadline are separate evidence.

File publication follows the same ordering: settle containment, observe both
workers' completion, then publish the staging file. Incomplete capture preserves
the prior artifact. If a writer is still live after the drain/settlement grace,
its staging path is retained and named rather than deleted through that writer.

## Maintenance and qualification

Keep this change as an ordinary support child of the existing bounded
availability-probe owner (#12581). Do not add a second preflight probe or a
second process owner when composing the native producer (#14545). Recount the
actual producer parent/head delta when those owners are composed; a small
support seam does not make the cumulative integration a small landing PR.

The direct controls live in `xtask/src/run/byte_capture/tests.rs`. They launch
real children and cover request/environment identity, literal non-UTF-8 bytes,
nonzero status, normal primary exit with a pipe-holding descendant, unrelated
sibling survival, enforced timeout, output overflow, pre-spawn refusal, and
escaped-pipe refusal for both byte capture and file publication. The inherited
byte-input, file-capture, process-owner, and complete text-capture controls remain
part of affected qualification. Fixture fallback cleanup records the original
start/session identity and revalidates a pidfd; it never signals a released
numeric group.

The maintained Linux source-module target includes the actual run owner,
process owner, cancellation contract, ANSI parser and CWD gate, plus the six
bounded availability-probe controls. It requires no replacement implementation:

```sh
cargo test -p xtask --no-default-features --features formal-runner --test formal_runner -- --test-threads=1
cargo clippy -p xtask --no-default-features --features formal-runner --test formal_runner -- -D warnings
```

Full automation and policy qualification, when admitted, use:

```sh
cargo test -p xtask --bin xtask run:: -- --test-threads=1
cargo clippy -p xtask --bin xtask --tests -- -D warnings
cargo xtask check-process-policy
cargo xtask check-traceability
```

The focused target compiles the exact run/process/cancellation sources with
their real tests when the product graph is outside the local resource budget.
Record its frozen source hashes, dependencies, lint configuration, actual
`cfg(test)` compiler arguments, native statuses and outer cleanup. It does
not qualify full xtask dispatch, a native Kani proof, or an unconstructed
producer composition.

Changes to the capability boundary also require the two negative compiler
fixtures in `xtask/tests/fixtures/owned-byte-capture/`. Temporarily include one
as a sibling module in the actual `formal_runner` qualification target; these
files are intentionally excluded from ordinary successful builds. A struct construction must
fail specifically with private-field error `E0451`, and deserializing the type
must fail specifically with missing-`Deserialize` error `E0277`. Record the
intentional injected source separately; any unrelated compiler failure is an
invalid control. The private-fields fixture must also compile when the precise
field-privacy guard is intentionally lifted in the qualification copy. Restore
all source bytes and qualify the unchanged positive graph afterward.

Behavioral counterfactuals must target the actual guard: reverting the byte
spawn to unowned group preparation must fail the descendant/sibling control;
restoring a post-reap resend must fail the observation-target survival control;
accepting an incomplete strict drain must fail the escaped-pipe control. Keep
each expected red witness distinct from ordinary build failures and preserve
the restored positive result. Source-review and local control results do not
grant CI, merge, or protected assurance authority.
