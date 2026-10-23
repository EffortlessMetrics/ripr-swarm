# Fixture: python_unreached_get_default

Spec: RIPR-SPEC-0260

## Given

`lookup` changes the fallback of `RATE_TABLE.get(tier, 0 -> 1)` over a module-level literal dict. The only test looks up `"silver"`, a key the table holds, so the default never runs.

## When

`cargo xtask fixtures python_unreached_get_default`

## Then

The finding is `weakly_exposed`, not `exposed` (#9429).

## Must Not

Promote beyond the stated class. The governed RIPR-SPEC-0108 honesty corpus
independently checks this outcome.
