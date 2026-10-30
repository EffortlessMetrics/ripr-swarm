# Fixture: python_dataclass_default_unknown_method_decorator

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A plain `@dataclass` `Invoice` with a `tax_rate` default, an unknown decorator on another method. The test
builds `Invoice(100)` without `tax_rate` and asserts the rounded total.

## When

The `tax_rate` default changes from 0.2 to 0.25.

## Then

One nonempty `static_unknown` finding withholds credit for the changed default.

## Must Not

Credit the assertion as distinguishing the old and new default. The decorator runs at class creation and may replace the class contents, so the model is not the class.
The honesty corpus independently rejects a false promotion after re-blessing.
