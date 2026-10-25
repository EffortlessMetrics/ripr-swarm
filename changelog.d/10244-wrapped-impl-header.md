<!-- section: Fixed -->
- The first line of a rustfmt-wrapped `impl<..> Trait` header (no body brace or `where` yet) no longer seeds `field_construction` or `call_deletion` probes and a false `reachable_unrevealed` warning; it is declaration-only like a one-line `impl` header and seeds a single `static_unknown` (#10244).
