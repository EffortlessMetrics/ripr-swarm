<!-- section: Fixed -->
- Refuse cross-crate test relations through aliased or forwarded include macros,
  and extern rebinding or source splices following character literals (#13714).

Preserve test-side re-export binding checks after character literals, including aliased roots, local modules, type aliases and globs.
