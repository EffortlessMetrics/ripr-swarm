<!-- section: Fixed -->
- TypeScript `paths` aliases now resolve through a root tsconfig
  `extends` chain (relative bases in the same directory, up to 8 hops),
  so the common `tsconfig.base.json` layout no longer reports aliased
  tests as `no_static_path`; package-name, cross-directory, cyclic, or
  missing bases still fail closed (#11204).
