# lifetimes
1. Change (lib.rs): longest tie-break `a.len() >= b.len()` -> `>`
2. Ideal test: `assert_eq!(longest("aa","bb"), "aa")` (tie; head returns "bb")
3. Shape: returns reference with explicit lifetime; also &[u8] slice and struct<'s>; test inputs must outlive result; tie case needs distinct same-length strings.
