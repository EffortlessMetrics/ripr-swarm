# Fixture: python_module_same_line_prior_assert

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The unchanged test puts `assert False; assert over_limit(11)` on one physical line.
CPython raises at the first assertion under both module constant values, before
calling the reader. The extracted later assertion must not borrow the first
assertion's physical line as evidence of its execution.

## When

The single module constant changes from 10 to 12.

## Then

One nonempty `static_unknown` finding withholds credit for the changed value.

## Must Not

Credit the unreachable assertion. The honesty corpus independently rejects
promotion even if the fixture's generated expected classification is re-blessed.
