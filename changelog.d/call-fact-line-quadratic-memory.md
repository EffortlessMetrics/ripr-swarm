<!-- section: Fixed -->
- A single Rust source line with many call expressions no longer takes memory quadratic in the call count. Call facts on one line now share one line allocation, repeated callee names collapse before allocation, and at most 256 distinct callee names are kept per line; calls past that budget are dropped, which removes reach evidence rather than adding any (#8574).
