# Fixture: collection_argument_imported_constructor

Spec: RIPR-SPEC-0094

## Given

A function imported from another indexed file shadows the tuple constructor and builds a custom length-equality field that coerces to the owner vector.

## When

The real public check reads the neutral push rewrite.

## Then

Both effects stay weakly_exposed. Neutral and wrong-value compile and pass one named test; deletion compiles and fails equality.

## Must Not

- Infer actual field type from same-file constructor spelling.
- Omit indexed companion source from runtime compilation.
