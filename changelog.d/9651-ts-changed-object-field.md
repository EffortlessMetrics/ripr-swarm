<!-- section: Fixed -->
- TypeScript preview: an assertion on the changed field of a single-line
  returned object literal (`expect.soft(d.perimeter).toBe(14)` after
  `perimeter: 2 * (w + h)` became `2 * w + h`) now counts as observing the
  change, so the second term of an `expect.soft` pair no longer reads as
  unreached (#9651).
