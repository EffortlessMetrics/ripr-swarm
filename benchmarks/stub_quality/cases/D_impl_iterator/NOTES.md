# impl_iterator
1. Change (lib.rs): evens_below range `0..n` -> `0..=n`
2. Ideal test: `assert_eq!(evens_below(6).collect::<Vec<_>>(), vec![0,2,4])` (head adds 6)
3. Shape: returns opaque impl Iterator; no nameable type, must collect; boundary only visible with even n (existing test uses odd n).
