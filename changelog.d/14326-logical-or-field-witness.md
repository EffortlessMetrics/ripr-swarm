<!-- section: Fixed -->
- Analysis: a changed constructor field or return value spelled with a logical-or (`verbose: flag == "-v" || flag == "--verbose"`) is no longer read as an opaque closure, so an exact assertion on that field closes the gap instead of leaving propagation weak with nothing missing (#14326).
