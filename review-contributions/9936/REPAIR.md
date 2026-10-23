# #9936 review contribution for canonical adoption

This branch carries portable source artifacts for the existing canonical PR #9983.
It is not a competing product PR. Do not merge these artifact files into main.

Exact source-only repair: 653b860a669df2bc1b0b763a442be69ea511382a
(source tree ca171fc42127e00344fe8e423b17f6abc3ab2dc8, identical to reviewed abcadfd89).
Exact composed source: 017df5f102194b398c9f9879df8becda9efdc45b
(tree 33e261a44efd26285496627dffdde12cc8b6e5a9).
Its ordered parents are the source-only repair above and main basis
f63a722a243bcd9574288ccf28ba1ec63f41d549.

The bundle preserves both exact commits. It requires existing prerequisites
83e84e320126fc56b8b892db0afbebdbf5f376fb (the prior review regression contribution)
and f63a722a243bcd9574288ccf28ba1ec63f41d549 (main).
After loading those prerequisites, verify and import with:

    git bundle verify 9936-masker-repair.bundle
    git fetch ./9936-masker-repair.bundle refs/heads/codex/review-9936-ready:refs/heads/review-9936-import
    git show --format=fuller --no-patch 017df5f102194b398c9f9879df8becda9efdc45b

The canonical owner can adopt that exact source commit into the existing branch
after checking its live head and custody. Original d6a932f8 and regression 83e84e3
remain ancestors. This contributor has not pushed the canonical branch.

The patch is an alternative content transfer against exact main basis f63a722a.
Applying it to an index loaded from that basis recreates tree 33e261a4.
It does not transfer commit ancestry; the bundle does.

SHA-256:
- bundle: 69293b09030fb671388d232a574801a9f2ad352cc41208989717c5ab303a1f12
- patch: 12d888ec331164117ca995a5d57f46c50aa721bdccb7c0c669081dbf65f266f0

The repair derives regex/JSX span masks from the existing OXC parser, preserves
native source kind on selected strong-credit scans, and keeps Unicode source
offsets through the existing byte adapter. The AST visitor is optional under
lang-typescript. Main's independent division-only lexical path is retained.
Context-free ambiguous snippets intentionally retain the existing fallback.

Observed validation:
- Source-only abcadfd89: cargo test --locked --offline -p ripr --lib typescript:
  1691 passed, 0 failed, 0 ignored, native exit 0.
- All 98 exact-helper/native syntax/native runtime write/no-write controls passed.
- Independent source review: 41 controls passed, including Unicode byte offsets.
- Independent integrated-source review: 41 normal/typed + 8 division compatibility
  controls passed; an explicit negative control preserves independence of the masks.
- Bundle verify and exact patch-application tree comparison passed.
- Exact integrated 017df5f1: 1729 TypeScript product tests passed, 0 failed,
  0 ignored, native exit 0. Frozen executable hash is recorded in validation-final.json.
- check-fast passed all 10 applicable gates; stable origin/main selector matched
  all 11 changed paths. Workspace Clippy passed with warnings denied.
- precommit passed on exact 017df5f1 and basis f63a722a. Goldens were explicitly
  skipped by its selector because no output-surface or golden file changed.
- Relocated frozen integration executable passed all 19 review_9936 tests.
- See validation-final.json and retained check-fast/precommit reports for scope.
  The earlier source-only result remains separate from this integration result.

Retained independent receipts:
- source receipt SHA-256: 7898d1a0a41d8e19302d2a8c08432f5f361c5250f8caccc1405e7d96a6b45980
- integration receipt: eae6e9c82a926b6b84a869e1b0e93c4d598cc15a1a3893753d6510b4bbc6229b
- exact 017df binding addendum: 7134d3effce3d00733e2237b8833023f803155000b1a4e1ecf5b52dd82fb5439

Current authoritative continuation and full limitations:
https://github.com/EffortlessMetrics/ripr-swarm/pull/9983#issuecomment-6369649390

Canonical adoption, joined current-head review, hosted merge-subject verification
and required CI remain pending. No required qualification or merge is requested by
publishing this review artifact branch.
