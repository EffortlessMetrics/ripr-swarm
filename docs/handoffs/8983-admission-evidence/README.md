# #8983 admission investigation

Verified on #8934 head `08fca17212fa9cdf8439911b6f180e23068f1b19`, source tree `687bbd46912a54ae2f5c74055c2e1ab95261b1b5`, base/main `fc3589098d5b9c9340861f65bbb2ecbf80d95c2f`. This is an investigation handoff, not a product candidate or merge review.

The #8927 takeover at https://github.com/EffortlessMetrics/ripr-swarm/issues/8927#issuecomment-6293808874 remains the production delivery owner of #8934. Its current review is repair-required (https://github.com/EffortlessMetrics/ripr-swarm/pull/8934#issuecomment-6297561924). No ownership takeover, candidate edit, hold removal, Ready or merge was performed here. All-state PR search for `8983` returned #8934 and an unrelated historical #5229; current #8983 had no comments or assignees when inspected. This task does not claim exclusive production ownership.

#8984 remains Draft at `6c5ec4a2d6bb62bf4ecbef23873b3c9a24dab155`, tree `3e6b9d41fc3923e886159a2595ed8d8287af01bd`. Only its exact aligned diff bytes are used here. Its fixture source at base and #8934 has the tail call `notifier.send(order_id)` on line18, so hunk start16 is authentic; old start15 manufactures CallDeletion. We did not integrate #8984 or change either owner's branch. Its unresolved generic return-guidance finding is separate.

## Executed observations

`authentic_nonempty_admission_matrix` through public `check_workspace`, JSON and Human renderers: 264 scenarios, 396 findings, exit0, 1 named integration test passed. Each row asserts analysis completeness, exact finding count/family inventory, exact expression and line, and nonempty related tests. This is an observation recorder with identity assertions, not a passing dead-credit acceptance test.

Two stimuli:

- Aligned inherited tail call: exactly SideEffect + ReturnValue, both at18, expression `notifier.send(order_id)`, zero CallDeletion.
- Numeric return: exactly ReturnValue at2, expression `base * 2`; correct `compute_score(4)=8`, wrong `base + 1` gives5.

For each stimulus: six macros (`assert_eq`, `assert`, `assert_ne`, `debug_assert_eq`, `debug_assert`, `debug_assert_ne`), bare/`std::`, eleven contexts. Six deeper dead contexts: block-return, loop-return, else-of-true, uncalled fn, impossible match arm, cfg-false. Extraction controls: plain after-return, if-false, uncalled closure. Live controls: direct and a direct assertion followed by dead sibling.

- SideEffect: all48 deeper-dead rows for assert_ne/debug_assert* wrongly credit (debug_assert_eq exposed, others weakly_exposed). Both spellings' direct/mixed controls retain their original credit. assert_eq/assert refuse all six deeper contexts. Plain std::assert direct/mixed is already refused on this #8934 head: existing qualified-assert regression, not fixed here.
- The tail fixture's ReturnValue also receives weak payload-observer credit, including all48 assert_ne/debug_assert* deeper-dead rows and12 assert rows. Those are observation_unverified proximity observations; they do not establish that any assertion observes the returned bool, which is invariant true for both payloads.
- Numeric ReturnValue: assert_ne wrongly credits all12 deeper-dead rows and credits direct/mixed weakly. Every debug_assert* row is reachable_unrevealed/Observe=no, including all direct/mixed positives. Therefore broad return-family debug applicability is not established. This subject independently confirms return-value assert_ne applicability, rather than relying on the tail fixture's payload observer.
- Numeric ReturnValue additionally shows12 dead plain/std::assert boolean-equality rows weakly_exposed, while bare direct/mixed is exposed and qualified direct/mixed weakly_exposed. Treat this as a concrete adjacent acceptance decision, not an assumption about every return family.
- All extraction controls remain uncredited for every tested macro/subject. Total observed dead-credit findings132, split SideEffect48, tail ReturnValue60, numeric ReturnValue24. No genuine standalone CallDeletion, ErrorPath, Predicate, MatchArm, core/drop-in/shadowing, release profile or cross-platform claim is made.

