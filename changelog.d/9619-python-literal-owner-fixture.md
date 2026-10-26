<!-- section: Fixed -->
- A Python pytest test that receives the owner's object from a fixture that
  only returns `OwnerClass(<literals>)` (`@pytest.fixture` /
  `def account(): return Account(100)`) now reads like the same construction
  inlined in the test, instead of abstaining with `unresolved_pytest_fixture`.
  Any other fixture shape, and any other fixture or hook that can reach the
  instance first, keeps the limit (#9619).
