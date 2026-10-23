# Fixture: python_guarded_write_parameter_rebound

Spec: RIPR-SPEC-0260; governing #9833 and review repair #9868.

## Given

A parenthesized assignment changes the guard parameter before it is read.

## When

`cargo xtask fixtures python_guarded_write_parameter_rebound`

## Then

Exactly one changed-field finding is `weakly_exposed`. Native Python executes the old and new variants with the same test; both variants pass. Guard entry alone cannot promote this case.

## Must Not

Promote the finding to exposed. The RIPR-SPEC-0108 honesty corpus independently pins this negative.
