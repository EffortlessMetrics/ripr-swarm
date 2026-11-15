# Fixture: python_dunder_truthiness_replaced_test

Spec: RIPR-SPEC-0238

## Given

An authored `Money.__bool__` comparison changes from `!= 0` to `> 0`.
The observing test definition is replaced by an active test that passes both predicates. Independent review of #9795 found this over-credit on candidate 0ab4a251.

## When

`cargo xtask fixtures python_dunder_truthiness_replaced_test`

## Then

The single changed owner is `weakly_exposed` (a gap): the replaced definition
never runs, so its assertion cannot provide exact truthiness credit, and the
active replacement asserts nothing about `Money`. The finding names the
replaced test as one that never runs (`related_test_never_runs`).

## Must Not

Promote an assertion without its active test definition and unchanged owner binding.
