# Fixture: python_table_reflection_exec

Spec: RIPR-SPEC-0028

## Given

A store that runs evaluated code which imports the table module under a name built at runtime and stores the original price.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. Evaluated code (`exec`, `eval`, `compile`) can import any module, so the set of modules it reaches is unknown and every table's credit is withheld.

## Must Not

Rely on the import graph when the file runs evaluated code, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module.
