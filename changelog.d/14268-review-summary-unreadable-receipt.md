<!-- section: Fixed -->
- `ripr agent review-summary` and the generated CI summary no longer describe
  a receipt file that is present but unreadable, truncated or not a receipt
  as a missing receipt. The surface reports `invalid` with the read failure,
  the movement and "what changed" text say the receipt is unreadable or
  invalid, and the CI summary names the file instead of "No agent receipt
  yet". For a receipt that is not advisory, the remaining step now says to
  reissue the receipt instead of repeating the receipt's own guidance
  (#14268).
