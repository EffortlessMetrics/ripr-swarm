# Golden Output Changes

## Pending — owner_return_pin_identity_traps (1)

Reason:
RIPR-SPEC-0197: initial golden for owner-return pins (#4478)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (2)

Reason:
RIPR-SPEC-0122 / #4520: bounded human check output leads the exposure line with the plain word the summary uses

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (3)

Reason:
RIPR-SPEC-0197 (#4478) acceptance fixture re-rendered in main's format: every identity trap stays non-exposed; per-finding confidence tracks main's current scoring, the 0-exposed contract is unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (4)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — owner_return_pin_identity_traps (5)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (6)

Reason:
RIPR-SPEC-0224, #5508: observation_unconfirmed now reads 'ripr could not confirm that this assertion observes the changed behavior', an unknown rather than a claim that the assertion misses. The miss token and every decision field are unchanged.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (7)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (9)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (10)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (11)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (12)

Reason:
RIPR-SPEC-0003: #7477 gives reason-specific uncertainty-resolution actions without verdict-change promises or unsupported deep/fixture-config routes; classifications, evidence, and decidable guidance unchanged.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (13)

Reason:
RIPR-SPEC-0003: #7477 conditional assertion inspection and actual configured runner guidance; whole JSON evidence and classifications independently preserved.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (14)

Reason:
RIPR-SPEC-0001: #7593 pure proximity reach prompts inspection before assertion changes; exposure evidence is unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (15)

Reason:
RIPR-SPEC-0259: Next step for error_path/return_value weakly_exposed findings names the missing input when the strongest related oracle is already exact (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (16)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (17)

Reason:
RIPR-SPEC-0001: output contract changed - 7798 collapse tautological unknown-sink flow row when sink text restates the kind label

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (18)

Reason:
RIPR-SPEC-0108: #7642 matched oracle rows with a proximity-only relation now carry no_call_path on exposed and unknown classes so sibling tests no longer render as unqualified oracle evidence

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (19)

Reason:
RIPR-SPEC-0122: #7643 hidden-block bullets append each finding's currentness category when the omitted set mixes categories, connecting the count line's clauses to its rows

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (20)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (21)

Reason:
RIPR-SPEC-0122: #8389 Summary names base-side findings, Hidden total equals listed bullets, all-unknown footer no longer claims a found absence

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
