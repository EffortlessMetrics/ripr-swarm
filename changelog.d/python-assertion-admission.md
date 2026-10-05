<!-- section: Fixed -->
- A parametrized Python test that calls the changed code but asserts nothing
  no longer lists `pytest.mark.parametrize` as its oracle. Parameters are test
  inputs, not a check. Python rows with no recognized assertion now also say
  whether ripr established that the test has no assertion
  (`extraction_complete_no_assertion`) or saw something assertion-like it
  could not resolve, such as a custom helper, a same-module wrapper, an
  opaque fixture or a `raise` (`assertion_like_present_but_unresolved`).
  Only the established state can later justify a "no assertion" reason. Verdicts
  are unchanged (#5571).
