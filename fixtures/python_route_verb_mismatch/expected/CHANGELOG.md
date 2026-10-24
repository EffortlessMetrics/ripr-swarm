# Golden Output Changes

## Pending — python_route_verb_mismatch (1)

Reason:
RIPR-SPEC-0028: new fixture pinning that a client call with a different HTTP verb does not relate to a route decorator (#9793)

Command:
`cargo xtask goldens bless python_route_verb_mismatch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_route_verb_mismatch (2)

Reason:
RIPR-SPEC-0028: diff hunk header now matches the fixture file so the change lands on the dict-field return (#9793)

Command:
`cargo xtask goldens bless python_route_verb_mismatch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
