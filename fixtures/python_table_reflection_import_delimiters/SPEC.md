# Fixture: python_table_reflection_import_delimiters

Spec: RIPR-SPEC-0028

## Given

The same namespace package, with a bridge whose import follows a string holding a closing bracket on the same line after a semicolon.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. Import links are read from whole statements, so the bridge connects to the table module and the reflective store is in its component.

## Must Not

Count brackets inside comments or strings when finding where an import statement ends, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module.
