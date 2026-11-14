# Workflow contract test family

Run the family with `cargo test --locked -p repo-policy --test workflow_contracts`.
For a focused replay, append `-- hosted_cache_continuity::` (or another module).

This bounded executable replaces five small integration binaries from source
`46b62971d1ea2e06d916c09e35212c10fe4c7835` (#12914). Each original suite keeps
its module, assertions, platform gates, child-only environment setup and private
scratch names. The three Linux-only suites remain Linux-only inside the portable
target. The target does not change the parent process environment or directory.

`fixtures/inventory-map.json` freezes the native Linux baseline's 29 logical
`(binary_id, full_test_name, ignored)` identities and their one-to-one successor
names. `inventory_contract` compares that mapping with this executable's actual
native complete and ignored-only listings. Missing mappings, duplicate mappings,
omitted native cases and changed ignored state have executable negative controls. Its five new conservation tests are explicitly
named separately from the old cases. Intentional future test additions or
renames must update the mapping/control inventory; an absent case cannot silently
become a smaller successful denominator. Non-Linux rows follow the preserved
module platform gates; native non-Linux execution remains separate proof.

This only reduces repeated linking within one integration family. It changes no
required check, selected features, shard-union verification or production crate.
The 7-GB whole-workspace archive rejected by #11594 is not restored.
