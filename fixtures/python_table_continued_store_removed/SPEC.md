# Fixture: python_table_continued_store_removed

Spec: RIPR-SPEC-0028

## Given

The continued-definition table of `python_table_continued_foreign_store` with no foreign store.

## When

The silver entry changes from 5 to 6.

## Then

The finding stays `exposed`: nothing restores the value, so the assertion fails after the edit.

## Must Not

Refuse every continued definition. This control keeps the refusal tied to the store.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
