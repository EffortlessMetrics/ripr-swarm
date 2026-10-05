<!-- section: Fixed -->
- A parametrized Python test that calls the changed code but asserts nothing
  no longer lists `pytest.mark.parametrize` as its oracle. Parameters are test
  inputs, not a check. Python rows with no recognized assertion now also say
  whether ripr established that the test has no assertion
  (`extraction_complete_no_assertion`) or saw something that could still fail
  it (`assertion_like_present_but_unresolved`): a custom, imported or
  same-module helper, a `self.` method, an unknown global, a non-built-in
  fixture, `usefixtures`, an asserting setup hook, an imported base class,
  `pytest.warns`, a `raise` or a process exit. Only the established state can
  later justify a "no assertion" reason. Verdicts are unchanged (#5571).
