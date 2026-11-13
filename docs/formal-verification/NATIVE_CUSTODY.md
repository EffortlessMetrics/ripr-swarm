# Native Kani capture, replay and maintenance

This is the maintained native evidence route for #14472/#12470 and the
registered currentness pilot in #14473. `formal-kani` and `cargo xtask kani`
call the same argument parser, collector, output interpreter and process owner.
The focused binary avoids compiling the unrelated application facade. Its
qualification is recorded separately from qualification of the full `xtask`
binary.

The producer retains raw evidence and derives `ripr.native-proof-run.v1` from
it. The shared auditor checks the document; the shared replay checks retained
bytes and reproduces the native interpretation. The
[registry and scoreboard](ASSURANCE_REGISTRY.md) join the exact prior
registration to those observations and to current source. A successful
unregistered harness never becomes a registered claim by name matching.

## Supported first profile

The admitted profile is Linux x86-64, Kani **0.68.0**, CBMC **6.11.0**, and the
observed Kani bundle's pinned compiler:

- Rust toolchain `nightly-2026-08-21-x86_64-unknown-linux-gnu`.
- Compiler `rustc 1.100.0-nightly (8925ea358 2026-08-20)`.
- Ordinary proof harnesses with default checks, no substitutions or contract
  replacements, and the exact declared unwind bounds.

The product's Rust toolchain builds the collector. The separate Kani compiler
builds verification programs. The collector records the bundle metadata and
hashes `kani-compiler`, `kani-driver` and `cbmc` before and after the run. These
are observed local inputs, not signatures or an authenticated supply chain.

Install the pinned verifier and build the focused frontdoors from the checkout
that will be captured:

```sh
cargo install --locked kani-verifier --version 0.68.0
cargo kani setup
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo build --locked -p xtask --no-default-features \
  --features formal-runner --bin formal-kani --bin formal-assurance
```

Use the actual build target directory when it differs from `target`. Build-only
profile variables in this example apply to the build command. Do not carry
compiler flags, wrappers, profile overrides or alternate Cargo target settings
into a proof invocation and then describe it as the default profile. Unsupported
configuration is a recorded refusal.

## Run one registered claim

Start in the repository root, with a stable source tree and the matching
collector binary. Run the currently qualified semantic subject explicitly:

```sh
target/debug/formal-kani \
  --package ripr-contract \
  --harness domain::probe::kani_proofs::currentness_action_and_base_side_are_disjoint \
  --default-unwind 32 \
  --claim-registry .ripr/formal-claims.json \
  --registered-claim ripr.currentness.candidate-action \
  --timeout-secs 120 --harness-timeout-secs 60
```

The equivalent ordinary frontdoor is `cargo xtask kani` followed by the same
options. `--package` accepts `ripr` or `ripr-contract`; omitting it retains the
existing two-package scope. The native compiled inventory for every selected
package is retained before applying the harness filter. This pilot selects one
of the contract package's compiled harnesses and establishes no full-repository
coverage claim. Unselected registered claims remain missing from current native
assurance. A qualified mapping exists for the currentness harness's six required
covers; other registry entries keep their explicit outstanding mappings.

Registration is prepared and retained **before** verifier execution. The join
requires exact claim and registry generations, ordered assumptions, source and
authority manifests, provider, bounds, native subject and cover identities.
Success requires a current registered `ESTABLISHED` row. `VERIFIED` in the
unregistered observation table alone is insufficient. A recorded counterexample
or an incomplete/setup/timeout result exits nonzero and retains its actual state.

The whole-run deadline starts before registration, source capture and version
preflight. Each command receives at most the remaining budget. The version
probe is additionally capped at ten seconds and resolved Cargo metadata at
thirty seconds. Final source observations have separately bounded ten-second
Git calls, so an exhausted verifier deadline can still leave an honest final
receipt. An external CI deadline should allow that bounded settlement and
publication. The per-harness native timeout remains an independent limit.

## Retained evidence and lifecycle

Each invocation creates a fresh immutable directory beneath
`target/ripr/reports/kani-evidence`. Locally its name includes the process ID and
a nonce. In GitHub Actions it is grouped as
`kani-evidence/<run-id>-<run-attempt>/<pid>-<nonce>`. CI identifiers are also
recorded separately in the report. Several invocations in one job therefore
have different bundles.

