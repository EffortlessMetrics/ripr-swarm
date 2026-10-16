# Python owner-shadow false-oracle corpus

An independently authored, small adversarial corpus for one current defect: an imported production function can retain oracle credit after the test replaces its local binding. The test observes a fixed substitute payload, while RIPR reports the changed production return as `exposed`.

This branch contains evidence, inputs and replay tests. It is not a production repair or a proposed merge into main. No product source, policy, pins, workflows or settings changed. No source PR, Ready transition or hosted qualification was requested.

## Observed identity

- Repository: `EffortlessMetrics/ripr-swarm`.
- Executed clean source: `e00018da7d224bd13b4c4e3466d1e626ca0b15f6`.
- Observed version: `ripr 0.11.0 (e00018da7d224bd13b4c4e3466d1e626ca0b15f6)`.
- Executed CLI SHA-256: `a8b026b503edbc2d0dd3a0a462095eb8ebe8f27723fadeca95b5878a3e19810a`.
- Rust 1.95.0, default features, Linux x86_64; Python 3.12.14.
- One Cargo job and one Rust test thread; CLI invocations serial, one Rayon thread and a 30-second deadline each. Cargo commands had independent 600-second deadlines.
- Worktree doctor, independently resolved zero-path check-fast, locked `cargo check -p ripr --all-targets`, and locked source CLI build exited 0. Baseline logs and the actual compiler-artifact executable message are retained. An all-target compile is not test-suite execution.

[identity.json](identity.json) binds the executable actually copied and consumed. [executions.json](executions.json) records native statuses and their interpretation. The binary is retained locally by the task and is not uploaded or distributed. This is source-built CLI evidence, with no installed/package/release/Windows/hosted-CI equivalence claim.

## Independent expected results

`corpus.json` pins every expected class and native old/new result literally. Those labels were authored before observing the CLI output. Neither verifier computes expected values with RIPR.

The production mutation changes only `return amount * 2` to `return amount * 3`; the negative-input exception is unchanged. All tests expect literal 30 where they read a value. The real owner at input 10 returns 20 before and 30 after. A local function, lambda or `Mock(return_value=30)` returns 30 in both versions, independent of the production change. The exception-only test uses input -1, which never reaches the changed return.

[verify_native.py](verify_native.py) directly executes all 11 authored test bodies against both source versions in isolated module namespaces. It checks not only pass/fail but `AssertionError` for every expected failure. Twenty-two executions match: five real-result controls reject the old value; all changed-value tests pass. These are authored native reductions, not an untouched upstream suite or pytest collection run.

[verify_cli.py](verify_cli.py) invokes the real CLI for every case, parses complete JSON stdout, requires successful/complete analysis and exactly one intended return-value probe/finding for `python:src/shop.py::fee` at line 4, then compares the class to the literal expectation. No empty subject, setup failure or missing output can pass.

| Case | Literal expected class | Observed class |
| --- | --- | --- |
| direct_literal | exposed | exposed |
| local_def | weakly_exposed | exposed |
| local_lambda | weakly_exposed | exposed |
| mock_payload | weakly_exposed | exposed |
| alias_mock_payload | weakly_exposed | exposed |
| exception_only | weakly_exposed | weakly_exposed |
| unrelated_local_def | exposed | exposed |
| bare_shadow_with_other_alias | weakly_exposed | exposed |
| real_alias | exposed | exposed |
| local_import | exposed | exposed |
| nested_helper_local_shadow | exposed | exposed |

The CLI verifier exits **1** because exactly five shadowed-owner cases receive false `exposed` credit. The exception-only control stays `weakly_exposed`; genuine direct/aliased/body-import calls and unrelated/nested helper controls stay `exposed`. Each underlying CLI command exits 0 with a nonempty complete result. [observed-receipt.json](observed-receipt.json) and `cases/*/observed.stdout.json` retain the raw evidence. This is no population accuracy estimate.

## Replay

From this evidence branch, with Python 3.12 and an independently supplied RIPR executable:

