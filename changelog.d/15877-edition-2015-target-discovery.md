<!-- section: Fixed -->
- `ripr check` follows Cargo's edition 2015 rule that declaring any `[[bin]]`, `[[test]]`, `[[bench]]` or `[[example]]` turns autodiscovery of that kind off: an undeclared `src/main.rs` or `src/bin/*.rs` no longer seeds findings, and an undeclared `tests/*.rs` no longer credits a discriminator, unless the matching `auto*` key is `true` (#15877)
