<!-- section: Fixed -->
- Python findings now credit `self.assertAlmostEqual` at its default
  tolerance, the callable form of `assertRaisesRegex` (the callable forms
  of `assertRaises` and `pytest.raises` now relate the test as a call), and a
  `pytest.raises(...) as exc` or `self.assertRaises(...) as cm` block whose
  raised error is compared with `==` afterwards
  (`str(exc.value) == "'empty'"`). A fluent
  `assert_that(...).is_equal_to(...)` chain now reads as an opaque custom
  helper (`static_unknown`) instead of a gap. Each of these tests fails on
  its corpus mutant, so the earlier `weakly_exposed` was a false actionable
  verdict (RIPR-SPEC-0238 rules 3 and 12 to 14, #6603).
- A Python change that removes or adds an additive call term
  (`total + shipping_fee(subtotal)`) is no longer `exposed` when every
  strong related test call gives that term the value 0, for a same-module
  callee that is one guarded return. The finding names the missing input
  where the term is nonzero (RIPR-SPEC-0238 rule 20, #6733).
