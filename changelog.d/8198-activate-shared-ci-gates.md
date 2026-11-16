<!-- section: Internal / CI -->
- Required Rust CI uses the trusted base's lightweight user-text matcher when
  available, retaining the legacy base judge, and evaluates golden and fixture
  contracts once through their shared producer with both reports retained (#8198).
