# Fixture: python_guarded_write_same_line_mask

Spec: RIPR-SPEC-0260; governing #9833 and review repair #9868.

## Given

A second statement on the changed physical line immediately replaces the value.

## When

`cargo xtask fixtures python_guarded_write_same_line_mask`

## Then

Exactly one changed-field finding is `weakly_exposed`. Native Python executes the old and new variants with the same test; both variants pass. Guard entry alone cannot promote this case.

## Must Not

Promote the finding to exposed. The RIPR-SPEC-0108 honesty corpus independently pins this negative.
