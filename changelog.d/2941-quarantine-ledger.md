<!-- section: Changed -->
- LSP backend: the open-document store, save digests and last published diagnostics now live in an owned `QuarantineLedger` (lsp/state.rs) with the digest and quarantine-marker transitions as methods; behavior is unchanged (#2941).
