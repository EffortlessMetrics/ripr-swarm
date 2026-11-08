<!-- section: Fixed -->
- Rust owner pins now read a method called on a tuple-constructor receiver (`Money(2).add(..)`) and a `.0` read of a one-field integer tuple struct the changed tail builds whole, under an explicitly imported standard operator trait, so a test that pins `r.0` of an `.add(` result is credited instead of reading `weakly_exposed` (#12796).
