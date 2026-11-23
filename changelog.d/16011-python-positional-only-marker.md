<!-- section: Fixed -->
- Python preview no longer reports `exposed` for a `def` header edit that only
  adds, removes or moves the positional-only `/` marker when no related test
  passes a moved parameter by keyword; it reads `weakly_exposed` and names the
  keyword call to add (#16011).
