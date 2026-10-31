# Fixture: python_table_parenthesized_store_removed

Spec: RIPR-SPEC-0028

## Given

The parenthesized table of `python_table_parenthesized_foreign_store` with no foreign store.

## When

The silver entry changes from 5 to 6.

## Then

The finding stays `exposed`: nothing restores the value, so the assertion fails after the edit.

## Must Not

Refuse every parenthesized dict. This control keeps the foreign-store refusal tied to the store.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original module and fails on the edited one, so the credit is a genuine kill.
