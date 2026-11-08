<!-- section: Docs -->
- New `RIPR-SPEC-0411` documents every `StopReason` wire value with its trigger, class impact and owning spec, marks `max_depth_reached`, `external_crate_boundary` and `feature_unknown` as reserved and never emitted on current main, and adds registry tests that fail if a production site starts pushing a reserved token (#5514, #12546).
