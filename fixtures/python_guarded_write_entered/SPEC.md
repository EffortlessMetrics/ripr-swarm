# Fixture: python_guarded_write_entered

Spec: RIPR-SPEC-0260; governing #9833 and review repair #9868.

## Given

A native plain-if comparison with a trailing comment retains credit for its exact field observer.

## When

`cargo xtask fixtures python_guarded_write_entered`

## Then

Exactly one changed-field finding is `exposed`. Native Python executes the old and new variants with the same test; the new variant fails its exact assertion.

## Must Not

Lose healthy plain-if credit solely because its trailing comment contains keyword text.
