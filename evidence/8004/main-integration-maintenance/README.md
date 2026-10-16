# Owned-branch main integration for Draft #8004

Current phase: maintenance to refresh a material stale qualification subject; fresh independent new-head review remains parent-owned.

## Exact subjects and operation

- Old candidate `8e0444b1a62ead18584d40b18278c6e8716dc56c`, tree `da4222d49d37aa82662003e0b6ef4ca8715dbac3`, integrated basis `f2ba9522267e88dfb03e3006e83129dfd2a8bb72`.
- Landed main/canonical #8180 squash `d08c77d144c016078364fcc1fcf2fa593d21cefc`; dependency owner's exact head was `4b80f51a8616a94a5924fb28292253f054a92db4`.
- New candidate `36dbb87430ef858aa143b5fc3205b376b0caee11`, parents exactly old 8e / landed main d08, tree `024d04e46de6ac531ca7485d9b7b6b4e7dafc2e4`. Merge base with main is d08.
- Ordinary `git merge --no-ff` of landed main, no conflicts, no hand edits, no cherry-pick or independent dependency patch. Tree is byte-identical to already executed prospective composition `8ad1a1efc9fd3fe7e334944d19d97efbd5d4a950` from the earlier integration packet.

The merge brings 1,315 already-committed main paths into the old head; the PR delta against the new main basis remains 173 paths. Complete path inventories and raw commit metadata are retained. No source mutation occurred after the merge during validation.

## Ownership and protection

The existing published claim and PR continuation name Rook / #7415. Current head stayed at exact 8e across all pre-work remote reads; current source-writer handoffs record no active competing writer, no ownership-transfer/competing-writer claim was found in the current conversation, and no local worktree holds the candidate branch. The current parent delegation assigns this bounded maintenance within that owner lane; ownership was not reassigned.

Supported branch metadata explicitly reports `protected: false`. The two returned repository rulesets contain one branch rule, active only for `~DEFAULT_BRANCH`, and one tag rule. No ruleset change/bypass was made. The fetch wrapper rejected percent-encoded slash branch URLs and the direct branch-rules URL at argument validation; supported unencoded branch metadata and complete ruleset-list/detail reads supplied the relevant evidence. These validation errors are not repository permission denials.

Immediately before publication the expected remote head is rechecked. Publication is an ordinary non-force push; no divergent remote advance may be overwritten. Initial stale merge subject was `086e9012f13d6ed1bcdfdb26b89796c38404e5f9`, parents 97cc/8e, tree aa1da914, missing zip9. That actual old job's zip8 cargo-deny failure remains historical evidence.

## Refreshed local proof

All seven commands exit zero on exact 36db: cargo test --locked -p ripr --lib inherent (24); --lib source_snapshot (5); --lib module_shadow_snapshot (3); --test private_inherent_receiver (9); --test cli_help_hierarchy (20); --test returned_element_field_pin (10); cargo check --locked -p xtask. Seventy-one distinct tests pass, zero failed/ignored/empty subjects. Complete logs, exact commands and executable hashes are retained. No OOM occurred; cgroup memory-cache reclaim events are recorded.

Rust 1.95.0 / locked dependencies / default features / development-test debug0 / two build jobs / Rayon four. Task-local temporary target directories exist before execution. Tests use a dedicated retained Cargo target. The earlier prospective exact-tree five archive/inventory test passes remain applicable and are linked as reused proof, not falsely counted as new executions. No broad benchmark or full hosted matrix was duplicated.

## Review and qualification limits

The [independent 8e review](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5543295375) and [earlier bounded composition packet](https://github.com/EffortlessMetrics/ripr-swarm/tree/c13aaeab0f11177f25a3db78e497c6c1319515f3/evidence/8004/integration8e) retain their exact subjects. A [landed-basis comparison](https://github.com/EffortlessMetrics/ripr-swarm/blob/44dce1bc547cb782f2de947756cad003cbf2a7d7/evidence/8004/integration8e-landed-reconciliation.json) establishes the same tree before this ancestry maintenance.

**New head requires fresh independent review before Ready; parent arranges it.** Prior 8e source/control/performance evidence is useful retained evidence, not a fresh review of 36db. Per-query work, unique-key storage, approximately 15.225s sync64 compact latency, two censored 120s predecessor runs and two unrun dependent warms remain. No new performance or full no-regression clearance. Current merge ref verification is subject evidence; actual hosted jobs must later be checked for their own checkout. Required hosted qualification, native Windows, full-workspace runtime and released-bundle acceptance remain separate.

No PR merge, Ready, thread resolution, external bot contact, release/deploy, protections/credential/security change, destructive deletion or issue #8005 access/retry. Existing #8201 native-fanout ownership is untouched.

## Continuation

Existing owner remains Rook. Parent consumes new exact head/tree/base and refreshed local proof, independently reviews 36db, and then handles any separately authorized Ready transition. Preserve twelve unresolved threads and their established technical dispositions; do not equate unresolved state with a new source defect. No additional source repair is requested by this maintenance.

Task resources under `/workspace/integration8004/maintenance` and its evidence/target are retained for parent review and exact-tree replay. Inherited checkout and target remain untouched. All earlier failures/receipts remain intact.

## Publication result

Ordinary non-force source push succeeded from exact 8e to 36db. Remote PR readback at 05:56:50 UTC confirms head 36db, base d08, Draft true. New actual GitHub merge ref `4232b0459b58eb3283c9f9d1ee42faf690679446`, parents d08/36db, whole tree `024d04e46de6ac531ca7485d9b7b6b4e7dafc2e4`, is byte-identical to tested head. All five canonical zip9 blobs are included. The stale-material-subject pre-Ready prerequisite is cleared. Hosted job actual checkouts remain separate evidence; no fresh independent new-head review, Ready or required hosted qualification is claimed.

Named hosted security proof: new Security run 38858454779 passes cargo-deny job 116835091002 and dependency-review job 116835091181. The cargo-deny log explicitly records git log HEAD 4232b0459b58eb3283c9f9d1ee42faf690679446. This clears that known inherited gate on the corrected actual checkout, without claiming required product-runtime qualification.
