<!-- section: Fixed -->
- A Rust `field_construction` finding on a struct literal the owner returns
  directly (its tail, or `Ok(..)`/`Some(..)` around it) now reads `exposed`
  when a related test compares the owner's whole result with a struct literal that names
  the changed field with an independent value
  (`assert_eq!(build(3), Config { retries: 4, .. })`, through `Ok(..)` or
  `Some(..)`, or a once-used `let c = build(3);`), and the type and field
  compare by derived `PartialEq` (RIPR-SPEC-0225). A field read copied into
  the expected literal (`Config { retries: c.retries, .. }`) no longer counts
  as observing the field.
