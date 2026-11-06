# Fixture Contracts

Fixtures are BDD-style mini workspaces used to prove analyzer behavior and
output contracts. They should be readable by humans and agents without needing
chat context.

Each fixture directory should have this shape:

```text
fixtures/<name>/
  SPEC.md
  input/
    Cargo.toml
    src/lib.rs
    tests/<name>.rs
  diff.patch
  expected/
    check.json
    human.txt
    context.json
    lsp-diagnostics.json
    github.txt
```

Required for every fixture:

- `SPEC.md`
- `diff.patch`
- `expected/check.json`

`SPEC.md` must include:

- `Spec: RIPR-SPEC-NNNN`
- `## Given`
- `## When`
- `## Then`
- `## Must Not`

A SPEC can declare the exact finding classes independently of generated output:

````markdown
```expected-classes
[["probe:src_lib.rs:predicate:c80557eb", "weakly_exposed"]]
```
````

Each pair is `[finding.id, classification]`; copy the anchor from the finding,
then choose the class from the reviewed fixture intent. The block covers the
entire finding set (use `[]` for no findings). Duplicate anchors, unknown classes,
missing/extra findings and class mismatches fail `check-fixture-contracts` and
`goldens check`. `goldens bless` validates fresh output before replacing any
expected files; change the authored expectation explicitly when the reviewed
behavior changes. Never generate a new expectation as part of reblessing.

Declarations are mandatory for `python_adversarial_*`, `*_negative*` and
`*_control*` fixtures. Other fixtures can opt in. `goldens check` prints the
with/without ledger per language; `policy/fixture_expected_classes.json` sets
minimum declared counts. Raise these floors as coverage grows. These declarations
pin reviewed classes; they do not independently establish analyzer correctness.
The hosted rebless workflow additionally refuses all `check.json` byte changes,
including class changes even when a SPEC is edited.

Optional expected outputs become required when the fixture claims that surface:

- human output: `expected/human.txt`
- agent context: `expected/context.json`
- LSP diagnostics: `expected/lsp-diagnostics.json`
- LSP code actions: `expected/lsp-code-actions.json`
- editor workflow projection: `expected/lsp-hover.md`,
  `expected/vscode-status.json`, and
  `expected/first-useful-action-status.json`
- GitHub annotations: `expected/github.txt`
- editor-agent loop: `expected/editor-agent-loop/`

Manifest-only fixture corpora can define their own checked shape when a
contract is not an executable diff workspace. `fixtures/editor_gap_cockpit`
uses nested cases with `expected/lsp-diagnostics.json`,
`expected/lsp-hover.md`, `expected/lsp-code-actions.json`,
`expected/vscode-status.json`, and `expected/gap-projection.json`; the shape is
validated by `cargo xtask check-fixture-contracts` and summarized by
`cargo xtask lsp-cockpit-report`. `fixtures/editor_first_pr_bridge` uses
nested cases with `expected/vscode-status.json`,
`expected/setup-diagnosis.md`, `expected/lsp-diagnostics.json`,
`expected/lsp-code-actions.json`, and `expected/first-pr-status.json` to pin
first-pr packet success and fail-closed states.
`fixtures/editor_adoption_assurance` uses nested cases with
`expected/vscode-status.json`, `expected/setup-diagnosis.md`,
`expected/lsp-diagnostics.json`, `expected/lsp-code-actions.json`,
`expected/first-pr-status.json`, and `expected/receipt-status.json` to pin
first-use compatibility, root, receipt, first-pr, and preview-adapter states.
`fixtures/editor_actionable_gap_queue` uses nested cases with
`expected/vscode-status.json`, `expected/lsp-code-actions.json`,
`expected/current-repair-packet.md`, `expected/repo-gap-map.md`, and
`expected/receipt-status.json` to pin the local actionable repair queue,
read-only repo orientation, receipt movement, and fail-closed queue states.
`fixtures/bun-ub-cross-language-dogfood` is a manifest-only corpus that records
advisory Bun Blob TypeScript witness receipts for the calibrated stable-byte
cross-language route.

Run:

```bash
cargo xtask check-fixture-contracts
cargo xtask fixtures
cargo xtask fixtures <name>
cargo xtask goldens check
cargo xtask goldens bless <name> --reason "RIPR-SPEC-NNNN: explain change"
```

`cargo xtask fixtures` and `cargo xtask goldens check` run `ripr check` against
each fixture workspace, write actual JSON and human outputs under
`target/ripr/fixtures/<name>/`, and compare stable expected files. The checked
surfaces are:

- `expected/check.json`
- `expected/human.txt`, when present

`cargo xtask goldens bless <name> --reason "..."` is the only command that
updates expected output. It requires an explicit reason and appends
`expected/CHANGELOG.md`. For wording changes to default human output, see the
bless path in [`docs/USER_TEXT_GLOSSARY.md`](../docs/USER_TEXT_GLOSSARY.md).

The current fixture baseline covers:

- primary behavior gaps: `boundary_gap`, `weak_error_oracle`, `snapshot_oracle`
- defaults-first adoption examples: `opaque_fixture_builder`
- negative/noise cases: `format_only_diff`, `comment_only_diff`,
  `import_only_diff`, `unrelated_test_mentions_token`
- strong-oracle controls: `strong_boundary_oracle`, `strong_error_oracle`
- metamorphic syntax variants: `boundary_gap_multiline_assert`,
  `boundary_gap_nested_tests`, `boundary_gap_reordered_tests`,
  `weak_error_oracle_assert_matches`
- editor/LSP workflow projection: `editor_lsp_workflow`
- editor gap cockpit projection: `editor_gap_cockpit`
- editor first-pr bridge projection: `editor_first_pr_bridge`
- editor adoption assurance projection: `editor_adoption_assurance`

For defaults-first adoption examples, see
[`EXAMPLE_CORPUS.md`](EXAMPLE_CORPUS.md). For calibration scenarios, see
[`CALIBRATION_CORPUS.md`](CALIBRATION_CORPUS.md). These indexes map existing
fixtures to proof-loop questions and bounded runtime artifacts without adding a
new fixture runner surface.
