<!-- section: Fixed -->
- An owner call inside an empty catch-all macro defined in the test body (`macro_rules! discard { ($($t:tt)*) => {}; }` then `discard!(discount(100))`) no longer counts as a test input at the changed boundary; such calls never run (#15539).
