# Fixture: atomic_ordering_swap_strong_object_oracle

Spec: RIPR-SPEC-0108

## Given

A call is edited only in a `std::sync::atomic::Ordering` argument
(`Ordering::AcqRel` to `Ordering::Release`). The reached test asserts the
whole returned `Claim` object with `assert_eq!`, a strong exact-value oracle.

## When

The exact diff is analyzed through the current Rust producer.

## Then

The edit stays `static_unknown`. No assertion on a returned value can observe a
memory-ordering change, so the strong whole-object oracle must not promote it.

## Must Not

- Report `exposed` because a strong value oracle is reached.
- Seed a call or value family for an ordering-only edit (#9080).
