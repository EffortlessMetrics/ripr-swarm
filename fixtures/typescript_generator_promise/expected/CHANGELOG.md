# Golden Output Changes

## Pending — typescript_generator_promise (1)

Reason:
RIPR-SPEC-0243: retain the dormant generator .then callback as a public golden for the independent evidence-promotion-honesty refusal (#8286); Node runtime controls establish that the wrong value passes.

Command:
`cargo xtask goldens bless typescript_generator_promise --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
