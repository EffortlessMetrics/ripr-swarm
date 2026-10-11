# Independent oracle for #7312

Pinned source: CPython v3.14.0 `ebf955df7a89ed0c7968f79faec1de49f61ed7cb`,
`Lib/unittest/case.py::assertListEqual` and
`Lib/test/test_unittest/test_case.py::testEquality`. The fetched `case.py` has
SHA-256 `a79b422eec43c90352e25a54bce8cee106811974c8dea9405199e4caa689ffe9`.
The fixture and literal expected values are independently authored.

Native execution used CPython 3.12.14, separately from this 3.14 source pin.
Running `python -m unittest discover -s tests` in the fixture's `input`:

| Return | Tests run | Failures | Errors | Exit |
| --- | ---: | ---: | ---: | ---: |
| `[1, 2]` | 1 | 0 | 0 | 0 |
| `[1, 1]` in a separate source copy | 1 | 1 | 0 | 1 |

The failure is `Lists differ: [1, 1] != [1, 2]`, at `test_labels`' exact
assertion. Neither setup nor registration failure supplies this witness.

A second independently authored native matrix compares `[0, 'ready']` with
`[0, 'pending']`. The exact literal-list assertion rejects only the wrong
implementation. An unrelated equal-list assertion, a no-op fake receiver,
removal of the assertion, and a skipped test all accept both implementations.
Ten rows run one registered test each with zero setup errors; only the wrong
exact row has one failure, and the two skip rows have one recorded skip each.

Analyzer removal witness: on baseline production
`3865a4d07e28c67be720c32692ed9896875bfda9`,
`cargo test -p ripr --lib unittest_list_equal` runs three tests: two negative
tests pass and the positive fails with `weakly_exposed` versus literal expected
`exposed`. After the initial source repair, all three pass. A preliminary invalid test
field and parser-Identifier compile errors were corrected; neither compilation
failure is counted as the behavioral red witness.

Real CLI checks (`ripr check --root <source copy> --diff diff.patch --mode fast
--json`) independently expect one finding: the exact fixture is `exposed`,
`exact_value`, `strong`, for both `unittest.TestCase` and imported `TestCase`
bases; removal, an unrelated equal-list assertion and a fake
receiver are unexposed. A malformed changed owner yields zero findings with
`analysis_complete: false`, `partial_with_limitations` and a producer-owned
Python parse failure; zero findings here are not a clean result. A malformed
test file instead yields one `no_static_path` finding with `analysis_complete:
true` and preview advisory output. That existing test-index limitation is not
repaired or treated as evidence for the supported assertion shape.

Independent LLM review of candidate `4672156a843860e375ca88f43822e64e588abc1d`
found two false promotions, both reproduced through the real CLI:

- `labels(abort())` inside the list assertion: `abort` raises `SkipTest`, so
  correct and wrong implementations each register one skipped test and never
  call the owner, while that candidate reported `exposed`, strong, yes.
- `Outer.Inner.test_labels` in nested unittest classes: default native discovery
  registers zero tests (CLI exit 5), while that candidate reported the same
  `exposed` credit.

The repaired support boundary requires a zero-argument owner call and a
module-level class. The two permanent control fixtures and their
`evidence-promotion-honesty-corpus/corpus.json` rows independently require one
`static_unknown` finding and forbid promotion. They prevent dishonest golden
reblessing, beyond unit-test source expectations.

This proves a bounded static evidence correction and native discrimination of
these source-derived examples. It does not establish general unittest support,
runtime test adequacy, hosted CI, branch protection, or merge readiness.
