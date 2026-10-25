# Fixture: python_module_formatted_generator_test

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The unchanged test has an explicit boolean equality assertion, followed by
`value = f"{(yield 1)}"`. CPython returns an unexecuted generator under both
module constant values; it executes neither the assertion nor the reader.
The explicit comparison keeps semantic boolean normalization from hiding the
missing generator guard.

## When

The single module constant changes from 10 to 12.

## Then

One nonempty `static_unknown` finding withholds credit for the changed value.

## Must Not

Credit an assertion in a deferred generator body. Formatted-string expressions
can carry executable generator syntax inside a single lexer string token.
The honesty corpus independently rejects a false promotion after re-blessing.
