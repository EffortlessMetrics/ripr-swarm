<!-- section: Fixed -->
- Repo and diff error-path shapes no longer treat `return Err(V)` or
  `return x.ok_or(V)?` written inside a closure, async block, or nested
  `fn` as the enclosing function's error. Those returns leave the inner
  body. Plain function-level returns are unchanged (#6904).
