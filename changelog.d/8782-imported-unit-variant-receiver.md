<!-- section: Fixed -->
- An inherent method called on a bare enum variant brought in by exactly one non-aliased `use Enum::*` or `use Enum::Variant` now resolves to the enum's own impl. The two serde-derive rename rows therefore reach the `rust_inherent_receiver_unestablished` withhold and read `static_unknown` instead of a false gap, and their known-false entries are removed (#8782).
