<!-- section: Fixed -->
- Rust Display/Debug `fmt` reach no longer types a binding as the local type from a foreign-prefixed initializer (`let merge = other_crate::Merge { .. };`); only bare, `crate::`, `self::` and `super::` initializers do, matching annotations (#8999).
