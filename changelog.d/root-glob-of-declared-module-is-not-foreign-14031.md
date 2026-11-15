<!-- section: Fixed -->
- A crate whose root file re-exports a module it declares (`mod global_rng; pub use global_rng::*;`) no longer loses assertion credit in every test: the glob is read as the file's own module, not a foreign glob that might rebind `assert_eq!`. Before, every finding in such a crate read unknown or weak, with no usable repair route (#14031).
