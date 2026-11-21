# Fixture: guard_operand_wrapper_agreement

Spec: RIPR-SPEC-0197

## Given

Dropping `n != 0` preserves the result of `is_pow2(4)`. The test renders
the result and passes it to a helper whose assertion names neither the owner
nor its input.

## When

Analyze the diff that drops the guard.

## Then

The return-value finding must read `weakly_exposed`, matching
the direct pins in `guard_operand_dropped_agreement`.

## Must Not

Crediting the helper
without recovering its pinned call would incorrectly mark the edit exposed.

```expected-classes
probe:src_lib.rs:return_value:90a53480 -> weakly_exposed
```
