# Independent re-review at repaired head 490963fb

**REPAIR_REQUIRED.** Both earlier findings are independently confirmed repaired, but the repair introduces an adjacent exclusion-admission regression. Pre-Ready source-review prerequisites are not satisfied. No candidate source or tests were edited by this reviewer.

## [P1] Preserve lexical excluded-path admission before resolving file aliases

Status: verified_current at `490963fbff4bde6f425a667dda21d7d85205568d`, tree `d1b2b13342877504ccd100ae271fcf2c4919cfd4`. Owner: `crates/ripr/src/analysis/input_boundary.rs`, `observe_module_read`, lines85-98 / `resolved_probe_path`.

Failure mode: `resolved_probe_path` canonicalizes observed files before the `.git` / Cargo-owned `target` component check. A symlink under either excluded directory pointing back to a regular captured source file therefore loses the excluded component and is admitted. Custody inventory skips those directories, including their alias bindings. The new guard tests only the target's canonical relative path.

Independent real native CLI reproduction, frozen fixture with one changed Rust file/two findings:

```text
src/lib.rs: include!("../.git/linked.rs");        # analogous target/linked.rs case also executed
.git/linked.rs -> <selected-root>/src/impl.rs   # src/impl.rs is a regular captured file
```

| Native subject | .git alias | Cargo target alias |
| --- | --- | --- |
| Retained original native 6a47f5bf | exit2, both publication flags false | exit2, both flags false |
| Exact repaired native 490963fb | exit0, complete_set=true, command_success=true | exit0, both flags true |

Both repaired reports have two native findings and `complete_with_findings`; all native member lengths/hashes were independently checked. The include-boundary advisory remains stderr-only and does not refuse publication. The alias entries are absent from `identity.observed_inputs`; their regular target is present. These were four real native processes on two bounded fixtures, not mocked helper calls.

The exact pre-repair `0ffd9456` input_boundary module was also imported into a tiny independent Rust harness: it rejects the lexical `.git/private.rs -> captured source.rs` observation. The repaired module accepts the same observation. The native baseline process above is explicitly historical 6a47, not mislabeled as a rebuilt 0ffd executable; the old guard blob is identical across those heads.

Why material: the schema/help contract deliberately excludes metadata/build-directory inputs from custody and promises the native read/probe owners will refuse those reads. Accepting the canonical target does not establish custody of the traversed alias binding inside the skipped directory. This is a demonstrated exclusion/refusal regression; **no outside-root escape or malicious-filesystem guarantee is claimed**. No transient-change or crash-durability inference is needed for this stable-path counterexample.

Fix direction: preserve selected-root-relative lexical exclusion checks as well as canonical containment/alias identity. Reconcile requested/canonical root spellings first so the newly fixed selected-root alias positive stays valid; do not simply revert the root-alias repair. Preserve existing missing-probe, outside-root, metadata/Cargo-target and nonregular-file refusals. Add actual CLI negatives for aliases under both skipped directories pointing into otherwise captured regular source, alongside the selected-root-alias positive. Rerun affected exact-new-head boundary/CLI controls. Source repair remains with the candidate owner. Confidence: high.

## Earlier findings: repaired

- Root-alias false refusal: independently executed physical-root and aliased-root fanout successfully, checked both manifest member hashes, and compared aliased badge/detail/human member bytes with separate standalone native commands. Added an in-root symlink to an outside regular file afterward; capture still refused with exit2 and both flags false. Six independent CLI processes passed the intended positive/negative controls. `root-alias/receipt.json` and streams retain results.
- Sticky process-wide FAILED: the flag was removed; exact-module independent harness confirms the failing root refuses, a concurrently active unrelated root stays valid, and a later fresh boundary stays valid. `lifecycle-confirm.log` retains the result. Source inspection confirms registration/observation use the registry lock and failure belongs to active states, rather than an unsafe last-ACTIVE reset.

The earlier stable in-root-symlink escape premise remains refuted by pre-analysis nonregular-input rejection. The new P1 is different: symlink entries under directories deliberately skipped by inventory are admitted after canonicalization erases their lexical exclusion.

