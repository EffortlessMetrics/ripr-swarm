# Golden Output Changes

## Pending — inherent_receiver_local_wrapper (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the new same-name local delegation positive and its native foreign-entry/last-hop controls; repository helper reach is independently pinned. Existing expectations are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_local_wrapper --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
