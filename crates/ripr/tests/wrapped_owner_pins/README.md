# Wrapped return-value owner-pin tests

This bounded family groups `claims_payload_owner_pin`,
`claims_payload_eq_owner_pin`, and `unwrapped_ok_owner_pin` under the
`wrapped_owner_pins` integration target (#12914). The three original sources
are unchanged; their fixture directories remain distinct. They exercise the
same owner-return pin rules through the public API, including the existing
CLI audit parity checks. No test moves into the product's library test target.

`inventory_contract.rs` freezes the nine native cases listed from baseline
`2c03ae6671e375a2f16d3ee30c581f09f596ea88` with Rust 1.95.0 and default
features on Linux. Each `(old target, old native name, ignored)` row maps to
target `wrapped_owner_pins`, native name
`wrapped_owner_pins::<old target>::<old native name>`, and the same ignored
state. The original sources have no platform or feature gates. Native execution
on other hosts remains part of their applicable qualification.

The conservation test compares both complete and ignored-only native listings.
Four negative controls reject duplicate mappings, missing mappings, missing
native cases, and changed ignored state. These migration controls supplement
the ordinary shard exact-cover check; they do not replace test execution.

Run the entire family with:

```sh
cargo test --locked -p ripr --test wrapped_owner_pins
```

The old target name remains a module filter for focused selection. For example:

```sh
cargo test --locked -p ripr --test wrapped_owner_pins claims_payload_owner_pin::
```

Rollback restores all three original target paths and their traceability,
specification, and process-policy references together.
