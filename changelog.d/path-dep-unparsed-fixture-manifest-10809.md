<!-- section: Fixed -->
- An unparsable `Cargo.toml` that no workspace `members` pattern selects and no path dependency targets (for example a deliberately invalid test fixture) no longer marks the path-dependency graph `limited`, so large repositories such as cargo report a complete analysis instead of `partial_with_limitations` (#10809).