Native semantic controls: Rust1.95, `rustc --edition=2024 --test -C debug-assertions=yes`. Four binaries (two subjects × correct/wrong), each lists12 tests;48 exact single-test executions have expected outcomes. Bare assert_ne/debug_assert_eq/debug_assert/debug_assert_ne direct assertions reject wrong production; block-return and uncalled-fn assertions pass wrong production. Correct production passes all. Debug-disabled/release behavior was not tested. See runtime receipt, sources and raw stdout/stderr.

## Reproduce

In a disposable checkout of the exact #8934 head, copy `issue8983_probe.rs` into `crates/ripr/tests/`, create `target` (repository TMPDIR points there), create an evidence directory, and copy `aligned-effect.patch` into it:

```bash
PATH=/workspace/.cargo/bin:/usr/bin:/bin \
RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo \
CARGO_TARGET_DIR=/absolute/owned-target CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 \
CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 \
ISSUE8983_EVIDENCE=/absolute/evidence \
cargo test --locked -p ripr --no-default-features --features lang-rust \
  --test issue8983_probe -- --nocapture
```

Paths above describe this configured environment; use actual configured local toolchain locations elsewhere. To execute native fixture semantics, compile `runtime_driver.rs` with Rust2024, then run it with the matrix evidence root argument. It writes native sources/binaries/raw output under the matrix root's parent/runtime. Run it in an owned output parent.

Configuration hashes: Cargo.lock `08706615a21ea0f32a3a8ebfee7daf4bf447843145b5f13b5cc679ac3b4b3a2d`; rust-toolchain.toml `48e029bb0b480b4b3bab15cb3de843dbfd6a7b9497163cc79c3af1d3026440bb`; .cargo/config.toml `e5bdb9055b7cdd689030fca3b94b244cca1da9c0cc8142e9180cd8c95c18e341`. Baseline probe source SHA256 `10364bce378d26c5c2ba39478982ae513dbc031ebb60b76232026e06caaf8e3e`; captured binary `565489f8e8895012de533cbcb1dfff9b10c35c51b5b818f542bdd5e882cbc19e`. Raw matrix SHA256 `9ab46dfa8d7ec2d24e26459a998179288972b704b332f41adb70d687344ff00d`.

No default-feature/full-workspace/hosted qualification, existing #8934 suite replay, proposed fix acceptance or merge clearance is claimed. lang-rust-only build emits inherited unused warnings; build and behavioral execution completed. An early unconfigured rustup invocation failed before compilation on read-only /home/agent/.rustup; configured RUSTUP_HOME/CARGO_HOME succeeded. An initial TSV-only formatter used a nonexistent nested JSON location key; the raw matrix was unaffected and the formatter was corrected to probe.line (396 complete rows).

Full local evidence, original and experiment executable witnesses and owned generated state are retained at /workspace/issue8983-evidence, /workspace/issue8983-target and /workspace/issue8983-inspect for the accepting analyzer owner. Existing shared target and primary checkout were not changed. No deletions performed.

## Discriminating admission-only experiment and scope recommendation

The four-line experiment in `admission-only.patch` adds only the four missing macro names to `admits_equality_assertion`, for ReturnValue/SideEffect/CallDeletion. This is a diagnostic control, not a proposed patch. Same harness/configuration and264 scenarios/396 authentic findings: exit0. It changes144 findings:108 dead credits disappear, but36 direct/mixed live credits also disappear. All48 SideEffect dead rows become uncredited; all16 formerly credited SideEffect direct/mixed rows for those four macros also become uncredited. Numeric assert_ne's12 dead rows and4 direct/mixed rows likewise lose credit. The numeric debug macros were already uncredited and remain so. The24 adjacent plain/std::assert dead ReturnValue observations remain.

