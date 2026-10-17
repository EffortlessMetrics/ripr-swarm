# Fixture: rust_competing_reverse_effect_oracle

Spec: RIPR-SPEC-0094

## Given

A genuine shared-sink trait observer reads a competing production write that
masks removal of the changed effect. The unchanged compiling observer passes
before and after removal, so proximity plus the shared handle is insufficient.

## When

`cargo xtask fixtures rust_competing_reverse_effect_oracle` analyzes the first write change. Compile the
source and removal control with `rustc --edition 2024 --test <source> -o <tests>`.

## Then

The changed effect stays weakly_exposed with weak discrimination and a visible
observation_unconfirmed observer. A single production field/parameter use is
required by the bounded exception; ordinary real shared-sink controls remain
positive and fail on their changed-write removal.

## Must Not

- Credit a replacement write as protection for the changed write.
- Broaden aliases, erase decoys or imply general mutation adequacy.