```bash
python3 verify_native.py corpus.json replay-native.json
python3 verify_cli.py /absolute/path/to/ripr replay
```

The first command should exit 0. At the recorded source/executable, the second must exit 1 on the five named mismatches. For a repair that retains the existing syntactic related-test relation, all eleven literal static expectations should pass without changing those labels. A separately justified relation-layer correction to `no_static_path` also refuses false promotion and requires an explicit corpus-contract revision; it does not justify blessing the current `exposed` results. Every CLI child has a 30-second deadline. Source build recipe at the recorded source:

```bash
CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1 cargo build --locked -p ripr --bin ripr --message-format=json
```

Use the executable from that build's compiler-artifact message, then record its actual hash/version. Do not substitute this receipt for a different executable.

## Owning production path and repair boundary

At the recorded source:

- [`strong_test_imports_owner_from_module`](https://github.com/EffortlessMetrics/ripr-swarm/blob/e00018da7d224bd13b4c4e3466d1e626ca0b15f6/crates/ripr/src/analysis/language/python/related_tests.rs#L1287) accepts an import without excluding a local substitute, distinguishing the imported alias from a different bare local, or matching the related row's line.
- [`sink_alignment.rs`](https://github.com/EffortlessMetrics/ripr-swarm/blob/e00018da7d224bd13b4c4e3466d1e626ca0b15f6/crates/ripr/src/analysis/language/python/sink_alignment.rs#L606) pools aliases across tests and can credit token presence without retaining the asserting test's binding/receiver identity.
- The direct/import-alias/changed-sink branches consume these identity facts. The selected observation is one source-to-sink identity defect; the corpus does not authorize a blanket IR rewrite.

Independent read-only design review challenged the straightforward text guard. The `nested_helper_local_shadow` positive is essential: only an uncalled helper's local shadows `fee`, while the outer assertion still reads the real imported function. A whole-body text veto would create a false gap there. The real alias and body-import positives also prevent solving the negative cases by disabling imports wholesale.

The review identified additional source-supported leads: same-file duplicate test names need complete row identity, token-only alias matching can observe an unrelated receiver, and one existing synthetic alias test supplies mismatched related/source test names. These leads were not separately executed through the analyzer in this pass and are not advertised as additional verified CLI defects. The eventual source owner should audit sibling credit paths, preserve scope-aware binding and ordinary imports, add the confirmed negative to the shared honesty corpus, and obtain exact-head review before requesting expected-green qualification.

## Ownership and state

Current native GitHub issue/PR searches preceded selection; [ownership-search.json](ownership-search.json) retains results and limits. No exact active local-shadow repair was found in those bounded searches; this is the author selection judgment, with unknown search state/assignee fields and possible truncation retained explicitly. [Final query arguments and source-premise check](publication-selection.json) are included. The active None-message repair under [#7849](https://github.com/EffortlessMetrics/ripr-swarm/issues/7849) remains with its existing owner and unpublished candidate; this corpus does not duplicate that work. [#5574](https://github.com/EffortlessMetrics/ripr-swarm/issues/5574) is a per-row tri-state refactor with explicit classification invariance, not this corrective promotion claim.

Classification: **verified_current** for the five CLI false-credit reductions at the stated source. No production repair, full golden run, full test suite, hosted CI, merge, installed artifact, release or aggregate 0.11 readiness is claimed. Source repair and its honest regression graduation remain the next engineering transition.

## Independent evidence review

A read-only reviewer replayed exact evidence head `846dede69db91a8a43040c3e7803fb110b51a924` from a review-owned Git archive, verified all 93 original manifest hashes, independently matched the frozen binary hash/version, passed all 22 native controls, and reproduced the same eleven complete CLI subjects and five false credits. [Independent receipt](independent-review-receipt.json). No blocking evidence finding was emitted. This supports evidence publication and defect filing, with no source merge-readiness disposition. The successor metadata commit adds this receipt, exact query arguments/limits, and the relation-layer classification caveat; corpus, verifiers, inputs, native results and CLI results are unchanged.
