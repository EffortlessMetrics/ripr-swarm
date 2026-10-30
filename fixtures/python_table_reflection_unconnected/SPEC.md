# Fixture: python_table_reflection_unconnected

Spec: RIPR-SPEC-0028

## Given

A table module and an unrelated module that reflects over `math`. Nothing imports the reflective module, and it imports nothing from the workspace.

## When

The silver entry changes from 5 to 6.

## Then

The finding stays `exposed`: the reflective file is not in the import component of the table module, so it cannot reach the table. The assertion fails after the edit.

## Must Not

Withhold credit for every table because some unconnected file uses `getattr`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original module and fails on the edited one.
