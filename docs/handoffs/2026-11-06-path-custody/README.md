# Bounded #4006 custody handoff

Owner: delegated documentation/path lane; central integration remains with thread
01a1a959-f983-763a-96e6-7dbe1cced019. No integration, public activation, release,
settings or hosted-CI request is performed by this handoff.

## Source and contract

Input: swarm b51aebc7e4735529befd677ebf14071c7f09e477,
tree d3142dd59d4dcfea5e6b4981732f61c8516f3fa0. Separate docs commit:
497f97817 (roadmap corrections). Custody patch:
6d949fcc9091074ea16f446d2bf3d256a0dd0a45,
tree 188fbcaf26b73fd8abb8143f420e85dd6aea0333.

Reuse open swarm PR #13320, verified e9ce6779b3d4072285547b3b57073ae19d77ea71.
It owns the ledger/manifest leg. This patch requires that producer; it does not
reimplement its capture or project a substitute candidate. Its accepted role is
range_touched (union of per-record paths across the fixed release base..cut),
not net source_changed/swarm_changed/swarm_only or K active paths.

The bundle carries role, exact repository/base/cut/tree, decoded count,
NUL-framed digest, retained recipe and raw denominator-packet digest. The public
reader derives expected values from the independently selected manifest and raw
#2768 bytes, compares the bundle and recomputes the visible receipt comparison.
Absent input is rejected, not treated as an empty inventory. Valid empty input
has its explicit zero count and SHA-256 empty-stream digest.

Fetched public ripr d731a19bf13e5a5c2257d3c320f734f65ccd9894,
tree 57c0f03ff224d5eb56cd897672858188ddd359ce. Its original reader and material
blobs exactly equal the inspected swarm input (Git blobs 682b887964d83a83130cdb2ad05577a9684b8d74
and 234c4aa2738e6ea4afa2fb3a8d047a883cd8f0ba, respectively). This proves patch
surface applicability; it does not prove effective trusted public execution.
Public #1893 and draft #1889 holds remain. No attribution history was changed.

## Executed proof and limitations

Linux x86_64, rustc 1.95.0 (59807616e 2026-04-14). Reader/shared authority proof:
15 passed, 0 failed, 0 ignored; harness Clippy --lib --tests -- -D warnings passed.
The harness compiles byte-identical copies of the actual reader modules, imports
the actual path_inventory/path_custody modules and runs their actual tests.
source-sha256.json binds those bytes. Native subprocess transport is deliberately
unavailable; no network response or Git tree observation is manufactured.
Native API/tree transport, the complete handoff producer module, full xtask
compilation, hosted gates and public activation were NOT established.
The producer call site and fixture were reviewed and formatted, not executed.
Overall review: REVIEW_INCOMPLETE for combined #13320 integration and those
remaining dimensions. Do not treat this branch as merge/release readiness.

The old reader admits a fixture with no path custody: the deliberately failing
reader-red.txt is the intended missing-custody witness. Removing the new equality
check lets a same-subject altered inventory through even after the attacker
refreshes the claimed comparison observation and outer qualification digest;
reader-removal-control.txt fails with "accepted path substitution". Source was
restored before the green run. An earlier removal attempt was caught by the
receipt-comparison check; the negative was strengthened to update that claimed
observation as well. Corpus-audit is an independent Python check of eight fixed
pins, not execution of the PR-evidence producers.

Initial harness setup failed because a #[path] module resolved child files in
the wrong directory; staging byte-identical modules as mod.rs repaired that
instrument error. An initial unrestricted cargo test also attempted empty
doctests with a missing repository-configured TMPDIR. The retained passing
command explicitly uses --lib; no doctest pass is claimed.

The denied gh API read for PR #13325 was not retried or substituted. That
metadata was unnecessary for this isolated repair.

## Reproduce without building ripr

Run from the repository containing the patch. Use an external disposable
directory; it will hold only a small crate and selected module copies.

