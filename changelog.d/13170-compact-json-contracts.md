<!-- section: Fixed -->
- `ripr check --format json-compact` now retains the ordinary JSON format's
  structured refusal documents and explicit suppression-policy support,
  including the notice when a detected policy was not applied. Compact output
  removes only insignificant whitespace after the existing report, provenance,
  action and findings-budget projections; member order, number spelling,
  string escapes and bounded timeout recovery remain intact (#13170, #13103).