Before parsing a new capture attempt, the frontdoor invalidates the canonical
`kani.json` and `kani.md` aliases. It publishes a same-contract `NOT_STARTED`
checkpoint before source discovery or tool execution. Invalid arguments produce
a typed refusal checkpoint without invoking the verifier. A later interruption
or exclusive-create failure cannot leave a previous success as the current
attempt. Prior immutable bundles remain available. JSON is published last as
the report commit marker; Markdown is a projection of the same DTO.

The bundle contains:

- The initial checkpoint and final JSON/Markdown report, when final publication
  completes.
- Exact stdout/stderr bytes, arguments, working directory, bounded capture
  limits, elapsed observations and native exit status for each invocation.
- Before/after source manifests and Git identity observations, including the
  selected source-input closure and exact package-specific explicit inputs.
- Resolved Cargo metadata, configuration commitments, observed Cargo key paths,
  and before/after installed-verifier commitments.
- The compiled native listing for each selected package, selected and observed
  subject sets, all native assertion/cover properties and exact raw block hashes.
- The prepared registry commitment when registration was requested, followed
  by the derived registered rows.

Successful byte capture uses the existing isolated process owner. It requires
complete bounded pipe drains and the owned process-group settlement observation;
EOF alone does not establish cleanup. The Linux ordinary-exit control includes
a descendant that closes both pipes and stays active until the owner stops it.
This is an owned-group claim, not containment of a process that deliberately
escapes into a different session.

The bounded profile permits 1 MiB small streams, 4 MiB Git file inventories,
16 MiB native/metadata streams, and 96 MiB retained run artifacts. Source reads
are capped at 8 MiB per file, 256 MiB in the captured closure and 20,000 files.
Hitting a limit records incomplete evidence; it never silently truncates a
passing proof. Increase a bound only with a measured need, an owner review and
the associated overflow control.

## Replay and project the exact run

Use the immutable bundle printed by the producer. Do not search for the newest
directory or reuse an alias after another attempt starts. For a bundle assigned
to the shell variable `run_dir`:

```sh
target/debug/formal-kani --replay "$run_dir/kani.json"
target/debug/formal-assurance --root . \
  --run "$run_dir/kani.json" --artifacts "$run_dir" \
  --format json > "$run_dir/assurance.json"
target/debug/formal-assurance --root . \
  --run "$run_dir/kani.json" --artifacts "$run_dir" \
  --format markdown > "$run_dir/assurance.md"
```

Replay verifies artifact sizes/hashes, recorded commands and profile, source
commitments, compiled inventory, exact native result blocks and cover/assertion
interpretation. It neither runs a verifier nor upgrades historical evidence to
current source. A failed native report can replay consistently while the replay
command still exits nonzero because that historical lane did not succeed.

The scoreboard independently prepares the current registry/source closure and
uses the same replay owner before joining registered observations. Inspect the
specific claim's `status`, `provenance`, source generation and missing-obligation
fields. Its provenance is
`local_retained_bytes_checked_not_authenticated`; arbitrary supplied documents
cannot authenticate who executed a verifier. The report also retains manual
obligations and unrelated unselected claims without manufacturing passing rows.

The existing scheduled/manual workflow retains the exact run/attempt artifact
group for fourteen days. Its scoped pilot projects the immutable registered
currentness run before the existing broad lane starts another invocation. It
checks the exact registered row rather than inferring assurance from the broad
lane's status. The workflow remains advisory, with no pull-request subscription
or required-check promotion. Expired artifacts remove replayability; preserve
release/audit bundles according to their owner before expiration.

## Maintain a meaningful negative control

The registered currentness control changes exactly one production expression
in `crates/ripr-contract/src/domain/probe.rs`:

```rust
// Original
matches!(self, Self::CandidateCurrent)
// Declared mutant
!matches!(self, Self::BaseDeleted)
```

This admits stale/unknown source states as candidate actions. It changes the
production decision while preserving the harness and its exhaustive four-state
domain. Work in an isolated checkout with a retained original file and a restore
guard. Verify that the declared original expression occurs exactly once.

First qualify any new native failure presentation from raw output and review its
exact assertion identity, description and location. Record the mapping in the
declared control and update affected generations. Then freeze all source,
registration, configuration and collector inputs for the admitted trilogy:

1. Run the registered command above and retain the positive bundle and original
   production file bytes.
