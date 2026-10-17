1. Change: `tier()` bulk threshold `qty >= 10` -> `qty > 10` (crates/core/src/pricing/tier.rs).
2. Ideal test: `assert_eq!(tier(10), (20, "bulk"));` (or `line_total(100, 10) == 800`; app: `invoice(&[(100,10)]) == 800`).
3. Shape: free fn, returns tuple, nested module (pricing/tier.rs), workspace member (core; consumer in app).
