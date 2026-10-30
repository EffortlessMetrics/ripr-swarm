# Fixture: rust_same_test_disconnected_effect_oracle

Spec: RIPR-SPEC-0094

## Given

Real Counter dispatch shares a test with a parser equality after dispatch, a
parser equality before dispatch, or a fresh-handle Observation equality.
A separate direct smoke caller has no assertion.

## When

`cargo xtask fixtures rust_same_test_disconnected_effect_oracle` analyzes the
push change. Compile the original and a control deleting only the push with
`rustc --edition 2024 --test <source> -o <tests>` and execute both.

## Then

Both effect families remain weakly_exposed with weak discrimination and visible
observation_unconfirmed rows. Originals and compiling removals each pass 4/4.
Public paired controls retain real ordered shared-sink observers across direct
and reverse loops, operators, for_each and explicit next; their removals fail.

## Must Not

- Borrow a dispatch, instance or later execution for an unrelated assertion.
- Admit fresh handles, aliases, custom equality, inequality or deferred code.
- Erase existing corpus entries or claim general alias support or adequacy.
