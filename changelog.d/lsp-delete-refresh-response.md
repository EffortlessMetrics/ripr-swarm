<!-- section: Internal / CI -->
- The external-delete LSP lifecycle test consumes both its refresh response and
  first diagnostics before checking deletion, with deterministic wire-order
  and invalid-publication controls (#14099, #14467).