Bare direct assertions become static_unknown with `could not identify the test function uniquely`; qualified direct assertions become reachable_unrevealed with `std::<macro>!` opaque-expansion refusal. This is an executed compatibility counterexample to fixing only the outer dispatch. At the exact tested revision, `analysis/syntax/owner_pin.rs` around2008 collects only assert_eq/assert candidates; `names_candidate_assertion` around3698 and `is_standard_qualified_macro` around3714 accept only standard-qualified assert_eq, and classifier `StdRootedCalls` / root proof in `classify/owner_pin.rs` also specializes in assert_eq. Both the genuine candidate collector and qualified-root/escape authority must be considered by the owning fix; do not bypass binding/root safety or inflate return-pin applicability to make positives pass.

Recommended bounded acceptance for the current analyzer owner:

1. Use the aligned tail fixture only for genuine SideEffect+ReturnValue; retain exact nonempty family/count/line/expression setup pins. Keep standalone CallDeletion proof with #8934's owner and its authentic semicolon-terminated stimulus; this investigation provides none.
2. For SideEffect, route assert_ne/debug_assert_eq/debug_assert/debug_assert_ne through actual execution admission, with the six deeper-dead contexts refusing and both bare/std direct plus live-sibling controls retaining prior classes/oracles/strength. Retain extraction-negative controls and identity/binding/root refusal. Widening dispatch alone fails that acceptance.
3. Numeric ReturnValue assert_ne is independently reproduced and belongs in the scope decision. Debug return-family credit is not generally demonstrated: numeric positives do not credit; weak tail credits observe payload proximity. Keep the concrete plain/std::assert dead-return rows visible for an explicit owner disposition. Do not assert applicability to ErrorPath/Predicate/MatchArm or broad return families without their own nonempty reachable positives.
4. Keep execution admission separate from oracle strength and sink identity: no automatic debug-return pin or stronger inequality classification. Extend standing honesty corpus only with authentic independently pinned specimens, then focused removal/live-preservation proof and appropriate cache/schema/golden qualification through the existing owner.

The experiment was restored byte-for-byte to original `owner_pin.rs` (SHA256 `fa3c9dce9494520f61fb388b710ac9b636c4b210b665c1bb04881989ab8af64f`), and tracked production diff is empty. The captured original binary was then executed again on a fresh evidence root; this uses the retained original witness and avoids a redundant third build. See restored log and comparison receipt. No surgical experiment changes are submitted as production code.

Live owner reconciliation at handoff: #8934 head remained08fca172 and #8984 remained Draft6c5ec4a. The parallel isolated #8934/#8984 reviewer has now recorded authentic standalone/compact-line provenance failures and the inherited test's obsolete CallDeletion assumption at https://github.com/EffortlessMetrics/ripr-swarm/pull/8934#issuecomment-6298862872. This task's generated assertions are multiline statements; it does not establish compact wrapper/whole-line admission. Preserve that review lane and current writer. Its failure perspective reinforces that macro-name dispatch is insufficient; the next accepting production owner must reconcile both evidence sets in the same shared consumer, rather than creating two writers.

For durable storage, raw matrices, both Cargo logs, restored-run log, TSV,144-row experiment delta, resource receipt, native generated source/list/compile/test stdout/stderr are packaged in `raw-evidence.tar.gz` (SHA256 `c74fd81b60b9bd6259e3224945e635e79c8d0185dd5346e63eadc6a416ca4d8e`,119KiB). `tar -dzf` compared the archive byte content to the retained expanded files and exited0. Extract it into an owned scratch directory to inspect the raw evidence. Original executable witnesses remain local at the previously named evidence directory, with hashes in comparison-receipt.json; no rebuilt executable is substituted. The publication adds only this bounded evidence directory, with no production/test-suite change or competing PR.
