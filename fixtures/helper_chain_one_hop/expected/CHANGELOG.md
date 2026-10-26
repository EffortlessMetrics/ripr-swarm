# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0159: new fixture pinning the bounded helper-transfer behavior (one-hop relation, two-hop chain, fail-closed controls)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0159: review repairs - method/path-qualified call sites refused, strict literal binding, chain stops at test functions, hop parameter binding fixed, controls diff realigned to the real changed lines

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0162: the propagation_unknown why-hint stops asserting the propagation the class marks unknown, and unknown-class limitation prose renders under the Analyzer limit label (human-only; the shared decision-layer text is unchanged)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (3)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (4)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (5)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (6)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/human.txt`

## Pending — helper_chain_one_hop (7)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — helper_chain_one_hop (8)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (9)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: review fix #5268 - quoted string/char literals are now byte-encoded in the Rust gap discriminator key so distinct predicates never share one identity; golden-drift.json shows zero semantic flips on the re-run

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (11)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (12)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (13)

Reason:
RIPR-SPEC-0001 unresolved boundary input (#6674, #6693): an operand ripr cannot map to related-test inputs reads infection_unknown 'Changed boundary input is unresolved' instead of a missing equality discriminator with 'observed values: unknown'; no exposed finding

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (14)

Reason:
RIPR-SPEC-0001: the human 'Why unknown' line for infection_unknown says 'reaches a sink' only when propagation is yes (CodeRabbit review on #6796); propagation is not yes here, so the line no longer claims a sink

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (15)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (16)

Reason:
RIPR-SPEC-0003: #7477 gives reason-specific uncertainty-resolution actions without verdict-change promises or unsupported deep/fixture-config routes; classifications, evidence, and decidable guidance unchanged.

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (17)

Reason:
RIPR-SPEC-0003: #7477 conditional assertion inspection and actual configured runner guidance; whole JSON evidence and classifications independently preserved.

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (18)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (19)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (20)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (21)

Reason:
RIPR-SPEC-0001: an unmapped local boundary operand and a neutral input-free fallback name the literal inputs that would settle the unknown (#9276)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (22)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (23)

Reason:
RIPR-SPEC-0122: infection_unknown next step names a concrete action (#9244 item 5); wording only, no classification change

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (24)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (25)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (26)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — helper_chain_one_hop (27)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (28)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (29)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (30)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_one_hop (31)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless helper_chain_one_hop --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
