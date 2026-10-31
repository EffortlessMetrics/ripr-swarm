<!-- section: Fixed -->
- Beside a `Cargo.toml`, JavaScript or TypeScript under `test/` or `tests/` no longer counts toward package-led detection, so fixture files in a Rust crate that has a `package.json` do not switch on TypeScript preview; `src/` and the root still count (#11418).
