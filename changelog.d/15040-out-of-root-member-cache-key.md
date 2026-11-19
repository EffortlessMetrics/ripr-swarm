<!-- section: Fixed -->
- Rust: a workspace member outside the root (`members = ["../shared"]`) is now
  part of the classified cache key. Declaring or removing a trait in such a
  member, or renaming its crate, changed what a cold run reported while a warm
  run kept serving the earlier seam classes (#15040).
