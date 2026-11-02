<!-- section: Internal / CI -->
- Hosted Rust gates no longer run the nextest archive measurement steps (the 6.5 GiB archive build); the measurement was only evidence for #11594 and is removed from `rust-gates.yml` (#11968).
