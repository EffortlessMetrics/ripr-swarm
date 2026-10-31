<!-- section: Fixed -->
- Rust operator findings no longer treat a type-path call as the changed operator method when a same-module `type`, `trait` or foreign-named item can shadow an impl whose self type is written as a qualified path, which could read as a false `weakly_exposed` (#10625).