```bash
python3 - <<'PY'
from pathlib import Path
import shutil
root=Path.cwd()
out=Path('/tmp/ripr-path-reader-proof')
assert not out.exists(), 'choose a fresh proof directory'
(out/'src/source_promotion_acceptance').mkdir(parents=True)
(out/'Cargo.toml').write_text('''[package]
name="ripr-path-custody-reader-proof"
version="0.1.0"
edition="2024"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="1"
sha2="0.10"
''')
shutil.copyfile(root/'docs/handoffs/2026-11-06-path-custody/harness-Cargo.lock',out/'Cargo.lock')
src=root/'xtask/src/reports'
shutil.copyfile(src/'source_promotion_acceptance.rs',out/'src/source_promotion_acceptance/mod.rs')
for p in (src/'source_promotion_acceptance').glob('*.rs'):
    shutil.copyfile(p,out/'src/source_promotion_acceptance'/p.name)
(out/'src/lib.rs').write_text('''#![allow(dead_code)]
mod run {
 pub struct Output {pub timed_out:bool,pub stdout_truncated:bool,pub stderr_truncated:bool,
 pub status:Option<std::process::ExitStatus>,pub stdout:String,pub stderr:String}
 pub fn capture_output_in_dir_with_timeout_bounded(_: &std::path::Path,_:&[String],
 _:&[(String,String)],_:&std::path::Path,_:std::time::Duration,_:usize,_:&str)->Result<Output,String>{
 Err("transport is outside the isolated reader proof".into())}
}
mod reports {
 #[path="'''+str(src/'path_inventory.rs')+'''"] pub(crate) mod path_inventory;
 #[path="'''+str(src/'path_custody.rs')+'''"] pub(crate) mod path_custody;
}
mod source_promotion_acceptance;
''')
PY
cargo test --manifest-path /tmp/ripr-path-reader-proof/Cargo.toml --locked --lib
cargo clippy --manifest-path /tmp/ripr-path-reader-proof/Cargo.toml --locked --lib --tests -- -D warnings
```

Full commands used this same harness with local CARGO_HOME/RUSTUP_HOME under
/workspace and --manifest-path /workspace/ripr-docs-evidence/reader-harness/Cargo.toml.
No full library build was repeated.

## Storage and continuation

Owned isolated checkout: /workspace/ripr-docs-011; its Git common directory is
/workspace/ripr-docs-011/.git. Retained witnesses and harness are under
/workspace/ripr-docs-evidence. Toolchain/cache use /workspace/ripr-rustup and
/workspace/ripr-cargo. Observed free space before generation: 30 GiB; bounded
additional compiler/cache/harness budget: 1 GiB, plus source checkout. Actual
sizes: toolchain 614 MiB, cargo cache 34 MiB, evidence/build 153 MiB. No full ripr
build admitted. Toolchain/build state retained for possible focused follow-up;
source witnesses retained pending receiver verification, not deleted.

Next owner action: reconcile #13320, compile/run the combined handoff producer
and scoped reader tests in the owning qualification environment, then refresh
applicable source review. Public trusted deployment/activation remains with its
existing owner and holds. scope_pending and inactive policy bytes are unchanged.

## Coordinated schema-lane release-note correction

The schema lane reported exact-b51aebc7e five-producer replay and 104 scoped
passes (rows digest b1ecc45e0da4b1fcdf855123944b7ca29c634aa602743b404048b68796856c92).
Its raw archive was not fetched or replayed here; those are attributed lane
results. Local source inspection independently confirms producer constants in
output/repo_exposure.rs: 0.5, output/repo_seams.rs: 0.3, and
output/mutation_calibration.rs: 0.3. A separate docs commit corrects those three
0.11 target versions and says "including additive" span fields. The 0.10
baseline versions and all other release-note text are preserved. No schema
replay was duplicated or promoted to this patch's executed proof.

## Executed combined producer follow-up

