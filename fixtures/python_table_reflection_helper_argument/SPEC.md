# Fixture: python_table_reflection_helper_argument

Spec: RIPR-SPEC-0028

## Given

A reflective helper that receives the table module as an argument. The test imports both the helper and the table module and has the helper store the original price.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. The helper names neither the table nor its module, but the test connects them: the helper's import component reaches the table module.

## Must Not

Treat a reflective helper as harmless because it never imports the owner, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module.
