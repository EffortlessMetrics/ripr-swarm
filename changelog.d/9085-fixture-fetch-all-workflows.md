<!-- section: Internal / CI -->
- The Rust-only feature, coverage, test-analytics, UB-review and Windows advisory workflows fetch the JSON owner-return fixture's locked dependencies too, so the pinned `owner_pin_execution` tests no longer fail offline after a workspace serde_json bump (#9085).
