<!-- section: Fixed -->
- A Rust test that reaches a trait-impl owner only through a blanket `impl<T: Trait> Other for T` method is now related to that owner (#6732). The owner pin does not read that hop, so the gap is withheld as `static_unknown` under `rust_indirect_owner_call_unresolved` instead of being reported as a gap.
