<!-- section: Fixed -->
- A changed Rust field write whose test runs the method and then checks only other fields of the value (directly, or through a getter that returns another field) is now reported as `weakly_exposed` with the missing field check, instead of `static_unknown`. Any other shape still abstains (#12771, RIPR-SPEC-0228 examples 3, 4 and 6).
