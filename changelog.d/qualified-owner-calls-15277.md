<!-- section: Fixed -->
- An exact pin through a turbofish call (`doubled::<u8>(5)`, `s.apply::<u8>(4)`) or a fully qualified call of a trait function without `self` (`<Square as Make>::make(2)`) is now credited like its plain spelling instead of reading as a gap (#15277).
