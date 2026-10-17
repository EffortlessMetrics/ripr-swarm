# generic_bounds
1. Change (lib.rs): count_above filter `*x > t` -> `*x >= t`
2. Ideal test: `assert_eq!(count_above(&[1,4,9], &4), 1)` (head returns 2)
3. Shape: generic fn with T: PartialOrd; generator must pick a concrete T and a boundary value equal to the threshold; bounds give no hint about which T.
