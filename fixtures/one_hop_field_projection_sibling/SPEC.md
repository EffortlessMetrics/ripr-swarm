# Fixture: one_hop_field_projection_sibling

Spec: RIPR-SPEC-0159

## Given

A changed primitive count field and the retained sibling_field assertion control.

## When

`cargo xtask fixtures one_hop_field_projection_sibling` analyzes the single-line field change.

## Then

The field reports `propagation_unknown`. The strong assertion cannot observe this changed field path.

## Must Not

Pool reach and unrelated equality, credit a discarded result, or substitute a sibling field.
