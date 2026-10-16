# #8229 bounded integration follow-up

Candidate head: `67dc5af9a15590fc8476d09217232831a0009952` (unchanged); advertised PR base: `e660a99480dc13a4ed93b131042d11506fd484b6`.

GitHub merge ref and advertised merge commit: `717e16559445806c75e0b13a483745da80504af9`, parents in order `[e660a99480dc13a4ed93b131042d11506fd484b6, 67dc5af9a15590fc8476d09217232831a0009952]`, tree `9bdd63a67209ab4a62a0b489e635a781b6e6327f`. Connected GitHub Git-object reads and local Git reads agree. Local `git merge-tree --write-tree <base> <head>` produces that exact tree, without conflicts. This is an advertised merge-subject verification, not proof of an Actions checkout.

The merge tree differs from the reviewed candidate tree `87daa9ea61cb717b743733e0734346702573e001` in 31 paths. The full delta and inventory are retained. Relevant changes annotate proximity-only matched oracle rows, label mixed-currentness hidden human bullets, and simplify first-run help. The disclosure producer, adapters, normalized selection predicate, analyzed helper and JSON projection remain byte-identical to the reviewed candidate; build inputs and workflows are unchanged. A clean textual composition alone was not accepted as proof: affected execution below ran on the actual merge commit.

## Observed composition proof

Linux x86_64, installed Rust/Cargo 1.95.0, default features. Detached review-owned checkout at the merge commit, clean before and after execution. Existing Cargo target was reused only after the prior run's consumers ended, first preserving its exact candidate executable separately. Default PATH omitted Cargo; execution used `/workspace/.cargo/bin`, CARGO_HOME `/workspace/.cargo`, RUSTUP_HOME `/workspace/.rustup` and CARGO_TARGET_DIR `/workspace/review-8229-target`. Its native version identifies the merge commit; executable SHA256 `b998a2157058dc71b5c86ae8308ace60122bafd29409dd3ac8fa322f29d6bd8d`.

- `cargo test --locked -p ripr --test cli_smoke partial_preview_disclosure`: native exit0, 2 passed, 532 filtered out.
- `cargo test --locked -p ripr --test cli_smoke unsupported_check_exit`: native exit0, 4 passed, 530 filtered out.
- External bounded native harness: native exit0, 36 invocations. Complete JSON bytes and human projections for Python, TypeScript and JavaScript, each enabled budgets1/2/3, disabled budgets1/3 and mixed malformed input. Raw streams, argv, budgets, config/diff bytes, hashes and literal assertions are retained. Nonempty Rust/preview subjects, counts/samples, omitted advisories, partial/non-consumable/gate-ineligible state, disabled guidance and usable-partial exit0 are checked. Malformed inputs retain the Rust finding, disclose unsupported_input and exit2. JavaScript budget2 asserts one selected .js count/sample and human singular wording; budget3 asserts both selected .js paths and plural wording. This is focused semantic proof, not a broad benchmark or the hosted required matrix.

## Review reconciliation

All three submitted review bodies and the complete returned timeline were inspected; zero inline review threads were returned. The independent exact-head reviews remain applicable to unchanged source/test/contract dimensions.

- The later review correctly observes no checked-in JavaScript variant in the new matrix. Classify this as a non-blocking regression-coverage suggestion, rather than a demonstrated source defect. JavaScript routes through LanguageId::TypeScript and the same selected-path predicate; the new independent composition-native controls now directly verify its count/sample behavior. No source/test change was made.
- The singular/plural wording concern is refuted by control flow: enabled budget1 expects no advisory; budget2 is the singular positive, budget3 the plural positive. Both negative literals protect omitted/disabled output. The spec ID and both symbols are present in traceability.
- The review's refutation mentions Rust not being enabled; that explanation is inaccurate. Rust is enabled in these tests. Rust paths never become preview advisory rows because detect_preview_advisories rejects non-preview languages. The selected-path advisory filter cannot delete Rust findings already produced upstream.
- The retained pre-repair red is valid historical evidence, while the known wrong-order shaping failure remains excluded from final proof. Provider proof-intent rejection is an instrument limitation, not an observed product test failure; independent native execution supplies the relevant oracle.
- CodeRabbit subsequently reports no actionable comments. Its inconclusive docstring percentage is advisory and does not demonstrate a contract defect. Gemini quota and expired/skipped review-provider output remain missing provider evidence, never clean review. No bot was contacted and no thread resolved.

## Ready decision and remaining boundary

The documented Draft-to-Ready handoff in `docs/PR_AUTOMATION.md` requires reviewed source, dispositioned material findings, affected local proof, and verified merge subject with reasonably expected green qualification. Those bounded prerequisites are now established for the observed candidate/base/merge tree; no blocking source, oracle or composition finding remains. Ready requests hosted qualification and does not require that qualification to have already completed.

Live read at task entry already showed Ready (`draft=false`) and unmerged; this task made no Ready transition. Existing Ready-triggered Routed Rust Small run `38864156917` is in progress, with GitHub Hosted Rust gates job `116853138097`. The seven-job inventory is retained. Do not re-toggle Ready or manually dispatch: that would duplicate/cancel current work. Wait for the existing required result and inspect its actual checkout commit/parents/tree from logs before treating it as hosted proof. Overall merge-readiness disposition remains REVIEW_INCOMPLETE / WAITING_REQUIRED_CHECKS. No merge authority or REVIEW_READY is claimed here.

Live main is now `1a572a6e9d1b931b472dc9cc9af40f8e30260571`, while the PR still advertises the earlier base and merge above. The four-commit/14-path later-main delta was inspected (retained `later-main.json`): documentation, executable-index fixture setup batching and an ineligible-repair next-step hint. It does not change the disclosure producer, adapters, render path used by check, Cargo inputs or workflows, and introduces no identified material prerequisite for #8210. That unrelated movement does not require branch restacking or invalidate this bounded composition proof. Current-main composition itself is not qualified by this run.

A connected read of `/repos/EffortlessMetrics/ripr-swarm/rules/branches/main` returned HTTP400 INVALID_ARGUMENT (endpoint not allowed by the fetch wrapper). No route/credential bypass or retry occurred. Full current protection policy is therefore unestablished here; that read gap concerns merge convergence, not the documented Ready prerequisite judgment. No automatic approval review rejected an action.

## Resources and continuation

Source checkout was removed after all owned consumers ended and clean status was verified. Common Git store/original checkout remain untouched. Merge build state/binary, prior candidate executable (hash `8c8fd43cd63323e92f65440a2264ef0793a6fc307c617b83f1d1a49cc4ac9b9c`) and raw evidence remain retained for review/recovery. Started with 26 GiB free; 10 GiB reserve maintained, 24 GiB free at terminal execution. No source changes, Ready/merge/thread transition, broad benchmarks, manual dispatch, release/deploy or security changes occurred. #8201 retains fanout ownership.

Next owner: parent delivery thread. Consume the current required run, verify its actual checkout and terminal gate results, then perform the authorized final review/merge decision separately. No source repair or Ready re-toggle is indicated by this evidence.
