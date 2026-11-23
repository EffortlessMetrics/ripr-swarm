# Fixture: rust_turbofish_owner_call

Spec: RIPR-SPEC-0197
Related spec: RIPR-SPEC-0108
Issue: #15277

## Given

The production `fn doubled<T: Into<u64>>(x: T) -> u64` changes from
`x.into() * 2` to `x.into() + 2`. The test pins `doubled::<u8>(5)` to `10`,
spelling the call with a turbofish.

## When

```bash
cargo xtask fixtures rust_turbofish_owner_call
```

## Then

Exactly one `return_value` finding reads `exposed`. The related test's
relation is `direct_owner_call`, and the exact pin is read through the
turbofish exactly as the plain `doubled(5u8)` spelling is: a turbofish only
fixes the generic arguments of the same function.

The authored crate compiles, its test passes before the diff and fails after
it (`5 + 2` is `7`, not `10`).

## Must Not

- Read the turbofish call as an unconfirmed observation.
- Accept an unclosed turbofish, a turbofish path value with no call, or a call
  chained after the owner call as the owner call. The unit test
  `a_turbofish_call_has_the_plain_call_shape` in `owner_pin/tests.rs` keeps
  those refused.
