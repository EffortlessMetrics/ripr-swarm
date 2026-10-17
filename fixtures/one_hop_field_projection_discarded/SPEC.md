# Fixture: one_hop_field_projection_discarded

Spec: RIPR-SPEC-0159

## Given

A changed primitive count field and the retained discarded_owner_result assertion control.

## When

`cargo xtask fixtures one_hop_field_projection_discarded` analyzes the single-line field change.

## Then

The field reports `propagation_unknown`. The strong assertion cannot observe this changed field path.

## Must Not

Pool reach and unrelated equality, credit a discarded result, or substitute a sibling field.
