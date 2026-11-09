<!-- section: Fixed -->
- A whole-value field pin whose expected struct literal gives the changed field a value that neither the old nor the new owner computes for the call's literal arguments no longer counts as a discriminator, for `assert_eq!` and for the claims macros alike (RIPR-SPEC-0197, #12864, #12394).
- The claims `assert_ok_eq!` / `assert_some_eq!` pins now also credit a whole-value field pin of the `field_construction` family (`assert_ok_eq!(config(2), Config { retries: 3 })` beside an `Ok(Config { .. })` tail) under the same gates as the `assert_eq!` twin (#12394).
