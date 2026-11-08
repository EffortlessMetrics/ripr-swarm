<!-- section: Fixed -->
- Rust: `assert_ok_eq!(owner(..), literal)` and `assert_some_eq!(owner(..), literal)`
  from the `claims` crate now pin an `Ok`/`Some` tail like the unwrapped
  `assert_eq!`, only under the manifest-verified `use claims::..` import. The
  related test reads `exact_value` / strong in that case; no import evidence, a
  rename, a local rebind, a non-literal expected operand or the wrong
  direction keep the line `unknown` with the macro limitation (#12304).
