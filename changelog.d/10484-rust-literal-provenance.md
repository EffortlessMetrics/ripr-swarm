<!-- section: Fixed -->
- A Rust owner call whose argument merely contains a literal inside a call chain (`initial(&std::env::var("WHO").unwrap_or_default())`) no longer reads that literal as the input it received: the call contributes no input row, and a boundary that depends on it reads as the result of a call (#10484).
