# Fixture: python_conditional_version_attribute_write

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

An unchanged Python assertion imports the module affected by a single-line edit.
The assertion observes a replacement callable or a dead definition, or returns before the assertion.

## When

The changed constant or conditional function body is analyzed by `ripr check`.

## Then

The nonempty finding remains `static_unknown`: this assertion does not observe the changed behavior.
Actual Python execution passes on both source revisions.

## Must Not

Credit an unreached body by evaluating its stale definition or by trusting physical continuation indentation.
The honesty corpus independently rejects promotion even if a golden is re-blessed.
