<!-- section: Fixed -->
- TypeScript preview: a flipped `typeof` guard (`typeof input === "function"`
  to `!==`) is no longer offered as a repair start when a test already passes
  an input of another written kind (`resolveValue(42)`) and asserts the result
  exactly. Negating the operator sends every input down the other branch, so
  that test already notices the edit and the finding reads `exposed`. This
  holds only when the guard is the whole condition; a negated clause joined by
  `&&` / `||`, or an edit that changes the kind name, still needs an input of
  that kind (#12281).
