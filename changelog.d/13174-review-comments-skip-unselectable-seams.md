<!-- section: Changed -->
- `ripr review-comments` no longer evaluates second-stage caller-file seams
  that cannot be selected once a first-stage seam has matched, which is where
  a small diff on a large workspace spent its budget before timing out.
  Selection, warnings and omission counts are the same. One reported number
  changes: `classified_seams_considered` in the `analysis_scope` section, and
  the canonical-analysis progress count, now count evaluated seams, so they
  are smaller when seams are skipped (#13174).
