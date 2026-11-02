<!-- section: Fixed -->
- Python preview analysis now records `assert_awaited`, `assert_awaited_once`,
  `assert_awaited_with`, and `assert_awaited_once_with` on async mocks as
  `mock_expectation` oracles instead of unknown custom helpers (#6585, #12312).
