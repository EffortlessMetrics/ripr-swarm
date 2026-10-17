# io_time
1. Change (lib.rs): backoff exponent cap `attempt.min(5)` -> `min(6)`
2. Ideal test: `assert_eq!(backoff(6), Duration::from_millis(3200)); assert_eq!(backoff(10), Duration::from_millis(3200))` (head: backoff(6) = 6400ms, backoff(10) = 6400ms)
3. Shape: Path/Duration/Instant params, fs read and env var side effects, private env_or; generator must create temp files and avoid nondeterministic Instant tests.
