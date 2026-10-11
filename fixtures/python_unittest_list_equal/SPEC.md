# Fixture: python_unittest_list_equal

Spec: RIPR-SPEC-0028

## Given

A return changes one short numeric list element. A direct standard unittest
module-level class uses `self.assertListEqual(labels(), [1, 2])` as its only test statement.
The expected list is independently authored and contains literal values.

Source provenance: CPython v3.14.0 commit
`ebf955df7a89ed0c7968f79faec1de49f61ed7cb`,
[assertListEqual](https://github.com/python/cpython/blob/ebf955df7a89ed0c7968f79faec1de49f61ed7cb/Lib/unittest/case.py#L1121)
and
[testEquality](https://github.com/python/cpython/blob/ebf955df7a89ed0c7968f79faec1de49f61ed7cb/Lib/test/test_unittest/test_case.py#L897).
This minimal fixture derives the assertion's equality behavior from those
sources; it copies no implementation or upstream test body.

## When

`cargo xtask fixtures python_unittest_list_equal`

## Then

One Python finding records strong exact-value evidence, whole-list observation,
and `exposed`, through the real CLI and existing admission/alignment gates.

## Must Not

Credit a same-name fake receiver, visible framework override, skipped test,
unrelated operand repeating the changed literal, self-computed expectation, or
slice, input-bearing owner call, or nested unittest class. Broader assertion forms remain explicitly limited. Parser refusal remains
unsupported syntax rather than a clean zero finding.

Independent native oracle: `python -m unittest discover -s tests` from `input`
runs one test successfully; replacing the return with `[1, 1]` fails that exact
test. The analyzer does not execute unittest and does not claim runtime adequacy.
