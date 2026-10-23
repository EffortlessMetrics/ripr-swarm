# Golden Output Changes

## Pending — typescript_then_function_scoped_var_shadow (1)

Reason:
RIPR-SPEC-0246 rule 2 and RIPR-SPEC-0108: a nested block var shadows the
callback receiver at function scope; actual wrong-owner Vitest controls pass,
so the local fake assertion cannot promote the imported owner (#6798).

Command:
`ripr check --root fixtures/typescript_then_function_scoped_var_shadow/input --diff fixtures/typescript_then_function_scoped_var_shadow/diff.patch --format json --mode fast`

Scoped JSON, human and human-full projections use the repository's cwd,
newline and canonical-next-action normalization. Aggregate goldens/honesty
gates were not executed in this lane.

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
