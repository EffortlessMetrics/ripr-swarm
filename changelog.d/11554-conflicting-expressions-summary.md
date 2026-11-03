<!-- section: Added -->
- The check summary gains an additive `conflicting_expressions` count, emitted only when it is above zero: the number of changed expressions that carry findings of more than one exposure class, so a second classification of the same expression is visible instead of silently inflating the per-class counts (#11554).
