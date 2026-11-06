<!-- section: Fixed -->
- Rust boundary analysis reads every supported direct free-function call on a
  source line, so a second call at the boundary no longer appears in the evidence
  beside a claim that its input is missing. Mixed-owner and shadowed calls remain
  conservative. Recovered inputs pair only with the actually observed call result;
  masking wrappers and unsupported operators cannot borrow their boundary credit (#13373).
- Preserve whole primitive-boolean negation observers only after existing type and binding checks; nominal negation cannot borrow that identity (#13373).
