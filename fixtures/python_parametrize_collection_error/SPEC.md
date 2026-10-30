# Fixture: python_parametrize_collection_error

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

A test stacks two parametrize marks. The healthy one, `@pytest.mark.parametrize("quantity, rate", [(100, 0.1), (50, 0.0)])`, has a row on the `100` boundary and an exact assertion `bulk_discount(quantity) == rate`. The other, `@pytest.mark.parametrize("q", [1], indirect=None)`, fails pytest collection ("expected Sequence or boolean for indirect, got NoneType"), so pytest 9.1.1 runs none of the test's cases.

## When

The boundary changes from `>= 100` to `> 100`.

## Then

One finding that lends the test no credit. The refused mark gives the test the `pytest.mark.parametrize(collection error)` activation control, so the finding carries the `decorator_indirection` limit instead of crediting the exact assertion.

## Must Not

Credit `exposed` from a test that never runs, or let the healthy sibling mark's rows count as activating inputs. The honesty corpus rejects that false promotion independently of the unit tests (#10867).
