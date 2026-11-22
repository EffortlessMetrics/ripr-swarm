<!-- section: Fixed -->
- `ripr pilot`: for a gap no test reaches, the terminal now says which
  function no test is seen calling and that the test to add must call it
  (asserting the changed value, or observing the effect for a call-only change),
  instead of describing the strongest oracle of tests that never run the code
  or saying there is no test file to use. The gap-count line says the total
  covers the whole repository (`ripr check` counts only the change) and that
  `--max-seams` lengthens the ranked list in the Markdown brief, since the
  terminal always shows one gap (#15278).
