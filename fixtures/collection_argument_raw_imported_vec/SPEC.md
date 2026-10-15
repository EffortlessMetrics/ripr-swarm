# Fixture: collection_argument_raw_imported_vec

Spec: RIPR-SPEC-0094

## Given

An out-of-line module declares `r#Vec<T>` with length-only equality, imported by a workspace glob.

## When

The real public check reads the neutral push rewrite.

## Then

Both effects stay weakly_exposed. Neutral and wrong-value compile and pass one named test; deletion compiles and fails equality.

## Must Not

- Authenticate Vec from unnormalized declarations in another indexed file.
- Omit companion sources from runtime compilation.
