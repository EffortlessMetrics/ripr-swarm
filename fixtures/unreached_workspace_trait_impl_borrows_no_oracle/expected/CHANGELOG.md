# Golden Output Changes

## Pending — unreached_workspace_trait_impl_borrows_no_oracle (1)

Reason:
RIPR-SPEC-0231: new fixture for a workspace trait impl no test reaches

Command:
`cargo xtask goldens bless unreached_workspace_trait_impl_borrows_no_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unreached_workspace_trait_impl_borrows_no_oracle (2)

Reason:
RIPR-SPEC-0108: add human-full golden for the unreached workspace trait impl fixture

Command:
`cargo xtask goldens bless unreached_workspace_trait_impl_borrows_no_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — unreached_workspace_trait_impl_borrows_no_oracle (3)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C); merge of origin/main into the #9169 candidate, formatting-only

Command:
`cargo xtask goldens bless unreached_workspace_trait_impl_borrows_no_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — unreached_workspace_trait_impl_borrows_no_oracle (4)

Reason:
RIPR-SPEC-0108 / RIPR-SPEC-0122: compose #9169 with main #9657; preserve both no_static_path findings and all machine/full-human evidence while adding main's bounded Evidence legend.

Operation:
Manual integration edit: inserted the existing main five-line bounded Evidence legend after the compact Evidence line. No classifications or machine/full-human outputs changed.

Updated:
- `expected/human.txt`

## Pending — unreached_workspace_trait_impl_borrows_no_oracle (5)

Reason:
RIPR-SPEC-0122: compose #9169 with main #9986's full-human note for a head-only match-arm after row. The two no_static_path findings and every machine/bounded-human evidence stage remain unchanged.

Operation:
Manual integration edit: added the one line emitted by main's existing renderer after `after:  None =>`. The official fixture comparison first rejected only the missing full-human note; JSON and bounded human outputs were byte-identical.

Updated:
- `expected/human-full.txt`
