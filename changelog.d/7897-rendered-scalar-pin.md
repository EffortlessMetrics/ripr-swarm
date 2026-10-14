<!-- section: Fixed -->
- An exact rendering of a primitive-scalar owner result now pins the owner: `assert_eq!(format!("{:?}", shine(2)), "94")` reads `exposed` instead of an unrevealed gap. Format flags, width, extra text, non-literal expected values and non-scalar owners still refuse, and a nested `std::matches!` or `std::format!` is no longer read as the plain macro (#7897).
