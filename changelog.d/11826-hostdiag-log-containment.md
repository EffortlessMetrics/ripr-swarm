<!-- section: Fixed -->
- The VS Code test host copies its primary log directory only when it is a real directory under the artifact root: a `..`, absolute or symlinked component is refused, matching the matrix receipt containment (#11826). Windows junctions are covered by the same `lstat` rule but not exercised natively.
