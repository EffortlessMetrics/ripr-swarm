<!-- section: Fixed -->
- The published `ripr` source package no longer carries the rendered-help
  test harness (`tests/cli_help_hierarchy.rs` and its two helpers). Release
  qualification now runs every one of its cases against the executable
  installed from the qualified archive instead of a checkout build (#15216).
