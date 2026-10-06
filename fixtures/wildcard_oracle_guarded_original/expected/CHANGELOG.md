# Golden Output Changes

## Pending — wildcard_oracle_guarded_original (19e43c73)

Reason:
Independent score(1)==2 original/wrong controls pin whole-wildcard
non-promotion and retained exact/guarded discrimination. Actual source-bound
fast-mode CLI output retains one finding and one related test; no runtime
execution or representative-project accuracy claim is made by the analyzer.

Command:
`ripr check --root fixtures/wildcard_oracle_guarded_original/input --diff fixtures/wildcard_oracle_guarded_original/diff.patch --mode fast --json`
Repeated directly for human and human-full output with distinct cold caches;
CLI binary SHA2563690bf6f1a6cd80e6bc7d9f1fb97b7c6a5686ae2da09ffb1daf2263c77bc927d.

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - canonical fixture argv and landed Rust guidance

Reason:
The fixture runner passes forward-slash root/diff arguments on every platform.
The original frozen Windows calls used backslash arguments; quoting preceded
path normalization and left unnecessary quotes in the human goldens. Only
those known safe command arguments are reprojected here; hostile argument
quoting and the existing normalization/comparison policy remain unchanged.
The two wildcard short reports also retain the landed #5355/#5423
"Write a test for it" route at src/lib.rs:2; exposed and full reports do not
gain that route. Finding IDs, weak/strong oracles, evidence and recommendation
prose are unchanged. Required run 37215568518 observed the 12 human drifts
after policy, Clippy and 12016 tests passed; its first-difference report and
exact combined source justify this narrow expectation, pending successor CI.
No blanket bless, class promotion or representative accuracy claim is made.

Updated:
- expected/human.txt
- expected/human-full.txt

## Pending — wildcard_oracle_guarded_original (1)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless wildcard_oracle_guarded_original --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — wildcard_oracle_guarded_original (2)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless wildcard_oracle_guarded_original --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — wildcard_oracle_guarded_original (3)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless wildcard_oracle_guarded_original --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — wildcard_oracle_guarded_original (4)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless wildcard_oracle_guarded_original --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
