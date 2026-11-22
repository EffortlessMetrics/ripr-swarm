<!-- section: Fixed -->
- `ripr check` no longer reports findings in a module that only a `mod` behind a `#[cfg]` no build satisfies mounts (`#[cfg(any())] mod helper;`, or a module inside a `#[cfg(any())]` inline module); rustc strips such a declaration before it loads the file. Feature, target and `test` gates still mount their module (#15877)
