<!-- section: Fixed -->
- Bare `ripr repair` refusals now say which seam each reason is about. When
  your diff is available, seams outside your changed lines that cannot start a
  repair either (a route that is not ready, a named limitation) are counted
  instead of listed, so a refusal no longer reads as "no gap here" and "missing
  assertion" about the same change (#11111).
