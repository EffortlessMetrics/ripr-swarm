<!-- section: Fixed -->
- `ripr explain` and the human evidence window now gloss `local flow reaches
  unknown sink` with what it means — ripr could not classify where the changed
  value ends up — and the "To change the verdict" recipe is qualified as
  provisional when that unclassified sink is the only propagation evidence,
  instead of naming an assertion target shape ripr said it could not classify
  (#8744).
