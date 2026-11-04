# Fixture: tuple_equal_inputs

Spec: RIPR-SPEC-0229

Additional contract: RIPR-SPEC-0108; #12808.

## Given

A one-field integer tuple method changes subtraction to addition. Its external
related test calls the method with two zero values and pins the projected result.

## When

Analyze the change in `diff.patch` against the fixture workspace.

## Then

The return-value finding is `weakly_exposed`: both versions return zero on the
observed inputs. The verdict-corpus runtime controls independently run the test
against the old and new expressions.

## Must Not

- Credit this input as a discriminator for the changed arithmetic.
- Infer runtime mutation results from the static finding.
