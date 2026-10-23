# Fixture: rust_unused_trait_effect_oracle

Spec: RIPR-SPEC-0094

## Given

An indexed `Iterator::next` can call `record`, but neither native test constructs
or dispatches the iterator. `direct_smoke` calls `record` without an assertion;
`parser_only` compares only its parser result. The review counterexample is
retained as compiling Rust, with a diff changing `push(0)` to `push(1)`.

## When

`cargo xtask fixtures rust_unused_trait_effect_oracle` analyzes both effect
families. Compile the input and a control deleting only the push with
`rustc --edition 2024 --test <source> -o <tests>` and execute each binary.

## Then

SideEffect and CallDeletion stay `weakly_exposed` with weak discrimination.
The visible parser equality has `observation_unconfirmed`. Native originals
and write-removal controls both pass 2/2; static output does not imply mutation
adequacy. Paired public controls preserve loop, `for_each`, explicit-loop and
operator observers on both direct and unique reverse dispatch paths.

## Must Not

- Admit an unrelated parser equality from an unused trait declaration.
- Promote unknown calls, wrong receivers or ambiguous reverse names.
- Erase the parser row or broaden #3161's unresolved value-flow claim.
