# Fixture: rust_inline_branch_value_boundary_only

Spec: RIPR-SPEC-0108

## Given

The only assertion calls `discount(5)` at the threshold and expects 2.

## When

The value returned above the threshold changes from 1 to 3.

## Then

The finding names the changed return value. The threshold assertion still passes on both revisions, so it leaves the finding below `exposed`.

## Must Not

Credit the assertion with detecting an edit in the other branch (#15529).
