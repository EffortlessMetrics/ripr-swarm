<!-- section: Fixed -->
- TypeScript tests that import a class through a barrel (`import { Url } from
  "../src"` with `export * from "./u"` in the index) now relate to that class's
  methods and getters instead of reporting `no_static_path`; the relation
  discloses as `re_export_chain_followed` (#13592).