Swarm #13320 was freshly read back at the same
`e9ce6779b3d4072285547b3b57073ae19d77ea71`; its reviewed writer's ledger work was
reused, not edited or duplicated. Isolated proof composition merged that object
with lane handoff `916a96cc4b05ef9272749b089bf0c33c2f50dc6c`. The one module
conflict was resolved by retaining the newer release_final_subjects modules
and #13320's public release_ledger visibility. No central candidate was changed.
The final local proof object (including the added native producer tests) is
`e160bfbdc50579ee6e9fd82a78b41df55f3fa58c`, tree
`8ae126c82973c71ef5cd3d9ef55216bc4b2c78ff`. This is an isolated test composition,
not a selected release candidate or a public-source activation.

New integration unit `8ec87d1817977c1daaf77cb94bf033bf09e46567` adds repository
producer fixture controls and a local receipt transport round-trip. The actual
handoff producer admits the selected manifest/bundle, revalidates fixture native
responses and constructs its original production receipt. That receipt is
serialized, written to an owned fixture file, reread byte-for-byte with digest
comparison, decoded and passed to the actual public reader. A changed observation
then receives the intended public comparison refusal. Fixture native decisions
remain explicitly synthetic; local file transport is not hosted authentication.

The new producer negatives reaccept changed bundle bytes while preserving the
selected manifest/ledger. Wrong count, same-count newline partition, recipe,
base/cut/tree, denominator digest and missing role refuse. Removing the shared
equality guard makes the actual producer admit a negative, failing the unchanged
new test with "handoff admitted the negative control"; restoration returns green.

### Executed source and bounds

The source module proof compiles the actual production handoff, native decision
codec, manifest admission, file snapshot custody, path comparator and public
reader, together with their selected original repository fixture tests. Commands:

```text
cargo test --manifest-path /workspace/ripr-docs-evidence/producer-harness/Cargo.toml --lib
45 passed; 0 failed; 0 ignored; 0 filtered out
cargo clippy --manifest-path /workspace/ripr-docs-evidence/producer-harness/Cargo.toml --lib --tests -- -D warnings
passed
```

Two unrelated test-module declarations are explicitly removed only in staging:
the manifest's source-Git fixture group and the manifest-proposal dispatch group.
No production handoff/reader/file-snapshot/comparator/decision-codec code is changed
in staging. source-identities.json records original and staged hashes and the two
nonidentical module containers. The adapters for native API subprocess transport
and source-Git range operations return errors; they cannot manufacture successful
transport or source verification. Only the synthetic-fixture admission route ran.
The tiny command-role recipe helper uses the accepted #13320 string and shared
path_inventory authority. This selected-module build is not the complete xtask
binary, command dispatcher, library test suite or a hosted gate.

The repository-native xtask binary remains unrun: `xtask/Cargo.toml` directly
depends on `../crates/ripr`, and this environment has no cached libripr/xtask
artifacts. Building that graph would require the prohibited full-library build.
The producer call sites now have executed production-source proof, superseding
the earlier "not executed" limitation; whole-repository linkage/native transport
and effective trusted public activation remain REVIEW_INCOMPLETE. The warm
integration owner can run `cargo test -p xtask --bin xtask native_handoff` on its
combined candidate without starting a duplicate full-library build here.

### Reproduction and resource custody

`producer-proof/` retains the staging script, tiny harness manifests/lock, source
hashes, green/Clippy/removal logs and artifact hashes. The script's two absolute
paths at its top identify the composed checkout and disposable harness directory;
set those to owned local locations before reproduction. Copy harness-Cargo.toml
to that directory as Cargo.toml and harness-Cargo.lock as Cargo.lock, create src/,
then run the staging script followed by the two commands above. The script states
every excluded test group and adapter in ordinary source; inspect it before use.
No remote command or CI request is part of reproduction.

