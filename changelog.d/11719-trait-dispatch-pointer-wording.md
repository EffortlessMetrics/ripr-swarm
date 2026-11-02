<!-- section: Fixed -->
- The trait-dispatch ambiguity pointer now reads "called from N of the M `fmt` impls in this crate (...)" and "cannot tell which of the calling impls, if any, a test reaches", so the calling-impl count is no longer confused with the crate's total impl count; the message now says the crate (not the callers) has several impls of the method (#11719).
