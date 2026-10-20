# ripr golden drift report

Status: fail

This report summarizes expected-output drift for reviewer inspection. It never blesses goldens.

## Provenance

- binary: `/workspace/ripr-reverse-target/debug/ripr`
- ripr version: `ripr 0.11.0 (99ff69b63e38a3e0627d859be898e74299f8bb69)`
- revision: `99ff69b63e38a3e0627d859be898e74299f8bb69`

## Summary

- drift entries: 0
- fixture errors: 1

## Drift

None detected.

## Fixture / Golden Violations

## Violations

```text
fixture dependency preparation failed for owner_return_pin_fallible_literal
environment: CARGO_HOME="/workspace/.cargo" RUSTUP_TOOLCHAIN="1.95.0" RUSTUP_HOME="/workspace/.rustup" RUSTUP_AUTO_INSTALL="0"
cargo metadata --locked --offline --format-version 1 --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml failed with exit status: 101; stdout: ; stderr: error: failed to download `anyhow v1.0.100`

Caused by:
  attempting to make an HTTP request, but --offline was specified
next: run `cargo xtask goldens prepare` with this same environment (or `cargo xtask goldens prepare --offline` with a seeded cache), then retry the fixture/golden check. The fixture requires its locked anyhow 1.0.100; fetching another Cargo home does not prepare this subprocess. If the pinned toolchain is missing, install it explicitly before retrying offline. Do not bless availability drift.
```

