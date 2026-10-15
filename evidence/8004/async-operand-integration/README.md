# PR 8004 async operand repair and integration

Author delivery, 2026-10-15 UTC. **PR_IN_FLIGHT / REVIEW_INCOMPLETE**. Source is normally pushed to the existing open Draft. Rook assigns fresh independent review; this receipt grants no Ready or merge clearance.

## Exact objects

- Repository: EffortlessMetrics/ripr-swarm; PR8004; branch rook/7415-private-target-attribution.
- Assigned source8b: 8bbc8dd4dead78cdccdb10861fe4848103aac6dd, tree cda6f3f58de043bd5615caaa42984ff06024d7bb; historical merge base67866bc95a72462840c3eedac0f213ed658474b4.
- Frozen integration main: a142074061128a85e76c1ac494b9939f3a58428b.
- Normal merge013ea85816511a13f13d9728526d3337aecd17dc, tree dce136e3a5f2907c99cb80715a3bd1be57dc3952, parents source8b and maina142.
- Repair/published head **a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524**, tree **6bd2c8c772cfbb11eb9c8ae709fff98d465e7f85**, parent013ea858. Clean source; history preserved. Git push/readback and GitHub open/Draft/unmerged/mergeable=true are retained.
- Later observed main8a55a5be744368ff3592709957451c6ba04d8a4a. Textual composition clean, tree8feb57a27f0211ae136217cf8620ba542595c1d9. GitHub observed merge ef9978d9445b89486bcdb2dbe6360e5a7204371a has parents latest8a55 and head a2, and the same tree. **That later composition was not executed.** Its five commits include field-assignment probing/observation, a fact-cache bump, suppression/config diagnostics, LSP and structural workspace discovery. Relevant shared-code patches were inspected; the honesty corpus is unchanged. No repeated behind-only restack.
- Prior source8b proof remains immutable at62c8fe9d4f256f11e31f21f1eb151d8fbfac5a76/evidence/8004/repair-executable-identity and its author comment6191731802. Latest source8b review5532955967 was read before implementation.

## Reproduction and bounded change

The review's source-derived async concern is now executable-confirmed on the actual clean source8b CLI. Authoritative replay source8b-toolchain runs12 final stimuli through full/fresh-compact/diff (36 reports) with the explicit Cargo/Rust environment. Four dormant async/async-move negatives receive20 wrong owner-credit rows: eight direct repository, four direct diff, eight helper repository. All intended subjects and full/compact metric parity are checked. The exact requested assert_eq!({ let _ = async { JsonValue::Object(7).as_object() }; 7 }, 7) is retained.

Native truth is independent: the new12-case subset has24 executions, originals all pass, four outer/mixed mutants fail and eight dormant mutants survive. Final34-case matrix has68 executions (34 originals pass,14 mutants fail,20 survive); the extra same-line caller control has four individually filtered runs (genuine mutant fails, decoy survives). Earlier setup executions are retained separately and not added to a unique-case denominator. Invoked nested functions remain native-positive but statically unsupported.

Production extracts node_executes_in and shares the outer walk's item/closure/async ancestor exclusion with synthetic standard-assertion operand parsing. Both direct and free-helper predicates pass through it. Complete owner/module refusal, actual same-file helper declaration custody and operand-local shadow refusal remain intact. Real outer calls, including calls beside discarded futures, keep existing direct/high or repository helper/high credit. No general macro expansion, type/Cargo-target resolution, factory/parameter/field/cross-file flow or async activation interpretation is added. Performance concerns #8005 remain substantive and unbenchmarked.

## Integration and honesty

The sole actual merge conflict was fixtures/evidence-promotion-honesty-corpus/corpus.json. Complete keyed three-way comparison:294 base,309 candidate,297 main, all IDs unique, no divergent same-ID edits, equal non-case fields. Resolution preserves candidate bytes and appends the three complete main additions,312 rows. Four new cases yield316; every complete input-side row is verified equal. Raw inputs, hashes, conflict and resolution are retained. No count-based overwrite.

Two new fixtures alone received initialized goldens with RIPR-SPEC-0005 reasons. No old golden re-bless. Existing five false_actionable observation blobs match the prior immutable packet exactly; their case/truth bytes also match source8b. No authored Rust verdict-corpus label/observation edit. Main's own corpus changes were preserved.

## Exact committed-head local checks

