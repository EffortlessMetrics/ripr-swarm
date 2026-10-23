# Native angle-control contribution for #9936

Canonical owner: claude/fix-9936-ts-masker; PR #9983. No competing product PR or canonical push.

Contribution a3b0e245646a41e183d0db6e7ee3e23f9c683c38
Tree 03d0ff4e4d6b3c088865d8df258a3a7373e3b279
Parent e04f482bb13f559e01f8216294ecd4a9a10acc9c

Only the new test's three string choices change (4 additions / 3 deletions). Production, source-kind wiring, division-only path and dependencies remain byte-identical to adopted017df/e04. The .ts cases preserve glued n</a`/ but compare against .source.length; .tsx separates the closing-tag token as n < /a`/.source.length. For-await stays unchanged.

The portable native-valid-test-correction.patch applies cleanly to e04. The base64 bundle decodes to native-angle-controls.bundle and imports the exact tested commit; prerequisite e04 must exist. Bundle SHA256 62944c98a6b7b8fcc33931cb252840afc815e3fc25de2dd692d2cc3d638d92c7; patch SHA256 7cf73ec1c43357ba3e4361a50a496afd55b8d18a10aca744668338f330d4164e. Verify the bundle before import. Canonical owner must reread live head before adoption; use normal expected-head publication and do not overwrite an advanced branch.

Product proof on exact a3: cargo test --locked --offline -p ripr --lib review_9936_comparison_regex_after_angle_shapes_keeps_the_write_visible -> 1 passed / 0 failed, 13729 filtered; all five cases executed. Driver49499 consumed terminal0. Changed-file rustfmt --check ->0. This is focused product proof, not a full suite or a canonical successor qualification.

Actual selected test executable: /workspace/9936-e04-feature-target/debug/deps/ripr-821a705ef8245a1a. Frozen once to /workspace/9936-evidence/native-angle-testbin, SHA256 c532ec0323b03cd6986ca065de9b5b547d84bda959937d1d68bc87ed7f2d7730, 289513336 bytes. Relocated /tmp execution passes the same single test. Clean observed build record names a3, Rust1.95.0. The original regression and017df witnesses remain separately retained.

Independent exact-a3 review SOURCE ACCEPTED, receipt SHA256 32022f0b70ccf2eeb211171746a6f1512782c0ac8ec561434bae0ff6a5440270. Five full fixtures pass native TypeScript5.9.3 and actual producer/byte-wrapper/strict-write controls; all five reject obsolete bdd0. Eight direct fallback/context controls pass, including Unicode offsets, all-prefix rejection and generic TS/TSX ambiguity. The independent receipt predates driver completion; native-test-product-result.json and relocated log supply the terminal result afterward.

Feature-only failures classified: TS-only and Python-only profiles both exit101 on e04, exact basef63 and current main5950. The entire lib.rs blob is identical and explicitly requires lang-rust. These are inherited unsupported no-lang-rust profiles; no masker repair warranted. Full raw logs retained locally and indexed by SHA256.

Source-backed review disposition: module-spy reads owner.file, so owner.file SourceType is correct. Strong full-file admission/import/pruning/spy paths retain explicit type wiring. masked_owner_body remains a generic compatibility path. Generic plan_for accepts agreeing plans if both parses succeed or the unique successful parse; two failures try the next wrapper, disagreement stops with fallback. Preparation is trigger- and parser-success-dependent. No universal parse/typing guarantee is claimed. No additional material defect established by automated questions.

Integration planning only: main5950 versus basisf63 changes247 paths; e04 versus f63 changes11; overlap empty. merge-tree is clean, tree17a962da371d92b379b34411e3d0a65365b4d727. The inspected remote subject e3c3d7a uses older main6fd671b5; re-observe actual required checkout/combined tree at qualification. Behind-only movement does not request restacking.

Pending owner action: adopt this test-only contribution into canonical #9983, bind review to the actual adopted successor, capture the completed PR body and run offline cargo xtask check-pr-shape, then normal Ready/merge-ready required Rust qualification. After all current checks/reviews/holds permit, normal protected expected-head squash and main ancestry verification. PR remains Draft and no required qualification, merge, release, deployment or access change was requested here.

Resource ownership: root review-9936 owns the isolated feature worktrees/private target, bundle, logs and this frozen witness. Both command drivers19980 and49499 are terminal0; no Cargo/CI duplication. Source/evidence and all original negative/current witnesses are retained. The private mutable target is retained for the pending adopted-head review under the existing6GiB budget and12GiB reserve; it is not frozen evidence. Preserve the denied classic-protection read and denied evidence-deletion operations without retries.

