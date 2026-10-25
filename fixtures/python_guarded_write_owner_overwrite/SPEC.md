# Fixture: python_guarded_write_owner_overwrite

Spec: RIPR-SPEC-0260

Governing issue: #9833; review repair: #9868.

## Given

A later owner write replaces the changed field value before the test observes it.

## When

`cargo xtask fixtures python_guarded_write_owner_overwrite`

## Then

Exactly one changed-field finding is `weakly_exposed`. Native Python executes the old and new variants with the same test; both variants pass. Guard entry alone cannot promote this case.

## Must Not

Promote the finding to exposed. The RIPR-SPEC-0108 honesty corpus independently pins this negative.
