<!-- section: Fixed -->
- TypeScript monorepos: a `tsconfig.json` that `extends` a shared base in another directory (for example `"extends": "../../tsconfig.base.json"`) now resolves its `paths` aliases, with an inherited `baseUrl` anchored at the base that declares it as `tsc` does, instead of reporting an unsupported `extends` (#11204).
