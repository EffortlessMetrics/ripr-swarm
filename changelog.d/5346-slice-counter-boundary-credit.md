<!-- section: Fixed -->
- A boundary check on a per-byte loop counter now credits when the tests call the owner with literal `&[u8]` or byte-string inputs whose lengths differ by one and straddle the boundary; other lengths still abstain (Advances #5346).
