# Fixture: python_dataclass_default_call_default_async

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A plain `@dataclass` `Invoice` with a `tax_rate` default, a call default on an async method that runs at class creation. The test
builds `Invoice(100)` without `tax_rate` and asserts the rounded total.

## When

The `tax_rate` default changes from 0.2 to 0.25.

## Then

One nonempty `static_unknown` finding withholds credit for the changed default.

## Must Not

Credit the assertion as distinguishing the old and new default. Importing the class raises at class creation under both defaults, so the assertion cannot tell them apart.
The honesty corpus independently rejects a false promotion after re-blessing.
