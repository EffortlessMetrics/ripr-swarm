<!-- section: Fixed -->
- `ripr check --format human-full` and `ripr explain`: a changed predicate,
  return or error expression now shows the same span on both sides, so
  `before: bytes < unit` sits against `after:  bytes <= unit`. Before,
  `before:` kept the whole line (`if ... {`, or a trailing `;`) while `after:`
  showed the bare expression, which read as a structural edit.
  The old line is cut to the shape's span only when the edit falls inside the
  shape; match arms keep the whole old arm because their consumers parse it
  (#5312).
