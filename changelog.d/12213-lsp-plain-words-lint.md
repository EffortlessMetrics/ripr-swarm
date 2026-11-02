<!-- section: Changed -->
- `cargo xtask check-user-text` now also scans the editor strings of the LSP
  sidecar (`lsp-source`), so the retired words `seam` and `discriminator` cannot
  return to status, hover, code-action, lens or error text (#12213). Remaining
  LSP wording says gap or spot: the hover for a wrapper callee, the `ripr pilot`
  hint when no repair packet exists, and the deferred-inventory disclosure.
