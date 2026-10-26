<!-- section: Fixed -->
- Rust: an owner return pin no longer credits an expected value that a test-file helper computes from the owner, whether the helper is called directly, bound through a `let` chain, reached through a second helper, or folded into a constant (#7714).
- The evidence-promotion honesty corpus now carries a helper-computed expectation case (must stay `weakly_exposed`) and a literal-helper control (stays `exposed`) (#7714).
