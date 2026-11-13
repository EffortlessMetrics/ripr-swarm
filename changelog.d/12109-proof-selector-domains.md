<!-- section: Internal / CI -->
- Guard the actionability, gate-mode and repair-state proof selectors against
  omitted, duplicated or newly added enum constructors, with independent input
  witnesses and the existing literal behavior oracles (#12109).
- Keep the gate proof's zero/one-entry domain in immutable slices, avoiding
  unrelated allocation and vector cleanup in its native model (#12109).
