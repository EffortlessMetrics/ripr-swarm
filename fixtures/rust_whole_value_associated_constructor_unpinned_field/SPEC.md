# Fixture: rust_whole_value_associated_constructor_unpinned_field

Spec: RIPR-SPEC-0225

## Given

`Cfg` derives `PartialEq`. Its associated constructor `Cfg::fresh(n)` returns `Cfg { retries: n + 1, verbose: false }`, changed from `retries: n + 2`. The test imports `Cfg` directly and runs `assert_eq!(Cfg::fresh(3), Cfg { retries: 7, verbose: false })`.

## When

`cargo xtask fixtures rust_whole_value_associated_constructor_unpinned_field` runs the public analyzer.

## Then

The field_construction finding reads `weakly_exposed`. Neither version computes `retries: 7` for `fresh(3)` (5 before, 4 after), so the assertion cannot tell the two apart. The closed-owner check reads the admitted `Cfg::fresh(3)` call as the `fresh(3)` it resolves to and refuses the pin, as it does for a free function (#15345).

## Must Not

- Read `exposed` because the qualified `Cfg::fresh(..)` call text could not be evaluated by the closed-owner check.
