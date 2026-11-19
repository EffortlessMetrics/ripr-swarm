<!-- section: Fixed -->
- Python: `assertRaises` / `assertRaisesRegex` / `pytest.raises` in the callable form with a module-qualified or aliased callable (`self.assertRaisesRegex(ValueError, "blank level", levels.parse, "")`) now links the test to the function it calls, as the bare-name callable already did. Such a test read as a heuristic link with an `unknown` oracle before. (#15078)
