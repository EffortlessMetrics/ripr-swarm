<!-- section: Fixed -->
- A production fn sharing a physical line with a `#[test]` fn no longer loses its probe: a line owned by both a production and a test function stays production-eligible instead of being classified as test evidence (#9076).
