# Fixture: python_strip_side_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`clean` changes `return s.strip()` to `return s.lstrip()`. The only test calls
`clean("  a")`, a string with no trailing whitespace, where both versions
return `"a"`, and asserts the exact value.

## When

`cargo xtask fixtures python_strip_side_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed call passes a
string with trailing whitespace, so the suite passes with either method. The
missing discriminator names that input. This is a #16203 false-exposed
correction.

## Must Not

Promote the finding to `exposed` while every strong related call passes a
string both versions treat alike. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
