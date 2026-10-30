<!-- section: Fixed -->
- Checked-in fixture JSON goldens no longer rewrite a backslash in asserted source text (an escaped quote or a `\u{..}` escape) to a forward slash; only a Windows host folds path separators (#11119).
