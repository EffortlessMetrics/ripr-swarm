<!-- section: Fixed -->
- The TypeScript preview adapter no longer reads a `for (const [a, b] of rows)`
  loop over an empty `const` table as holding the loop body's assertion: the
  loop runs no iterations, so a change is not reported as exposed through it
  (#9683).
