# Fixture: python_table_code_replacement_removed

Spec: RIPR-SPEC-0028

## Given

The reader of `python_table_code_replacement` with the code replacement removed.

## When

The silver entry changes from 5 to 6.

## Then

The finding stays `exposed`: the reader is unchanged, so the assertion fails after the edit.

## Must Not

Refuse every table read. This control keeps the refusal tied to the code replacement.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
