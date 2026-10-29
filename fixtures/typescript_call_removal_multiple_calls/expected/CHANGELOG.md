# Golden Output Changes

## Pending — typescript_call_removal_multiple_calls (1)

Reason:
RIPR-SPEC-0243: pin independently reproduced counted-removal false credit as weakly_exposed (#10466)

Command:
`cargo xtask goldens bless typescript_call_removal_multiple_calls --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_call_removal_multiple_calls (2)

Reason:
RIPR-SPEC-0243: formatting-only refresh of human output to main's current summary wording; verdicts unchanged

Command:
`cargo xtask goldens bless typescript_call_removal_multiple_calls --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_call_removal_multiple_calls (3)

Reason:
RIPR-SPEC-0088: TypeScript check JSON findings state canonical_gap_id (#10709, #10919); fixture added by #10466 before it landed

Command:
`cargo xtask goldens bless typescript_call_removal_multiple_calls --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
