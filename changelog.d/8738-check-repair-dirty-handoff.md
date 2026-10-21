<!-- section: Fixed -->
- `ripr check` no longer lists `ripr repair <seam-id>` as a bare runnable next
  action when the seam's production-file inline `#[cfg(test)]` target has
  uncommitted changes, which `ripr repair` refuses with exit 2; the line now
  states the commit-or-stash requirement up front and leaves the
  `ripr agent stub` route in place, using the edit cage's own committed-file
  predicate (#8738).
