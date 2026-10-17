<!-- section: Fixed -->
- Qualifying Rust findings whose changed tail `match` is selected by a plain
  owner-parameter field (a candidate-current weak finding with a strong oracle in
  a directly calling test and an eligible match shape; RIPR-SPEC-0229) may now
  carry the typed static limitation `rust_field_match_selector_unresolved`:
  ripr cannot align the field value at the owner invocation or establish that an
  assertion observes that invocation's return. Classification, probe IDs and
  exposure credit are unchanged, but the existing typed-limitation contract
  withholds canonical repair-gap projection, which can change repair readiness
  and derived gate routing. Inspect the existing call and assertion before
  adding a test. The seam cache schema moves to `1.82`, so existing Rust caches
  are rebuilt on the next run (#8479).
