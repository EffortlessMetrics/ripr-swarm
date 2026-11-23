# ripr-contract

Internal implementation package for `ripr` (ADR 0027).

`ripr-contract` holds the domain DTOs and adapter result shells used by the
`ripr` facade. It is published to crates.io only in lockstep with `ripr`
(same version, exact version requirement from its consumer) because Cargo
packaging requires a published package for a normal dependency.

The public API is the `ripr` crate: contract types are re-exported at their
established `ripr::` paths. This crate's own API is not a separate public
contract, carries no separate stability promise, and may change in any
release. Use `ripr`, not this crate, as a direct dependency.

## License

Licensed under either of

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.