Additional owned checkout: /workspace/ripr-path-combined and its .git directory.
Additional bounded target: /workspace/ripr-docs-evidence/producer-harness/target,
190 MiB observed. Available space before this follow-up was 29 GiB; the additional
budget was 512 MiB for the source composition, target and temporary fixtures.
The sparse 64 MiB manifest-budget fixture is owned and cleaned by its fixture.
Retained source/evidence targets await receiver verification; no unrelated files
or processes were cleaned. The composed dependency object is not pushed onto an
integration branch. Separate docs commits remain independently cherry-pickable.

## Repository-native proof — supersedes native linkage limitation

The coordinator clarified that dependency compilation for scoped repository tests
is permitted, retiring only the earlier task-local no-build interpretation. The
full-library test suite remained excluded. Repository resource safeguards and
all public/security/attribution holds remained in force.

Actual composed subject: `e160bfbdc50579ee6e9fd82a78b41df55f3fa58c`, tree
`8ae126c82973c71ef5cd3d9ef55216bc4b2c78ff`, clean before and after execution.
Its fetchable proof-only ref is
`refs/heads/handoff/011-path-native-composition-e160bfbd`; this does not select a
release candidate. The earlier module results retain their original subject,
commands and explicit staging limits; they are not relabeled native results.

Executed from `/workspace/ripr-path-combined` using installed Rust 1.95.0:

```text
cargo test --locked -p xtask --bin xtask native_handoff -- --nocapture
native exit 0
15 passed; 0 failed; 0 ignored; 5421 filtered out
```

This built ripr as a dependency, compiled the actual repository xtask test binary
and executed the real retained producer/reader tests without staging substitutions
or transport adapters. The new producer negatives and original receipt local
round-trip both passed; public_preflight_requires_complete_native_handoff_inputs
also ran. No full-library test suite, live GitHub transport or hosted activation
ran. Source-Git verification outside this test filter remains outside this proof.

Guard result: 386.34 seconds; no guard stop; peak owned RSS 6,197,891,072 bytes;
minimum free disk 28,478,136,320 bytes. Admission measured the cgroup memory cap
(16 GiB) and available space before starting. Limits: one Cargo job, incremental
and debug info disabled, 18 GiB additional growth budget, 6 GiB disk reserve,
10 GiB aggregate owned RSS, 12 GiB per-process address space, 45-minute outer cap.
The dedicated driver owned its process group and descendants. Readback after
terminal exit found no group members. Complete admission, sampling, result and
cleanup receipts are retained under native-proof/.

Executed binary (retained in the owned target, not duplicated):
`/workspace/ripr-path-native-target/debug/deps/xtask-c409048ff7d9c22e`,
338,759,376 bytes, SHA-256
`3f9b0b1f6959085241dec3c3c419a27320c1f19c3b1aadc3a867a93f61d2e1fe`.
Original native command log SHA-256:
`f4aaf5c2192b8f83f170a5ffd0bf0128a344be672b76bf033fe6d0f6b75e3db0`.
The raw log and guard samples are stored as base64 JSON envelopes to preserve
exact bytes, including terminal newlines; verify bytes/hash after decoding.

Additional target `/workspace/ripr-path-native-target` is 1.5 GiB and retained as
warm mutable build state pending owner verification; the recorded executed binary
must be rehashed before reuse. The owning task retains the admission driver and
original witnesses. No unrelated cleanup or process termination occurred.

The native linkage/producer-test gap is now closed on this exact composed subject.
Overall integration and effective trusted public admission remain separate from
these focused local passes. No central branch, candidate pin, scope_pending or
inactive hard-cut policy was changed; #1893/draft #1889 holds remain unchanged.

## Final local source-Git and dispatcher coverage

The same unchanged, clean native subject `e160bfbdc50579ee6e9fd82a78b41df55f3fa58c`
(tree `8ae126c82973c71ef5cd3d9ef55216bc4b2c78ff`) and the same rehashed actual
xtask binary executed two additional existing scoped repository fixture groups:

```text
cargo test --locked -p xtask --bin xtask reports::release::candidate_harness::live_head::tests::source_tests:: -- --nocapture
1 passed; 0 failed; 0 ignored; 5435 filtered out; native exit 0
cargo test --locked -p xtask --bin xtask reports::release::candidate_harness::live_head::producer::tests:: -- --nocapture
6 passed; 0 failed; 0 ignored; 5430 filtered out; native exit 0
```

The source-Git fixture creates a disposable local repository and its own local
candidate ref. Actual AdmittedSource admission/revalidation and verify_ranges
check first-parent versus all-reachable history/counts/order, manifest-versus-
ledger path count/digest/recipe, altered packet identities, missing inventory,
wrong base/cut, moved local ref, changed source head/tree/package and foreign or
ambiguous origin spellings. These are real local Git operations, not API mocks;
no fetch from GitHub or protected remote-ref mutation is performed.

The six actual command-dispatch fixtures exercise release-manifest-proposal
through XtaskCommand and dispatch: complete typed serialization (without granting
acceptance), incomplete/stale/unknown input refusal, unsafe/existing destination
refusal, aggregate/per-file serialization growth limits and partial-write byte
preservation. The result is a proposal, not independently accepted authority.

Both guard receipts ended normally: source-Git 30.02 seconds (27.90 test seconds),
dispatch 2.00 seconds (1.10 test seconds), no guard stop. Memory observations are
sampled, not precise high-water measurements. Both owned process groups were
empty after exit, and no owned native fixture-prefix directories remained under
the composed checkout's target. Original log SHA-256 values:

- source-Git: 04d8d5c2b5ffe0dd8d08e16324dcb77ceab2827afa1044fc35423071817cdd20
- dispatch: 27730eea5ff9dea19a100dd0e7ce5dff0ad18e71bbe501f3c6550db570ef6dbe

local-source-proof/ retains each separate command/admission/result/cleanup,
original-byte log and guard-sample envelopes, drivers and artifact hashes. These
results supersede the earlier local source-Git/dispatcher execution gaps only;
previous module and native_handoff receipts retain their original subjects.

## Integration packet

Dependency: reuse swarm #13320 at e9ce6779b3d4072285547b3b57073ae19d77ea71.
Minimal custody implementation sequence after reconciling that dependency:

1. 6d949fcc9091074ea16f446d2bf3d256a0dd0a45 — bundle/receipt custody and public comparison, spec and controls.
2. 8ec87d1817977c1daaf77cb94bf033bf09e46567 — actual producer negatives and local original-receipt round-trip.

The separate docs commits remain 497f97817 (historical roadmap corrections) and
5f951ee06 (three producer-version corrections). Handoff/evidence commits are
separate from those implementation units. The isolated composition's only merge
conflict retained both the newer release_final_subjects modules and the
#13320 public release_ledger visibility; do not drop either side.

Verified production locations on the exact composed tree:

- xtask/src/reports/path_custody.rs::PathCustody::compare
- xtask/src/reports/release/candidate_harness/live_head.rs::LiveHeadSnapshot::verify_ranges
- xtask/src/reports/release/candidate_harness/live_head/handoff.rs::admit_with
- xtask/src/reports/source_promotion_acceptance/material.rs::validate
- xtask/src/reports/release/candidate_harness/live_head/source_tests.rs
- xtask/src/reports/release/candidate_harness/live_head/producer/tests.rs

Remaining unproven boundary: real native GitHub acceptance reads and effective
trusted hosted public admission/activation. Local origin-string validation and
synthetic native decision fixtures do not prove remote identity or grant public
execution authority. Public #1893 and draft #1889 holds remain with their owners.
No public canary, workflow activation, protected-ref/settings mutation, broad
suite or publication was attempted. Central integration remains the sole owner;
this packet adds no separate review or approval gate.
