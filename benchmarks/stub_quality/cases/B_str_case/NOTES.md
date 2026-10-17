# str_case

1. Changed behavior: digit-then-uppercase now gets an underscore: to_snake("v2Name") gives "v2_name" (and acronyms split per letter)

2. Ideal discriminating test:
```rust
assert_eq!(to_snake("v2Name"), "v2name");
```

3. Shape: generic free fn to_snake<T: AsRef<str>>(T) -> String; nested closure-free char loop
