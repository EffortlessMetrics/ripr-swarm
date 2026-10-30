# Fixture: python_table_reflection_dynamic_import

Spec: RIPR-SPEC-0028

## Given

A store that loads the table module by a name built at runtime and mutates it reflectively.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. A reflective file that also loads modules by name makes the set of modules it can reach unknown, so every table's credit is withheld.

## Must Not

Rely on the import graph when the module is chosen at runtime, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module.
