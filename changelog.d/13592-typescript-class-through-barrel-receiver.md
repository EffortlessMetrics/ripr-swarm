<!-- section: Fixed -->
- TypeScript checks follow a class imported through a re-export barrel
  (`import { Url } from "../src"`) to its methods and constructor, so the
  owner no longer reads `no_static_path` when the same test importing the
  declaring module reads `exposed`; same-named classes of other modules,
  ambiguous `export *`, mocked modules, shadowed names and type-only uses stay
  uncredited (#13592).
