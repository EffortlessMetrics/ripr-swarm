<!-- section: Changed -->
- An editor refresh whose deadline (or supersede/cancel) fires while
  analysis is running is now classified as that abort, not as an analysis
  failure. The refresh wraps the abort as `workspace analysis failed:
  analysis cancelled: DeadlineExceeded`, which the old `analysis cancelled:`
  prefix check missed. Cancellation is now decided from the refresh token's
  observed abort and a typed internal error, never from error text; public
  wording is unchanged (#4860).
