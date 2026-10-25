<!-- section: Fixed -->
- An item a test imports or defines under the owner's name (`use super::other::inc;`, a nested `fn inc`) no longer reads as the owner passed as a function value, so a real weak gap beside it stays `weakly_exposed` instead of `static_unknown` (#10162).
