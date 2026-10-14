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

Composition review of `6771a8473724b474381f9648d9ea97a2476d65ce`, against
main `872eb81f28a3f363abe27c3cdb88f223f12d8b82`, found two more discriminating
controls through the real CLI:

- Two module-level unittest classes with the same `test_labels` method name:
  the valid second class's `beta()` list assertion was `weakly_exposed` because
  assertion expansion selected the first class by file/name alone. Native
  correct/wrong runs each registered two tests; only wrong `B.test_labels`
  failed. Declaration-line identity keeps the credited assertion on its row.
- `self.assertListEqual(labels().fixed_list(), [1, 2])`: the owner returns
  `Answer(0)` versus `Answer(1)`, while `fixed_list()` always returns `[1, 2]`.
  Both native variants ran one test without failures/errors/skips, but the CLI
  reported `exposed` / strong / yes. The compared callee must be a static name
  or attribute chain rooted in a name; calls/subscripts in that chain remain
  unknown. The derived-result fixture and honesty row pin the non-promotion.

After the composition repairs, all six focused regressions and all 568 Python
adapter tests pass. The same real CLI inputs report the duplicate-method case as
`exposed` / strong / yes and the derived-result case as `static_unknown` /
unknown. Imported `TestCase` and module-qualified free-call positives retain
credit; removed, unrelated, fake, effectful and nested-registration controls do
not. These outcomes retain independent literal expectations.

This proves a bounded static evidence correction and native discrimination of
these source-derived examples. It does not establish general unittest support,
runtime test adequacy, hosted CI, branch protection, or merge readiness.

Published review of `50c3b2a10675055268b2f59cd0a3c6d842552866` identified
a Python-equality edge: `[1, 1]` and `[1, True]` compare equal. Independently
executing the exact assertion against each implementation runs one test with
zero failures/errors and exit 0, yet that candidate's real CLI reports one
`exposed`, strong, yes finding. The permanent equal-values fixture and honesty
row require one `weakly_exposed`, exact-value, strong finding and forbid
promotion: the assertion is strong in shape but cannot distinguish this edit.

Whole-list change credit now requires literal-list returns on both sides and
either unequal lengths or a provably different same-kind scalar value. Decoded
values avoid false credit for alternate integer, float or string spellings.
Mixed scalar types remain conservatively uncredited, including actually unequal
large integers versus rounded floats; no lossy numeric conversion supplies proof.
Empty-list length changes retain a real discrimination witness. Nonliteral,
unpaired and unsupported return shapes remain outside this bounded credit.

Rendered CLI review of source `414b63c61dbc2a24753acd59380bd37d18a02505`
found that its corrected weak finding still emitted an exact-assertion repair
card for the assertion already present. The equal-values control therefore also
requires no invented typed discriminator, placement, repair card or strengthen-
test next step. The warning and strong oracle remain; withholding ungrounded
repair authority does not suppress the finding or promote the change.

Final production `0cd2b3798f445e4d87280a329862400627f83745` passes all 569
Python adapter tests. Its real CLI retains the five literal fixture outcomes
and omits every ungrounded repair field/card for equal values. All 410 goldens,
honesty checks and dogfood pass. The final independent corpus guard rejects the
retained actual `414b63c61` bad-card JSON/human output with exit 1 specifically
for `must_not_have_verify_command`; restored repaired artifacts pass with exit 0
and their original SHA-256 values are verified. This mutation challenges dishonest
reblessing of an advisory card, separately from non-promotion checks.

Review of `61550c57f9a092dfb5e24967fd55f4f7bdb4f168` exposed a spelling
mismatch: native `labels ()`, `(labels)()` and `labels . labels ()` calls each
run one unittest, pass `[1, 2]` and reject `[1, 1]`, but the real CLI reports
weak exposure with unknown oracle strength instead of the plain call's exposed,
strong, yes result. The independently literal CLI verifier rejects that exact
retained binary with exit 1. The positive fixture now uses `labels ()` to retain
this end-to-end regression without adding another fixture.

The bounded list path now obtains its direct operand's callee from the admitted
AST and shares that identity across test relation, operand filtering and module-
call alignment. Generic textual call matching is unchanged. Seven spelling and
import cases require exposed/strong/yes; seven rival, rebound, derived-call and
subscript controls forbid exposure. Static-callee validation precedes name
extraction, so grouping cannot turn a derived result into an owner call.

Repair source `e4cc301590d86f3566d0088fcc283f0906d3f537` matches the four
changed Rust blobs exercised by eight focused regressions and all 570 Python
adapter tests. Its real CLI passes the unchanged literal verifier (exit 0): all
four call spellings are exposed/strong/yes, while equal-value and unsupported-
shape controls retain their prior outcomes. The executable SHA-256 is
`d26c0fdbbb51f43e881ba7a34a91d4250ec22084aee2ba9dc99b2fd4bcb91960`.
All 410 goldens, honesty and dogfood pass; generated drift is limited to the
positive fixture's three oracle-text fields and its blessing record.
