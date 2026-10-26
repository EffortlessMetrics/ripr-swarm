<!-- section: Fixed -->
- A struct field declaration inside an item macro such as `pin_project! { pub struct X { .. } }` is no longer read as a `field_construction` and a false `reachable_unrevealed` warning; it is a declaration-only `static_unknown`, while a construction inside a macro keeps its probe (#10268).
