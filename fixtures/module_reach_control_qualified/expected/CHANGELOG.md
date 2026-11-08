# Golden Output Changes

## Pending — module_reach_control_qualified (1)

Reason:
RIPR-SPEC-0159: #8999 module-qualified entry reach honesty fixture

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (2)

Reason:
RIPR-SPEC-0122: re-bless human render drift after merging main; verdicts unchanged

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (3)

Reason:
Summary terms legend from main (#10004); RIPR-SPEC-0122

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (4)

Reason:
Conditional ReturnValue guidance when an exact assertion exists (RIPR-SPEC-0259, #8954); aligned tail-return and semicolon effect fixtures (RIPR-SPEC-0094)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (5)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (6)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (5)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/human.txt`

## Pending — module_reach_control_qualified (7)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (8)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (9)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (10)

Reason:
RIPR-SPEC-0122 plain wording for the check screen's whole-seam/untied-spot refusals (#11619)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (11)

Reason:
RIPR-SPEC-0001: additive summary.conflicting_expressions member (#11554)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (12)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (13)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (14)

Reason:
RIPR-SPEC-0122: render the missing check in plain language

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (15)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (16)

Reason:
RIPR-SPEC-0083: #13037 default missing-check line says no assertion was tied to the changed value when the next step says an exact assertion exists

Command:
`cargo xtask goldens bless module_reach_control_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_control_qualified (17)

Reason:
RIPR-SPEC-0005: restore the inherited human refusal line to the exact output of the repaired public-parent candidate; other evidence and classification are unchanged.

Actual producer command (initial artifact comparison, not a goldens bless):
`ripr check --root fixtures/module_reach_control_qualified/input --diff fixtures/module_reach_control_qualified/diff.patch --mode fast --legend`

Producer: verified hosted artifact 13393509401 from run 41980182060, candidate `62b2b3a2ca790e82281acb8efb6d93ff9c950b0d`, tree `37bbda3224104f29a60e408408e41564074ce09c`; native payload SHA256 `6048cbb6ddd9ade11ab0160ba18dae7e786fc88ff2532e1d722663814340ce49`. Dedicated cache/Rayon 1 and unchanged compiled canonical normalization/comparison helpers were used. Actual normalized bytes equal main `22eba11598aeb86f645233c9c6741c56f522df63` byte for byte. Canonical goldens/dogfood and required hosted qualification remain pending; no bless/check pass is claimed.

Updated:
- `expected/human.txt`
