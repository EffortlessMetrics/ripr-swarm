<!-- section: Fixed -->
- Python: a test whose whole body sits on its `def` line (`def test_x(fee_case): assert ...`) no longer skips the `unresolved_pytest_fixture` limit, which had produced a false `exposed`. The parameter lines of a multi-line signature no longer count as fixture uses, so such tests read like their one-line-signature twins (#10685).
