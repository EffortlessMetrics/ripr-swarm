<!-- section: Fixed -->
- Python: a parametrize argname whose `indirect=` value ripr cannot settle (a name such as `indirect=IND`, or a computed list) now counts toward the fixture limit (without creating a fixture edge), so the test keeps `unresolved_pytest_fixture` instead of being credited as if pytest passed the literal values (#10685).
