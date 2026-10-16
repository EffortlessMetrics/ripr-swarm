# Bounded integration follow-through for Draft #8004

Exact candidate: `8e0444b1a62ead18584d40b18278c6e8716dc56c`, candidate tree `da4222d49d37aa82662003e0b6ef4ca8715dbac3`, source basis `f2ba9522267e88dfb03e3006e83129dfd2a8bb72`.

## Executed composition subjects

| Subject | Local commit | Tree | Parents |
|---|---|---|---|
| main + exact 8e | cdb813a86972b34c37ac769ac3a7b2202f7c0e2d | 4b98340bac3a8ce9dbe0317566f36426fe9d86c1 | fbc8f7e58f11f25a8196495dfb88e7b393a5f4c4, 8e0444b1a62ead18584d40b18278c6e8716dc56c |
| prospective main + exact 8e + canonical pending #8180 | 8ad1a1efc9fd3fe7e334944d19d97efbd5d4a950 | 024d04e46de6ac531ca7485d9b7b6b4e7dafc2e4 | cdb813a86972b34c37ac769ac3a7b2202f7c0e2d, 4b80f51a8616a94a5924fb28292253f054a92db4 |

Both merges are clean `git merge-tree --write-tree` compositions, materialized as isolated detached checkouts. These local commits were not pushed as source candidates. `subjects.json` records actual Git trees, parents and nine selected blobs per subject, including absent main snapshot files as null. The prospective delta is exactly canonical #8180's five files. All product, tools, toolchain and root manifest bytes are identical between the two compositions; the lock and unpublished xtask zip/API adaptation differ. The submitted candidate is unchanged.

The source basis-to-main delta includes returned-element field attribution and shared OwnerPinSyntax changes, as well as CLI/help/output work. Focused receiver/cache controls, returned-element integration and complete CLI-help target were chosen for those affected consumers. The later 97cc-to-fbc delta is CLI group help and pilot work; its changed-path inventory is retained. No native-fanout #8201 work was included.

## Main + 8e correctness results

All commands use Rust 1.95.0, `--locked`, default features, dedicated target directory, two build jobs, Rayon four; development/test debug symbols are disabled. No release or benchmark claim follows. Environment and actual executable hashes are retained.

| Command suffix after `cargo test --locked -p ripr` | Passed |
|---|---:|
| `--lib inherent` | 24 |
| `--lib source_snapshot` | 5 |
| `--lib module_shadow_snapshot` | 3 |
| `--test private_inherent_receiver` | 9 |
| `--test cli_help_hierarchy` | 20 |
| `--test returned_element_field_pin` | 10 |

All six commands exited zero. All 71 test names are distinct, all terminal summaries are nonempty, zero failures/ignored tests. Raw `corrected-*.log`, `corrected-checks.json` and `main8e-distinct-tests.json` retain the exact runs. The nine public receiver tests include genuine private/public positives, scope/comment/shadow negatives, and the executable identity matrix; the unit controls exercise full/compact owner evidence and snapshot replacement/key boundaries. This executes the previously unrun main+8e tree, without claiming the independent review's full 888-unit/189-report matrix was repeated.

Initial failed attempt is preserved: inherent filter exited 101 with 19 pass and five fixture-setup failures before assertions. Repository config forces temporary paths under checkout `target`; dedicated external Cargo output left that directory absent. Creating that task-local directory fixed setup, with unchanged source/tests. No OOM was observed. An invalid target name was present in the initial planned queue but never reached, then removed before continuation. Other collection/setup errors and the forbidden CLI read are in `instrument-notes.md`.

## Prospective dependency compatibility

`cargo test --locked -p xtask --bin xtask create_zip_archive_writes_flat_package_contents` passes one archive roundtrip test. `cargo test --locked -p xtask --bin xtask vsix_inventory_` passes four inventory refusal/bounds tests. Both exit zero, five distinct tests, zero failures/ignored; logs, exact executable hash and commands are retained in prospective-*.log/prospective-checks.json. No new CP437/non-UTF-8 or defensive name-error discrimination is claimed.

