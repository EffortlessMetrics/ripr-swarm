# Golden Output Changes

## Pending — python_same_stem_sibling_owner_not_related (1)

Reason:
RIPR-SPEC-0028: add flat-layout fixture; a same-stem test that only calls a sibling owner is not related, so the owner no test references reads no_static_path

Command:
`cargo xtask goldens bless python_same_stem_sibling_owner_not_related --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_same_stem_sibling_owner_not_related (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless python_same_stem_sibling_owner_not_related --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_same_stem_sibling_owner_not_related (3)

Reason:
RIPR-SPEC-0028: the sibling-owner tests call discounted_total(5000) and discounted_total(20000), which agree on both sides of the amount >= DISCOUNT_THRESHOLD change and never sit on the amount == 10000 boundary where > and >= disagree, so the exact-value oracle reaches the owner but does not observe the changed comparison and exposure honestly drops from exposed to weakly_exposed; loyalty_price stays no_static_path.

Command:
`cargo xtask goldens bless python_same_stem_sibling_owner_not_related --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
