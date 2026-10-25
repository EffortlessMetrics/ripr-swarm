# Golden Output Changes

## Pending — python_route_rival_app (1)

Reason:
RIPR-SPEC-0238: new fixture pinning that a client over a same-named application from another module does not relate to the changed route owner (#10110)

Command:
`cargo xtask goldens bless python_route_rival_app --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
