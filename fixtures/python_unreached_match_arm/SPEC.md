# Fixture: python_unreached_match_arm

Spec: RIPR-SPEC-0260

## Given

`kind` changes the returned label of its wildcard `case _` arm. The only test calls `kind(0)` and `kind(5)`, both exact strong oracles, and each is taken by an earlier arm, so no observed input executes the changed return.

## When

`cargo xtask fixtures python_unreached_match_arm`

## Then

The finding is `weakly_exposed`, not `exposed`: the edited arm is never evaluated under an observed input (#9429).

## Must Not

Promote beyond the stated class. The governed RIPR-SPEC-0108 honesty corpus
independently checks this outcome.
