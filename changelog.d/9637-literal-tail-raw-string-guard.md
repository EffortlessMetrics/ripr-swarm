<!-- section: Fixed -->
- A helper whose only body line is an ordinary string literal ending in `r` (`"other"`, `"user"`) or containing `#` is no longer refused as a raw string, so the literal-tail match-arm route resolves it; real raw strings (`r"`, `r#"`, `br#"`, `cr"`) still refuse (RIPR-SPEC-0164, #9637).
