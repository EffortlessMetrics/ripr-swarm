<!-- section: Fixed -->
- Generic LSP clients now see an analysis failure once, not twice. Two
  fixes for the same disclosure (#8636 and #8642) both merged, so the first
  failure produced two `window/showMessage` warnings; the per-failure-signature
  path from #8642 is kept and the per-transition path from #8636 is removed
  (#8583).
