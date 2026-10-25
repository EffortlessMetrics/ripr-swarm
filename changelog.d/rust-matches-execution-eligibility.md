<!-- section: Fixed -->
- Rust return/error `assert!(matches!(..))` checks now share test execution admission, so ignored tests and broad expected-panic fallbacks cannot supply strong exposure evidence. Bounded expected-panic controls retain their conservative readings, while unresolved competing panic sources withhold credit. Existing equality outcomes are unchanged (#9478).
