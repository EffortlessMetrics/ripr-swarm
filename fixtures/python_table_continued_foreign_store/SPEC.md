# Fixture: python_table_continued_foreign_store

Spec: RIPR-SPEC-0028

## Given

A dict constant defined on the second physical line of an explicitly continued simple-statement pair (`MARKER = 0; \` then `RATES = {...}`). A second module imported by the test stores the original silver price back into the table.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. The foreign store restores 5 before the assertion, so the assertion holds before and after the edit. Dict discovery must reset at a semicolon as it does at a newline, so the continued definition is still watched for foreign stores.

## Must Not

Let an explicit continuation hide the table name behind the first statement, skip the foreign-store scan, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
