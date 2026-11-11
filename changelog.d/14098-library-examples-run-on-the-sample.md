<!-- section: Fixed -->
- Library examples: `check_diff`, `explain` and `render_json` analyze a scratch
  copy of the bundled sample with a valid diff, so they report a complete
  analysis in under a second instead of an incomplete one after scanning the
  whole crate, and report the changed `validate_currency` line as 22 rather
  than 21. `check_diff` also shows the same findings after the tests pin the
  threshold and the exact error. Tests now run all three (#14098).
