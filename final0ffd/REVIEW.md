# Fresh independent review of integrated #8201 head 0ffd9456

Assigned exact-head source/oracle/integration review: **COMPLETE, with a verified blocking source finding**. Final disposition: **REPAIR_REQUIRED**. The earlier pre-Ready assessment in commentary and evidence commit `422a558ed1ffcb0a358fadd8f5ef19e89f55f852` is withdrawn: the final conversation refresh returned two newly posted substantive threads, and independent counterexamples below changed the judgment. No Ready transition occurred.

## Verified findings and late-thread reconciliation

### [P1] A symlink-selected root is falsely refused by fan-out

Status: verified_current at exact `0ffd9456...`. Existing thread/comment `4291492134` concerns `analysis/input_boundary.rs` lines55-81. Physical selected root fan-out exits0; the identical stable root through a directory symlink exits2 with the outside-root/Git/target diagnostic; standalone JSON through that alias exits0. All fixture files below the physical root are regular and unchanged. The selected root is canonicalized successfully by capture, but observed paths use `std::path::absolute`, retaining the alias. Comparing that lexical spelling with the canonical root falsely marks an ordinary in-root read escaped.

Why material: the new native output-set adapter cannot handle an otherwise valid native selected root and gives a false outside-root explanation. Neither help/schema explicitly excludes a symlink spelling of the selected root; the regular-file inventory below that selected root succeeds. This is a fail-closed compatibility/correctness defect, not an observed outside-root read or false-success admission.

Fix direction: reconcile requested/canonical root and observed-file identities at their existing boundary owner; for absent probes preserve identity using a resolved existing prefix, or consistently rebase reads to the captured canonical selected root. Preserve the existing nonregular-input and excluded-directory refusals. Add a real native CLI root-alias positive plus physical-root and outside/excluded negative controls; rerun focused fanout units/CLI parity after the source-owner repair.

Validation: `symlink-experiment/receipt.json` and raw streams retain three real exact-head CLI executions, physical fanout0 / alias fanout2 / alias standalone0. Confidence: high. This blocks the pre-Ready source-review predicate until repair or an explicit evidence-backed supported-scope decision by the owner.

The same bot comment's reverse, stable in-root-symlink escape premise is **refuted within the documented stable-filesystem boundary**: independently adding an in-root symlink to an external regular file causes custody capture to refuse before analysis with `requires regular input files`, exit2. This covers the existing symlink case; hostile concurrent replacement remains outside the contract. The finding is the root-alias false refusal, not a demonstrated custody escape.

### [P2] Process-wide FAILED remains sticky across internal boundary lifecycles

Status: confirmed helper behavior; current external product impact bounded. Existing comment `4291492127` concerns lines8-10. A tiny Rust harness imports the exact unchanged input_boundary module, creates a boundary, removes its owned empty root and observes a failed read, drops that boundary, then creates a fresh boundary on a separate stable root. Its verify still returns the old failure because FAILED is process-wide and never cleared. The harness exits0 only because it asserts this unwanted observed Err; it is a defect witness, not a product pass.

Fix direction: scope failure to the active State/root, or synchronize registration/failure/lifecycle consistently; do not introduce an unsynchronized last-ACTIVE reset. Confidence: high for internal state behavior. The boundary is currently instantiated only in the private CLI adapter; no public repeated-command-in-one-process route was established. This is a nonblocking lifecycle hardening concern for the current one-command CLI surface, independently of the P1 above. Owner disposition of the still-open thread remains required before merge; this review does not resolve it or contact bots.

Both comments appeared in the completion-time thread refresh after the initial empty inventory. Their text was treated as leads; the findings/refutation above come from current source and independent execution. The earlier empty-thread observation and tests remain accurate for their original times/subjects, but do not clear these counterexamples.

## Exact subject and preservation

Published candidate `0ffd94561c92c6dbfbcc7efcfee59d6c15cc729e`, tree `9ccd7607ad39fb4d9650f4afed523130064ded91`; tested integration basis `27b74d26f0b7e35a08c614b29bb7e369e7b146fd`. Permitted PR metadata plus Git fetch/ls-remote agree on the requested candidate. It descends from independently reviewed `6a47f5bf094e5b010845b544f9464333a8b90d7d` through two ordinary merge commits; no rebase/replacement or independent migration is inferred or introduced.

First integration commit `0bcd78c904e7eba53a5cb0e8bd857180aa56db49` merges the original candidate with landed #8180 main `d08c77d144c016078364fcc1fcf2fa593d21cefc`; tree `d07c837e4ffbff4cd21a2976dcefad4c2df0549b` matches the previously independently executed local composition. The final candidate's parents are that first merge and tested main `27b74d26...`; independent merge-tree recomputation yields the exact final tree. Against tested main, the feature delta remains 15 files, 1,729 additions and 37 deletions.

Original/current Git blobs agree for AGENTS.md, review-pr procedure, committed-source/input-boundary/module-graph guards, analysis mod/pipeline, check dispatch, check_fanout, check help, CLI smoke registration and native_fanout test source. The original native pipeline count, custody, publication and parity oracles were preserved. Original independent experiments and negative witnesses remain evidence at their original head/binary, not relabeled as executions at this head.

The meaningful later-main change is landed #8161: shared related-test no-call-path annotations and human hidden-bullet currentness labels. Inspected its production/test diff, followed values through existing native projections, and selected exact-head controls below. This affects native bytes, so the final-head parity rerun is necessary. No #8004 classifier/cache work or #8210 repair was attempted.

## Actual advertised merge

