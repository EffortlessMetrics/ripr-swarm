<!-- section: Fixed -->
- An exact pin through a turbofish call (`doubled::<u8>(5)`, `s.apply::<u8>(4)`) is now credited like its plain spelling instead of reading as a gap, and a fully qualified call of a trait function without `self` (`<Square as Make>::make(2)`) is now related to the impl it names, as `Square::make(2)` is (#15277).
