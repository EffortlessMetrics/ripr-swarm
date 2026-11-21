# Shard verifier executable transport

`shard-verifier-artifact.py` is the unprivileged executable handoff in
`rust-test-shards.yml` (#14580). It adds no build or workflow trigger.

Shard 1 retains its stripped `xtask` and an identity document containing the
repository, run, producer attempt, actual checkout SHA and binary SHA-256.
It stages those two files in a fresh `RUNNER_TEMP` directory and passes that
exact path to the upload step; restored build-cache payloads cannot be reused.
The verifier resolves a complete same-run artifact inventory, chooses the
highest canonical positive attempt at or below its current attempt, then
re-reads that artifact by exact ID. Ambiguous, future, expired or malformed
candidates refuse; a bad newest candidate never falls back to an older one.

The ZIP is downloaded by that ID and checked against GitHub's SHA-256 digest.
Its only members must be regular `xtask` and `identity.json` files. The producer
identity and binary digest must agree before launching from a fresh private
directory under `RUNNER_TEMP`, outside the checkout. The selected artifact ID,
name, digest, producer attempt and verification attempt are retained in
`verifier-artifact.json`. Temporary executable bytes are cleaned on ordinary
success and failure. Abrupt runner loss relies on hosted-runner disposal.

The verifier job alone has `actions: read` for artifact metadata/downloads,
within the existing routed caller's read permission. It has no App token or
write authority. `GH_TOKEN` and `GITHUB_TOKEN` are removed from the launched
binary's environment. The rest of the existing job context remains unprivileged.

This validates transport identity, not trustworthy candidate behavior. A G
candidate can change its own code. Never use this executable in a privileged
finalizer or take its receipt as independent proof of G. The controller's
source binding and authoritative producer job/attempt supersession remain
#14500 responsibilities. In particular, missing newer artifacts do not prove
that a newer producer never failed or was cancelled. Current attempt is
reobserved before launch, but this is not an atomic finalization transaction.

Earlier-attempt reuse is supported only when that artifact has the identity
format above. Old-format or expired artifacts fail closed: rerun the producer
through the normal authorized route instead of falling back to arbitrary files.
The changed workflow is itself an existing shard evidence input, so mixed old
and new producer generations do not silently qualify.

Run the Linux command-entry/ZIP controls with:

```sh
python3 tools/ci/test_shard_verifier_artifact.py -v
cargo test -p repo-policy --test shard_verifier_artifact
```

The Python suite executes the actual workflow producer and launch blocks against
inert marker executables, real ZIP archives, and a fixture GitHub API. It covers
valid selection, reuse, candidate shadows, missing/ambiguous/nonregular members,
wrong identities/digests, current-attempt replacement, failure propagation and
cleanup. A restored `target/xtask-binary` fixture remains untouched across two
producer executions, each of which emits a distinct fresh payload path. The regular
Linux repo-policy test invokes the same suite. These are
local transport controls, not observed hosted downloads or queue qualification.

## Bounded Nextest discovery (#14906)

`nextest-discovery.py` is the Linux shard caller's stdlib supervisor. It runs
partition discovery first and independently lists the full workspace on shard 1.
Each phase has a 480-second cap, above the recorded 6.1-minute listing. Two
maximum-length phases leave 24 minutes of the existing 40-minute test step;
this is not a guarantee that the entire suite fits. Sampled successful whole
test-step times (discovery plus execution) were 686–937 seconds in runs
44553602873, 44562926168, 44576937506's healthy shard and 44486867385;
whole successful jobs were 1352–1922 seconds, uploads 1–2 seconds. These are
comparators, not separately measured execution times or a worst-case bound. The 60-minute job also
includes earlier preparation/build steps. Upload time is not reserved by changing
those caps: a discovery failure returns early to the existing `!cancelled()`
upload, while outer job cancellation remains a limitation. Reassess budgets
against measured phase durations before expanding workload; no hosted latency
qualification is claimed by fixture tests.

Only successful nonempty discovery publishes the canonical sorted list, after
phase status and metadata are written. Duplicate rows remain for the unchanged
verifier to reject. No shard-success status or JUnit is synthesized. A child
exit 124/137 is distinct from a supervisor timeout. Cancellation tears down the
owned process group; normal exit with surviving group members refuses. The direct
child is reaped; orphan grandchildren rely on host init and a deliberately escaped
session is outside this POSIX boundary. SIGKILL and blocked kernel IO still rely
on the outer workflow bound.

Artifact exports add only `discovery-*.status` and
`discovery-*.metadata.json`. Metadata contains a validated phase, stderr byte
count and present/empty bucket, never child stderr payload, commands or decoded
diagnostics. No generic secret-redaction claim is made. Expected test names remain
in the canonical lists. Stdout staging is private, bounded to 64 MiB and removed
on ordinary completion/failure; no raw partial stream is an artifact. Exactly one
writer owns each phase in the existing serial shard shell.

Run `python3 tools/ci/test_nextest_discovery.py` for 21 helper controls plus five
actual workflow-shell fixtures with inert Cargo/Git/Rust commands. The Linux
`cargo test -p repo-policy --test nextest_discovery` target runs the same suite.
The fixtures shorten the helper budget through an inert interpreter wrapper;
they never run Nextest, build the workspace, dispatch CI or claim native test
execution. Verifier, independent full-list denominator and JUnit requirements
are unchanged.

During process acquisition TERM/INT only record cancellation until the child
handle is assigned. Cancellation is then checked before stream setup. Teardown
also records repeated signals until group cleanup and direct-child reap finish;
publication retains the exception-based cancellation guard. Two synchronous
spawn-handoff controls pin TERM/INT at this ownership boundary.
