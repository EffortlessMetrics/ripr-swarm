# Fixture: python_dunder_truthiness_replaced_test

Spec: RIPR-SPEC-0238

## Given

An authored `Money.__bool__` comparison changes from `!= 0` to `> 0`.
The observing test definition is replaced by an active test that passes both predicates. Independent review of #9795 found this over-credit on candidate 0ab4a251.

## When

`cargo xtask fixtures python_dunder_truthiness_replaced_test`

## Then

The single changed owner stays `static_unknown`; the stale or replaced assertion
cannot provide exact truthiness credit.

## Must Not

Promote an assertion without its active test definition and unchanged owner binding.
