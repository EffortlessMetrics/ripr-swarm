<!-- section: Fixed -->
- Follow up on nested config discovery: stop at dangling or cyclic `.git`
  entries so enclosing `ripr.toml` policy cannot override a selected project's
  defaults; preserve local config and explicit CLI precedence (#2804).
