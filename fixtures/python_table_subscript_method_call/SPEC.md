# Fixture: python_table_subscript_method_call

Spec: RIPR-SPEC-0028

## Given

A dict constant. A second module imported by the test calls a method on a subscripted table value at import time.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. A method call on `METHOD_TABLE["silver"]` can mutate the value in place, so the scan counts it as a possible write. This is a conservative refusal; the fixture makes no claim that this call changes the table at runtime.

## Must Not

Read a call on a subscripted table value as a pure read and promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original module and fails on the edited one, so this is a genuine positive withheld by a conservative scanner rule; the fixture makes no runtime-mutation claim.
