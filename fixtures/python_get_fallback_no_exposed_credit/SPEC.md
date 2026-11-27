# Fixture: python_get_fallback_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`score` changes `return d.get("k", 0) + 1` to `return d.get("k", 5) + 1`. The
only test calls `score({"k": 2})`, a dict that holds the key, so the fallback
never runs and both versions return `3`, and asserts the exact value.

## When

`cargo xtask fixtures python_get_fallback_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed call passes a dict
without the key, so the suite passes with either fallback. The missing
discriminator names that input. This is a #16203 false-exposed correction.

## Must Not

Promote the finding to `exposed` while every strong related call passes a dict
that holds the key. The governed RIPR-SPEC-0108 honesty corpus independently
forbids promotion.
