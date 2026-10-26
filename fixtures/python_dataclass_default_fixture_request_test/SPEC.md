# Fixture: python_dataclass_default_fixture_request_test

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A plain `@dataclass` `Invoice` with a `tax_rate` default, a test that requests a fixture. The test
builds `Invoice(100)` without `tax_rate` and asserts the rounded total.

## When

The `tax_rate` default changes from 0.2 to 0.25.

## Then

One nonempty `static_unknown` finding withholds credit for the changed default.

## Must Not

Credit the assertion as distinguishing the old and new default. A fixture can patch the class before the assertion runs, so the assertion may not read the edited default.
The honesty corpus independently rejects a false promotion after re-blessing.
