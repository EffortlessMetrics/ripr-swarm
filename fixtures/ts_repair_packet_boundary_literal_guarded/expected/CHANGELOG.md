# Golden Output Changes

## Pending — ts_repair_packet_boundary_literal_guarded (1)

Reason:
RIPR-SPEC-0087: guarded boundary stays non-delegatable (source challenge on #4429)

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_literal_guarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_repair_packet_boundary_literal_guarded (2)

Reason:
RIPR-SPEC-0046 RIPR-SPEC-0047: check JSON now carries a top-level source_subject stamp with the analysis-time content digests of the files a derived gap ledger names (#4544); no finding, classification, or human output changed.

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_literal_guarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