Git fetch/ls-remote and local object inspection bind advertised merge `d10fe03915af68dd3f993e3f6601905680164853`, parents current observed main `e660a99480dc13a4ed93b131042d11506fd484b6` / candidate `0ffd9456...`, tree `9d36aee6f1db2b780a3fced6c300a20e5245e1f1`. Independent merge-tree recomputation agrees. Landed #8180 is an ancestor; remote merge's zip9 declaration and zip9.0.3 lock agree with current main. The earlier stale-merge Ready hold is superseded for this subject.

The difference from tested base27b74 to current maine660 is #8164's first-run help/README/Quickstart/guide tests, inspected separately. Full analysis/output subtrees and check/fanout command/test blobs agree between the executed candidate and actual advertised merge. No native fanout logic differs across that remaining main advance. The advertised Git object is established; execution as an actual hosted job checkout is not.

## Independently executed final-head proof

All commands below exited 0 on exact published head `0ffd94561c92c6dbfbcc7efcfee59d6c15cc729e`:

| Command/filter | Passed | Other subjects |
| --- | ---: | --- |
| `cargo test --locked -p ripr --lib fanout_` | 14 | 12 fanout controls + 2 incidental existing matches; 11,060 filtered out |
| `cargo test --locked -p ripr --lib analysis::classifier::finding::tests::exposed_` | 2 | 11,072 filtered out |
| `cargo test --locked -p ripr --lib hidden_bullets_` | 2 | 11,072 filtered out |
| `cargo test --locked -p ripr --lib hidden_block_selected_mixed_omitted_names_base_side_count` | 1 | 11,073 filtered out |
| `cargo test --locked -p ripr --test cli_smoke native_fanout` | 9 | 532 filtered out |

Thus 19 unit executions and 9 native CLI controls passed, zero failed/ignored. Actual one-pipeline-entry counter, custody/date/facts/drift/exclusion/caps/partial-write admission were rerun at the final head; standalone byte parity/config/Python navigation/source stamps/limited JSON/refusal/exit/destination controls were also rerun there. Shared landed annotation/currentness controls retain literal positive and negative row/category expectations. No assertion was changed to obtain a pass.

Environment: owned detached `/workspace/review-8201-final`; Linux x86_64, pinned Rust/Cargo 1.95.0, locked dependencies, default Rust/Python/TypeScript features, incremental off. Cargo operations serialized against retained `/workspace/ripr-swarm/target`; raw driver receipts bind commands, cwd, exact HEAD, elapsed time and exit status. Tests execute existing source without modification. Frozen original binary is retained separately; no duplicate final-head executable was frozen or promoted.

No broad benchmark, full workspace/golden, check-fast/precommit, fresh-registry/cargo-deny, Windows, release or downstream run is claimed. Prior timing results are not reused as new-head performance proof. Persistent drift detection remains an observation guarantee, not an immutable snapshot, hostile-filesystem transaction or crash-durability guarantee.

## Current substantive discussion

Read integration handoff6209316951, review submissions and substantive discussion. Completion-time refresh returned two new unresolved current-head threads (4291492127 and4291492134); independent adjudication is above. The original source/publication reviews and their earlier missing-qualification dispositions remain historical scope evidence. Earlier stale-subject hold6209166006/review5543487059 is superseded by the new actual merge containing #8180. The new P1 is a separate source predicate and is not hidden by that integration success.

Comment6209124827 coordinates #8210's existing standalone preview-advisory/partial-partition repair, explicitly separate from the fanout interface and its classifiers/exit/completeness semantics. This integrated candidate does not implement or alter that lane; its native adapter byte parity follows the shared existing projections. That coordination is not being treated as an unresolved candidate-introduced fanout defect, nor is this review claiming the standalone advisory issue repaired. Initial permitted inline-thread read returned no threads; the completion-time read returned the two current unresolved threads above. No threads were replied to or resolved. Empty threads are not used as a substitute for substantive review.

## Ready and merge judgments

**Pre-Ready prerequisites are not currently satisfied because the independently reproduced P1 source defect remains unresolved.** The new integration/remote-subject prerequisite is satisfied and all selected exact-head controls passed, but the root-alias counterexample fails. Required checks need not already pass before Ready; neither the historical cargo-deny failure nor the denied check inventory is being used as a blanket pre-Ready blocker. The source owner must repair the boundary or explicitly settle the supported-root contract, then obtain affected exact-new-head review. This reviewer makes no candidate edit or Ready transition.

Merge remains unqualified: REVIEW_READY plus applicable required checks, protection/conversation requirements and actual hosted checkout binding still govern merge. The configured required context was named in parent coordination as `Ripr Rust Small Result`; this review does not independently claim its current result or an exhaustive required-check inventory. The previously Forbidden paginated check-run operation was not retried through any route. Complete hosted coverage remains unavailable, not all-green. Expected Ready-triggered missing proof does not itself require keeping Draft after the technical prerequisites above are satisfied.

No source changes, Ready/merge, thread resolution, bot contact, release/deploy, denied archive retry, UB consumer/pin edit or PR-body mutation occurred. Candidate owner/parent retains qualification decisions. The assigned review is complete with REPAIR_REQUIRED; source repair belongs to the candidate owner. No release or adoption clearance follows.

## Retention

Evidence supplements the exclusively owned independent evidence branch. Source worktree is review-owned and released after terminal tests/publication; original frozen candidate evidence, all raw final-head logs and inherited mutable build state remain retained. Storage stays within the existing additional 10 GiB admission and 10 GiB free reserve; no inherited cleanup. Review/publication readback must verify the resulting evidence payloads and posted body before completion.
