<!-- section: Fixed -->
- An ownerless probe (for example a changed constant declaration) analyzed under a relative `--root` now keeps its own inline tests as related tests, so the finding reports the reaching test and moves from `no_static_path`-style evidence to `static_unknown` with a related test; the classification stays `static_unknown` and is not promoted to an actionable finding (#7617).
