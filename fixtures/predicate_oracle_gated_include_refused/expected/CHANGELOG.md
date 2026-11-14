# Golden Output Changes

## Pending — predicate_oracle_gated_include_refused (1)

Reason:
RIPR-SPEC-0197: unknown-gated Module edge is admitted but the remaining include! edge keeps its IncludedFile refusal (#14537)

Command:
`cargo xtask goldens bless predicate_oracle_gated_include_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
