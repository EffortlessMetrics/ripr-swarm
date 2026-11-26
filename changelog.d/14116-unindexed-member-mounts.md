<!-- section: Fixed -->
- Draft mode no longer refuses assertions across the whole workspace because
  another member, outside the draft index, compiles one indexed file through
  `#[path = "..."]`. That member is now read in full, and only the files it
  mounts lose placement. A member whose sources cannot be bounded (a symlink,
  a non-literal or nested `#[path]`, an `include!` that leaves the member, or
  more source than the reading allows) still switches placement off (#14116).
