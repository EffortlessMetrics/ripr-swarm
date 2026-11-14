<!-- section: Fixed -->
- Rust checks: a test that sits under the same feature-gated `mod name;` declaration as the code it tests (for example `#[cfg(feature = "x")] mod x;` with both in `src/x.rs`) is no longer reported as an inert assertion with "make the check run" advice; the finding reads as an analyzer limit instead (#14463).
