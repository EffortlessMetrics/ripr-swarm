# Golden Output Changes

## Pending

Reason:
pin Cluster B free-function module-identity false-exposed: same-named function from a different module must not credit exposed

Command:
`cargo xtask goldens bless python_adversarial_free_function_module_collision --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
