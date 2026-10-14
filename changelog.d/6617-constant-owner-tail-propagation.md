<!-- section: Fixed -->
- A function whose whole body is one literal-only expression (`fn answer() -> u32 { 40 + 2 }`) now establishes propagation to its return, so an exact pin like `assert_eq!(answer(), 42)` reads `exposed` instead of a false `weakly_exposed` gap. Any name, call, cast, closure, macro, block, branch or statement in the body still gets no witness (#6617).
