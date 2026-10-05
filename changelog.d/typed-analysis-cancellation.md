<!-- section: Fixed -->
- An editor refresh whose deadline (or supersede/cancel) fires while
  analysis is running now reports that abort (for example
  `DeadlineExceeded`) instead of an analysis failure, and `ripr
  review-comments` now writes its timeout receipt when the budget deadline
  aborts diff discovery, language facts or canonical analysis. The wrapped
  abort text (`workspace analysis failed: analysis cancelled: …`) used to
  miss the old `analysis cancelled:` prefix check. Cancellation is now
  decided from the cancellation token's observed abort and a typed internal
  error, never from error text; message wording is unchanged (#4860).
