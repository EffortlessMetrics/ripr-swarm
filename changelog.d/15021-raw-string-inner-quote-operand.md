<!-- section: Fixed -->
- Pin that whitespace inside a raw string containing an inner quote (`r#"a "b"  c"#`) stays operand content when a guard operand is compared, so an edit that changes only that whitespace is not read as a retained operand (#15021).
