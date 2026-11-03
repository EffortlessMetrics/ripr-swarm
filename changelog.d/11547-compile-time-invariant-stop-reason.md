<!-- section: Fixed -->
- A changed compile-time assertion (`const _: () = assert!(..)`, `static_assertions::const_assert!`) now reports the stop reason `compile_time_invariant` instead of the false `no_changed_rust_line`; a violating edit fails to build, so ripr names that rather than a test gap. The class stays `static_unknown` (#11547).
