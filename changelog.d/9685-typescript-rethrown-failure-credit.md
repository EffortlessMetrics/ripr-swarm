<!-- section: Fixed -->
- TypeScript findings credit a test whose assertion sits in a `try` with a
  handler that only rethrows the caught error (`catch (e) { throw e; }`), or in
  the callback of a same-file wrapper that only rethrows. Both previously read
  `weakly_exposed` and asked for an assertion the test already had. A handler
  or wrapper that can swallow the failure is still not credited (#9685,
  RIPR-SPEC-0243).
