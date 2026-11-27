<!-- section: Fixed -->
- An edit to a line inside an `unsafe fn`, or inside an `unsafe {}` block with
  an unchecked assumption, now keeps the `unsafe_obligation` stop reason. Since
  #15348 that edit is reported once, as its own behavior, but the warning that
  the change alters an unsafe contract no test can discharge was being dropped
  along with the duplicate. It is now shown on the remaining finding, and the
  finding's class is unchanged (#16302).
