<!-- section: Fixed -->
- A library root outside `src/` now names its own modules: with
  `[lib] path = "source/entry.rs"`, a test calling
  `core_lib::pricing::discount(100)` gives the owner in
  `source/pricing.rs` its boundary input instead of reading
  `infection_unknown`. A symlinked `src` directory or crate root file is read
  where the link lands, so a change Git reports at the link target keeps the
  test relation the plain layout has (#14495, #15909).
