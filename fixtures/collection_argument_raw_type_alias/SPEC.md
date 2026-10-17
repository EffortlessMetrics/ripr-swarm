# Fixture: collection_argument_raw_type_alias

Spec: RIPR-SPEC-0094

## Given

`type r#Vec<T> = Numbers<T>` binds the same name as Vec, to a length-only collection.

## When

The real public check reads the neutral push rewrite.

## Then

Both effects stay weakly_exposed. Neutral and wrong-value compile and pass one named test; deleted push compiles and fails equality.

## Must Not

- Authenticate the standard vector from raw/unraw spelling differences.
- Count compilation failure or an unexecuted test as evidence.
