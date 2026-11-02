<!-- section: Fixed -->
- Doc examples on items gated by a `#[cfg(..)]` the doctest build satisfies under cargo defaults (`doctest` on, `test` off, `feature = "x"` in the owning manifest's default-enabled features) register as tests again; target, custom, unknown and malformed predicates, `cfg_attr`, and gates without an establishing manifest still refuse (#12262, #12230).
