<!-- section: Docs -->
- Baseline review `owner` and `review_after` are documented as operator-set
  ledger content: `baseline create` writes them `null`, shrink-only
  `baseline update` preserves entries, and `ripr zero status` evaluates a
  deadline only on a complete review record, pinning that an incomplete record
  stays `missing_metadata` (#6833 residual).
