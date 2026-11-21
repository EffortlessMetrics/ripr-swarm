<!-- section: Fixed -->
- Analysis: a same-file test that parses a string literal with `"..".parse::<T>()` now reaches a changed `impl FromStr for T` the same way `T::from_str("..")` does, instead of reading as a weak, name-only relation; an untyped receiver, another type, a redeclared type or a local `FromStr` trait keeps its previous reading (#5334).
