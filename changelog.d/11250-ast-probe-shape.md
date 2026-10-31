<!-- section: Fixed -->
- The TypeScript preview now derives a changed line's probe family from the
  oxc AST node covering that line instead of the raw line text, so a ternary
  inside a string, a statement inside a switch arm, and a multiline
  return-ternary continuation are no longer misread from their text (#11250).
