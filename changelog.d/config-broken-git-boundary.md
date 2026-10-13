<!-- section: Fixed -->
- Follow up on nested config discovery: stop at dangling or cyclic `.git`
  entries so enclosing `ripr.toml` policy cannot override a selected project's
  defaults. Implicit check and cache root selection respect the same boundary;
  preserve local config and explicit CLI precedence (#7597).