This is useful local compatibility evidence for the exact composed tree, not migration ownership, landing, cargo-deny clearance or hosted qualification. Local cargo-deny is not installed. No dependency source fix was authored or copied into the submitted candidate.

## Remote qualification boundary

Final recheck at 2026-10-16 05:26:39 UTC: main is still `fbc8f7e58f11f25a8196495dfb88e7b393a5f4c4`; #8004 remains Draft at exact 8e. Actual merge ref is still `086e9012f13d6ed1bcdfdb26b89796c38404e5f9`, parents `97cc171257fd76a1b3b9e79cc41ad949e0ecc861` / 8e, tree `aa1da91442e93efce823769a1951d4e63ccf63c9`. Hosted cargo-deny job [116804248064](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38848487488/job/116804248064) actually checked out that commit and failed yanked zip 8.6.0. Its failure is preserved, not rerun locally or relabeled as runtime qualification.

Canonical #8180 remains open at `4b80f51a8616a94a5924fb28292253f054a92db4`, with zip requirement 9 and lock 9.0.3. Existing owner retains it. Its cargo-deny job [116806740647](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38849304738/job/116806740647) passed on its own actual merge `c7e9556e47a1b0b9d2c4c98b9631f206c7d4cdea`, parents 97cc/4b80, tree `87f3faf0c6a2de61222daa0d4c343cae624dccc4`. Latest routed run [38851160920](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38851160920) and its result passed. Earlier cancelled run 38851128304/failed result remain in the 42/42 returned check records; no undifferentiated all-green claim. These are canonical-owner evidence, not proof of the prospective #8004 composition or all required merge gates.

## Review findings and retained limitations

[Independent exact-head review 5543295375](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5543295375), immutable packet `f975e98e2eecc4126c1aaca9846e934975e2266c/evidence/8004/independent-shadow8e` and readback `ce0fca6ecc08487b32112b2d8b5e2b19b26bc394` remain authoritative for fresh independent full correctness/performance review. This work adds local composition proof and current remote reconciliation, not another broad benchmark or independent source review.

All 13 returned current review conversations are reconciled individually in `thread-reconciliation.json` and `threads.md`: one already resolved, twelve unresolved, no states changed. Exact source and independently executed evidence support the correctness repairs; sparse sync-chain documentation is present. The two performance threads are partially addressed with residual cost retained. A fresh local OwnerPinSyntax default still exists; shared snapshot/answer/fragment facts provide recovery, not literal run-scoped threading or global workspace-scan clearance. Selected profile absence of UnitStructFacts work is not a global result. No finding is dismissed solely because it is outdated or automatically marked addressed.

Roughly 15.225-second compact sync64 single-file latency, the 3.75x single/split ratio, per-query AST/owner work, unique-key memory, two 120-second censored predecessor runs and two unrun dependent warms remain limitations. No uncensored predecessor compact median, general no-regression/full performance qualification, native Windows, full-workspace runtime or released-bundle acceptance is established.

## Delivery disposition

**LOCAL_COMPOSED_CORRECTNESS_SUPPORTED / WAITING_INTEGRATION_PROOF — keep Draft.** No new candidate source repair is requested by this bounded evidence.

Dependency/owner: canonical #8180's existing owner; pending landing and actual-subject reconciliation. Next authorized consumer action: parent/delivery owner consumes these receipts and current canonical-owner qualification; once that repair lands, inspect the resulting main and GitHub #8004 merge ref's actual commit/parents/tree and zip manifest/lock/API blobs. Refresh only proof affected by material deltas. A landing notification or matching hypothetical tree does not establish the hosted checkout. Required hosted qualification and any promotion remain separate authorized transitions. Later jobs must be checked for their actual checkout identity.

Task resources: `/workspace/integration8004/{main8e,prospective,publication}`, dedicated target, scripts and evidence are retained for parent follow-through/replay of these exact trees. The inherited `/workspace/ripr-swarm` checkout/target are untouched except ordinary shared Git object/ref additions for task worktrees. No deletion or unrelated process changes.

No candidate source push, dependency takeover/merge, Ready, merge, review-thread resolution, release/deploy, bot contact, security/credential change, issue #8005 access/retry or #8201 overlap.