All11 focused commands returned0 on a2 (head-focused-exits and head-*.log):
- 387 test-grip,175 related-test,221 owner-pin,33 call-identity,8 name-scope,43 rerun units.
- 9 receiver,5 native-path,23 assertion-helper,17 rerun integrations;1 current-main suppression smoke control.
- All have zero failures/ignored tests. Working-tree shaping runs are separate;1782 source-input digests match the final source.

Clean default-feature CLI identifies a2 exactly; SHA25612693b34e1e5609028a13d45e50ded775420a0546525a202dcfbbd72d9e4af2f. Final head-cli contains102 raw reports for34 cases, zero contract violations; head-same-line-cli adds three producer reports retaining both subjects. Four owner seams per full case and both diff probes/every retained oracle row are checked; fresh compact metrics agree. Both complete new committed corpus reports replay equal after removing only artifact metadata.

- All15 check-fast gates pass, no skips, including full workspace/all-targets Clippy. Independent141-path set exactly equals the complete precommit change inventory; head/base/tree and effective reports are retained.
- 463 selected/actually executed/compared golden fixtures, identical sets; zero drift/errors.
- Rust corpus passes:21/485 false verdicts,0/621 contradictions, oracle44/44, overclaims0/44.
- Python corpus passes:0/69 false; TypeScript passes:7/169 false. These are current-head observations, not an attribution of changes solely to the async repair. Historical source8b26/485 and10/169 remain historical.
- Honesty passes; advisory dogfood passes with nonempty boundary_gap1 and weak_error_oracle2 findings.
- Documented locked fixture fetch prepared anyhow1.0.100 in the same Cargo home.

These are selected Linux/default-feature local checks, not hosted required checks, Windows or full-workspace runtime qualification.

## Incomplete proof and actual capacity stop

Final precommit returned1 after its additional selected package-Clippy profile consumed the remaining capacity margin. Its ripr package Clippy passed; xtask package Clippy was deliberately interrupted with SIGINT before crossing the8GiB reserve. The raw tool labels this source_or_policy_failure; it is retained unchanged with the exact capacity/ownership receipt, not relabeled a pass. Full workspace Clippy/check-fast already passed but does not substitute for this configuration's incomplete terminal run.

All-corpora preflight returned73 before any Perl build:8.480GiB available,2.5GiB producer budget plus8GiB reserve did not fit. Rust/Python/TypeScript were then checked independently through their documented commands. **No current-head Perl execution is claimed.** Source8b's historical Perl result remains12/36 false; it does not qualify a2.

Next owner Rook: fresh independent exact-head review; resume selected-xtask precommit Clippy and Perl in an adequately provisioned environment, and establish affected latest-main composition/hosted qualification before any later permitted transition. Keep Draft. No Ready, merge, release, thread resolution or bot contact.

## Setup, attempts and resources

Bash/Linux; Rust/Cargo1.95.0, GCC14.2. Explicit environment: CARGO_HOME=/workspace/.cargo, RUSTUP_HOME=/workspace/.rustup, PATH prefixed /workspace/.cargo/bin, CARGO_TARGET_DIR=/workspace/ripr-swarm/target, CARGO_BUILD_JOBS=4. Initial rustup default-home read-only failure was recovered with these existing workspace locations, without installing or changing credentials. Standalone CLI driver initially omitted this wrapper; authoritative source8b-toolchain and final-head replays correct it. Unsupported owner_helper setups are retained as non-admitted candidate attempts, not rejection evidence. A new-golden reason missing a spec ID was rejected before generation and corrected. No setup/argument failure is called a behavioral red.

Native reproduction: rustc --edition=2024 --test --crate-name control source.rs -o separate-output, run original, replace only Self::Object(value) => Some(*value), with Self::Object(value) => None,, compile/run mutant. Raw source/argv/digests/exits/logs are retained. CLI argv and input roots are retained in each results.json. Scratch drivers, mutable caches and executables are excluded from this durable packet.

Original checkout /workspace/ripr-swarm work@9f7d06bf remains tracked-clean. Owned /workspace/async8004, raw evidence, target and independent frozen source8b/intermediate/head CLI copies remain retained for open-Draft review. No destructive cleanup or settings/security/process-policy change. Exact owned compiler group25296 was interrupted; cargo25307 and clippy-driver25453 were subsequently observed terminated zombies with RSS0 under PID1, awaiting reaping. No live compiler was left running and no reaping claim is made. Publication uses an alternate Git index and bounded evidence objects, avoiding another full checkout or deleting any unique work.

The source branch is the product candidate. This evidence-only child commit is not another product PR. Raw log/patch whitespace remains raw evidence, not a source diff-check claim. Source diff-check passed.
