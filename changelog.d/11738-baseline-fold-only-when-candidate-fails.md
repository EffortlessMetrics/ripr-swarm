<!-- section: Fixed -->
- `ripr check` no longer copies and re-reads a changed function's whole
  pre-change body for every related test of every changed line when it asks
  whether a test fails on both versions. The pre-change body is built only
  when the test already fails on the changed code, which a passing test never
  does. A long function with many changed lines no longer pays its length
  once per changed line and test there. Findings are unchanged (#11738).
