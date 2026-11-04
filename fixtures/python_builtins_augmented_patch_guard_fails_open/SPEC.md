# Fixture: python_builtins_augmented_patch_guard_fails_open

Spec: RIPR-SPEC-0260

Governing claim: #12592

## Given

The module aug-assigns a name on the `builtins` module alias (`b.isinstance += 1`)
before an owner whose arm is guarded by `isinstance(x, str)`. The diff changes
only the guarded return. The sole strong test calls with an `int` literal that
does not satisfy `isinstance(x, str)` and asserts the fall-through result.

## When

`cargo xtask fixtures python_builtins_augmented_patch_guard_fails_open`

## Then

The finding stays `exposed`, not `weakly_exposed`: the augmented builtins-module
write makes the `isinstance` guard unreadable, so the row is not excluded. A
companion unit test in `guard_atoms_tests.rs` pins the same shape.

## Must Not

Treat `b.isinstance += 1`, `|=` or other augmented stores through a builtins
module alias as a harmless read. Credit `weakly_exposed` from a guard that no
longer names the real builtin.
