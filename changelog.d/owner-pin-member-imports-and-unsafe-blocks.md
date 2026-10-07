<!-- section: Fixed -->
- An exact `assert_eq!` in one workspace member now pins an owner it
  imports from another member (`use pricing::score;` in `orders`) when
  the manifests bind that crate name to the owner's package by `path`.
  Before, a virtual workspace's member imports read as foreign and the
  pin was refused, leaving a discriminating test `weakly_exposed`. A
  `package =` rename to another package, a `git` or registry source, a
  `[patch]`/`[replace]` or a substituting `.cargo/config` still refuses
  (RIPR-SPEC-0197 rule 5, #6955).
- `assert_eq!(unsafe { byte_at(b"xyz", 1) }, b'y')` now pins an `unsafe fn`
  owner: an `unsafe` block whose only content is the owner call counts as
  the call. A block with a statement, or anything chained after the call
  or the block, is still refused (RIPR-SPEC-0197 rule 1, #6955).
