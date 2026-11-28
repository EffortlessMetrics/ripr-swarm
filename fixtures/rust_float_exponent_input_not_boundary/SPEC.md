# Fixture: rust_float_exponent_input_not_boundary

Spec: RIPR-SPEC-0108

## Given

A test that calls `bulk_price(1.0e5)` and asserts `90000.0`. The float literal `1.0e5` is 100000.0.

## When

`bulk_price` changes its discount condition from `qty >= 1.0` to `qty > 1.0`; the only input that separates the two bodies is `1.0`.

## Then

The finding is not `exposed`: the call's input is not the boundary value.

## Must Not

Read `1.0e5` as the value `1.0` (its mantissa digits) and credit the call as the boundary input (#6912). The honesty corpus independently rejects the false promotion after re-blessing.
