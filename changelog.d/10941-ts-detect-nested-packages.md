<!-- section: Changed -->
- TypeScript auto-detection (no `ripr.toml`) now also recognizes a `tsconfig.json`, `jsconfig.json` or detectable TypeScript source one level below the repository root and under `packages/*`, `apps/*`, `libs/*` and `libraries/*`, so monorepos such as remeda no longer report `language_adapter_unavailable`. Vendored, hidden and deeper trees never trigger it (#10941).
