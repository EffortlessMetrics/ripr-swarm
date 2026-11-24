<!-- section: Changed -->
- `ripr agent stub` now fills a boundary stub's other parameters from an existing test's literal call instead of leaving `todo!("ripr: choose `amount`")`: for `rate_bps >= 10_000` beside `assert_eq!(tax_cents(10_000, 825), Ok(825))`, the stub writes `amount = 10_000` and names the test it came from (#8361).