2. Apply only the declared replacement. Run the same package, exact harness,
   unwind and timeouts **without** `--claim-registry` and `--registered-claim`.
   Retain the nonzero native result and complete mutated production file bytes.
3. Restore the original bytes exactly and rerun the registered command. Retain
   this separate restored-positive bundle.

Construct the closed pair manifest documented in
[ASSURANCE_REGISTRY.md](ASSURANCE_REGISTRY.md#paired-native-negative-controls)
under a directory containing all three bundles and the two source images. Each
artifact entry commits the exact byte length and `sha256:` digest. Project it
with the original positive:

```sh
target/debug/formal-assurance --root . \
  --run "$positive_dir/kani.json" --artifacts "$positive_dir" \
  --control-pair "$pair_file" --format json
```

Admission requires the original and restored registered positives, exactly one
source-file change, the same tool/profile/selection and complete cover domain,
and exactly the declared failed assertion set in the mutant. Compiler/setup
errors, timeouts, unwind failures, unrelated assertions, repeated run IDs, extra
source changes and a missing restoration are refused. The mutant stays
unregistered. A format sample collected before the final registry freeze cannot
stand in for this trilogy.

## Source/configuration limits and requalification

The closure includes selected package sources, applicable local dependencies,
Cargo/toolchain inputs, the collector/policy owners, ignored/untracked files
under governed roots, and literal Rust include/module-path inputs. Test-only
exclusion follows actual `cfg(test)` ancestry and external declarations, never
filenames. Every selected/dependency/collector build script is scanned when
present. Qualified include macros are recognized. Dynamic/generated includes,
unsupported inline module paths, symlink traversal and out-of-root inputs refuse
admission rather than guessing their effective bytes.

The profile observes parent/user Cargo configuration and rejects unknown
proof-affecting keys, Kani manifest metadata, check-disabling flags, compiler
wrappers, `CARGO_ALIAS_KANI`, stubs and unreviewed verification-only replacements.
Cargo permits [environment aliases](https://doc.rust-lang.org/cargo/reference/config.html#alias),
so checking the recorded `cargo kani` arguments alone cannot establish the
default command route. Configuration
values that may contain local credentials are committed by hash; raw secret
values are not copied into reports. Only the fixed reviewed domain-cover
generator is admitted among verification-dependent macro definitions. A source
digest is not proof that a `cfg(kani)` body corresponds to a production body.
Registered correspondence and explicit trust review remain required.

This is a deliberately bounded Cargo/Rust profile, not a complete interpreter of
arbitrary build systems. Rust/Cargo dependencies, build-script/compiler/solver
semantics, operating-system observations and specification adequacy remain
named trust assumptions. Equal before/after snapshots detect observed drift;
they do not detect an intervening change-and-restore or provide an immutable
filesystem snapshot. A source, registry, compiler, solver, profile, harness or
cover-location change invalidates its old join until requalified.

For a pinned upgrade, retain native listing and positive/counterexample format
samples; update the single parser/profile owner and strict schema together;
review complete harness/cover/assertion mappings; rerun the actual producer,
trilogy, schema and passive refusal controls. Keep observed custody distinct from
an authenticated governed CI producer. Do not promote a required check merely
because the local collector succeeds.

## Validation and rollback

```sh
cargo test --locked -p xtask --no-default-features \
  --features formal-assurance --lib
cargo test --locked -p xtask --no-default-features \
  --features formal-runner --test formal_runner
cargo clippy --locked -p xtask --no-default-features \
  --features formal-runner --bin formal-kani --bin formal-assurance -- -D warnings
```

The focused process target imports the actual process/cancellation source and
uses independent watchdog custody. Its fixtures exercise missing tools, bounded
version discovery, interrupted/invalid attempts, malformed native inventories,
non-UTF8 bytes, native failure, timeout, source drift and stale success aliases.
It is separate from full application/xtask artifact qualification. Validate the
**emitted** initial, successful and failed reports against
`schema/native-proof-run-v1.schema.json`; hand-authored fixtures alone do not
protect producer/schema parity.

Rollback through an ordinary source/workflow revert and preserve immutable
evidence directories. Withdraw any assurance or CI promotion that depended on a
failed admission law. Never rewrite an old successful report to describe a new
source generation or silently substitute an unregistered result for its claim.
