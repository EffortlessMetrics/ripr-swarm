# tok_lexer

1. Changed behavior: numbers of exactly MAX_NUMBER_LEN (8) digits are now rejected as NumberTooLong (`>` became `>=`)

2. Ideal discriminating test:
```rust
assert_eq!(tokenize("12345678"), Ok(vec![Token::Num(12345678)])); // 8 digits is allowed; 9 digits -> Err(NumberTooLong(0))
```

3. Shape: free fn tokenize(&str) -> Result<Vec<Token>, LexError> (custom error enum); private-const boundary; match on char