## Exact-head tests and limits

Independently ran, locked/default features/Rust1.95/Linux, serialized against the retained target:

- `cargo test --locked -p ripr --lib fanout_`: 16 passed, 0 failed/ignored, 11,060 filtered out; 14 fanout controls plus 2 incidental matches.
- `cargo test --locked -p ripr --test cli_smoke native_fanout`: 10 passed, 0 failed/ignored, 532 filtered out.
- Six independent selected-root-alias/in-root-symlink CLI controls passed; independent lifecycle module harness passed its desired isolation assertions.
- Four old/new native exclusion observations and two old/new exact-module observations establish the new counterexample. Their harnesses exit0 when recording behavior; the two repaired native admissions violate the desired refusal, and are not counted as product passes.

The green existing/new author controls do not discriminate an alias below an excluded directory that points back into the captured tree. Earlier review/native/performance receipts remain bound to their original heads/binaries. No broad benchmarks, workspace/goldens, hosted checks, native Windows, fresh-registry/cargo-deny, release or downstream qualification were repeated. Check-fast/precommit/red-control results in the author handoff are reported author evidence, not independently rerun here.

## Source and integration identity

Candidate parent is reviewed `0ffd94561c92c6dbfbcc7efcfee59d6c15cc729e`. Four-path repair only: input_boundary, native_fanout CLI tests, original changelog and traceability. Original feature/other native presentation owners remain unchanged. AGENTS/review procedure/Draft-to-Ready policy did not change in this repair.

Actual advertised merge `c70847dc374dad04e93738b430a9ad4f6cb68ff7`, parents `1a572a6e9d1b931b472dc9cc9af40f8e30260571` / candidate, tree `f17ace60df7f3ebb8b2ecd47a8c21345d6cc3670`; independent merge-tree agrees. It includes landed #8180; zip9/zip9.0.3 remains incorporated. Full analysis/output and native check/fanout test blobs agree with the executed candidate. Tested basis is `27b74d26f0b7e35a08c614b29bb7e369e7b146fd`; later advertised-parent changes are help/docs/contributor test guidance and the separate task-first repair-message path. Current observed main `895e4a47...` is one pilot-test-only change beyond that advertised parent; it changes no relevant native implementation. No behind-only source update was attempted or required by this finding.

Thus integration identity is established; no execution as a hosted job checkout is claimed. The source-level P1, not the old zip8 failure or missing hosted check coverage, is the remaining pre-Ready blocker.

## Discussion and transitions

Permitted current thread read shows both original comments4291492127/4291492134 resolved (the latter outdated); this review did not resolve or reply to either and does not rely on their resolution as repair proof. The independent reproductions establish the repaired predicates. The adjacent new P1 requires separate owner repair/disposition and affected new-head review.

**Ready:** technical prerequisites remain blocked by the new verified source counterexample. Checks need not already pass before Ready; the distinction is unchanged.

**Merge:** independently still requires current REVIEW_READY, applicable required checks/protections/conversation requirements and actual hosted checkout identity. The prior Forbidden check-run pagination hold was preserved with no alternate-route retry. No Ready, merge, thread resolution, bot contact, archive retry, release/deploy, candidate edit, UB consumer/pin or #8004 work occurred. #8210 stays separate.

## Evidence and retention

Raw commands/results, repair diff, subject/blob audit, tiny exact-module drivers, fixture source and symlink relationships, native member/refusal payloads and digests are retained. The symlinks themselves are omitted from the Git evidence packet; `symlink-relationships.json` plus drivers preserve their targets/setup. Drivers are published as Markdown code blocks. Executables/build caches are omitted remotely, with identities recorded; distinct original6a and repaired490 native negative witnesses remain locally retained for owner reproduction. The repaired native witness is a real copy with SHA-256 `b9fac7f82935ff7c1fda8530e8a69d83565bf9c9d8c44edc2e77f94f91cdfb91`, not a release artifact. Additional storage stays inside the 10GiB budget with more than 10GiB free. All drivers are terminal; only the owned source review worktree is released after publication/readback.
