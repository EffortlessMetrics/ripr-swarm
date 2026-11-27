<!-- section: Fixed -->
- After `ripr agent stub --write` appends a test module after a function, `ripr check` no longer reports a `static_unknown` finding on the function's unchanged closing `}` (git's diff alignment shows it as added), so `verify` completes once the gap is exposed (#15270).
