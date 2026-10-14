# Fixture: python_unittest_list_equal_nested_class

Spec: RIPR-SPEC-0028

## Given

The list return changes `[1, 1]` to `[1, 2]`, but its exact assertion belongs to
`Outer.Inner`, nested unittest classes. Default native unittest discovery
registers zero tests (CLI exit 5), so no assertion observes the changed return.
This independently authored #7312 review control shares the CPython source pin
and literal oracle described in `../python_unittest_list_equal/ORACLE.md`.

## When

`cargo xtask fixtures python_unittest_list_equal_nested_class`

## Then

One `static_unknown` finding retains an unknown custom-helper oracle. The new
module-level support boundary cannot infer default nested unittest registration.

## Must Not

Report `exposed`, a strong exact oracle, or discrimination yes. The governed
RIPR-SPEC-0108 honesty corpus independently forbids promotion. Zero registered
subjects are not native test evidence.
