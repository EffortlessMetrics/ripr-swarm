<!-- section: Fixed -->
- TypeScript: classifying method-like `{` blocks in a mock-payload owner now indexes matching parentheses once and memoizes each brace prefix, so a deeply nested `if` body stays near-linear in depth instead of stalling analysis (#7698).
