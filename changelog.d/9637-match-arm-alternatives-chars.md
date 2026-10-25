<!-- section: Fixed -->
- A caller-side boundary over a string `match` helper is now credited when the helper's arms use `|` alternatives, escaped string literals or char literals against a char scrutinee, or call a helper that returns one literal, so tests that pin each arm expose the changed constant instead of reporting an unknown operand (RIPR-SPEC-0164, #9637).
