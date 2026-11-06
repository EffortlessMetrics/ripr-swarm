<!-- section: Fixed -->
- Rust boundary analysis reads every supported direct free-function call on a
  source line, so a second call at the boundary no longer appears in the evidence
  beside a claim that its input is missing. Mixed-owner and shadowed calls remain
  conservative (#13373).
