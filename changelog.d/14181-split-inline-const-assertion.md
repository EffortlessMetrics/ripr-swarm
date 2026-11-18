<!-- section: Changed -->
- Rust checks: an inline const assertion that rustfmt splits over several
  lines (`const {` / `assert!(N <= 249);` / `};`) now reads like the
  one-line form. Its lines select no runtime family and keep one
  `static_unknown` finding naming the `compile_time_invariant` stop reason;
  an edited line no longer leaves a second finding for the old text (#14181).
