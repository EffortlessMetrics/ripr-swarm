<!-- section: Fixed -->
- TypeScript boundary liveness: a changed `if` statement whose branch pair cannot be read (one-line, or nested in an arrow or `function`) now withholds boundary credit instead of leaving the witness standing, so a dead expected value on those shapes no longer reads as a discriminator (#10776). A changed ternary is unaffected.
