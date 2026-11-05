<!-- section: Fixed -->
- A boundary check on a per-byte loop counter now credits when the tests call the owner with literal `&[u8]` or byte-string inputs whose lengths differ by one and straddle the boundary; other lengths still abstain (Advances #5346).
- Refuse counter-boundary credit when another return, conditional tail, loop exit, or unrecognized fallible suffix can mask the firing branch, retaining an adversarial honesty-corpus control (#5346).

- Refuse specialized boundary credit when retained lexical bindings can shadow Result constructors, including raw identifiers and owners without fallible suffixes (#5346).
- Withhold specialized counter credit beside unknown attribute or derive producers that can generate hidden Result constructors; retain unambiguous builtin derives (#5346).
- Require proved valid, unchanged primitive hex-byte input before specialized counter credit across an earlier fallible conversion, preventing custom error equality or rebound bytes from masking the boundary (#5346).

- Require an unchanged parsed counter trajectory and representable primitive counter bounds; destructuring writes, implicit mutable receivers, ref-mut aliases and overflowing predicate literals withhold specialized credit (#5346).
