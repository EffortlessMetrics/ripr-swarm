# Fixture: python_guarded_write_read_only

Spec: RIPR-SPEC-0260

Governing issue: #9833; review repair: #9868.

## Given

A guarded write is followed by field truthiness, a dictionary-key read and an annotation without a value; these do not replace the observed field.

## When

`cargo xtask fixtures python_guarded_write_read_only`

## Then

Exactly one changed-field finding is `exposed`. Native Python executes the old and new variants with the same test; the new variant fails its exact assertion.

## Must Not

Lose healthy plain-if credit because later syntax reads the field or annotates it without a value.
