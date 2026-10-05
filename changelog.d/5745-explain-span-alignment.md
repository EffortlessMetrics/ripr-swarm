<!-- section: Fixed -->
- `ripr check --format human-full` and `ripr explain`: a changed predicate,
  return or error expression now shows the same span on both sides, so
  `before: bytes < unit` sits against `after:  bytes <= unit`. Before,
  `before:` kept the whole line (`if ... {`, or a trailing `;`) while `after:`
  showed the bare expression, which read as a structural edit.
  The removed line is cut to the added line's span only when both lines share
  the same framing; otherwise it stays whole (#5312).
