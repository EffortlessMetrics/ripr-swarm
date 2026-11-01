<!-- section: Fixed -->
- A changed `unsafe fn` or an `unsafe` block that calls `unreachable_unchecked` / `assert_unchecked` now reports the stop reason `unsafe_obligation` instead of the generic `static_probe_unknown`, with a next step to review the safety contract by hand. The class stays `static_unknown`, nothing is credited, and no test-stub repair is offered (#11549).
