# name_validator

1. Changed behavior: usernames of exactly MIN_LEN (3) chars are now rejected as TooShort (`<` became `<=`)

2. Ideal discriminating test:
```rust
assert!(validate_username("abc").is_ok());
```

3. Shape: free fn validate_username(&str) -> Result<(), Box<dyn Error>>; plus validate_all(&[&str]) -> Result<usize, Box<dyn Error>> using ?
