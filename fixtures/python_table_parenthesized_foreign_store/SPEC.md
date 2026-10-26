# Fixture: python_table_parenthesized_foreign_store

Spec: RIPR-SPEC-0028

## Given

A parenthesized dict constant. A second module imported by the test stores the original silver price back into the table at import time.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. The foreign store restores 5 before the assertion, so the assertion holds before and after the edit. The foreign-write scan must watch every dict binding the evaluator admits, parenthesized ones included.

## Must Not

Index only the unparenthesized spelling of a dict binding and miss the foreign store, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
