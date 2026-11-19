<!-- section: Fixed -->
- A weakly exposed return value of a `-> bool` function whose related tests
  already assert its exact result (`assert!(f(..))`, `assert!(!f(..))`) no
  longer advises replacing broad assertions with exact equality; the next
  step asks for a test with an input whose boolean result differs between
  the old and new code. The dropped-guard agreement rule now compares
  retained operands as Rust token sequences, so `a as u8 == 0` rewritten to
  `aasu8 == 0` is no longer read as a dropped guard (RIPR-SPEC-0197 rule 5,
  RIPR-SPEC-0259). Classes and stages are otherwise unchanged (#15149).
