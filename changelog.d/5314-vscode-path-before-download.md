<!-- section: Fixed -->
- The VS Code extension now uses a compatible `ripr` already on PATH before downloading a server when `ripr.server.version` is empty, so users who installed ripr with cargo or binstall make no network call on first use. Pinning `ripr.server.version` keeps the managed download first (#5314).
