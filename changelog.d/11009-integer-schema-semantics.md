<!-- section: Fixed -->
- The shared verification-contract validator now accepts a JSON number with a zero fractional part (`1.0`, `1e0`) and the full `u64` range as a schema `integer`, as Draft 2020-12 does, while still rejecting fractions and floats beyond 2^53 where rounding could hide one (#11009).
