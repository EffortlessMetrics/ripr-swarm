<!-- section: Fixed -->
- The Rust cold pilot no longer walks a test body's whole syntax tree once per assertion to look for an owner-shadowing item when the body has no `fn` of that name and no `use` item, which cut the regex pilot about a third on top of #12710 (#12741).
