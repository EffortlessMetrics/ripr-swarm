<!-- section: Fixed -->
- An exact assertion that only reads the `Ok` or `Some` payload (`f(1).unwrap()`, `Ok(..)`, `Some(..)`, `assert_ok_eq!`) is no longer published as a strong oracle on the same owner's `Err(..)` or `None` seam: it passes only when the call took the success exit, so it cannot notice the failure arm changing. The pin that names the failure arm keeps its credit (#12757).
