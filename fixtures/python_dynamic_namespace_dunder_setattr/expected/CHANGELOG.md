# Golden Output Changes

## Pending — python_dynamic_namespace_dunder_setattr (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: pin #10184 namespace-writer refusal; module-object and constant findings remain weakly_exposed, where pre-repair CLI exposed both.

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

The same run's generated `human-full.txt` is retained for the
RIPR-SPEC-0108 expected-class invariant.
