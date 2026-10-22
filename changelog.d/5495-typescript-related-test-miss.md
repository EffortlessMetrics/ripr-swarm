<!-- section: Added -->
- TypeScript related-test rows on a `weakly_exposed` finding now carry a typed, row-local `miss` (`no_call_path`, `weak_assertion` or `observation_unconfirmed`) and the shared `why`, assigned after the finding is final and never read by any decision; rows whose own evidence does not support a reason keep no miss (#5495).
