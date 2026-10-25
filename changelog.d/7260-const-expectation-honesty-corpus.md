<!-- section: Fixed -->
- The evidence-promotion honesty corpus now carries a self-computed `const` expectation case (must stay `weakly_exposed`) and a literal-const control (stays `exposed`), so removing the self-credit guard makes `cargo xtask goldens check` drift on the fixture (#7260).
