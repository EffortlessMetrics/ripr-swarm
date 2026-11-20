# Fixture: rust_inline_operator_branch_not_taken

Spec: RIPR-SPEC-0108

## Given

The test asserts that `Money(2) + Money(3)` produces `Money(0)`.

## When

The overloaded operator changes subtraction to addition in the value returned for a receiver above 100.

## Then

The operator call reaches the method. Its receiver selects the unchanged else branch, so the changed return value remains weakly exposed.

## Must Not

Promote operator reach to activation of the changed branch (#15529).
