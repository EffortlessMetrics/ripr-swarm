<!-- section: Fixed -->
- Python preview now recognizes `self.assertIsNone` as an exact-value assertion
  and `self.assertIsNotNone` as a weak relational one, instead of treating
  them as unresolved custom helpers; neither is promoted to `exposed` without
  an expected value (#6585).
