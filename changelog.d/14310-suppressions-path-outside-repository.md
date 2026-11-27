<!-- section: Security -->
- ripr no longer reads a `suppressions.path` policy that escapes the
  repository being analyzed. A `..` path, an absolute path, or a link whose
  target leaves the repository stops with one fixed message naming the entry
  and how to repair it, before the target is opened. A link whose target stays
  inside the repository still loads, and a missing policy file still means no
  accepted debt (#14310).
