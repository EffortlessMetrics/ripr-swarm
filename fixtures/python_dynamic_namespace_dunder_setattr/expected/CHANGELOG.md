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

## Pending — python_dynamic_namespace_dunder_setattr (2)

Reason:
RIPR-SPEC-0122 / #10258: the human view now fits or wraps the complete missing-discriminator prose instead of clipping it with an ellipsis. Only that wrapping changes; classes, stages, findings, IDs, JSON and human-full are unchanged.

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dynamic_namespace_dunder_setattr (3)

Reason:
RIPR-SPEC-0028 / #10289: a module-object write whose related test reloads or replaces the module namespace is withheld as a runtime-substitution static limit (static_unknown) instead of a weak finding whose repair card asserts on the stale imported object. The other finding is unchanged.

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dynamic_namespace_dunder_setattr (4)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192 receiving composition

- `human.txt`: name kind and reason on every listed hidden row, preserving currentness and the selected finding. JSON and registered full-human expectations are unchanged.
- Proof: this fixture passes on exact main `aeeeddacd1e24f3b04e713572717e7b172966431`; native receiving producer `a3946a7f1002ae6f069cd18fb4ed45454667b326` emitted the intended presentation before the expectation repair. No verdict or supported-repair claim is added.

## Pending — python_dynamic_namespace_dunder_setattr (5)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dynamic_namespace_dunder_setattr (6)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_dynamic_namespace_dunder_setattr --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
