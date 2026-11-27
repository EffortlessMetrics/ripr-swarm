# Fixture: rust_deref_receiver_reexported_std

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `model::Ipv4Addr::octets` changes. The crate root re-exports `std::net` and imports `crate::net::Ipv4Addr`, so `Guard`'s `type Target = Ipv4Addr;` names `std::net::Ipv4Addr`, not the workspace type. The test asserts `Guard(Ipv4Addr::LOCALHOST).octets() == [127, 0, 0, 1]`, which runs std code. It passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_reexported_std` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A `use` that spells the target name refuses the deref reading, whatever the path root.

## Must Not

- Credit `model::Ipv4Addr::octets` because `Guard` derefs to a type named `Ipv4Addr`.
