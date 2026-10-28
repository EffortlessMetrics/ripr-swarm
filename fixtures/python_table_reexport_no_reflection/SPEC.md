# Fixture: python_table_reexport_no_reflection

Spec: RIPR-SPEC-0028

## Given

The re-exported table of `python_table_reexport_reflection` with the reflective store removed.

## When

The silver entry changes from 5 to 6.

## Then

The finding stays `exposed`: nothing restores the value, so the assertion fails after the edit.

## Must Not

Refuse every re-exported table. This control keeps the refusal tied to the reflective access.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
