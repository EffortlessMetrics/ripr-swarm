# Independent review of Draft #8229 / #8210

Reviewed repository: EffortlessMetrics/ripr-swarm. Exact head `67dc5af9a15590fc8476d09217232831a0009952`, tree `87daa9ea61cb717b743733e0734346702573e001`; candidate basis `d08c77d144c016078364fcc1fcf2fa593d21cefc`. Observed PR base `e660a99480dc13a4ed93b131042d11506fd484b6`. Review performed 2026-10-16 UTC on Linux x86_64, Rust/Cargo 1.95.0, default features, in an independent detached checkout. The submitted source was not edited.

## Disposition

No actionable source or contract findings emitted. The bounded native disclosure repair passes independent semantic checks. Overall disposition is **REVIEW_INCOMPLETE for merge readiness**, because this Draft has not entered the Ready-triggered protected qualification and current hosted work remains unfinished. This does not authorize Ready, merge, thread resolution, CI dispatch, publication or fanout qualification.

## Inspected surfaces and ownership

All seven changed files were inspected (retained `diff.patch`): pipeline producer, CLI module admission and both new tests, schema, RIPR-SPEC-0082, traceability and changelog. Additional inspection followed `partial_scope` from Rust adapter result through preview adapter dispatch, normalized `PartialDiffScope::selects`, `detect_preview_advisories`, `PreviewLanguageAdvisory::analyzed`, and the actual JSON and human consumers. Config admits TypeScript as the JavaScript-family adapter; routed JavaScript paths use that same authority. Existing exclusions and missing-source filtering remain downstream of the new partition check. Disabled routing keeps raw presence. Perl is explicitly excluded from the new source-partition filter; its supplied evidence semantics and limitations remain unchanged. No classifier, partition, public DTO, policy, process, network, feature, dependency or renderer-local authority changed.

Root AGENTS.md, `.agents/skills/review-pr/SKILL.md`, review guidelines, Rust/security/workflow rules and linked repository engineering/architecture/review/validation context were read. #8201 retains native fanout ownership; no fanout implementation or qualification is claimed.

## Independently observed native proof

`build.log` records `cargo build --locked -p ripr --bin ripr`, exit 0. `identity.json` binds source, tree and environment. `native-review.py` is a separate external harness, not a repository source change. Its final matrix has 24 native invocations, with full stdout/stderr, exact argv, budget, config and diff bytes, exits and hashes retained in `native-receipt.json`.

`partial-tests.log` independently records both checked-in partial_preview_disclosure tests passing (2 passed, 0 failed; 532 filtered out), executing their 20 CLI controls. `unsupported-tests.log` records all four unsupported_check_exit tests passing (4 passed, 0 failed; 530 filtered out). These filtered test runs are not a full suite.

For each of Python and TypeScript: enabled budgets 1/2/3 cover omitted / one selected / two selected preview files; disabled budgets 1/3 retain two detected files and enablement guidance; each runs both JSON and human. Assertions require real nonempty Rust subjects and the expected preview subjects, literal selected paths, counts and samples, raw input count 3, native exit 0, partial/non-consumable/gate-ineligible scope and human incomplete wording. Full enabled scope restores complete_with_findings and two analyzed preview paths. This rejects suppress-all-advisories, stale samples, raw-presence counts and false completeness alternatives.

Two additional mixed malformed inputs (one per preview language), both JSON and human, preserve the usable Rust finding while reporting unsupported_input, incomplete analysis and native exit 2. This supplies an alternate failure-path oracle beyond the new tests.

The retained original author red log was inspected: both omitted-preview assertions fail with analyzed=true, count 2 and unselected samples. It is author-produced historical proof, not a red run independently executed here. No mutation experiment modified source, honoring the no-source-changes limit. The final positive/disabled/unsupported controls independently challenge wrong-result alternatives.

## Reported evidence and hosted limits

The immutable author packet at `72c40767a7b3d20fe185e7d76ffb3c21fdc8d744/repair-8210/README.md` was read; its identity and 13 terminal receipt rows were inspected and retained with explicit author provenance. Those receipts bind exit 0 to the requested head for focused tests, check-fast/precommit/traceability and six fixtures. The 70-comparison claim was read, not independently repeated as a 70-case comparison.

`ci-observed.json` preserves all 23 returned check rows (total_count 23, requested per_page 100), each at the reviewed head. The CI job read confirms Rust-only feature testing succeeded; Perl/release proof was skipped. Windows advisory subset, wheel build and ub-review were in progress at observation. Skipped coverage/analytics/advisory jobs are not passes. Routed Rust Small is admitted on ready_for_review, not ordinary Draft activity. Neither local checks nor a green aggregate establish protected integration qualification. No Windows/default-preview hosted execution or merge-object checkout is qualified by this review.

## Currentness, residuals and resources

Production/test stimulus/test oracle/public contract/traceability/head dimensions were reviewed at this exact candidate. Current-main composition, complete required-check policy, full workspace/golden/dogfood proof, nondefault preview features, native Windows, Perl packet runtime, package/release and #8201 fanout proof remain outside the established result. No substantive open review concerns existed at entry; final state is re-read before publication.

Build target and native executable are retained locally under the identity's Cargo target directory for this receipt/recovery consumer; no binary is uploaded. Evidence and small fixture inputs are retained here. Review-owned materialized source is released after terminal tests and a clean-tree verification; the common Git store and original checkout remain untouched. Admission started with 30 GiB free, a 10 GiB reserve and 12 GiB estimated peak; observed usage remained below that allowance. No denied operation, credential change or security change was needed.

Next consumer: the parent can use this exact-head semantic review and the raw native matrix, then arrange any authorized integration/hosted qualification. Keep Draft under the current instruction.
