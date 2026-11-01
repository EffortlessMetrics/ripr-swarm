<!-- section: Fixed -->
- The trait-dispatch ambiguity pointer now reads "called from N of the M `fmt` impls in this crate (...)" so the calling-impl count is no longer confused with the crate's total impl count, and its message no longer says "several impls" when one impl calls the owner (#11719).
