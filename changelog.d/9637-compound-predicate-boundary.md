<!-- section: Fixed -->
- A changed Rust predicate such as `s.starts_with('-') && s.len() > 1` no longer asks for a test where `s.starts_with('-') && s.len()` equals `1`, a value no input can produce. The equality-boundary reading now uses only the one comparison the edit changed, and gives no boundary when two comparisons or an unchanged comparison leave it ambiguous (#9637).
