<!-- section: Fixed -->
- `[perl].producer` now fails closed at parse time against the existing managed allowlist (`perl-ripr-facts`, `perllsp`, `perl-lsp`), so a typo is a key-named `ripr config validate` error instead of silently disabling managed producer mode (#7669).
