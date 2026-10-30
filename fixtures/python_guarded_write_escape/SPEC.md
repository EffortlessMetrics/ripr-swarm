# Fixture: python_guarded_write_escape

Spec: RIPR-SPEC-0260

Governing issue: #9833; review repair: #9868.

## Given

An escaped quote followed by a native return leaves the changed write unexecuted.

## When

`cargo xtask fixtures python_guarded_write_escape`

## Then

Exactly one changed-field finding is `weakly_exposed`. Native Python executes the old and new variants with the same test; both variants pass. Guard entry alone cannot promote this case.

## Must Not

Promote the finding to exposed. The RIPR-SPEC-0108 honesty corpus independently pins this negative.
