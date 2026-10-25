<!-- section: Fixed -->
- A changed comparison on a `mut` parameter is now matched to the inputs related tests pass while no write can reach the comparison first, instead of reading the parameter as an unreadable local (#10306).
