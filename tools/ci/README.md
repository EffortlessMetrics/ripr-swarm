# Shard verifier executable transport

`shard-verifier-artifact.py` is the unprivileged executable handoff in
`rust-test-shards.yml` (#14580). It adds no build or workflow trigger.

Shard 1 retains its stripped `xtask` and an identity document containing the
repository, run, producer attempt, actual checkout SHA and binary SHA-256.
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

The Python suite executes the actual workflow launch block and helper against
inert marker executables, real ZIP archives, and a fixture GitHub API. It covers
valid selection, reuse, candidate shadows, missing/ambiguous/nonregular members,
wrong identities/digests, current-attempt replacement, failure propagation and
cleanup. The regular Linux repo-policy test invokes the same suite. These are
local transport controls, not observed hosted downloads or queue qualification.
