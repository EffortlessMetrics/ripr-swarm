# Fixture: struct_literal_untouched_field_builder

Spec: RIPR-SPEC-0197

## Given

`RequestBuilder::build` returns a `Request` struct literal and the diff changes
only the `timeout_ms` default (`unwrap_or(30_000)` to `unwrap_or(3_000)`). The
related tests assert the untouched `url` field and read `timeout_ms` only after
setting it to 500 on the builder.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: the literal confirms only through
the fields the diff changed, and the one assertion on `timeout_ms` reads the
test's own value, so nothing the diff moved reaches an assertion (#10916).

## Must Not

- Promote a struct literal to `exposed` because a test asserts a field the diff
  left untouched.
