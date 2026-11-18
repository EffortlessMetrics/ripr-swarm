<!-- section: Changed -->
- An integration test that imports a library function through one test-crate
  module that `pub use`s a library module (`use self::<module>::<path>::<entry>;`)
  is credited as reaching it when the resolver resolves the hop; aliased,
  shadowed and cyclic routes stay uncredited (#9971, #12387).
